//! Settings window (General, Shortcuts, Rules, Pinboards, About).
use crate::core::Core;
use crate::mac;
use crate::model::PINBOARD_COLORS;
use crate::settings::{ExcludedApp, Retention, Theme as ThemePref};
use crate::ui::theme::{hex_to_hsla, Theme};
use crate::ui::widgets::{button, icon_button, pref_row, section_title, segmented, toggle};
use crate::assets::icon;
use gpui::{prelude::*, *};

#[derive(Clone, Copy, PartialEq, Eq)]
enum PrefsTab {
    General,
    Shortcuts,
    Rules,
    Pinboards,
    About,
}

impl PrefsTab {
    const ALL: [PrefsTab; 5] = [PrefsTab::General, PrefsTab::Shortcuts, PrefsTab::Rules, PrefsTab::Pinboards, PrefsTab::About];
    fn label(&self) -> &'static str {
        match self {
            PrefsTab::General => "General",
            PrefsTab::Shortcuts => "Shortcuts",
            PrefsTab::Rules => "Rules",
            PrefsTab::Pinboards => "Pinboards",
            PrefsTab::About => "About",
        }
    }
}

pub struct Prefs {
    tab: PrefsTab,
    recording: bool,
    hotkey_error: Option<SharedString>,
    /// Pinboard being renamed inline (id, current text).
    renaming: Option<(i64, String)>,
    focus_handle: FocusHandle,
    scroll: ScrollHandle,
}

const DEFAULT_HOTKEY: &str = "cmd-shift-v";

pub fn open_prefs(cx: &mut App) {
    if let Some(handle) = cx.global::<Core>().prefs {
        if handle.update(cx, |_, window, _| window.activate_window()).is_ok() {
            mac::window::activate_self();
            return;
        }
        cx.global_mut::<Core>().prefs = None;
    }
    // A regular window must not be opened while the app is inactive (AppKit sends a spurious
    // key-window notification that gpui cannot handle). Activate first, then open on the next tick.
    mac::window::activate_self();
    cx.spawn(async move |cx| {
        cx.background_executor().timer(std::time::Duration::from_millis(40)).await;
        cx.update(|cx| open_prefs_window(cx)).ok();
    })
    .detach();
}

fn open_prefs_window(cx: &mut App) {
    if cx.global::<Core>().prefs.is_some() {
        return;
    }
    let screen = mac::screen::screen_under_mouse().or_else(|| mac::screen::screens().into_iter().next());
    let (sx, sy, sw, sh) = screen.map(|s| (s.x as f32, s.y as f32, s.width as f32, s.height as f32)).unwrap_or((0., 0., 1440., 900.));
    let (w, h) = (720.0_f32, 540.0_f32);
    let bounds = Bounds::new(point(px(sx + (sw - w) / 2.0), px(sy + (sh - h) / 2.5)), size(px(w), px(h)));
    let result = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("Paste Settings".into()),
                appears_transparent: false,
                traffic_light_position: None,
            }),
            focus: true,
            show: true,
            kind: WindowKind::Normal,
            is_movable: true,
            is_resizable: true,
            is_minimizable: true,
            display_id: None,
            window_background: WindowBackgroundAppearance::Opaque,
            app_id: None,
            window_min_size: Some(size(px(620.), px(420.))),
            window_decorations: None,
            tabbing_identifier: None,
        },
        |window, cx| {
            window.on_window_should_close(cx, |_, cx| {
                cx.global_mut::<Core>().prefs = None;
                true
            });
            let view = cx.new(|cx| Prefs {
                tab: PrefsTab::General,
                recording: false,
                hotkey_error: None,
                renaming: None,
                focus_handle: cx.focus_handle(),
                scroll: ScrollHandle::new(),
            });
            let fh = view.read(cx).focus_handle.clone();
            window.focus(&fh);
            view
        },
    );
    match result {
        Ok(handle) => {
            cx.global_mut::<Core>().prefs = Some(handle);
            mac::window::activate_self();
        }
        Err(e) => log::error!("open settings: {e}"),
    }
}

