use regex::Regex;
use reqwest::blocking::Client;
use reqwest::header;

const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/84.0.4147.125 Safari/537.36,gzip(gfe) ";

pub struct VideoDetails {
    pub video_id: String,
    pub api_key: String,
    pub continuation: String,
    pub duration: i32,
}

impl VideoDetails {
    pub fn get(video_id: &str) -> reqwest::Result<Self> {
        let html = Self::fetch_raw_html(video_id).unwrap();
        Ok(Self {
            video_id: video_id.to_owned(),
            api_key: Self::extract_api_key_from_html(&html).unwrap(),
            continuation: Self::extract_continuation_from_html(&html).unwrap(),
            duration: Self::extract_duration_from_html(&html).unwrap(),
        })
    }

    fn fetch_raw_html(video_id: &str) -> reqwest::Result<String> {
        let client = Client::new();
        let res = client
            .get(&Self::build_video_url(video_id))
            .header(header::USER_AGENT, self::USER_AGENT)
            .send()?
            .text()?;
        Ok(res)
    }

    fn build_video_url(video_id: &str) -> String {
        format!("https://www.youtube.com/watch?v={}", video_id)
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
}
