extern crate regex;
extern crate reqwest;
extern crate serde;
extern crate serde_derive;
extern crate serde_json;

use regex::Regex;
use reqwest::blocking::Client;
use reqwest::header;
use std::fs::OpenOptions;
use std::io::Write;

const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/84.0.4147.125 Safari/537.36,gzip(gfe) ";
const POST_BODY: &str = r#"{"hidden": false, "context": {"client": {"hl": "en", "gl": "JP", "userAgent": "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/84.0.4147.125 Safari/537.36,gzip(gfe)", "clientName": "WEB", "clientVersion": "2.20200822.00.00", "osName": "X11", "browserName": "Chrome", "browserVersion": "84.0.4147.125"}}}"#;

type JsonObject = serde_json::Map<String, serde_json::Value>;
type JsonArray = Vec<serde_json::Value>;

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

fn fetch_raw_html(client: &Client, video_id: &str) -> Result<String, reqwest::Error> {
    let res = client
        .get(&build_video_url(video_id))
        .header(header::USER_AGENT, USER_AGENT)
        .send()?
        .text()?;
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

fn extract_continuation_from_json(data: &JsonObject) -> Option<String> {
    Some(
        data.get("continuationContents")?
            .get("liveChatContinuation")?
            .get("continuations")?
            .as_array()?
            .get(0)?
            .as_object()?
            .get("liveChatReplayContinuationData")?
            .get("continuation")?
            .as_str()?
            .replace("%3D", ""),
    )
}

fn extract_actions_from_json(data: &JsonObject) -> Option<JsonArray> {
    Some(
        data.get("continuationContents")?
            .get("liveChatContinuation")?
            .get("actions")?
            .as_array()?
            .to_owned(),
    )
}

fn extract_timestamp_from_json(data: &serde_json::Value) -> Option<&str> {
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
) -> Result<Option<(String, JsonArray)>, reqwest::Error> {
    let res = client
        .post(&build_api_url(continuation, api_key))
        .header(header::CONTENT_TYPE, "application/json")
        .body(POST_BODY)
        .send()?;
    let data: JsonObject = serde_json::from_reader(res).unwrap();
    let continuation = extract_continuation_from_json(&data);
    let actions = extract_actions_from_json(&data);
    Ok(if let (Some(c), Some(a)) = (continuation, actions) {
        Some((c, a))
    } else {
        None
    })
}

fn fetch_all_live_chats(video_id: &str) -> JsonArray {
    let client = Client::builder().cookie_store(true).build().unwrap();
    let raw_html = fetch_raw_html(&client, video_id).unwrap();
    let api_key = extract_api_key_from_html(raw_html.as_str()).unwrap();
    let mut continuation = extract_continuation_from_html(raw_html.as_str()).unwrap();
    let duration = extract_duration_from_html(raw_html.as_str()).unwrap() as f64;

    let mut live_chats = Vec::with_capacity((duration / 200.0) as usize);
    loop {
        let api_response = fetch_live_chats_once(&client, &continuation, &api_key).unwrap();
        if let Some((c, mut a)) = api_response {
            continuation = c;
            if let Some(timestamp) = extract_timestamp_from_json(a.last().unwrap()) {
                print!(
                    "\rProgress: {:.2}%; Total live chats: {}",
                    timestamp.parse::<f64>().unwrap() / duration * 100.0,
                    live_chats.len() + a.len()
                );
                std::io::stdout().flush().unwrap();
            }

            live_chats.append(&mut a);
        } else {
            println!();
            break;
        }
    }

    live_chats.shrink_to_fit();
    live_chats
}

fn main() {
    print!("Live streaming id: ");
    std::io::stdout().flush().unwrap();
    let mut video_id = String::new();
    std::io::stdin().read_line(&mut video_id).unwrap();
    video_id = video_id.trim_end().to_owned();
    println!("Set target: {}", video_id);

    let live_chats = fetch_all_live_chats(video_id.as_str());
    println!(
        "Fetched all live chats successfully!: {} live chats",
        live_chats.len()
    );

    let dist = "./".to_owned() + video_id.as_str() + "-chats.json";
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&dist)
        .unwrap();
    serde_json::to_writer_pretty(&file, &live_chats).unwrap();

    println!("Live chats was saved at: {}", dist);
}
