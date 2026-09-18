//! Application-wide state shared by all windows (a gpui Global).
use crate::clipboard::MonitorHandle;
use crate::db::Db;
use crate::hotkey::Hotkeys;
use crate::mac::workspace::FrontApp;
use crate::model::{ClipItem, ItemKind, Pinboard, PINBOARD_COLORS};
use crate::settings::{data_dir, Settings};
use crate::tray::Tray;
use crate::ui::preview::Preview;
use crate::ui::prefs::Prefs;
use crate::ui::shelf::Shelf;
use anyhow::Result;
use gpui::{hsla, Global, Hsla, WindowHandle};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

#[derive(Clone, Debug)]
pub struct AppIcon {
    pub path: Option<PathBuf>,
    pub tint: Hsla,
}

pub struct Core {
    pub db: Db,
    pub settings: Arc<RwLock<Settings>>,
    pub history: Vec<Arc<ClipItem>>,
    pub pinboards: Vec<Pinboard>,
    pub pinned: HashMap<i64, Vec<Arc<ClipItem>>>,
    icons: HashMap<String, AppIcon>,
    pub monitor: MonitorHandle,
    pub front_app: Option<FrontApp>,
    pub shelf: Option<WindowHandle<Shelf>>,
    pub prefs: Option<WindowHandle<Prefs>>,
    pub preview: Option<WindowHandle<Preview>>,
    pub hotkeys: Option<Hotkeys>,
    pub tray: Option<Tray>,
    pub ax_prompted: bool,
    pub own_pid: i32,
}

impl Global for Core {}

impl Core {
    pub fn load(db: Db, settings: Arc<RwLock<Settings>>, monitor: MonitorHandle) -> Result<Core> {
        let history = db.history()?.into_iter().map(Arc::new).collect();
        let pinboards = db.pinboards()?;
        let mut pinned = HashMap::new();
        for pb in &pinboards {
            pinned.insert(pb.id, db.pinboard_items(pb.id)?.into_iter().map(Arc::new).collect());
        }
        Ok(Core {
            db,
            settings,
            history,
            pinboards,
            pinned,
            icons: HashMap::new(),
            monitor,
            front_app: None,
            shelf: None,
            prefs: None,
            preview: None,
            hotkeys: None,
            tray: None,
            ax_prompted: false,
            own_pid: std::process::id() as i32,
        })
    }

    pub fn settings(&self) -> Settings {
        self.settings.read().unwrap().clone()
    }

    pub fn update_settings(&self, f: impl FnOnce(&mut Settings)) -> Settings {
        let mut guard = self.settings.write().unwrap();
        f(&mut guard);
        guard.save();
        guard.clone()
    }

