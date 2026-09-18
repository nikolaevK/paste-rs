use crate::settings::Theme as ThemePref;
use gpui::{hsla, rgb, rgba, Hsla, Window, WindowAppearance};

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub dark: bool,
    pub shelf_bg: Hsla,
    pub shelf_border: Hsla,
    pub text: Hsla,
    pub text_secondary: Hsla,
    pub text_tertiary: Hsla,
    pub card_bg: Hsla,
    pub card_border: Hsla,
    pub pill_bg: Hsla,
    pub pill_hover: Hsla,
    pub pill_selected: Hsla,
    pub field_bg: Hsla,
    pub accent: Hsla,
    pub menu_bg: Hsla,
    pub menu_border: Hsla,
    pub menu_hover: Hsla,
    pub separator: Hsla,
    pub window_bg: Hsla,
    pub sidebar_bg: Hsla,
    pub control_bg: Hsla,
    pub control_border: Hsla,
    pub shadow: Hsla,
}

impl Theme {
    pub fn light() -> Theme {
        Theme {
            dark: false,
            shelf_bg: rgba(0xf4f4f6c8).into(),
            shelf_border: rgba(0x00000018).into(),
            text: rgb(0x1d1d1f).into(),
            text_secondary: rgba(0x3c3c4399).into(),
            text_tertiary: rgba(0x3c3c4366).into(),
            card_bg: rgb(0xffffff).into(),
            card_border: rgba(0x0000000f).into(),
            pill_bg: rgba(0x00000000).into(),
            pill_hover: rgba(0x0000000c).into(),
            pill_selected: rgba(0x00000016).into(),
            field_bg: rgba(0x00000012).into(),
            accent: rgb(0x0a84ff).into(),
            menu_bg: rgba(0xf6f6f8f2).into(),
            menu_border: rgba(0x00000022).into(),
            menu_hover: rgb(0x0a84ff).into(),
            separator: rgba(0x0000001a).into(),
            window_bg: rgb(0xececee).into(),
            sidebar_bg: rgb(0xe3e3e6).into(),
            control_bg: rgb(0xffffff).into(),
            control_border: rgba(0x00000026).into(),
            shadow: rgba(0x00000026).into(),
        }
    }

    pub fn dark() -> Theme {
        Theme {
            dark: true,
            shelf_bg: rgba(0x1c1c1ec8).into(),
            shelf_border: rgba(0xffffff1a).into(),
            text: rgb(0xf5f5f7).into(),
            text_secondary: rgba(0xebebf599).into(),
            text_tertiary: rgba(0xebebf566).into(),
            card_bg: rgb(0x2c2c2e).into(),
            card_border: rgba(0xffffff14).into(),
            pill_bg: rgba(0xffffff00).into(),
            pill_hover: rgba(0xffffff14).into(),
            pill_selected: rgba(0xffffff24).into(),
            field_bg: rgba(0xffffff1c).into(),
            accent: rgb(0x0a84ff).into(),
            menu_bg: rgba(0x2a2a2cf2).into(),
            menu_border: rgba(0xffffff22).into(),
            menu_hover: rgb(0x0a84ff).into(),
            separator: rgba(0xffffff1f).into(),
            window_bg: rgb(0x1e1e20).into(),
            sidebar_bg: rgb(0x262628).into(),
            control_bg: rgb(0x3a3a3c).into(),
            control_border: rgba(0xffffff26).into(),
            shadow: rgba(0x00000066).into(),
        }
    }

    pub fn for_window(window: &Window, pref: ThemePref) -> Theme {
        match pref {
            ThemePref::Light => Theme::light(),
            ThemePref::Dark => Theme::dark(),
            ThemePref::System => match window.appearance() {
                WindowAppearance::Dark | WindowAppearance::VibrantDark => Theme::dark(),
                _ => Theme::light(),
            },
        }
    }
}

pub fn hex_to_hsla(hex: &str) -> Hsla {
    match crate::util::hex_to_rgba(hex) {
        Some((r, g, b, a)) => gpui::Rgba { r, g, b, a }.into(),
        None => hsla(0.0, 0.0, 0.5, 1.0),
    }
}

/// Black or white, whichever reads better on `bg`.
pub fn contrast_text(bg: Hsla) -> Hsla {
    let c: gpui::Rgba = bg.into();
    let lum = 0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b;
    if lum > 0.6 { rgba(0x000000d9).into() } else { rgb(0xffffff).into() }
}

pub fn white_alpha(a: f32) -> Hsla {
    hsla(0.0, 0.0, 1.0, a)
}