/// Shifted US-layout symbols reported by AppKit → base key, so the hotkey stays registrable.
fn unshift(key: &str) -> Option<&'static str> {
    Some(match key {
        "~" => "`", "!" => "1", "@" => "2", "#" => "3", "$" => "4", "%" => "5", "^" => "6", "&" => "7",
        "*" => "8", "(" => "9", ")" => "0", "_" => "-", "+" => "=", "{" => "[", "}" => "]", "|" => "\\",
        ":" => ";", "\"" => "'", "<" => ",", ">" => ".", "?" => "/",
        _ => return None,
    })
}

fn is_function_key(key: &str) -> bool {
    key.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()).map(|n| (1..=20).contains(&n)).unwrap_or(false)
}

fn keystroke_string(ks: &Keystroke) -> String {
    let mut parts = Vec::new();
    let (key, shifted) = match unshift(&ks.key) {
        Some(base) => (base.to_string(), true),
        None => (ks.key.clone(), false),
    };
    if ks.modifiers.control {
        parts.push("ctrl");
    }
    if ks.modifiers.alt {
        parts.push("alt");
    }
    if ks.modifiers.shift || shifted {
        parts.push("shift");
    }
    if ks.modifiers.platform {
        parts.push("cmd");
    }
    parts.push(key.as_str());
    parts.join("-")
}

impl Prefs {
    pub fn debug_set_tab(&mut self, n: usize, cx: &mut Context<Self>) {
        if let Some(t) = PrefsTab::ALL.get(n) {
            self.tab = *t;
            cx.notify();
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((id, name)) = &mut self.renaming {
            let ks = &event.keystroke;
            match ks.key.as_str() {
                "escape" => self.renaming = None,
                "enter" => {
                    let (id, name) = (*id, name.trim().to_string());
                    if !name.is_empty() {
                        let _ = cx.global_mut::<Core>().rename_pinboard(id, &name);
                    }
                    self.renaming = None;
                    cx.refresh_windows();
                }
                "backspace" => {
                    name.pop();
                }
                "space" => name.push(' '),
                _ => {
                    if !ks.modifiers.platform && !ks.modifiers.control {
                        if let Some(ch) = &ks.key_char {
                            if !ch.chars().any(|c| c.is_control()) && name.chars().count() < 40 {
                                name.push_str(ch);
                            }
                        }
                    }
                }
            }
            cx.notify();
            return;
        }
        if !self.recording {
            let ks = &event.keystroke;
            match ks.key.as_str() {
                "w" if ks.modifiers.platform => {
                    cx.global_mut::<Core>().prefs = None;
                    window.remove_window();
                }
                "escape" => {
                    cx.global_mut::<Core>().prefs = None;
                    window.remove_window();
                }
                "q" if ks.modifiers.platform => cx.quit(),
                _ => {}
            }
            return;
        }
        let ks = &event.keystroke;
        if ks.key == "escape" {
            self.recording = false;
            cx.notify();
            return;
        }
        let is_modifier_only = matches!(ks.key.as_str(), "cmd" | "ctrl" | "alt" | "shift" | "fn" | "capslock" | "control" | "command" | "option");
        if is_modifier_only {
            return;
        }
        if !(ks.modifiers.platform || ks.modifiers.control || ks.modifiers.alt || is_function_key(&ks.key)) {
            self.hotkey_error = Some("Include ⌘, ⌃ or ⌥ in the shortcut (or use a function key)".into());
            cx.notify();
            return;
        }
        let new = keystroke_string(ks);
        self.recording = false;
        self.apply_hotkey(&new, cx);
    }

    fn render_sidebar(&self, theme: &Theme, cx: &mut Context<Self>) -> Div {
        let mut side = div().w(px(170.)).flex_shrink_0().h_full().bg(theme.sidebar_bg).p(px(10.)).flex().flex_col().gap(px(2.));
        for (i, t) in PrefsTab::ALL.iter().enumerate() {
            let sel = *t == self.tab;
            let t2 = *t;
            side = side.child(
                div()
                    .id(("prefs-tab", i))
                    .h(px(30.))
                    .px(px(10.))
                    .rounded(px(7.))
                    .flex()
                    .items_center()
                    .text_size(px(13.))
                    .cursor_pointer()
                    .when(sel, |d| d.bg(theme.accent).text_color(white()))
                    .when(!sel, |d| d.text_color(theme.text).hover(|s| s.bg(theme.pill_hover)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.tab = t2;
                        this.recording = false;
                        cx.notify();
                    }))
                    .child(t.label()),
            );
        }
        side
    }