    pub fn paused(&self) -> bool {
        self.monitor.paused.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn set_paused(&self, paused: bool) {
        self.monitor.paused.store(paused, std::sync::atomic::Ordering::Relaxed);
        if let Some(t) = &self.tray {
            t.set_paused(paused);
        }
    }

    pub fn icon_for(&mut self, bundle: &str) -> AppIcon {
        if let Some(icon) = self.icons.get(bundle) {
            return icon.clone();
        }
        let icon = load_icon(bundle);
        self.icons.insert(bundle.to_string(), icon.clone());
        icon
    }

    pub fn add_item(&mut self, item: ClipItem) {
        self.history.insert(0, Arc::new(item));
    }

    pub fn touch_item(&mut self, id: i64, created_at: i64) {
        if let Some(pos) = self.history.iter().position(|i| i.id == id) {
            let mut item = (*self.history.remove(pos)).clone();
            item.created_at = created_at;
            self.history.insert(0, Arc::new(item));
        }
    }

    pub fn apply_link_meta(&mut self, id: i64, title: Option<String>, favicon: Option<PathBuf>) {
        let hash = self.history.iter().find(|i| i.id == id).map(|i| i.hash.clone());
        let update = |item: &mut Arc<ClipItem>| {
            let mut copy = (**item).clone();
            if title.is_some() {
                copy.title = title.clone();
            }
            if favicon.is_some() {
                copy.favicon_path = favicon.clone();
            }
            *item = Arc::new(copy);
        };
        for item in self.history.iter_mut() {
            if item.id == id || (hash.is_some() && item.hash == *hash.as_ref().unwrap() && item.kind == ItemKind::Link) {
                update(item);
            }
        }
        for list in self.pinned.values_mut() {
            for item in list.iter_mut() {
                if hash.is_some() && item.hash == *hash.as_ref().unwrap() && item.kind == ItemKind::Link {
                    update(item);
                }
            }
        }
    }

    pub fn delete_item(&mut self, id: i64) -> Result<()> {
        self.db.delete(id)?;
        self.history.retain(|i| i.id != id);
        for list in self.pinned.values_mut() {
            list.retain(|i| i.id != id);
        }
        Ok(())
    }

    pub fn pin_item(&mut self, id: i64, pinboard_id: i64) -> Result<()> {
        let copy = self.db.copy_to_pinboard(id, pinboard_id)?;
        self.pinned.entry(pinboard_id).or_default().insert(0, Arc::new(copy));
        Ok(())
    }

    pub fn create_pinboard(&mut self, name: &str) -> Result<Pinboard> {
        let color = PINBOARD_COLORS[self.pinboards.len() % PINBOARD_COLORS.len()];
        let pb = self.db.create_pinboard(name, color)?;
        self.pinboards.push(pb.clone());
        self.pinned.insert(pb.id, Vec::new());
        Ok(pb)
    }

    pub fn rename_pinboard(&mut self, id: i64, name: &str) -> Result<()> {
        self.db.rename_pinboard(id, name)?;
        if let Some(pb) = self.pinboards.iter_mut().find(|p| p.id == id) {
            pb.name = name.to_string();
        }
        Ok(())
    }

    pub fn recolor_pinboard(&mut self, id: i64, color: &str) -> Result<()> {
        self.db.recolor_pinboard(id, color)?;
        if let Some(pb) = self.pinboards.iter_mut().find(|p| p.id == id) {
            pb.color = color.to_string();
        }
        Ok(())
    }

    pub fn delete_pinboard(&mut self, id: i64) -> Result<()> {
        self.db.delete_pinboard(id)?;
        self.pinboards.retain(|p| p.id != id);
        self.pinned.remove(&id);
        Ok(())
    }

    pub fn clear_history(&mut self) -> Result<()> {
        self.db.clear_history()?;
        self.history.clear();
        Ok(())
    }

    pub fn purge_expired(&mut self) {
        let retention = self.settings.read().unwrap().retention;
        if let Some(ms) = retention.millis() {
            let cutoff = crate::db::now_ms() - ms;
            match self.db.purge_older_than(cutoff) {
                Ok(n) if n > 0 => {
                    log::info!("purged {n} expired items");
                    self.history.retain(|i| i.created_at >= cutoff);
                }
                Err(e) => log::warn!("purge failed: {e}"),
                _ => {}
            }
        }
    }

    pub fn hotkey_display(&self) -> String {
        crate::hotkey::display(&self.settings.read().unwrap().hotkey)
    }
}

pub fn icon_path_for_bundle(bundle: &str) -> PathBuf {
    data_dir().join("icons").join(format!("{}.png", bundle.replace('/', "_")))
}

fn load_icon(bundle: &str) -> AppIcon {
    if bundle.is_empty() {
        return AppIcon { path: None, tint: hsla(0.0, 0.0, 0.55, 1.0) };
    }
    let path = icon_path_for_bundle(bundle);
    if path.exists() {
        let tint = dominant_tint(&path).unwrap_or_else(|| fallback_tint(bundle));
        AppIcon { path: Some(path), tint }
    } else {
        AppIcon { path: None, tint: fallback_tint(bundle) }
    }
}

fn fallback_tint(bundle: &str) -> Hsla {
    let mut h: u32 = 2166136261;
    for b in bundle.bytes() {
        h = (h ^ b as u32).wrapping_mul(16777619);
    }
    hsla((h % 360) as f32 / 360.0, 0.62, 0.5, 1.0)
}

/// Picks a vivid representative color from an app icon for the card header.
fn dominant_tint(path: &PathBuf) -> Option<Hsla> {
    let img = image::open(path).ok()?.thumbnail(32, 32).to_rgba8();
    let mut buckets = [(0.0f32, 0.0f32, 0.0f32, 0.0f32); 24];
    let mut total_opaque = 0.0f32;
    let mut avg = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    for px in img.pixels() {
        let a = px[3] as f32 / 255.0;
        if a < 0.5 {
            continue;
        }
        let (r, g, b) = (px[0] as f32 / 255.0, px[1] as f32 / 255.0, px[2] as f32 / 255.0);
        let (h, s, l) = rgb_to_hsl(r, g, b);
        total_opaque += 1.0;
        avg.0 += r;
        avg.1 += g;
        avg.2 += b;
        avg.3 += 1.0;
        if s < 0.25 || l < 0.12 || l > 0.92 {
            continue;
        }
        let bucket = ((h * 24.0) as usize).min(23);
        let weight = s * (1.0 - (l - 0.5).abs());
        buckets[bucket].0 += h * weight;
        buckets[bucket].1 += s * weight;
        buckets[bucket].2 += l * weight;
        buckets[bucket].3 += weight;
    }
    if total_opaque == 0.0 {
        return None;
    }
    let (best_i, best) = buckets
        .iter()
        .enumerate()
        .max_by(|a, b| a.1 .3.partial_cmp(&b.1 .3).unwrap())?;
    let colorful_share: f32 = buckets.iter().map(|b| b.3).sum::<f32>() / total_opaque;
    if best.3 <= 0.0 || colorful_share < 0.04 {
        // Mostly monochrome icon (Finder-like grays, black terminals): use a muted neutral.
        let l = (avg.2 + avg.1 + avg.0) / (3.0 * avg.3);
        let _ = best_i;
        return Some(hsla(0.6, 0.06, (l * 0.5 + 0.28).clamp(0.3, 0.55), 1.0));
    }
    let h = best.0 / best.3;
    let s = (best.1 / best.3).clamp(0.5, 0.9);
    let l = (best.2 / best.3).clamp(0.42, 0.6);
    Some(hsla(h, s, l, 1.0))
}

fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if (max - min).abs() < f32::EPSILON {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h / 6.0, s, l)
}
