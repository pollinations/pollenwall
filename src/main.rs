mod tui;

use anyhow::{bail, Result};
use dirs::home_dir;
use ipfs_api::{
    response::{FileLsResponse, IpfsHeader},
    IpfsApi, IpfsClient, TryFromUri,
};
use multibase::Base;
use std::{collections::HashMap, fs, path::PathBuf};
use tokio::io::AsyncWriteExt;
use tokio_stream::StreamExt;

use clap::{App, Arg};
use crossterm::style::Stylize;
use tui::{Tui, BEE, BRUSH};
const APP_FOLDER_NAME: &str = ".pollen_wall";
const DEFAULT_POLLINATIONS_MULTIADDR: &str = "/ip4/65.108.44.19/tcp/5005";

#[derive(Debug, PartialEq)]
enum PollenStatus {
    Processing,
    Done,
    OnceSetAsWallpaper,
}

#[derive(Debug)]
struct PollenInfo {
    // TODO: Decide if this id is redundant
    #[allow(dead_code)]
    id: String,
    //
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
    #[allow(dead_code)]
    fn new(id: String, source: String, hash_of_current_iteration: String) -> Self {
        Self {
            id,
            source,
            hash_of_current_iteration,
            last_polled_pic: None,
            status: PollenStatus::Processing,
        }
    }

    fn with_status(
        id: String,
        source: String,
        hash_of_current_iteration: String,
        status: PollenStatus,
    ) -> Self {
        Self {
            id,
            source,
            hash_of_current_iteration,
            last_polled_pic: None,
            status,
        }
    }
}

#[derive(Debug, Default)]
#[allow(dead_code)]
// Currently not used but maybe used later
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

