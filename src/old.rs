mod tui;

use anyhow::{bail, Result};
use dirs::home_dir;
use ipfs_api::{response::IpfsHeader, IpfsApi, IpfsClient, TryFromUri};
use multibase::Base;
use std::{collections::HashMap, fs, path::PathBuf, process::Command};
use tokio::io::AsyncWriteExt;
use tokio_stream::StreamExt;

use clap::{App, Arg};
use crossterm::style::Stylize;
use tui::{Tui, BEE, BRUSH};
const APP_FOLDER_NAME: &str = ".pollen_wall";

fn decode_msg(input: String) -> Result<String> {
    let decoded = Base::decode(&Base::Base64Pad, input)?;
    String::from_utf8(decoded).map_err(|err| anyhow::anyhow!(err))
}

fn get_app_folder_path(home: &str) -> PathBuf {
    let mut app_folder_path = PathBuf::new();
    app_folder_path.push(&home);
    app_folder_path.push(APP_FOLDER_NAME);
    app_folder_path
}

#[derive(Debug, Default)]
struct PolledPicInfo {
    hash: String,
    name: String,
    size: u64,
}

impl PolledPicInfo {
    fn new(hash: String, name: String, size: u64) -> Self {
        PolledPicInfo { hash, name, size }
    }
}

impl From<&IpfsHeader> for PolledPicInfo {
    fn from(header: &IpfsHeader) -> Self {
        PolledPicInfo::new(header.hash.clone(), header.name.clone(), header.size)
    }
}

#[derive(Debug, PartialEq)]
enum PollenStatus {
    Processing,
    Done,
    OnceSetAsWallpaper,
}

#[derive(Debug)]
struct PollenInfo {
    id: String,
    source: String,
    hash_of_current_iteration: String,
    last_polled_pic: Option<PolledPicInfo>,
    status: PollenStatus,
}

impl Default for PollenInfo {
    fn default() -> Self {
        PollenInfo {
            id: String::new(),
            source: String::new(),
            hash_of_current_iteration: String::new(),
            last_polled_pic: None,
            status: PollenStatus::Processing,
        }
    }
}

impl PollenInfo {
    fn new(id: String, source: String, hash_of_current_iteration: String) -> Self {
        Self {
            id,
            source,
            hash_of_current_iteration,
            last_polled_pic: None,
            status: PollenStatus::Processing,
        }
    }
}

