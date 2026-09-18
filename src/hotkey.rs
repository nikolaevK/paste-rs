use anyhow::{anyhow, Result};
use global_hotkey::hotkey::HotKey;
use global_hotkey::GlobalHotKeyManager;

pub struct Hotkeys {
    manager: GlobalHotKeyManager,
    current: Option<HotKey>,
}

/// Converts a gpui keystroke string ("cmd-shift-v") to global-hotkey syntax ("cmd+shift+v").
pub fn to_global_syntax(keystroke: &str) -> String {
    let ks = keystroke.trim();
    if let Some(prefix) = ks.strip_suffix("--") {
        return format!("{}+minus", prefix.replace('-', "+"));
    }
    if ks == "-" {
        return "minus".into();
    }
    ks.replace('-', "+")
        .replace("+minus", "+-")
        .replace("+pageup", "+PageUp")
        .replace("+pagedown", "+PageDown")
}

/// Human readable form using macOS symbols, e.g. "⌘⇧V".
pub fn display(keystroke: &str) -> String {
    let mut out = String::new();
    let parts: Vec<&str> = keystroke.split('-').collect();
    let (mods, key) = match parts.split_last() {
        Some((k, m)) => (m, *k),
        None => return String::new(),
    };
    let key = if key.is_empty() && keystroke.ends_with('-') { "-" } else { key };
    for m in mods {
        out.push_str(match *m {
            "ctrl" | "control" => "⌃",
            "alt" | "option" => "⌥",
            "shift" => "⇧",
            "cmd" | "command" => "⌘",
            _ => "",
        });
    }
    let key_str = match key {
        "space" => "Space".to_string(),
        "enter" => "↩".to_string(),
        "escape" => "⎋".to_string(),
        "tab" => "⇥".to_string(),
        "backspace" => "⌫".to_string(),
        "delete" => "⌦".to_string(),
        "up" => "↑".to_string(),
        "down" => "↓".to_string(),
        "left" => "←".to_string(),
        "right" => "→".to_string(),
        k => k.to_uppercase(),
    };
    out.push_str(&key_str);
    out
}

impl Hotkeys {
    pub fn new() -> Result<Hotkeys> {
        let manager = GlobalHotKeyManager::new().map_err(|e| anyhow!("hotkey manager: {e}"))?;
        Ok(Hotkeys { manager, current: None })
    }

    pub fn set(&mut self, keystroke: &str) -> Result<()> {
        let hk: HotKey = to_global_syntax(keystroke)
            .parse()
            .map_err(|e| anyhow!("invalid hotkey '{keystroke}': {e}"))?;
        if let Some(old) = self.current.take() {
            let _ = self.manager.unregister(old);
        }
        self.manager.register(hk).map_err(|e| anyhow!("register hotkey: {e}"))?;
        self.current = Some(hk);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn syntax() {
        assert_eq!(to_global_syntax("cmd-shift-v"), "cmd+shift+v");
        assert_eq!(display("cmd-shift-v"), "⌘⇧V");
        assert_eq!(display("ctrl-alt-space"), "⌃⌥Space");
        let hk: Result<HotKey, _> = to_global_syntax("cmd-shift-v").parse();
        assert!(hk.is_ok());
    }
}
