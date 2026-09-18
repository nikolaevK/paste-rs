//! Simulated ⌘V keystroke and Accessibility permission check.
use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::{CFString, CFStringRef};
use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use std::ffi::c_void;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
    static kAXTrustedCheckOptionPrompt: CFStringRef;
}

pub fn accessibility_trusted(prompt: bool) -> bool {
    unsafe {
        let key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
        let value = if prompt { CFBoolean::true_value() } else { CFBoolean::false_value() };
        let dict = CFDictionary::from_CFType_pairs(&[(key, value)]);
        AXIsProcessTrustedWithOptions(dict.as_concrete_TypeRef() as *const c_void)
    }
}

const KEY_V: u16 = 9;

/// Posts ⌘V (or ⌘⇧⌥V for "paste and match style" when `plain` is set) to the system.
pub fn send_paste_keystroke(plain: bool) -> bool {
    let Ok(source) = CGEventSource::new(CGEventSourceStateID::CombinedSessionState) else {
        return false;
    };
    let mut flags = CGEventFlags::CGEventFlagCommand;
    if plain {
        flags |= CGEventFlags::CGEventFlagShift | CGEventFlags::CGEventFlagAlternate;
    }
    let Ok(down) = CGEvent::new_keyboard_event(source.clone(), KEY_V, true) else {
        return false;
    };
    down.set_flags(flags);
    let Ok(up) = CGEvent::new_keyboard_event(source, KEY_V, false) else {
        return false;
    };
    up.set_flags(flags);
    down.post(CGEventTapLocation::HID);
    up.post(CGEventTapLocation::HID);
    true
}
