//! Direct NSWindow control for gpui windows (show/hide without recreating them).
use gpui::Window;
use objc2::rc::Retained;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSView, NSWindow, NSWindowCollectionBehavior, NSWindowStyleMask};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

pub fn ns_window(window: &Window) -> Option<Retained<NSWindow>> {
    let handle = HasWindowHandle::window_handle(window).ok()?;
    match handle.as_raw() {
        RawWindowHandle::AppKit(h) => {
            let view: &NSView = unsafe { h.ns_view.cast::<NSView>().as_ref() };
            view.window()
        }
        _ => None,
    }
}

/// Places the window at a Cocoa-space frame and brings it to front as key window
/// without activating the application.
pub fn show_at(win: &NSWindow, x: f64, y: f64, w: f64, h: f64) {
    let rect = NSRect::new(NSPoint::new(x, y), NSSize::new(w, h));
    win.setFrame_display(rect, true);
    win.makeKeyAndOrderFront(None);
}

pub fn hide(win: &NSWindow) {
    win.orderOut(None);
}

pub fn debug_frame(win: &NSWindow) -> String {
    let f = win.frame();
    format!(
        "frame=({}, {}, {}x{}) visible={} key={}",
        f.origin.x, f.origin.y, f.size.width, f.size.height, win.isVisible(), win.isKeyWindow()
    )
}

pub fn configure_shelf(win: &NSWindow) {
    win.setStyleMask(NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel);
    win.setHasShadow(false);
    win.setHidesOnDeactivate(false);
    win.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );
}

/// Captures the window's own contents to a PNG (used by the `shot` control command).
pub fn capture_png(win: &NSWindow, path: &std::path::Path) -> anyhow::Result<()> {
    use core_graphics::display::{
        kCGWindowImageBestResolution, kCGWindowImageBoundsIgnoreFraming, kCGWindowListOptionIncludingWindow, CGDisplay,
    };
    use core_graphics::geometry::{CGPoint, CGRect, CGSize};
    let null_rect = CGRect::new(&CGPoint::new(f64::INFINITY, f64::INFINITY), &CGSize::new(0.0, 0.0));
    let image = CGDisplay::screenshot(
        null_rect,
        kCGWindowListOptionIncludingWindow,
        win.windowNumber() as u32,
        kCGWindowImageBoundsIgnoreFraming | kCGWindowImageBestResolution,
    )
    .ok_or_else(|| anyhow::anyhow!("CGWindowListCreateImage returned null"))?;
    let (w, h) = (image.width() as usize, image.height() as usize);
    let bpr = image.bytes_per_row() as usize;
    let data = image.data();
    let bytes = data.bytes();
    let mut rgba = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        let row = &bytes[y * bpr..y * bpr + w * 4];
        for px in row.chunks_exact(4) {
            let a = px[3] as f32 / 255.0;
            let un = |c: u8| if a > 0.0 { ((c as f32 / a).min(255.0)) as u8 } else { 0 };
            rgba.extend_from_slice(&[un(px[2]), un(px[1]), un(px[0]), px[3]]);
        }
    }
    let img = image::RgbaImage::from_raw(w as u32, h as u32, rgba).ok_or_else(|| anyhow::anyhow!("bad image buffer"))?;
    img.save_with_format(path, image::ImageFormat::Png)?;
    Ok(())
}

pub fn set_accessory_policy() {
    if let Some(mtm) = MainThreadMarker::new() {
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    }
}

pub fn activate_self() {
    if let Some(mtm) = MainThreadMarker::new() {
        let app = NSApplication::sharedApplication(mtm);
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);
    }
}