// "65.108.44.19", 5001
#[tokio::main]
async fn main() -> Result<()> {
    let matches = App::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .arg(
            Arg::with_name("addr")
                .help("You may give a custom address to pollinations ipfs node.")
                .short("a")
                .long("address")
                .value_name("addr")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("home")
                .help("If \"pollen_wall\" couldn't determine your home directory, to help it please run it with \"--home <absolute-path-to-your-home-directory>\"")
                .long("home")
                .value_name("home")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("clean")
                .help("Remove pollens in \"~/.pollen_wall\" directory.")
                .short("c")
                .long("clean")
                .takes_value(false),
        )
        .get_matches();

    let tui = Tui::new();
    // Try to discover user's home directory
    let home = match home_dir() {
        Some(dir) => dir,
        None => {
            if let Some(path) = matches.value_of("home") {
                PathBuf::from(path)
            } else {
                tui.clear_lines(1)?;
                bail!("{} {}", BEE, "\"pollenwall\" couldn't determine the location of your home directory, to help it please run it with \"--home <absolute-path-to-your-home-directory>\"".blue());
            }
        }
    };

    let app_folder_path = get_app_folder_path(&home.to_string_lossy());
    // dbg!(&app_folder_path);
    if !app_folder_path.exists() {
        tui.app_folder_not_found()?;
        fs::create_dir_all(&app_folder_path)?;
    }

    if matches.is_present("clean") {
        // Some cleaning..
        fs::remove_dir_all(&app_folder_path)?;
        fs::create_dir_all(&app_folder_path)?;

        println!("{}{}{}", BRUSH, " Cleaned all pollens! ".green(), BRUSH,);
    }

    let mut mutltiaddr = "/ip4/65.108.44.19/tcp/5005";
    if matches.is_present("addr") {
        if let Some(addr) = matches.value_of("addr") {
            mutltiaddr = addr;
        }
    }

    let client = IpfsClient::from_multiaddr_str(mutltiaddr).unwrap();
    let processing_subscription = client.pubsub_sub("processing_pollen", true);
    let done_subscription = client.pubsub_sub("done_pollen", true);
    let mut merged = done_subscription.merge(processing_subscription);
    let mut pollens = HashMap::<String, PollenInfo>::new();
    let mut current_pollen_id: Option<String> = None;

    println!(
        "{}{}{}\n",
        BEE,
        " Waiting for a new pollens to arrive, keep it running.. zZzZ ".yellow(),
        BEE,
    );

    // Listen
    while let Some(input) = merged.next().await {
        match input {
            Ok(res) => {
                if let Some(msg) = res.data {
                    let msg = decode_msg(msg)?;
                    if !msg.contains("HEARTBEAT") {
                        let hash = msg.to_owned();
                        let path = format!("/ipfs/{}/output", &hash);

                        if let Some(sources) = res.topic_ids {
                            // There can be only one source in our current subscription
                            let source = sources.first().unwrap().clone();

                            match source.as_str() {
                                "done_pollen" => {
                                    println!("{}", "Pollen arrived!".green());
                                    if let Ok(res) =
                                        client.block_stat(&*format!("{}/input", &hash)).await
                                    {
                                        if pollens.contains_key(&res.key) {
                                            let done_pollen = pollens.get_mut(&res.key).unwrap();
                                            done_pollen.status = PollenStatus::Done;
                                        }
                                    }
                                }
                                "processing_pollen" => {
                                    // println!("From: {:?}", &source);
                                }
                                _ => println!("{}", "Unknown topic".red()),
                            }

                            // Store pollen info
                            if let Ok(res) = client.block_stat(&*format!("{}/input", &hash)).await {
                                if !pollens.contains_key(&res.key) {
                                    pollens.insert(
                                        res.key.to_owned(),
                                        PollenInfo::new(
                                            res.key.to_owned(),
                                            source.to_owned(),
                                            hash.to_owned(),
                                        ),
                                    );
                                } else if let Some(info) = pollens.get_mut(&res.key) {
                                    info.source = source.to_owned();
                                    info.hash_of_current_iteration = hash.to_owned();
                                }
                                current_pollen_id = Some(res.key);
                            }
                        }
                        // println!("{}", msg);

                        // Find the latest processing or outputted file
                        if let Ok(r) = client.file_ls(&path).await {
                            if let (Some(last_result), _) =
                                r.objects.values().next().unwrap().links.iter().fold(
                                    (None, 0_usize),
                                    |mut index: (Option<&IpfsHeader>, usize), header| {
                                        let extracted: String = header
                                            .name
                                            .chars()
                                            .filter(|c| c.is_numeric())
                                            .collect();
                                        if !extracted.is_empty() {
                                            let current_index = extracted.parse::<usize>().unwrap();
                                            if current_index > index.1 {
                                                index.1 = current_index;
                                                index.0 = Some(header);
                                            }
                                        }
                                        index
                                    },
                                )
                            {
                                let mut pollen_uuid = "";
                                let mut pollen_status_tag = "";
                                let mut pollen_status = PollenStatus::Processing;
                                let mut pollen_current_hash = "";
                                if let Some(id) = &current_pollen_id {
                                    if let Some(pollen_info) = pollens.get_mut(id) {
                                        pollen_uuid = &pollen_info.id;
                                        pollen_current_hash =
                                            &pollen_info.hash_of_current_iteration;
                                        match pollen_info.status {
                                            PollenStatus::Processing => {
                                                pollen_status_tag = "p";
                                            }
                                            PollenStatus::Done => {
                                                pollen_status_tag = "d";
                                                pollen_status = PollenStatus::Done;
                                            }
                                            _ => {}
                                        }
                                        pollen_info.last_polled_pic =
                                            Some(PolledPicInfo::from(last_result));
                                    }
                                }

                                // Currently only done pollens
                                if let PollenStatus::Done = pollen_status {
                                    // Maybe add the name?

                                    let mut fp = app_folder_path.clone();

                                    fp.push(format!(
                                        "{}_{}_{}",
                                        &pollen_uuid, pollen_status_tag, &last_result.name
                                    ));

                                    let mut file = tokio::fs::File::create(&fp).await?;
                                    let mut c = client.get(&last_result.hash);
                                    let mut cnt = 0;
                                    while let Some(Ok(buf)) = c.next().await {
                                        if cnt == 0 {
                                            // Hack, I am too tired to learn to get the contents
                                            // First 512 bytes shouldn't be written.
                                            file.write_all(&buf.slice(512..)).await?;
                                        } else {
                                            file.write_all(&buf.slice(0..)).await?;
                                        }
                                        cnt += 1;
                                    }

                                    // Set wallpaper in mac
                                    Command::new("osascript")
                                    .arg("-e")
                                    .arg(format!("tell application \"System Events\" to tell every desktop to set picture to \"{}\"", fp.as_path().to_str().unwrap()))
                                    .output()
                                    .expect("failed to execute process");

                                    if let Some(id) = &current_pollen_id {
                                        if let Some(pollen_info) = pollens.get_mut(id) {
                                            pollen_info.status = PollenStatus::OnceSetAsWallpaper;
                                        }
                                    }

                                    println!("{}", "Wallpaper set with the new pollen!".magenta());
                                    println!(
                                        "{}{}",
                                        "You may find this pollen at: ".yellow(),
                                        format!("https://ipfs.io/ipfs/{}\n", &pollen_current_hash)
                                    );
                                }

                                // let dv = pollens
                                //     .keys()
                                //     .filter(|key| {
                                //         PollenStatus::Done == pollens.get(*key).unwrap().status
                                //     })
                                //     .collect::<Vec<&String>>();
                                // let pv = pollens
                                //     .keys()
                                //     .filter(|key| {
                                //         PollenStatus::Processing
                                //             == pollens.get(*key).unwrap().status
                                //     })
                                //     .collect::<Vec<&String>>();

                                // println!("PROCESSING_POLLENS");
                                // println!("{:#?}", pv);
                                // println!("DONE_POLLENS");
                                // println!("{:#?}", dv);
                            }
                        }
                    }
                }
            }
            Err(err) => {
                dbg!(err);
            }
        }
    }
    Ok(())
}
