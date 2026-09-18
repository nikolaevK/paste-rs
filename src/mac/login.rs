//! Launch-at-login via a per-user LaunchAgent (works for bundled and bare binaries).
use std::path::PathBuf;

pub const LABEL: &str = "io.paste.rs";

fn agent_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join("Library/LaunchAgents").join(format!("{LABEL}.plist")))
}

pub fn set_enabled(enabled: bool) -> anyhow::Result<()> {
    let Some(path) = agent_path() else { anyhow::bail!("no home directory") };
    if enabled {
        let exe = std::env::current_exe()?;
        // If running from an .app bundle, launch the bundle via `open` so it gets proper app identity.
        let exe_str = exe.to_string_lossy().to_string();
        let program_args = if let Some(idx) = exe_str.find(".app/Contents/MacOS/") {
            let app = &exe_str[..idx + 4];
            format!("<string>/usr/bin/open</string><string>-a</string><string>{}</string>", xml_escape(app))
        } else {
            format!("<string>{}</string>", xml_escape(&exe_str))
        };
        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key><string>{LABEL}</string>
    <key>ProgramArguments</key><array>{program_args}</array>
    <key>RunAtLoad</key><true/>
    <key>ProcessType</key><string>Interactive</string>
</dict>
</plist>
"#
        );
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, plist)?;
    } else if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}