    fn render_general(&self, theme: &Theme, cx: &mut Context<Self>) -> Div {
        let core = cx.global::<Core>();
        let s = core.settings();
        let paused = core.paused();
        let retention_idx = Retention::ALL.iter().position(|r| *r == s.retention).unwrap_or(0);
        let retention_labels: Vec<&str> = Retention::ALL.iter().map(|r| r.label()).collect();
        let theme_idx = match s.theme {
            ThemePref::System => 0,
            ThemePref::Light => 1,
            ThemePref::Dark => 2,
        };
        div()
            .flex()
            .flex_col()
            .child(section_title("Startup", theme))
            .child(pref_row(
                "Launch Paste at login",
                None,
                toggle("launch", s.launch_at_login, theme).on_click(cx.listener(|_, _, _, cx| {
                    let core = cx.global::<Core>();
                    let next = !core.settings().launch_at_login;
                    if let Err(e) = mac::login::set_enabled(next) {
                        log::warn!("login item: {e}");
                    }
                    core.update_settings(|s| s.launch_at_login = next);
                    cx.notify();
                })),
                theme,
            ))
            .child(pref_row(
                "Clipboard tracking",
                Some("Pause to temporarily stop recording new items"),
                toggle("paused", !paused, theme).on_click(cx.listener(|_, _, _, cx| {
                    let core = cx.global::<Core>();
                    core.set_paused(!core.paused());
                    cx.notify();
                })),
                theme,
            ))
            .child(section_title("History", theme))
            .child(pref_row(
                "Keep history for",
                Some("Older items in Clipboard are removed automatically. Pinboards are never removed."),
                segmented("retention", &retention_labels, retention_idx, theme, |i, _, cx| {
                    let r = Retention::ALL[i];
                    cx.global::<Core>().update_settings(|s| s.retention = r);
                    cx.global_mut::<Core>().purge_expired();
                    if let Some(shelf) = cx.global::<Core>().shelf {
                        let _ = shelf.update(cx, |s, _, cx| s.refresh(cx));
                    }
                    cx.refresh_windows();
                }),
                theme,
            ))
            .child(pref_row(
                "Clear clipboard history",
                Some("Removes every item in Clipboard. Pinboards are kept."),
                button("clear-history", "Clear History…", theme, false).on_click(cx.listener(|_, _, window, cx| {
                    let answer = window.prompt(
                        PromptLevel::Warning,
                        "Clear clipboard history?",
                        Some("All items in Clipboard will be removed. Pinboards are kept. This can't be undone."),
                        &["Clear History", "Cancel"],
                        cx,
                    );
                    cx.spawn(async move |_, cx| {
                        if let Ok(0) = answer.await {
                            cx.update(|cx| {
                                if let Err(e) = cx.global_mut::<Core>().clear_history() {
                                    log::warn!("clear history: {e}");
                                }
                                if let Some(shelf) = cx.global::<Core>().shelf {
                                    let _ = shelf.update(cx, |s, _, cx| s.refresh(cx));
                                }
                                cx.refresh_windows();
                            })
                            .ok();
                        }
                    })
                    .detach();
                })),
                theme,
            ))
            .child(pref_row(
                "Move pasted items to the top",
                None,
                toggle("move-top", s.move_pasted_to_top, theme).on_click(cx.listener(|_, _, _, cx| {
                    cx.global::<Core>().update_settings(|s| s.move_pasted_to_top = !s.move_pasted_to_top);
                    cx.notify();
                })),
                theme,
            ))
            .child(pref_row(
                "Fetch link titles and icons",
                Some("Requests the page over the network to show a title and favicon"),
                toggle("links", s.fetch_link_previews, theme).on_click(cx.listener(|_, _, _, cx| {
                    cx.global::<Core>().update_settings(|s| s.fetch_link_previews = !s.fetch_link_previews);
                    cx.notify();
                })),
                theme,
            ))
            .child(section_title("Pasting", theme))
            .child(pref_row(
                "Always paste as plain text",
                Some("Strips formatting. Hold ⇧ when pasting for the opposite behaviour."),
                toggle("plain", s.paste_plain_default, theme).on_click(cx.listener(|_, _, _, cx| {
                    cx.global::<Core>().update_settings(|s| s.paste_plain_default = !s.paste_plain_default);
                    cx.notify();
                })),
                theme,
            ))
            .child(section_title("Appearance", theme))
            .child(pref_row(
                "Theme",
                None,
                segmented("theme", &["System", "Light", "Dark"], theme_idx, theme, |i, _, cx| {
                    let t = match i {
                        1 => ThemePref::Light,
                        2 => ThemePref::Dark,
                        _ => ThemePref::System,
                    };
                    cx.global::<Core>().update_settings(|s| s.theme = t);
                    cx.refresh_windows();
                }),
                theme,
            ))
    }

