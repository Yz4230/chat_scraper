extern crate regex;
extern crate reqwest;
extern crate serde;
extern crate serde_derive;
extern crate serde_json;

use regex::Regex;
use reqwest::{header, Client};
use std::io::{BufWriter, Write};
use std::sync::Arc;
use std::{fs, io};
use tokio::sync::Mutex;

const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/84.0.4147.125 Safari/537.36,gzip(gfe) ";
const POST_BODY: &str = r#"{"hidden": false, "context": {"client": {"hl": "en", "gl": "JP", "userAgent": "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/84.0.4147.125 Safari/537.36,gzip(gfe)", "clientName": "WEB", "clientVersion": "2.20200822.00.00", "osName": "X11", "browserName": "Chrome", "browserVersion": "84.0.4147.125"}}}"#;

type Json = serde_json::Value;

fn build_video_url(video_id: &str) -> String {
    "https://www.youtube.com/watch?v=".to_owned() + video_id
}

fn build_api_url(continuation: &str, api_key: &str) -> String {
    format!(
        "https://www.youtube.com/youtubei/v1/live_chat/get_live_chat_replay?continuation={continuation}%253D&key={api_key}",
        continuation = continuation,
        api_key = api_key
    )
}

async fn fetch_raw_html(
    client: &Client,
    video_id: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let res = client
        .get(&build_video_url(video_id))
        .header(header::USER_AGENT, USER_AGENT)
        .send()
        .await?
        .text()
        .await?;
    Ok(res)
}

fn extract_api_key_from_html(raw_html: &str) -> Option<String> {
    let re = Regex::new(r#""INNERTUBE_API_KEY":"(.*?)""#).unwrap();
    match re.captures(raw_html) {
        Some(caps) => Some(caps[1].to_owned()),
        None => None,
    }
}

fn extract_continuation_from_html(raw_html: &str) -> Option<String> {
    let re = Regex::new(r#""continuation":"([a-zA-Z0-9]+)""#).unwrap();
    match re.captures(raw_html) {
        Some(caps) => Some(caps[1].to_owned()),
        None => None,
    }
}

fn extract_duration_from_html(raw_html: &str) -> Option<i32> {
    let re = Regex::new(r#"\\"approxDurationMs\\":\\"(\d+)\\""#).unwrap();
    match re.captures(raw_html) {
        Some(caps) => Some(caps[1].to_owned().parse().unwrap()),
        None => None,
    }
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

async fn fetch_live_chats_once(
    client: &Client,
    continuation: &str,
    api_key: &str,
) -> Result<Option<(String, Json)>, Box<dyn std::error::Error>> {
    let res = client
        .post(&build_api_url(continuation, api_key))
        .header(header::CONTENT_TYPE, "application/json")
        .body(POST_BODY)
        .send()
        .await?;
    let data: Json = res.json().await?;
    let continuation = extract_continuation_from_json(&data);
    let actions = extract_actions_from_json(&data);
    Ok(if let (Some(c), Some(a)) = (continuation, actions) {
        Some((c.to_owned().replace("%3D", ""), a.to_owned()))
    } else {
        None
    })
}

async fn fetch_all_live_chats(video_id: &str) -> Result<Vec<Json>, Box<dyn std::error::Error>> {
    let client = Client::builder().cookie_store(true).build().unwrap();
    let raw_html = fetch_raw_html(&client, video_id).await.unwrap();
    let api_key = extract_api_key_from_html(raw_html.as_str()).unwrap();
    let mut continuation = extract_continuation_from_html(raw_html.as_str()).unwrap();
    let duration = extract_duration_from_html(raw_html.as_str()).unwrap() as f64;

    let estimated_chats = (duration / 200.0) as usize;
    let live_chats = Arc::new(Mutex::new(Vec::<Json>::with_capacity(estimated_chats)));
    let mut handlers = vec![];
    loop {
        match fetch_live_chats_once(&client, &continuation, &api_key).await? {
            Some((c, mut a)) => {
                continuation = c;
                let live_chats_arc = Arc::clone(&live_chats);

                handlers.push(tokio::spawn(async move {
                    let mut lock = live_chats_arc.lock().await;
                    let actions = a.as_array_mut().unwrap();
                    if let Some(timestamp) = extract_timestamp_from_json(actions.last().unwrap()) {
                        print!(
                            "\rProgress: {:.2}%; Total live chats: {}",
                            timestamp.parse::<f64>().unwrap() / duration * 100.0,
                            (*lock).len() + actions.len()
                        );
                        io::stdout().flush().unwrap();
                    }
                    (*lock).append(actions);
                }));
            }
            None => {
                println!();
                break;
            }
        }
    }

    for handler in handlers {
        handler.await;
    }
    let result = &*live_chats.lock().await;
    Ok(result.to_owned())
}

#[tokio::main]
async fn main() {
    print!("Live streaming id: ");
    io::stdout().flush().unwrap();
    let mut video_id = String::new();
    io::stdin().read_line(&mut video_id).unwrap();
    video_id = video_id.trim_end().to_owned();
    println!("Set target: {}", video_id);

    let live_chats = fetch_all_live_chats(video_id.as_str()).await.unwrap();
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
