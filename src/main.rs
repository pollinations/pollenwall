mod tui;

use anyhow::{anyhow, bail, Result};
use dirs::home_dir;
use ipfs_api::{
    response::{BlockStatResponse, FileLsResponse, IpfsHeader},
    IpfsApi, IpfsClient, TryFromUri,
};
use multibase::Base;
use std::{
    collections::HashMap,
    fs::{self},
    path::{Path, PathBuf},
    time::SystemTime,
};
use tokio::io::AsyncWriteExt;
use tokio_stream::StreamExt;

use clap::{App, Arg};
use crossterm::style::Stylize;
use tui::{Tui, BEE, BRUSH};
const APP_FOLDER_NAME: &str = ".pollen_wall";
const DEFAULT_POLLINATIONS_MULTIADDR: &str = "/ip4/65.108.44.19/tcp/5005";
const WALLPAPER_SET_DELAY: u64 = 100;
const HEARTBEAT: &str = "HEARTBEAT";

#[derive(Debug, PartialEq, Clone)]
enum Topic {
    ProcessingPollen,
    DonePollen,
    Unknown,
}

#[derive(Debug, PartialEq)]
enum PollenStatus {
    Processing,
    Done,
    OnceSetAsWallpaper,
}

#[derive(Debug, PartialEq)]
enum Model {
    WikiArt,
    VitB32,
    GuidedDiffusion,
    Unknown,
}

#[derive(Debug)]
struct PollenInfo {
    // TODO: Decide if this id is redundant
    #[allow(dead_code)]
    id: String,
    //
    topic: Topic,
    model_type: Option<Model>,
    text_input: Option<String>,
    hash_of_current_iteration: String,
    last_polled_evolution: Option<PolledEvolutionInfo>,
    status: PollenStatus,
}

impl Default for PollenInfo {
    fn default() -> Self {
        PollenInfo {
            id: String::new(),
            topic: Topic::Unknown,
            model_type: None,
            text_input: None,
            hash_of_current_iteration: String::new(),
            last_polled_evolution: None,
            status: PollenStatus::Processing,
        }
    }
}

impl PollenInfo {
    #[allow(dead_code)]
    fn new(
        id: String,
        topic: Topic,
        hash_of_current_iteration: String,
        model_type: Option<Model>,
        text_input: Option<String>,
    ) -> Self {
        Self {
            id,
            topic,
            hash_of_current_iteration,
            last_polled_evolution: None,
            model_type,
            text_input,
            status: PollenStatus::Processing,
        }
    }

    fn with_status(
        id: String,
        topic: Topic,
        hash_of_current_iteration: String,
        model_type: Option<Model>,
        text_input: Option<String>,
        status: PollenStatus,
    ) -> Self {
        Self {
            id,
            topic,
            hash_of_current_iteration,
            last_polled_evolution: None,
            model_type,
            text_input,
            status,
        }
    }
}

#[derive(Debug, Default)]
#[allow(dead_code)]
// Currently not used but maybe used later
struct PolledEvolutionInfo {
    hash: String,
    name: String,
    size: u64,
}

impl PolledEvolutionInfo {
    fn new(hash: String, name: String, size: u64) -> Self {
        PolledEvolutionInfo { hash, name, size }
    }
}

