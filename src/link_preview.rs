//! Fetches a page title and favicon for link items (best effort, off the main thread).
use crate::settings::data_dir;
use crate::util;
use std::io::Read;
use std::path::PathBuf;
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::OnceLock;
use std::time::Duration;

type Job = (String, Box<dyn FnOnce(LinkMeta) + Send>);

static QUEUE: OnceLock<SyncSender<Job>> = OnceLock::new();
const QUEUE_CAPACITY: usize = 64;

/// Queues a link for metadata fetching on the single background worker.
/// Returns false when the queue is full (the callback will not run).
pub fn enqueue(url: String, done: impl FnOnce(LinkMeta) + Send + 'static) -> bool {
    let tx = QUEUE.get_or_init(|| {
        let (tx, rx) = sync_channel::<Job>(QUEUE_CAPACITY);
        std::thread::Builder::new()
            .name("link-preview".into())
            .spawn(move || {
                while let Ok((url, done)) = rx.recv() {
                    done(fetch(&url));
                }
            })
            .expect("spawn link preview worker");
        tx
    });
    tx.try_send((url, Box::new(done))).is_ok()
}

enum Page {
    Html(String),
    NotHtml,
    Failed,
}

#[derive(Default)]
pub struct LinkMeta {
    pub title: Option<String>,
    pub favicon: Option<PathBuf>,
    pub image: Option<PathBuf>,
}

const MAX_IMAGE_BYTES: u64 = 8 * 1024 * 1024;
const LINK_IMAGE_MAX: u32 = 800;

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

/// Fetches title, favicon and preview image for a link. Always returns a value (possibly empty)
/// so callers can clear their "loading" state.
pub fn fetch(url: &str) -> LinkMeta {
    let Ok(parsed) = url::Url::parse(url) else { return LinkMeta::default() };
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return LinkMeta::default();
    }
    let agent = agent();
    let (title, image) = match fetch_page(&agent, url) {
        Page::Html(body) => (
            extract_title(&body),
            extract_image_url(&body, &parsed).and_then(|img_url| fetch_image(&agent, &img_url, url)),
        ),
        // A direct link to an image file: the image itself is the preview.
        Page::NotHtml => (None, fetch_image(&agent, url, url)),
        Page::Failed => (None, None),
    };
    let favicon = parsed.host_str().and_then(|host| fetch_favicon(&agent, &parsed, host));
    LinkMeta { title, favicon, image }
}

/// Downloads the HTML of a page (up to MAX_HTML bytes).
fn fetch_page(agent: &ureq::Agent, url: &str) -> Page {
    let Ok(resp) = agent.get(url).header("Accept", "text/html,*/*;q=0.8").call() else {
        return Page::Failed;
    };
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !ct.is_empty() && !ct.contains("html") && !ct.contains("xml") {
        return Page::NotHtml;
    }
    let mut bytes = Vec::new();
    if resp.into_body().into_reader().take(MAX_HTML).read_to_end(&mut bytes).is_err() {
        return Page::Failed;
    }
    Page::Html(String::from_utf8_lossy(&bytes).into_owned())
}

fn extract_title(body: &str) -> Option<String> {
    let re = regex::Regex::new(r"(?is)<title[^>]*>(.*?)</title>").ok()?;
    let raw = meta_content(body, "og:title")
        .or_else(|| meta_content(body, "twitter:title"))
        .or_else(|| re.captures(body).and_then(|c| c.get(1)).map(|m| m.as_str().to_string()))?;
    let title = util::decode_html_entities(&raw).split_whitespace().collect::<Vec<_>>().join(" ");
    if title.is_empty() {
        None
    } else {
        Some(title.chars().take(200).collect())
    }
}

/// Finds `<meta property|name="key" content="...">` in either attribute order.
fn meta_content(body: &str, key: &str) -> Option<String> {
    let key = regex::escape(key);
    let a = regex::Regex::new(&format!(
        r#"(?is)<meta[^>]+(?:property|name)\s*=\s*["']{key}["'][^>]*?content\s*=\s*["']([^"']+)["']"#
    ))
    .ok()?;
    let b = regex::Regex::new(&format!(
        r#"(?is)<meta[^>]+content\s*=\s*["']([^"']+)["'][^>]*?(?:property|name)\s*=\s*["']{key}["']"#
    ))
    .ok()?;
    a.captures(body)
        .or_else(|| b.captures(body))
        .and_then(|c| c.get(1))
        .map(|m| util::decode_html_entities(m.as_str()))
}

fn extract_image_url(body: &str, base: &url::Url) -> Option<String> {
    let candidate = meta_content(body, "og:image")
        .or_else(|| meta_content(body, "og:image:url"))
        .or_else(|| meta_content(body, "twitter:image"))
        .or_else(|| meta_content(body, "twitter:image:src"))
        .or_else(|| {
            let re = regex::Regex::new(r#"(?is)<link[^>]+rel\s*=\s*["']image_src["'][^>]*?href\s*=\s*["']([^"']+)["']"#).ok()?;
            re.captures(body).and_then(|c| c.get(1)).map(|m| m.as_str().to_string())
        })?;
    let resolved = base.join(candidate.trim()).ok()?;
    if resolved.scheme() != "http" && resolved.scheme() != "https" {
        return None;
    }
    Some(resolved.to_string())
}

pub fn link_image_path(page_url: &str) -> PathBuf {
    data_dir().join("link-images").join(format!("{}.png", util::hash_bytes(page_url.as_bytes())))
}

/// Downloads an image, scales it to LINK_IMAGE_MAX and stores it as PNG keyed by the page URL.
fn fetch_image(agent: &ureq::Agent, image_url: &str, page_url: &str) -> Option<PathBuf> {
    let path = link_image_path(page_url);
    if path.exists() {
        return Some(path);
    }
    let resp = agent.get(image_url).header("Accept", "image/*,*/*;q=0.5").call().ok()?;
    if resp.status().as_u16() != 200 {
        return None;
    }
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !ct.is_empty() && !ct.starts_with("image/") && !ct.contains("octet-stream") {
        return None;
    }
    let mut bytes = Vec::new();
    resp.into_body().into_reader().take(MAX_IMAGE_BYTES).read_to_end(&mut bytes).ok()?;
    let img = image::load_from_memory(&bytes).ok()?;
    if img.width() < 32 || img.height() < 32 {
        return None;
    }
    let img = if img.width() > LINK_IMAGE_MAX || img.height() > LINK_IMAGE_MAX { img.thumbnail(LINK_IMAGE_MAX, LINK_IMAGE_MAX) } else { img };
    img.save_with_format(&path, image::ImageFormat::Png).ok()?;
    Some(path)
}

fn fetch_favicon(agent: &ureq::Agent, url: &url::Url, host: &str) -> Option<PathBuf> {
    static MISSES: OnceLock<std::sync::Mutex<std::collections::HashSet<String>>> = OnceLock::new();
    let misses = MISSES.get_or_init(Default::default);
    let path = data_dir().join("favicons").join(format!("{}.png", host.replace(':', "_")));
    if path.exists() {
        return Some(path);
    }
    if misses.lock().map(|m| m.contains(host)).unwrap_or(false) {
        return None;
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
    if let Ok(mut m) = misses.lock() {
        m.insert(host.to_string());
    }
    None
}
