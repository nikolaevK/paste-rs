use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Retention {
    Forever,
    Year,
    Month,
    Week,
    Day,
}

impl Retention {
    pub const ALL: [Retention; 5] = [
        Retention::Forever,
        Retention::Year,
        Retention::Month,
        Retention::Week,
        Retention::Day,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Retention::Forever => "Forever",
            Retention::Year => "1 Year",
            Retention::Month => "1 Month",
            Retention::Week => "1 Week",
            Retention::Day => "1 Day",
        }
    }

    pub fn millis(&self) -> Option<i64> {
        const DAY: i64 = 24 * 60 * 60 * 1000;
        match self {
            Retention::Forever => None,
            Retention::Year => Some(365 * DAY),
            Retention::Month => Some(30 * DAY),
            Retention::Week => Some(7 * DAY),
            Retention::Day => Some(DAY),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    System,
    Light,
    Dark,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExcludedApp {
    pub bundle_id: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Hotkey in gpui keystroke syntax, e.g. "cmd-shift-v".
    pub hotkey: String,
    pub launch_at_login: bool,
    pub retention: Retention,
    pub paste_plain_default: bool,
    pub ignore_concealed: bool,
    pub excluded_apps: Vec<ExcludedApp>,
    pub theme: Theme,
    pub move_pasted_to_top: bool,
    pub fetch_link_previews: bool,
    pub close_after_paste: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            hotkey: "cmd-shift-v".into(),
            launch_at_login: false,
            retention: Retention::Forever,
            paste_plain_default: false,
            ignore_concealed: true,
            excluded_apps: vec![
                ExcludedApp { bundle_id: "com.apple.keychainaccess".into(), name: "Keychain Access".into() },
                ExcludedApp { bundle_id: "com.agilebits.onepassword7".into(), name: "1Password 7".into() },
                ExcludedApp { bundle_id: "com.1password.1password".into(), name: "1Password".into() },
            ],
            theme: Theme::System,
            move_pasted_to_top: true,
            fetch_link_previews: true,
            close_after_paste: true,
        }
    }
}

pub fn data_dir() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    let dir = base.join("Paste");
    let _ = std::fs::create_dir_all(dir.join("images"));
    let _ = std::fs::create_dir_all(dir.join("thumbs"));
    let _ = std::fs::create_dir_all(dir.join("icons"));
    let _ = std::fs::create_dir_all(dir.join("favicons"));
    let _ = std::fs::create_dir_all(dir.join("link-images"));
    dir
}

fn settings_path() -> PathBuf {
    data_dir().join("settings.json")
}

impl Settings {
    pub fn load() -> Settings {
        match std::fs::read(settings_path()) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|e| {
                log::warn!("settings parse error: {e}; using defaults");
                Settings::default()
            }),
            Err(_) => Settings::default(),
        }
    }

    pub fn save(&self) {
        let Ok(json) = serde_json::to_vec_pretty(self) else { return };
        let path = settings_path();
        let tmp = path.with_extension("json.tmp");
        let result = std::fs::write(&tmp, json).and_then(|_| std::fs::rename(&tmp, &path));
        if let Err(e) = result {
            log::error!("failed to save settings: {e}");
            let _ = std::fs::remove_file(&tmp);
        }
    }

    pub fn is_excluded(&self, bundle_id: &str) -> bool {
        self.excluded_apps.iter().any(|a| a.bundle_id == bundle_id)
    }
}