impl From<&IpfsHeader> for PolledEvolutionInfo {
    fn from(header: &IpfsHeader) -> Self {
        PolledEvolutionInfo::new(header.hash.clone(), header.name.clone(), header.size)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Args and tui
    let matches = App::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .arg(
            Arg::new("addr")
                .help("You may give a custom address to pollinations ipfs node.")
                .long("address")
                .value_name("addr")
                .takes_value(true),
        )
        .arg(
            Arg::new("home")
                .help("If \"pollen_wall\" couldn't determine your home directory, to help it please run it with \"--home <absolute-path-to-your-home-directory>\"")
                .long("home")
                .value_name("home")
                .takes_value(true),
        )
        .arg(
            Arg::new("clean")
                .help("Remove images in \"~/.pollen_wall\" directory.")
                .short('c')
                .long("clean")
                .takes_value(false),
        )
        .arg(
            Arg::new("attach")
                .help("Attach to a random processing pollen until its evolution is done.")
                .short('a')
                .long("attach")
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
    let mut pollen_uuid_to_attach: Option<String> = None;

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
                    if !msg.contains(HEARTBEAT) {
                        let hash = msg;

                        // Path for the current pollen output
                        let path = format!("/ipfs/{}/output", &hash);

                        // Unwrap is safe here because there will always be a topic.
                        let topic = match &*get_current_topic(&res.topic_ids.unwrap()) {
                            "done_pollen" => Topic::DonePollen,
                            "processing_pollen" => Topic::ProcessingPollen,
                            _ => Topic::Unknown,
                        };

                        // Ignore unknown topics
                        if let Topic::Unknown = topic {
                            continue;
                        }

                        // Get pollen uuid
                        if let Ok(BlockStatResponse {
                            key: pollen_uuid, ..
                        }) = client.block_stat(&*format!("{}/input", &hash)).await
                        {
                            let text_input =
                                get_text_input_from_pollen_uuid(&client, &pollen_uuid).await;
                            let model_type =
                                get_model_type_from_pollen_uuid(&client, &pollen_uuid).await;

                            if let Some(pollen) = pollens.get_mut(&pollen_uuid) {
                                // Pollen is being tracked already so update its info
                                pollen.topic = topic.to_owned();
                                pollen.hash_of_current_iteration = hash.to_owned();
                                pollen.model_type = model_type;
                                pollen.text_input = text_input;
                                match pollen.status {
                                    // Ignore pollen if it once set as wallpaper
                                    // This would help filtering for duplicate done messages.
                                    PollenStatus::OnceSetAsWallpaper => match topic {
                                        // Topic::ProcessingPollen => {
                                        //     // TODO: Additional logic of attaching to a processing pollen may go here.
                                        //     if matches.is_present("attach") {
                                        //         if let Some(uuid) = &pollen_uuid_to_attach {
                                        //             if pollen_uuid != *uuid {
                                        //                 // Ignore pollens which are not attached.
                                        //                 // Else even if it has the same uuid set the new evolution state as wallpaper
                                        //                 continue;
                                        //             } else {
                                        //                 // Renew status for keeping it attached
                                        //                 pollen.status = PollenStatus::Processing;
                                        //             }
                                        //         }
                                        //     }
                                        // }
                                        Topic::ProcessingPollen => {
                                            // TODO: Simplify this
                                            // Pass
                                        }
                                        Topic::DonePollen => {
                                            // Ignore done pollens which had been already set as wallpaper
                                            continue;
                                        }
                                        _ => {
                                            unreachable!();
                                        }
                                    },
                                    // Attaching logic for
                                    _ => {
                                        pollen.status = match topic {
                                            Topic::ProcessingPollen => PollenStatus::Processing,
                                            Topic::DonePollen => PollenStatus::Done,
                                            _ => unreachable!(),
                                        }
                                    }
                                }
                            } else {
                                // Pollen not tracked yet, store it
                                // Since it is a done pollen tag it.
                                pollens.insert(
                                    pollen_uuid.to_owned(),
                                    PollenInfo::with_status(
                                        pollen_uuid.to_owned(),
                                        topic.to_owned(),
                                        hash.to_owned(),
                                        model_type,
                                        text_input,
                                        match topic {
                                            Topic::DonePollen => PollenStatus::Done,
                                            Topic::ProcessingPollen => PollenStatus::Processing,
                                            _ => unreachable!(),
                                        },
                                    ),
                                );
                            }

                            // Find the latest evolution (image) of pollen
                            if let Ok(list_of_output_folder) = client.file_ls(&path).await {
                                if let Some(pollen_header) =
                                    get_the_latest_image_according_to_numbering(
                                        &list_of_output_folder,
                                    )
                                {
                                    let processing_pollens_count = pollens
                                        .values()
                                        .filter(|pollen| pollen.status == PollenStatus::Processing)
                                        .count();
                                    // We know that we have registered that pollen here so we can unwrap
                                    let pollen = pollens.get_mut(&pollen_uuid).unwrap();
                                    match pollen.status {
                                        PollenStatus::Processing => {
                                            if matches.is_present("attach") {
                                                // println!("{:?}", pollen.model_type);
                                                // println!("{:?}", pollen.text_input);

                                                // Attach to a random processing pollen
                                                if pollen_uuid_to_attach.is_none() {
                                                    pollen_uuid_to_attach =
                                                        Some(pollen_uuid.to_owned());
                                                }
                                                // A processing pollen is picked here naturally
                                                if let Some(uuid) = &pollen_uuid_to_attach {
                                                    if pollen_uuid == *uuid {
                                                        // New iteration arrived
                                                        println!("\n{}", "New generation of attached pollen is arrived!".green());
                                                        // Save pollen
                                                        let mut save_path = app_folder_path.clone();
                                                        save_path.push(&format!(
                                                            "{}_{}",
                                                            &pollen_uuid, &pollen_header.name
                                                        ));
                                                        let save_time = save_pollen(
                                                            &client,
                                                            &pollen_header.hash,
                                                            &save_path,
                                                        )
                                                        .await?;

                                                        // Set wallpaper
                                                        set_wallpaper_with_delay(
                                                            save_path.clone(),
                                                            pollen_header.hash.to_owned(),
                                                            processing_pollens_count,
                                                        );

                                                        // Update pollen info
                                                        if let Some(PollenInfo {
                                                            last_polled_evolution,
                                                            ..
                                                        }) = pollens.get_mut(&pollen_uuid)
                                                        {
                                                            *last_polled_evolution =
                                                                Some(PolledEvolutionInfo::from(
                                                                    pollen_header,
                                                                ));
                                                        }

                                                        // Keep storage clean
                                                        if let Some(save_time) = save_time {
                                                            clear_previous_pollens(
                                                                &app_folder_path,
                                                                &save_time,
                                                            )
                                                            .await?;
                                                        }
                                                    } else {
                                                        // Ignore pollens which are not attached.
                                                        continue;
                                                    }
                                                } else {
                                                    // No pollen id to attach..
                                                    // This might be unreachable
                                                    continue;
                                                }
                                            }
                                        }
                                        PollenStatus::Done => {
                                            if matches.is_present("attach") {
                                                if let Some(uuid) = &pollen_uuid_to_attach {
                                                    if pollen_uuid == *uuid {
                                                        // Attached pollen is done
                                                        // Empty the slot for a new one to attach
                                                        pollen_uuid_to_attach = None;
                                                    } else {
                                                        // Block other done pollens.
                                                        continue;
                                                    }
                                                }
                                            }

                                            println!("\n{}", "Pollen arrived!".green());

                                            // Save pollen
                                            let mut save_path = app_folder_path.clone();
                                            save_path.push(&format!(
                                                "{}_{}",
                                                &pollen_uuid, &pollen_header.name
                                            ));
                                            let save_time = save_pollen(
                                                &client,
                                                &pollen_header.hash,
                                                &save_path,
                                            )
                                            .await?;

                                            // Set wallpaper
                                            set_wallpaper_with_delay(
                                                save_path.clone(),
                                                pollen_header.hash.to_owned(),
                                                processing_pollens_count,
                                            );

                                            // Update pollen info
                                            if let Some(PollenInfo {
                                                status,
                                                last_polled_evolution,
                                                ..
                                            }) = pollens.get_mut(&pollen_uuid)
                                            {
                                                *status = PollenStatus::OnceSetAsWallpaper;
                                                *last_polled_evolution =
                                                    Some(PolledEvolutionInfo::from(pollen_header));
                                            }

                                            // Keep storage clean
                                            if let Some(save_time) = save_time {
                                                clear_previous_pollens(
                                                    &app_folder_path,
                                                    &save_time,
                                                )
                                                .await?;
                                            }

                                            // Remove from internal store with its uuid.
                                            pollens.remove_entry(&pollen_uuid);
                                        }
                                        _ => unreachable!(),
                                    }
                                } else {
                                    // Ignore model which is not a CLIP+VQGAN
                                    continue;
                                }
                            } else {
                                // Couldn't ls the output folder, ignore pollen
                                continue;
                            }
                        } else {
                            //Couldn't retrieve pollen uuid, then ignore this pollen.
                            continue;
                        }
                    }
                }
            }
            Err(err) => {
                // Pubsub error
                eprintln!("{:?}", err);
                continue;
            }
        }
    }
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
    let (result, _) = response.objects.values().next().unwrap().links.iter().fold(
        (None, 0_usize),
        |mut index: (Option<&IpfsHeader>, usize), header| {
            // Extract the digits from the name which has the format `ccc..._ddddd.jpg` example `processing_00005.jpg`.
            let extracted: String = header
                .name
                .chars()
                .filter(|c| c.is_numeric() && header.name.contains(".jpg"))
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
    );
    result
}