#[tokio::main]
async fn main() -> Result<()> {
    // Args and tui
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
    tui.hide_cursor()?;

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

    if !app_folder_path.exists() {
        tui.app_folder_not_found()?;
        // Create ~/.pollen_wall
        fs::create_dir_all(&app_folder_path)?;
    }

    // Clean ~/.pollen_wall folder
    if matches.is_present("clean") {
        // Some cleaning..
        fs::remove_dir_all(&app_folder_path)?;
        fs::create_dir_all(&app_folder_path)?;

        println!("{}{}{}", BRUSH, " Cleaned all pollens! ".green(), BRUSH,);
    }

    // Set pollinations address
    let mut mutltiaddr = DEFAULT_POLLINATIONS_MULTIADDR;
    if matches.is_present("addr") {
        if let Some(addr) = matches.value_of("addr") {
            mutltiaddr = addr;
        }
    }

    // Init
    let client = IpfsClient::from_multiaddr_str(mutltiaddr).unwrap();
    let processing_subscription = client.pubsub_sub("processing_pollen", true);
    let done_subscription = client.pubsub_sub("done_pollen", true);
    let mut merged = done_subscription.merge(processing_subscription);
    let mut pollens = HashMap::<String, PollenInfo>::new();

    println!(
        "{}{}{}",
        BEE,
        " Waiting for new pollens to arrive, keep it running.. zZzZ ".yellow(),
        BEE,
    );

    // Listen for `processing_pollen` and `done_pollen` topics
    while let Some(input) = merged.next().await {
        match input {
            Ok(res) => {
                if let Some(msg) = res.data {
                    // Decode base64 response
                    let msg = decode_msg(msg)?;
                    // Filter `HEARTBEAT` messages in the stream
                    if !msg.contains("HEARTBEAT") {
                        let hash = msg;
                        // Path for the current pollen output
                        let path = format!("/ipfs/{}/output", &hash);
                        // Unwrap is safe here because there will always be a topic.
                        let topic = get_current_topic(&res.topic_ids.unwrap());

                        match &*topic {
                            "done_pollen" => {
                                // Get the cid of `<hash>/input` which will always be constant for a pollen throughout its evolution.
                                // I'll refer it as uuid from now on.
                                if let Ok(res) =
                                    client.block_stat(&*format!("{}/input", &hash)).await
                                {
                                    let pollen_uuid = &res.key;

                                    if let Some(pollen) = pollens.get_mut(pollen_uuid) {
                                        // Pollen is being tracked already so update its info

                                        match pollen.status {
                                            // Ignore pollen if it once set as wallpaper
                                            // This would help filtering for duplicate done messages.
                                            PollenStatus::OnceSetAsWallpaper => {}
                                            _ => pollen.status = PollenStatus::Done,
                                        }
                                        pollen.source = topic.to_owned();
                                        pollen.hash_of_current_iteration = hash.to_owned();
                                    } else {
                                        // Pollen not tracked yet, store it
                                        // Since it is a done pollen tag it.
                                        pollens.insert(
                                            res.key.to_owned(),
                                            PollenInfo::with_status(
                                                res.key.to_owned(),
                                                topic.to_owned(),
                                                hash.to_owned(),
                                                PollenStatus::Done,
                                            ),
                                        );
                                    }

                                    // We know that here the hashmap includes this pollen.
                                    if let PollenStatus::Done =
                                        pollens.get_mut(pollen_uuid).unwrap().status
                                    {
                                        // Find the latest evolution (image) of pollen
                                        if let Ok(res) = client.file_ls(&path).await {
                                            if let Some(header) =
                                                get_the_latest_image_according_to_numbering(&res)
                                            {
                                                // It is a CLIP+VQGAN model
                                                println!("\n{}", "Pollen arrived!".green());

                                                // Make a path in the format of `uuid_name_extension`
                                                let mut file_path = app_folder_path.clone();
                                                // TODO: Add the `text_input` somewhere
                                                file_path.push(format!(
                                                    "{}_{}",
                                                    &pollen_uuid, &header.name
                                                ));
                                                // Make the file
                                                let mut file =
                                                    tokio::fs::File::create(&file_path).await?;

                                                // TODO: This should be unnecessary learn to use Bytes crate see hack below
                                                let mut cnt = 0;

                                                // Download and write the file
                                                let mut download_stream = client.get(&header.hash);
                                                while let Some(Ok(buf)) =
                                                    download_stream.next().await
                                                {
                                                    if cnt == 0 {
                                                        // Hack, I am too tired to learn to get the contents properly
                                                        // First 512 bytes shouldn't be written.
                                                        file.write_all(&buf.slice(512..)).await?;
                                                    } else {
                                                        file.write_all(&buf.slice(0..)).await?;
                                                    }
                                                    cnt += 1;
                                                }

                                                // Close file
                                                file.shutdown().await?;

                                                // Set wallpaper
                                                let wallpaper_path =
                                                    String::from(file_path.to_string_lossy());
                                                let cid = header.hash.clone();
                                                tokio::spawn(async move {
                                                    // We need to delay setting the wallpaper a little for Windows
                                                    // or there will be a black screen set.
                                                    tokio::time::sleep(
                                                        tokio::time::Duration::from_millis(100),
                                                    )
                                                    .await;

                                                    match wallpaper::set_from_path(&wallpaper_path)
                                                    {
                                                        // Notify user
                                                        Ok(_) => {
                                                            println!(
                                                                "{}",
                                                                "Wallpaper set with the new pollen!"
                                                                    .magenta()
                                                            );
                                                            println!(
                                                                "{}{}",
                                                                "You may find this pollen at: "
                                                                    .yellow(),
                                                                format!(
                                                                    "https://ipfs.io/ipfs/{}",
                                                                    &cid
                                                                )
                                                            );
                                                        }
                                                        Err(err) => {
                                                            eprintln!(
                                                                "{}{}",
                                                                " Failed to set wallpaper: ".red(),
                                                                err,
                                                            );
                                                        }
                                                    }
                                                });

                                                // Update pollen info
                                                if let Some(pollen) = pollens.get_mut(pollen_uuid) {
                                                    pollen.status =
                                                        PollenStatus::OnceSetAsWallpaper;
                                                    pollen.last_polled_pic =
                                                        Some(PolledPicInfo::from(header));
                                                }

                                                // TODO: Download the video result maybe?

                                                // TODO: Refactor this
                                                // Delete pollen from storage after some time
                                                let wallpaper_path =
                                                    String::from(file_path.to_string_lossy());
                                                tokio::spawn(async move {
                                                    tokio::time::sleep(
                                                        tokio::time::Duration::from_millis(4000),
                                                    )
                                                    .await;
                                                    tokio::fs::remove_file(wallpaper_path).await
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                            "processing_pollen" => {
                                // Currently ignoring these..
                                // Something interesting might be done later.
                            }
                            // We're not subscribing to any other topics.
                            _ => {
                                println!("{}{}", "Unknown topic: ".red(), topic);
                            }
                        }
                    }
                }
            }
            Err(err) => {
                // Pubsub error
                eprintln!("{:?}", err);
            }
        }
    }
    // Hopefully unreachable :)
    Ok(())
}

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

fn get_current_topic(topics: &[String]) -> String {
    // Unwrap is safe here because there will always be one topic.
    topics.first().unwrap().clone()
}

fn get_the_latest_image_according_to_numbering(
    response: &'_ FileLsResponse,
) -> Option<&'_ IpfsHeader> {
    let result = response
        .objects
        .values()
        .next()
        .unwrap()
        .links
        .iter()
        .fold(
            (None, 0_usize),
            |mut index: (Option<&IpfsHeader>, usize), header| {
                // Extract the digits from the name which has the format `ccc..._ddddd.jpg` example `processing_00005.jpg`.
                let extracted: String = header
                    .name
                    .chars()
                    .filter(|c| c.is_numeric() && header.name.contains("progress"))
                    .collect();
                if !extracted.is_empty() {
                    // Parse it as number, we know that it is numeric so unwrap is fine.
                    let current_index = extracted.parse::<usize>().unwrap();
                    // Continue folding to find the last one
                    if current_index > index.1 {
                        index.1 = current_index;
                        index.0 = Some(header);
                    }
                }
                index
            },
        )
        // We only need the header
        .0;
    result
}
