//! Fetches a page title and favicon for link items (best effort, off the main thread).
use crate::settings::data_dir;
use crate::util;
use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;

pub struct LinkMeta {
    pub title: Option<String>,
    pub favicon: Option<PathBuf>,
}

const MAX_HTML: u64 = 256 * 1024;
const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15 Paste-rs/0.1";

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(8)))
        .max_redirects(5)
        .user_agent(UA)
        .build()
        .into()
}

pub fn fetch(url: &str) -> Option<LinkMeta> {
    let parsed = url::Url::parse(url).ok()?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return None;
    }
    let agent = agent();
    let title = fetch_title(&agent, url);
    let favicon = parsed.host_str().and_then(|host| fetch_favicon(&agent, &parsed, host));
    if title.is_none() && favicon.is_none() {
        return None;
    }
    Some(LinkMeta { title, favicon })
}

fn fetch_title(agent: &ureq::Agent, url: &str) -> Option<String> {
    let resp = agent.get(url).header("Accept", "text/html,*/*;q=0.8").call().ok()?;
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !ct.is_empty() && !ct.contains("html") && !ct.contains("xml") {
        return None;
    }
    let mut body = String::new();
    resp.into_body()
        .into_reader()
        .take(MAX_HTML)
        .read_to_string(&mut body)
        .ok()
        .or_else(|| Some(0))?;
    let re = regex::Regex::new(r"(?is)<title[^>]*>(.*?)</title>").ok()?;
    let og = regex::Regex::new(r#"(?is)<meta[^>]+property=["']og:title["'][^>]+content=["']([^"']+)["']"#).ok()?;
    let raw = og
        .captures(&body)
        .and_then(|c| c.get(1))
        .or_else(|| re.captures(&body).and_then(|c| c.get(1)))
        .map(|m| m.as_str().to_string())?;
    let title = util::decode_html_entities(&raw).split_whitespace().collect::<Vec<_>>().join(" ");
    if title.is_empty() {
        None
    } else {
        Some(title.chars().take(200).collect())
    }
}

fn fetch_favicon(agent: &ureq::Agent, url: &url::Url, host: &str) -> Option<PathBuf> {
    let path = data_dir().join("favicons").join(format!("{}.png", host.replace(':', "_")));
    if path.exists() {
        return Some(path);
    }
    let candidates = [
        format!("{}://{}/favicon.ico", url.scheme(), host),
        format!("{}://{}/apple-touch-icon.png", url.scheme(), host),
    ];
    for candidate in candidates {
        let Ok(resp) = agent.get(&candidate).call() else { continue };
        if resp.status().as_u16() != 200 {
            continue;
        }
        let mut bytes = Vec::new();
        if resp.into_body().into_reader().take(2 * 1024 * 1024).read_to_end(&mut bytes).is_err() {
            continue;
        }
        let Ok(img) = image::load_from_memory(&bytes) else { continue };
        let img = if img.width() > 64 { img.thumbnail(64, 64) } else { img };
        if img.save_with_format(&path, image::ImageFormat::Png).is_ok() {
            return Some(path);
        }
    }
    None
}