    fn apply_hotkey(&mut self, new: &str, cx: &mut Context<Self>) {
        let core = cx.global_mut::<Core>();
        let result = match core.hotkeys.as_mut() {
            Some(hk) => hk.set(new),
            None => Err(anyhow::anyhow!("hotkeys unavailable")),
        };
        match result {
            Ok(()) => {
                core.update_settings(|s| s.hotkey = new.to_string());
                let disp = core.hotkey_display();
                if let Some(t) = &core.tray {
                    t.set_hotkey_display(&disp);
                }
                self.hotkey_error = None;
            }
            Err(e) => {
                let prev = core.settings().hotkey;
                if let Some(hk) = core.hotkeys.as_mut() {
                    let _ = hk.set(&prev);
                }
                self.hotkey_error = Some(SharedString::from(format!("Couldn't register: {e}")));
            }
        }
        cx.notify();
    }

    fn render_shortcuts(&self, theme: &Theme, cx: &mut Context<Self>) -> Div {
        let hotkey = cx.global::<Core>().hotkey_display();
        let is_default = cx.global::<Core>().settings().hotkey == DEFAULT_HOTKEY;
        let recording = self.recording;
        let rows: &[(&str, &str)] = &[
            ("Paste selected item", "↩"),
            ("Paste as plain text", "⇧↩"),
            ("Copy without pasting", "⌥↩ / ⌘C"),
            ("Quick Look preview", "Space"),
            ("Search", "Start typing / ⌘F"),
            ("Move between items", "← →"),
            ("Extend selection", "⇧← ⇧→ / ⇧Click"),
            ("Toggle item in selection", "⌘Click"),
            ("Select all", "⌘A"),
            ("Switch pinboard", "⇥ / ↑ ↓ / ⌘1…9"),
            ("New pinboard", "⌘N"),
            ("Add to pinboard", "⌘P / Drag onto a pinboard"),
            ("Open link", "⌘O"),
            ("Delete item", "⌘⌫ / ⌦"),
            ("Close Paste", "Esc / ⌘W"),
            ("Settings", "⌘,"),
        ];
        let mut list = div().flex().flex_col();
        list = list.child(section_title("Global", theme));
        list = list.child(pref_row(
            "Show Paste",
            Some("Works in every app. Click to record a new shortcut."),
            div()
                .flex()
                .flex_col()
                .items_end()
                .gap(px(4.))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(8.))
                        .when(!is_default && !recording, |d| {
                            d.child(
                                div()
                                    .id("hotkey-reset")
                                    .text_size(px(12.))
                                    .text_color(theme.accent)
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, _, cx| this.apply_hotkey(DEFAULT_HOTKEY, cx)))
                                    .child("Reset to ⌘⇧V"),
                            )
                        })
                        .child(
                    div()
                        .id("hotkey")
                        .h(px(28.))
                        .min_w(px(120.))
                        .px(px(12.))
                        .rounded(px(7.))
                        .border_1()
                        .border_color(if recording { theme.accent } else { theme.control_border })
                        .bg(theme.control_bg)
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(13.))
                        .text_color(if recording { theme.accent } else { theme.text })
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.recording = !this.recording;
                            this.hotkey_error = None;
                            window.focus(&this.focus_handle);
                            cx.notify();
                        }))
                        .child(SharedString::from(if recording { "Press keys…".to_string() } else { hotkey })),
                        ),
                )
                .when_some(self.hotkey_error.clone(), |d, e| d.child(div().text_size(px(11.)).text_color(hsla(0., 0.8, 0.55, 1.)).child(e))),
            theme,
        ));
        list = list.child(section_title("In the shelf", theme));
        for (label, keys) in rows {
            list = list.child(pref_row(
                *label,
                None,
                div().text_size(px(12.5)).text_color(theme.text_secondary).child(*keys),
                theme,
            ));
        }
        list
    }

    fn render_rules(&self, theme: &Theme, cx: &mut Context<Self>) -> Div {
        let s = cx.global::<Core>().settings();
        let mut list = div()
            .flex()
            .flex_col()
            .child(section_title("Privacy", theme))
            .child(pref_row(
                "Ignore confidential content",
                Some("Skips items marked as concealed or transient by password managers and similar apps"),
                toggle("concealed", s.ignore_concealed, theme).on_click(cx.listener(|_, _, _, cx| {
                    cx.global::<Core>().update_settings(|s| s.ignore_concealed = !s.ignore_concealed);
                    cx.notify();
                })),
                theme,
            ))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(section_title("Ignored applications", theme))
                    .child(button("add-app", "Add Application…", theme, false).on_click(cx.listener(|_, _, _, cx| {
                        let rx = cx.prompt_for_paths(PathPromptOptions {
                            files: true,
                            directories: false,
                            multiple: true,
                            prompt: Some("Ignore".into()),
                        });
                        cx.spawn(async move |_, cx| {
                            if let Ok(Ok(Some(paths))) = rx.await {
                                let _ = cx.update(|cx| {
                                    let mut added = Vec::new();
                                    for p in paths {
                                        if let Some(bundle) = mac::workspace::bundle_id_for_app(&p) {
                                            added.push(ExcludedApp { bundle_id: bundle, name: mac::workspace::app_display_name(&p) });
                                        }
                                    }
                                    cx.global::<Core>().update_settings(|s| {
                                        for a in added {
                                            if !s.excluded_apps.iter().any(|e| e.bundle_id == a.bundle_id) {
                                                s.excluded_apps.push(a);
                                            }
                                        }
                                    });
                                    cx.refresh_windows();
                                });
                            }
                        })
                        .detach();
                    }))),
            )
            .child(
                div()
                    .py(px(6.))
                    .text_size(px(11.5))
                    .text_color(theme.text_secondary)
                    .child("Anything copied while one of these apps is in front is not recorded."),
            );
        if s.excluded_apps.is_empty() {
            list = list.child(div().py(px(10.)).text_size(px(12.5)).text_color(theme.text_tertiary).child("No ignored applications"));
        }
        for (i, app) in s.excluded_apps.iter().enumerate() {
            let bundle = app.bundle_id.clone();
            list = list.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .py(px(7.))
                    .border_b_1()
                    .border_color(theme.separator)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(div().text_size(px(13.)).text_color(theme.text).child(SharedString::from(app.name.clone())))
                            .child(div().text_size(px(11.)).text_color(theme.text_tertiary).child(SharedString::from(app.bundle_id.clone()))),
                    )
                    .child(icon_button(("remove-app", i), "close", 11., theme).on_click(cx.listener(move |_, _, _, cx| {
                        let b = bundle.clone();
                        cx.global::<Core>().update_settings(|s| s.excluded_apps.retain(|a| a.bundle_id != b));
                        cx.notify();
                    }))),
            );
        }
        list
    }

    fn render_pinboards(&self, theme: &Theme, cx: &mut Context<Self>) -> Div {
        let pinboards = cx.global::<Core>().pinboards.clone();
        let counts: Vec<usize> = pinboards.iter().map(|p| cx.global::<Core>().pinned.get(&p.id).map(|l| l.len()).unwrap_or(0)).collect();
        let mut list = div().flex().flex_col().child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .child(section_title("Pinboards", theme))
                .child(button("new-pb", "New Pinboard", theme, false).on_click(cx.listener(|_, _, _, cx| {
                    let n = cx.global::<Core>().pinboards.len() + 1;
                    let _ = cx.global_mut::<Core>().create_pinboard(&format!("Pinboard {n}"));
                    cx.refresh_windows();
                }))),
        );
        if pinboards.is_empty() {
            list = list.child(div().py(px(10.)).text_size(px(12.5)).text_color(theme.text_tertiary).child("No pinboards yet. Pinboards keep items you use all the time, forever."));
        }
        for (i, pb) in pinboards.iter().enumerate() {
            let id = pb.id;
            let cur_color = pb.color.clone();
            list = list.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(10.))
                    .py(px(8.))
                    .border_b_1()
                    .border_color(theme.separator)
                    .child(
                        div()
                            .id(("pb-color", i))
                            .size(px(14.))
                            .rounded_full()
                            .bg(hex_to_hsla(&pb.color))
                            .cursor_pointer()
                            .on_click(cx.listener(move |_, _, _, cx| {
                                let idx = PINBOARD_COLORS.iter().position(|c| c.eq_ignore_ascii_case(&cur_color)).unwrap_or(0);
                                let next = PINBOARD_COLORS[(idx + 1) % PINBOARD_COLORS.len()];
                                let _ = cx.global_mut::<Core>().recolor_pinboard(id, next);
                                cx.refresh_windows();
                            })),
                    )
                    .child(match &self.renaming {
                        Some((rid, text)) if *rid == id => div()
                            .flex_1()
                            .h(px(24.))
                            .px(px(8.))
                            .rounded(px(6.))
                            .bg(theme.control_bg)
                            .border_1()
                            .border_color(theme.accent)
                            .flex()
                            .items_center()
                            .text_size(px(13.))
                            .text_color(theme.text)
                            .child(SharedString::from(text.clone()))
                            .child(div().w(px(1.5)).h(px(14.)).ml(px(1.)).bg(theme.accent))
                            .into_any_element(),
                        _ => {
                            let name = pb.name.clone();
                            div()
                                .id(("pb-name", i))
                                .flex_1()
                                .h(px(24.))
                                .px(px(8.))
                                .rounded(px(6.))
                                .flex()
                                .items_center()
                                .text_size(px(13.))
                                .text_color(theme.text)
                                .cursor_pointer()
                                .hover(|s| s.bg(theme.pill_hover))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.renaming = Some((id, name.clone()));
                                    window.focus(&this.focus_handle);
                                    cx.notify();
                                }))
                                .child(SharedString::from(pb.name.clone()))
                                .into_any_element()
                        }
                    })
                    .child(div().text_size(px(12.)).text_color(theme.text_tertiary).child(SharedString::from(crate::util::count_label(counts[i] as i64, "item", "items"))))
                    .child(icon_button(("pb-delete", i), "close", 11., theme).on_click(cx.listener(move |_, _, _, cx| {
                        let _ = cx.global_mut::<Core>().delete_pinboard(id);
                        cx.refresh_windows();
                    }))),
            );
        }
        list = list.child(div().pt(px(8.)).text_size(px(11.5)).text_color(theme.text_secondary).child("Click a name to rename it (↩ to save, Esc to cancel). Click the color dot to change the color."));
        list
    }

    fn render_about(&self, theme: &Theme, cx: &mut Context<Self>) -> Div {
        let dir = crate::settings::data_dir();
        let dir_label = dir.to_string_lossy().to_string();
        let count = cx.global::<Core>().history.len();
        let trusted = mac::paste::accessibility_trusted(false);
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(12.))
                    .child(div().size(px(48.)).rounded(px(12.)).bg(theme.accent).flex().items_center().justify_center().child(svg().path(icon("clipboard")).size(px(28.)).text_color(white())))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(div().text_size(px(18.)).font_weight(FontWeight::BOLD).text_color(theme.text).child("Paste"))
                            .child(div().text_size(px(12.)).text_color(theme.text_secondary).child(SharedString::from(format!("Version {} · native Rust build for Apple Silicon", env!("CARGO_PKG_VERSION"))))),
                    ),
            )
            .child(pref_row("Items in clipboard history", None, div().text_size(px(13.)).text_color(theme.text_secondary).child(SharedString::from(count.to_string())), theme))
            .child(pref_row(
                "Accessibility access",
                Some("Required to paste directly into other apps. Without it, Paste only copies."),
                if trusted {
                    div().text_size(px(13.)).text_color(hsla(0.38, 0.6, 0.4, 1.)).child("Granted").into_any_element()
                } else {
                    button("grant-ax", "Grant Access…", theme, true)
                        .on_click(|_, _, _| {
                            mac::paste::accessibility_trusted(true);
                            mac::workspace::open_url("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility");
                        })
                        .into_any_element()
                },
                theme,
            ))
            .child(pref_row(
                "Data folder",
                Some(&dir_label),
                button("reveal-data", "Show in Finder", theme, false).on_click(move |_, _, _| mac::workspace::reveal_in_finder(std::slice::from_ref(&dir))),
                theme,
            ))
            .child(div().pt(px(6.)).text_size(px(11.5)).text_color(theme.text_tertiary).child("Everything stays on this Mac. Paste never uploads your clipboard."))
    }
}

impl Render for Prefs {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pref = cx.global::<Core>().settings.read().unwrap().theme;
        let theme = Theme::for_window(window, pref);
        let content = match self.tab {
            PrefsTab::General => self.render_general(&theme, cx),
            PrefsTab::Shortcuts => self.render_shortcuts(&theme, cx),
            PrefsTab::Rules => self.render_rules(&theme, cx),
            PrefsTab::Pinboards => self.render_pinboards(&theme, cx),
            PrefsTab::About => self.render_about(&theme, cx),
        };
        div()
            .id("prefs-root")
            .size_full()
            .track_focus(&self.focus_handle)
            .key_context("Prefs")
            .on_key_down(cx.listener(Self::on_key_down))
            .flex()
            .flex_row()
            .bg(theme.window_bg)
            .text_color(theme.text)
            .child(self.render_sidebar(&theme, cx))
            .child(
                div()
                    .id("prefs-content")
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .px(px(22.))
                    .py(px(10.))
                    .child(content.w_full()),
            )
    }
}