async fn save_pollen(
    client: &IpfsClient,
    download_hash: &str,
    save_path: &Path,
) -> Result<Option<SystemTime>> {
    let mut file = tokio::fs::File::create(save_path).await?;

    // TODO: This should be unnecessary learn to use Bytes crate see hack below
    let mut cnt = 0;

    // Download and write the file
    let mut download_stream = client.get(download_hash);
    while let Some(Ok(buf)) = download_stream.next().await {
        if cnt == 0 {
            // Hack, I am too tired to learn to get the contents properly
            // First 512 bytes shouldn't be written.
            file.write_all(&buf.slice(512..)).await?;
        } else {
            file.write_all(&buf.slice(0..)).await?;
        }
        cnt += 1;
    }

    if let Ok(metadata) = tokio::fs::metadata(save_path).await {
        if let Ok(created) = metadata.created() {
            file.shutdown().await?;
            return Ok(Some(created));
        }
    }
    file.shutdown().await?;
    Ok(None)
}

fn set_wallpaper_with_delay(
    wallpaper_path: PathBuf,
    ipfs_hash: String,
    processing_pollens_count: usize,
) {
    tokio::spawn(async move {
        // We need to delay setting the wallpaper a little for Windows
        // or there will be a black screen set.
        tokio::time::sleep(tokio::time::Duration::from_millis(WALLPAPER_SET_DELAY)).await;

        match wallpaper::set_from_path(wallpaper_path.to_str().unwrap()) {
            // Notify user
            Ok(_) => {
                println!("{}", "Wallpaper set with the new pollen!".magenta());
                println!(
                    "{}{}",
                    "You may find this pollen at: ".yellow(),
                    format!("https://ipfs.io/ipfs/{}", &ipfs_hash)
                );
                println!(
                    "{}{}{}",
                    "Currently ".yellow(),
                    processing_pollens_count.to_string().green(),
                    " pollens are processing..".yellow(),
                );
            }
            Err(err) => {
                eprintln!("{}{}", " Failed to set wallpaper: ".red(), err,);
            }
        }
    });
}

