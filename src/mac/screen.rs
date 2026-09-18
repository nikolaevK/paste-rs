//! Screen geometry helpers, in gpui's coordinate space (origin top-left of the primary display).
use objc2::MainThreadMarker;
use objc2_app_kit::{NSEvent, NSScreen};

#[derive(Clone, Copy, Debug)]
pub struct ScreenRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// Cocoa frame origin (bottom-left based) for direct NSWindow placement.
    pub cocoa_x: f64,
    pub cocoa_y: f64,
}

impl ScreenRect {
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }
}

fn primary_height(mtm: MainThreadMarker) -> f64 {
    NSScreen::screens(mtm)
        .iter()
        .next()
        .map(|s| s.frame().size.height)
        .unwrap_or(0.0)
}

pub fn screens() -> Vec<ScreenRect> {
    let Some(mtm) = MainThreadMarker::new() else { return Vec::new() };
    let ph = primary_height(mtm);
    NSScreen::screens(mtm)
        .iter()
        .map(|s| {
            let f = s.frame();
            ScreenRect {
                x: f.origin.x,
                y: ph - (f.origin.y + f.size.height),
                width: f.size.width,
                height: f.size.height,
                cocoa_x: f.origin.x,
                cocoa_y: f.origin.y,
            }
        })
        .collect()
}

/// The screen currently containing the mouse cursor (falls back to the primary screen).
pub fn screen_under_mouse() -> Option<ScreenRect> {
    let mtm = MainThreadMarker::new()?;
    let ph = primary_height(mtm);
    let loc = NSEvent::mouseLocation();
    let (mx, my) = (loc.x, ph - loc.y);
    let all = screens();
    all.iter().copied().find(|s| s.contains(mx, my)).or_else(|| all.first().copied())
}
