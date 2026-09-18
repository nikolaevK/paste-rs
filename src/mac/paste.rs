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

const KEY_V_ANSI: u16 = 9;

#[link(name = "Carbon", kind = "framework")]
extern "C" {
    fn TISCopyCurrentKeyboardLayoutInputSource() -> *mut c_void;
    fn TISGetInputSourceProperty(source: *mut c_void, key: CFStringRef) -> *mut c_void;
    static kTISPropertyUnicodeKeyLayoutData: CFStringRef;
    fn UCKeyTranslate(
        layout: *const c_void,
        virtual_key_code: u16,
        key_action: u16,
        modifier_key_state: u32,
        keyboard_type: u32,
        key_translate_options: u32,
        dead_key_state: *mut u32,
        max_string_length: usize,
        actual_string_length: *mut usize,
        unicode_string: *mut u16,
    ) -> i32;
    fn LMGetKbdType() -> u8;
    fn CFRelease(cf: *const c_void);
    fn CFDataGetBytePtr(data: *const c_void) -> *const u8;
}

const UC_KEY_ACTION_DISPLAY: u16 = 3;
const UC_KEY_TRANSLATE_NO_DEAD_KEYS_BIT: u32 = 1 << 0;

/// Finds the virtual key that produces "v" on the current keyboard layout (Dvorak, AZERTY, …).
fn virtual_key_for_v() -> u16 {
    unsafe {
        let source = TISCopyCurrentKeyboardLayoutInputSource();
        if source.is_null() {
            return KEY_V_ANSI;
        }
        let data = TISGetInputSourceProperty(source, kTISPropertyUnicodeKeyLayoutData);
        let result = if data.is_null() {
            KEY_V_ANSI
        } else {
            let layout = CFDataGetBytePtr(data) as *const c_void;
            let kbd_type = LMGetKbdType() as u32;
            let mut found = KEY_V_ANSI;
            for code in 0u16..128 {
                let mut dead: u32 = 0;
                let mut len: usize = 0;
                let mut buf = [0u16; 4];
                let status = UCKeyTranslate(
                    layout,
                    code,
                    UC_KEY_ACTION_DISPLAY,
                    0,
                    kbd_type,
                    UC_KEY_TRANSLATE_NO_DEAD_KEYS_BIT,
                    &mut dead,
                    buf.len(),
                    &mut len,
                    buf.as_mut_ptr(),
                );
                if status == 0 && len == 1 && (buf[0] == 'v' as u16 || buf[0] == 'V' as u16) {
                    found = code;
                    break;
                }
            }
            found
        };
        CFRelease(source);
        result
    }
}

/// Posts ⌘V (or ⌘⇧⌥V for "paste and match style" when `plain` is set) to the system.
pub fn send_paste_keystroke(plain: bool) -> bool {
    let Ok(source) = CGEventSource::new(CGEventSourceStateID::CombinedSessionState) else {
        return false;
    };
    let key_v = virtual_key_for_v();
    let mut flags = CGEventFlags::CGEventFlagCommand;
    if plain {
        flags |= CGEventFlags::CGEventFlagShift | CGEventFlags::CGEventFlagAlternate;
    }
    let Ok(down) = CGEvent::new_keyboard_event(source.clone(), key_v, true) else {
        return false;
    };
    down.set_flags(flags);
    let Ok(up) = CGEvent::new_keyboard_event(source, key_v, false) else {
        return false;
    };
    up.set_flags(flags);
    down.post(CGEventTapLocation::HID);
    up.post(CGEventTapLocation::HID);
    true
}
