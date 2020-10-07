extern crate chat_scraper;
extern crate regex;
extern crate reqwest;
extern crate serde;
extern crate serde_derive;
extern crate serde_json;

use chat_scraper::video_details::VideoDetails;
use reqwest::blocking::Client;
use reqwest::header;
use std::io::{BufWriter, Write};
use std::{fs, io, sync};

const POST_BODY: &str = r#"{"hidden": false, "context": {"client": {"hl": "en", "gl": "JP", "userAgent": "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/84.0.4147.125 Safari/537.36,gzip(gfe)", "clientName": "WEB", "clientVersion": "2.20200822.00.00", "osName": "X11", "browserName": "Chrome", "browserVersion": "84.0.4147.125"}}}"#;

type Json = serde_json::Value;

fn build_api_url(continuation: &str, api_key: &str) -> String {
    format!(
        "https://www.youtube.com/youtubei/v1/live_chat/get_live_chat_replay?continuation={continuation}%253D&key={api_key}",
        continuation = continuation,
        api_key = api_key
    )
}

fn extract_continuation_from_json(data: &Json) -> Option<&str> {
    Some(
        data.get("continuationContents")?
            .get("liveChatContinuation")?
            .get("continuations")?
            .get(0)?
            .get("liveChatReplayContinuationData")?
            .get("continuation")?
            .as_str()?,
    )
}

fn extract_actions_from_json(data: &Json) -> Option<&Json> {
    Some(
        data.get("continuationContents")?
            .get("liveChatContinuation")?
            .get("actions")?,
    )
}

fn extract_timestamp_from_json(data: &Json) -> Option<&str> {
    Some(
        data.get("replayChatItemAction")?
            .get("videoOffsetTimeMsec")?
            .as_str()?,
    )
}

fn fetch_live_chats_once(
    client: &Client,
    continuation: &str,
    api_key: &str,
) -> Result<Option<(String, Json)>, Box<dyn std::error::Error>> {
    let data: Json = client
        .post(&build_api_url(continuation, api_key))
        .header(header::CONTENT_TYPE, "application/json")
        .body(POST_BODY)
        .send()?
        .json()?;
    let continuation = extract_continuation_from_json(&data);
    let actions = extract_actions_from_json(&data);
    Ok(if let (Some(c), Some(a)) = (continuation, actions) {
        Some((c.to_owned().replace("%3D", ""), a.to_owned()))
    } else {
        None
    })
}

fn fetch_all_live_chats(video_id: &str) -> reqwest::Result<Vec<Json>> {
    let video_details = VideoDetails::get(video_id).unwrap();
    let (api_key, mut continuation) = (video_details.api_key, video_details.continuation);

    let client = Client::new();
    let mut result = vec![];
    let (send, recv) = sync::mpsc::channel();
    let handle = std::thread::spawn(move || loop {
        match fetch_live_chats_once(&client, &continuation, &api_key).unwrap() {
            Some((c, mut a)) => {
                continuation = c;
                let actions = a.as_array_mut().unwrap().clone();
                send.send(actions).unwrap();
            }
            None => {
                println!();
                break;
            }
        }
    });

    for mut actions in recv {
        if let Some(timestamp) = extract_timestamp_from_json(actions.last().unwrap()) {
            print!(
                "\rProgress: {:.2}%; Total live chats: {}",
                timestamp.parse::<f64>().unwrap() / (video_details.duration as f64) * 100.0,
                result.len() + actions.len()
            );
            io::stdout().flush().unwrap();
        }
        result.append(&mut actions)
    }
    handle.join().unwrap();

    Ok(result)
}

fn main() {
    print!("Live streaming id: ");
    io::stdout().flush().unwrap();
    let mut video_id = String::new();
    io::stdin().read_line(&mut video_id).unwrap();
    video_id = video_id.trim_end().to_owned();
    println!("Set target: {}", video_id);

    let live_chats = fetch_all_live_chats(video_id.as_str()).unwrap();
    println!(
        "Fetched all live chats successfully!: {} live chats",
        live_chats.len()
    );

    let dist = "./".to_owned() + video_id.as_str() + "-chats.json";
    let file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&dist)
        .unwrap();
    serde_json::to_writer_pretty(BufWriter::new(file), &live_chats).unwrap();

    println!("Live chats was saved at: {}", dist);
}
