//! Background clipboard monitor: polls NSPasteboard's change count and turns new content into items.
use crate::db::Db;
use crate::mac::pasteboard::{self, Content, Snapshot};
use crate::mac::workspace::{self, FrontApp};
use crate::model::{ItemKind, NewItem};
use crate::settings::{data_dir, Settings};
use crate::util;
use futures::channel::mpsc::UnboundedSender;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

pub enum MonitorEvent {
    /// A brand new item was stored.
    New(crate::model::ClipItem),
    /// An existing history item was copied again and moved to the top.
    Touched { id: i64, created_at: i64 },
    /// Link metadata arrived for an item.
    LinkMeta { id: i64, title: Option<String>, favicon: Option<PathBuf> },
}

pub struct MonitorHandle {
    pub paused: Arc<AtomicBool>,
    /// Change count of the last write Paste itself made; the monitor skips it.
    pub own_change: Arc<AtomicIsize>,
}

const POLL: Duration = Duration::from_millis(150);
const MAX_TEXT_BYTES: usize = 10 * 1024 * 1024;

pub fn spawn(db: Db, settings: Arc<RwLock<Settings>>, tx: UnboundedSender<MonitorEvent>) -> MonitorHandle {
    let paused = Arc::new(AtomicBool::new(false));
    let own_change = Arc::new(AtomicIsize::new(-1));
    let handle = MonitorHandle { paused: paused.clone(), own_change: own_change.clone() };

    std::thread::Builder::new()
        .name("clipboard-monitor".into())
        .spawn(move || {
            let mut last = pasteboard::change_count();
            loop {
                std::thread::sleep(POLL);
                let count = pasteboard::change_count();
                if count == last {
                    continue;
                }
                last = count;
                if paused.load(Ordering::Relaxed) {
                    continue;
                }
                if own_change.load(Ordering::Relaxed) == count {
                    continue;
                }
                let front = workspace::frontmost_app();
                let snapshot = pasteboard::read();
                let settings_snapshot = settings.read().unwrap().clone();
                if let Err(e) = handle_snapshot(&db, &settings_snapshot, snapshot, front, &tx) {
                    log::warn!("clipboard capture failed: {e}");
                }
            }
        })
        .expect("spawn clipboard monitor");
    handle
}

fn handle_snapshot(
    db: &Db,
    settings: &Settings,
    snapshot: Snapshot,
    front: Option<FrontApp>,
    tx: &UnboundedSender<MonitorEvent>,
) -> anyhow::Result<()> {
    if snapshot.self_marker.is_some() {
        return Ok(());
    }
    if snapshot.transient {
        return Ok(());
    }
    if snapshot.concealed && settings.ignore_concealed {
        log::debug!("skipping concealed clipboard content");
        return Ok(());
    }
    let (app_bundle, app_name) = match &front {
        Some(app) => (app.bundle_id.clone(), app.name.clone()),
        None => (String::new(), String::new()),
    };
    if !app_bundle.is_empty() && settings.is_excluded(&app_bundle) {
        log::debug!("skipping excluded app {app_bundle}");
        return Ok(());
    }
    let Some(content) = snapshot.content else { return Ok(()) };
    if let Some(app) = &front {
        cache_app_icon(app);
    }

    let new_item = match content {
        Content::Text { text, rtf, html } => {
            if text.trim().is_empty() || text.len() > MAX_TEXT_BYTES {
                return Ok(());
            }
            let hash = util::hash_bytes(text.as_bytes());
            let char_count = text.chars().count() as i64;
            let (kind, color) = if util::looks_like_url(&text) {
                (ItemKind::Link, None)
            } else if let Some(c) = util::parse_color(&text) {
                (ItemKind::Color, Some(c))
            } else if rtf.is_some() || html.is_some() {
                (ItemKind::RichText, None)
            } else {
                (ItemKind::Text, None)
            };
            NewItem {
                kind,
                byte_size: text.len() as i64,
                text,
                rtf,
                html,
                color,
                image_path: None,
                thumb_path: None,
                files: vec![],
                app_bundle,
                app_name,
                char_count,
                width: 0,
                height: 0,
                hash,
            }
        }
        Content::Image { data, is_png } => {
            let hash = util::hash_bytes(&data);
            if let Some(id) = db.find_recent_by_hash(&hash)? {
                let created_at = db.touch(id)?;
                let _ = tx.unbounded_send(MonitorEvent::Touched { id, created_at });
                return Ok(());
            }
            let stored = store_image(&data, is_png, &hash)?;
            NewItem {
                kind: ItemKind::Image,
                text: format!("Image {}×{}", stored.width, stored.height),
                rtf: None,
                html: None,
                color: None,
                image_path: Some(stored.image_path),
                thumb_path: Some(stored.thumb_path),
                files: vec![],
                app_bundle,
                app_name,
                char_count: 0,
                width: stored.width as i64,
                height: stored.height as i64,
                byte_size: stored.byte_size as i64,
                hash,
            }
        }
        Content::Files(files) => {
            let joined = files.iter().map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>().join("\n");
            let hash = util::hash_bytes(joined.as_bytes());
            let size: u64 = files.iter().filter_map(|p| std::fs::metadata(p).ok()).map(|m| m.len()).sum();
            let names = files.iter().map(|p| pasteboard::file_name(p)).collect::<Vec<_>>().join("\n");
            let thumb_path = file_thumbnail(&files, &hash);
            NewItem {
                kind: ItemKind::File,
                text: names,
                rtf: None,
                html: None,
                color: None,
                image_path: None,
                thumb_path,
                char_count: files.len() as i64,
                files,
                app_bundle,
                app_name,
                width: 0,
                height: 0,
                byte_size: size as i64,
                hash,
            }
        }
    };

    if let Some(id) = db.find_recent_by_hash(&new_item.hash)? {
        let created_at = db.touch(id)?;
        let _ = tx.unbounded_send(MonitorEvent::Touched { id, created_at });
        return Ok(());
    }

    let item = db.insert(&new_item)?;
    let is_link = item.kind == ItemKind::Link;
    let id = item.id;
    let url = item.preview.trim().to_string();
    let _ = tx.unbounded_send(MonitorEvent::New(item));

    if is_link && settings.fetch_link_previews {
        let db = db.clone();
        let tx = tx.clone();
        std::thread::Builder::new()
            .name("link-preview".into())
            .spawn(move || {
                if let Some(meta) = crate::link_preview::fetch(&url) {
                    let _ = db.update_link_meta(id, meta.title.as_deref(), meta.favicon.as_ref());
                    let _ = tx.unbounded_send(MonitorEvent::LinkMeta { id, title: meta.title, favicon: meta.favicon });
                }
            })
            .ok();
    }
    Ok(())
}