async fn clear_previous_pollens(dir_path: &Path, current_creation_time: &SystemTime) -> Result<()> {
    if let Ok(mut directory_reader) = tokio::fs::read_dir(&dir_path).await {
        while let Ok(Some(entry)) = directory_reader.next_entry().await {
            let path = entry.path().clone();

            if let Ok(metadata) = tokio::fs::metadata(&entry.path()).await {
                if let Ok(entry_creation_time) = metadata.created() {
                    if current_creation_time.elapsed().unwrap().as_millis()
                        < entry_creation_time.elapsed().unwrap().as_millis()
                    {
                        #[cfg(target_os = "linux")]
                        // Needed in Linux because for a split second when the previous
                        // wallpaper is deleted the screen turns blue.
                        tokio::spawn(async move {
                            tokio::time::sleep(tokio::time::Duration::from_millis(
                                WALLPAPER_SET_DELAY + 500,
                            ))
                            .await;
                            // TODO: Handle this result
                            tokio::fs::remove_file(&path).await;
                        })
                        .await?;

                        #[cfg(not(target_os = "linux"))]
                        // Others are fine with this.
                        tokio::fs::remove_file(&path).await?;
                    }
                }
            }
        }
        return Ok(());
    }
    Err(anyhow!("Failed to read directory"))
}

async fn get_model_type_from_pollen_uuid(client: &IpfsClient, pollen_uuid: &str) -> Option<Model> {
    let mut f = client.cat(&format!("{}/model", pollen_uuid));
    let mut model_name: String = "".into();

    while let Some(Ok(buf)) = f.next().await {
        // This is somehow ugly, we know that the text is short and most likely
        // not more than 4096 bytes so one iteration is enough to fill the buffer.
        // Although this might create bugs later since we're not
        // controlling the length of the text.
        model_name = String::from_utf8_lossy(&buf).into();
    }

    if model_name.is_empty() {
        // eprintln!("{}", "No model info found".red());
        return None;
    }

    match model_name.as_str() {
        "\"Wiki Art\"" => Some(Model::WikiArt),
        "\"ViT-B/32\"" => Some(Model::VitB32),
        "\"QoL tweaks for nshepperd…P Guided Diffusion v2.4\"" => Some(Model::GuidedDiffusion),
        _ => {
            // eprintln!("{}{}", "Found unknown model: ".red(), model_name.yellow());
            Some(Model::Unknown)
        }
    }
}

async fn get_text_input_from_pollen_uuid(client: &IpfsClient, pollen_uuid: &str) -> Option<String> {
    let mut f = client.cat(&format!("{}/text_input", pollen_uuid));
    let mut text_input: String = "".into();

    while let Some(Ok(buf)) = f.next().await {
        // This is somehow ugly, we know that the text is short and most likely
        // not more than 4096 bytes so one iteration is enough to fill the buffer.
        // Although this might create bugs later since we're not
        // controlling the length of the text.
        text_input = String::from_utf8_lossy(&buf).into();
    }

    if text_input.is_empty() {
        // eprintln!("{}", "No text input found".red());
        None
    } else {
        Some(text_input)
    }
}
