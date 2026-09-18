//! Menu bar status item.
use tray_icon::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

pub const ID_SHOW: &str = "show";
pub const ID_PAUSE: &str = "pause";
pub const ID_CLEAR: &str = "clear";
pub const ID_PREFS: &str = "prefs";
pub const ID_QUIT: &str = "quit";

pub struct Tray {
    _icon: TrayIcon,
    pub show_item: MenuItem,
    pub pause_item: CheckMenuItem,
}

impl Tray {
    pub fn new(hotkey_display: &str) -> anyhow::Result<Tray> {
        let (rgba, w, h) = crate::assets::menubar_icon_rgba(44).ok_or_else(|| anyhow::anyhow!("icon render"))?;
        let icon = Icon::from_rgba(rgba, w, h)?;
        let menu = Menu::new();
        let show_item = MenuItem::with_id(ID_SHOW, format!("Show Paste\t{hotkey_display}"), true, None);
        let pause_item = CheckMenuItem::with_id(ID_PAUSE, "Pause Clipboard Tracking", true, false, None);
        menu.append(&show_item)?;
        menu.append(&pause_item)?;
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&MenuItem::with_id(ID_CLEAR, "Clear Clipboard History…", true, None))?;
        menu.append(&MenuItem::with_id(ID_PREFS, "Settings…", true, None))?;
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&MenuItem::with_id(ID_QUIT, "Quit Paste", true, None))?;
        let icon = TrayIconBuilder::new()
            .with_icon(icon)
            .with_icon_as_template(true)
            .with_tooltip("Paste")
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(true)
            .build()?;
        Ok(Tray { _icon: icon, show_item, pause_item })
    }

    pub fn set_hotkey_display(&self, hotkey_display: &str) {
        self.show_item.set_text(format!("Show Paste\t{hotkey_display}"));
    }

    pub fn set_paused(&self, paused: bool) {
        self.pause_item.set_checked(paused);
    }
}