pub struct StoredImage {
    pub image_path: PathBuf,
    pub thumb_path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub byte_size: usize,
}

const THUMB_MAX: u32 = 480;

pub fn store_image(data: &[u8], is_png: bool, hash: &str) -> anyhow::Result<StoredImage> {
    let dir = data_dir();
    let image_path = dir.join("images").join(format!("{hash}.png"));
    let thumb_path = dir.join("thumbs").join(format!("{hash}.png"));
    let img = image::load_from_memory(data)?;
    let (width, height) = (img.width(), img.height());
    if is_png {
        std::fs::write(&image_path, data)?;
    } else {
        img.save_with_format(&image_path, image::ImageFormat::Png)?;
    }
    let byte_size = std::fs::metadata(&image_path).map(|m| m.len() as usize).unwrap_or(data.len());
    let thumb = if width > THUMB_MAX || height > THUMB_MAX {
        img.thumbnail(THUMB_MAX, THUMB_MAX)
    } else {
        img
    };
    thumb.save_with_format(&thumb_path, image::ImageFormat::Png)?;
    Ok(StoredImage { image_path, thumb_path, width, height, byte_size })
}

/// Stores the frontmost app's icon once so cards can show it.
fn cache_app_icon(app: &FrontApp) {
    if app.bundle_id.is_empty() {
        return;
    }
    let path = crate::core::icon_path_for_bundle(&app.bundle_id);
    if path.exists() {
        return;
    }
    let Some(app_path) = &app.path else { return };
    if let Some(png) = workspace::icon_png_for_path(app_path, 128) {
        let _ = std::fs::write(path, png);
    }
}

/// For file items: an image thumbnail when the first file is an image, otherwise the Finder icon.
fn file_thumbnail(files: &[PathBuf], hash: &str) -> Option<PathBuf> {
    let first = files.first()?;
    let thumb_path = data_dir().join("thumbs").join(format!("file-{hash}.png"));
    if thumb_path.exists() {
        return Some(thumb_path);
    }
    let ext = first.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    if files.len() == 1 && matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "tiff" | "tif") {
        if let Ok(meta) = std::fs::metadata(first) {
            if meta.len() < 40 * 1024 * 1024 {
                if let Ok(img) = image::open(first) {
                    let t = img.thumbnail(THUMB_MAX, THUMB_MAX);
                    if t.save_with_format(&thumb_path, image::ImageFormat::Png).is_ok() {
                        return Some(thumb_path);
                    }
                }
            }
        }
    }
    let png = workspace::icon_png_for_path(first, 128)?;
    std::fs::write(&thumb_path, png).ok()?;
    Some(thumb_path)
}
