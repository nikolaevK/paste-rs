use gpui::{AssetSource, Result, SharedString};
use std::borrow::Cow;

pub struct Assets;

macro_rules! icons {
    ($($name:literal),* $(,)?) => {
        fn lookup(path: &str) -> Option<&'static [u8]> {
            match path {
                $(concat!("icons/", $name, ".svg") => Some(include_bytes!(concat!("../assets/icons/", $name, ".svg"))),)*
                _ => None,
            }
        }
        pub const ICON_NAMES: &[&str] = &[$($name),*];
    };
}

icons!("search", "plus", "gear", "clipboard", "file", "link", "close", "check", "pin", "pause", "image", "menubar");

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(lookup(path).map(Cow::Borrowed))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        if path == "icons" || path == "icons/" {
            Ok(ICON_NAMES.iter().map(|n| SharedString::from(format!("icons/{n}.svg"))).collect())
        } else {
            Ok(Vec::new())
        }
    }
}

pub fn icon(name: &str) -> SharedString {
    SharedString::from(format!("icons/{name}.svg"))
}

pub fn menubar_icon_rgba(px: u32) -> Option<(Vec<u8>, u32, u32)> {
    let svg = lookup("icons/menubar.svg")?;
    let tree = resvg::usvg::Tree::from_data(svg, &resvg::usvg::Options::default()).ok()?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(px, px)?;
    let scale = px as f32 / tree.size().width();
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    // tiny-skia stores premultiplied RGBA; un-premultiply for NSImage.
    let mut data = pixmap.take();
    for chunk in data.chunks_exact_mut(4) {
        let a = chunk[3] as f32 / 255.0;
        if a > 0.0 && a < 1.0 {
            for c in chunk.iter_mut().take(3) {
                *c = ((*c as f32 / a).min(255.0)) as u8;
            }
        }
    }
    Some((data, px, px))
}

/// Renders the app icon SVG to a PNG of the given size (used by packaging/build-app.sh).
pub fn render_app_icon(out: &str, size: u32) -> anyhow::Result<()> {
    let svg = include_bytes!("../assets/AppIcon.svg");
    let tree = resvg::usvg::Tree::from_data(svg, &resvg::usvg::Options::default())?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size).ok_or_else(|| anyhow::anyhow!("pixmap"))?;
    let scale = size as f32 / tree.size().width();
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    pixmap.save_png(out)?;
    Ok(())
}
