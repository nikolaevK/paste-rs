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

/// Synthesizes a real key press (down + up) through AppKit's event path, for testing.
/// `key` is a single character or one of: left, right, up, down, escape, enter, space, tab, backspace.
pub fn synthesize_key_press(key: &str, cmd: bool, shift: bool) {
    use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType};
    use objc2_foundation::NSString;
    let Some(mtm) = MainThreadMarker::new() else { return };
    let (code, chars, function): (u16, String, bool) = match key {
        "left" => (123, "\u{F702}".into(), true),
        "right" => (124, "\u{F703}".into(), true),
        "down" => (125, "\u{F701}".into(), true),
        "up" => (126, "\u{F700}".into(), true),
        "escape" => (53, "\u{1b}".into(), false),
        "enter" => (36, "\r".into(), false),
        "space" => (49, " ".into(), false),
        "tab" => (48, "\t".into(), false),
        "backspace" => (51, "\u{7f}".into(), false),
        k => {
            let ch = k.chars().next().unwrap_or('a');
            let code = match ch.to_ascii_lowercase() {
                'a' => 0, 's' => 1, 'd' => 2, 'f' => 3, 'h' => 4, 'g' => 5, 'z' => 6, 'x' => 7, 'c' => 8, 'v' => 9,
                'b' => 11, 'q' => 12, 'w' => 13, 'e' => 14, 'r' => 15, 'y' => 16, 't' => 17, '1' => 18, '2' => 19,
                '3' => 20, '4' => 21, '6' => 22, '5' => 23, '9' => 25, '7' => 26, '8' => 28, '0' => 29, 'o' => 31,
                'u' => 32, 'i' => 34, 'p' => 35, 'l' => 37, 'j' => 38, 'k' => 40, 'n' => 45, 'm' => 46, _ => 0,
            };
            (code, ch.to_string(), false)
        }
    };
    let mut flags = NSEventModifierFlags::empty();
    if function {
        flags |= NSEventModifierFlags::Function | NSEventModifierFlags::NumericPad;
    }
    if cmd {
        flags |= NSEventModifierFlags::Command;
    }
    if shift {
        flags |= NSEventModifierFlags::Shift;
    }
    let app = NSApplication::sharedApplication(mtm);
    let Some(win) = app.keyWindow() else {
        log::warn!("press: no key window");
        return;
    };
    let ns_chars = NSString::from_str(&chars);
    for ty in [NSEventType::KeyDown, NSEventType::KeyUp] {
        let ev = unsafe {
            NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
                ty,
                NSPoint::new(0.0, 0.0),
                flags,
                0.0,
                win.windowNumber(),
                None,
                &ns_chars,
                &ns_chars,
                false,
                code,
            )
        };
        if let Some(ev) = ev {
            win.sendEvent(&ev);
        }
    }
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
