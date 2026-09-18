//! Quick Look style preview window for the selected item (Space).
use crate::core::{AppIcon, Core};
use crate::mac;
use crate::model::{ClipItem, ItemKind, Payload};
use crate::ui::card::app_icon_view;
use crate::ui::shelf::SHELF_H;
use crate::ui::theme::{contrast_text, hex_to_hsla, Theme};
use crate::util;
use gpui::{prelude::*, *};
use std::sync::Arc;

pub struct Preview {
    item: Arc<ClipItem>,
    payload: Payload,
    app_icon: AppIcon,
    focus_handle: FocusHandle,
    scroll: ScrollHandle,
    pub closing: bool,
}

/// What the shelf should do once the preview has closed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseAction {
    None,
    Paste { plain: bool },
    Copy,
}

/// Very long texts are cut for the preview; laying out megabytes of text would stall the UI.
const PREVIEW_TEXT_CHARS: usize = 200_000;
const SCROLL_LINE: f32 = 42.0;

pub fn load_payload(item: &ClipItem, cx: &App) -> Payload {
    let mut payload = cx.global::<Core>().db.payload(item.id).unwrap_or_default();
    if payload.text.chars().count() > PREVIEW_TEXT_CHARS {
        let cut: String = payload.text.chars().take(PREVIEW_TEXT_CHARS).collect();
        payload.text = format!("{cut}\n\n… (truncated for preview)");
    }
    payload
}

pub fn open_preview(item: Arc<ClipItem>, app_icon: AppIcon, cx: &mut App) -> anyhow::Result<WindowHandle<Preview>> {
    let payload = load_payload(&item, cx);
    let screen = mac::screen::screen_under_mouse().or_else(|| mac::screen::screens().into_iter().next());
    let (sx, sy, sw, sh) = screen.map(|s| (s.x as f32, s.y as f32, s.width as f32, s.height as f32)).unwrap_or((0., 0., 1440., 900.));
    let w = (sw * 0.6).clamp(480., 1100.);
    let h = ((sh - SHELF_H) * 0.72).clamp(320., 760.);
    let x = sx + (sw - w) / 2.0;
    let y = sy + (sh - SHELF_H - h) / 2.0;
    let bounds = Bounds::new(point(px(x), px(y)), size(px(w), px(h)));
    let handle = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: None,
            focus: true,
            show: true,
            kind: WindowKind::PopUp,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            display_id: None,
            window_background: WindowBackgroundAppearance::Transparent,
            app_id: None,
            window_min_size: None,
            window_decorations: None,
            tabbing_identifier: None,
        },
        |window, cx| {
            let view = cx.new(|cx| {
                cx.observe_window_activation(window, |this: &mut Preview, window, cx| {
                    if !window.is_window_active() && !this.closing {
                        this.close(CloseAction::None, window, cx);
                    }
                })
                .detach();
                Preview {
                    item,
                    payload,
                    app_icon,
                    focus_handle: cx.focus_handle(),
                    scroll: ScrollHandle::new(),
                    closing: false,
                }
            });
            let handle = view.read(cx).focus_handle.clone();
            window.focus(&handle);
            view
        },
    )?;
    Ok(handle)
}

impl Preview {
    /// Swaps the previewed item in place (used when browsing with ← → or after a delete).
    pub fn set_item(&mut self, item: Arc<ClipItem>, payload: Payload, app_icon: AppIcon, cx: &mut Context<Self>) {
        self.item = item;
        self.payload = payload;
        self.app_icon = app_icon;
        self.scroll.set_offset(point(px(0.), px(0.)));
        cx.notify();
    }

    fn close(&mut self, action: CloseAction, window: &mut Window, cx: &mut Context<Self>) {
        if self.closing {
            return;
        }
        self.closing = true;
        window.remove_window();
        let shelf = cx.global::<Core>().shelf;
        cx.defer(move |cx| {
            if let Some(shelf) = shelf {
                let _ = shelf.update(cx, |shelf, window, cx| shelf.on_preview_closed(action, window, cx));
            }
        });
    }

    fn scroll_by(&mut self, dy: f32, cx: &mut Context<Self>) {
        // gpui clamps the offset to the content size during layout; only keep it from going above the top.
        let cur = self.scroll.offset();
        let next = (f32::from(cur.y) - dy).min(0.0);
        self.scroll.set_offset(point(cur.x, px(next)));
        cx.notify();
    }

    fn navigate(&mut self, delta: i64, cx: &mut Context<Self>) {
        let Some(shelf) = cx.global::<Core>().shelf else { return };
        let next = shelf.update(cx, |shelf, window, cx| shelf.preview_navigate(delta, window, cx)).ok().flatten();
        if let Some((item, payload, icon)) = next {
            self.set_item(item, payload, icon, cx);
        }
    }

    fn delete_current(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(shelf) = cx.global::<Core>().shelf else { return };
        let next = shelf.update(cx, |shelf, window, cx| shelf.preview_delete(window, cx)).ok().flatten();
        match next {
            Some((item, payload, icon)) => self.set_item(item, payload, icon, cx),
            None => self.close(CloseAction::None, window, cx),
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        let ks = &event.keystroke;
        let m = ks.modifiers;
        let page = f32::from(window.viewport_size().height) * 0.8;
        match ks.key.as_str() {
            "escape" | "space" => self.close(CloseAction::None, window, cx),
            "y" if m.platform => self.close(CloseAction::None, window, cx),
            "enter" => self.close(CloseAction::Paste { plain: m.shift }, window, cx),
            "c" if m.platform => self.close(CloseAction::Copy, window, cx),
            "o" if m.platform && self.item.kind == ItemKind::Link => {
                mac::workspace::open_url(self.item.preview.trim());
                self.close(CloseAction::None, window, cx);
            }
            "backspace" if m.platform => self.delete_current(window, cx),
            "delete" => self.delete_current(window, cx),
            "left" => self.navigate(-1, cx),
            "right" => self.navigate(1, cx),
            "up" => self.scroll_by(-SCROLL_LINE, cx),
            "down" => self.scroll_by(SCROLL_LINE, cx),
            "pageup" => self.scroll_by(-page, cx),
            "pagedown" => self.scroll_by(page, cx),
            "home" => self.scroll_by(-1e9, cx),
            "end" => self.scroll_by(1e9, cx),
            _ => {}
        }
    }

    fn render_content(&self, theme: &Theme, viewport_h: f32) -> AnyElement {
        let item = &self.item;
        match item.kind {
            ItemKind::Image => {
                let path = item.image_path.clone().or_else(|| item.thumb_path.clone());
                let t = *theme;
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .p(px(16.))
                    .children(path.map(|p| {
                        img(p)
                            .max_w_full()
                            .max_h_full()
                            .object_fit(ObjectFit::Contain)
                            .rounded(px(6.))
                            .with_loading(move || div().size(px(160.)).child(crate::ui::card::image_placeholder(&t, "image", true)).into_any_element())
                    }))
                    .into_any_element()
            }
            ItemKind::Color => {
                let hex = item.color.clone().unwrap_or_default();
                let color = hex_to_hsla(&hex);
                let rgba = util::hex_to_rgba(&hex).unwrap_or((0., 0., 0., 1.));
                div()
                    .size_full()
                    .bg(color)
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(8.))
                    .text_color(contrast_text(color))
                    .child(div().text_size(px(34.)).font_weight(FontWeight::BOLD).child(SharedString::from(hex)))
                    .child(div().text_size(px(14.)).opacity(0.8).child(SharedString::from(format!(
                        "rgb({}, {}, {})",
                        (rgba.0 * 255.).round(),
                        (rgba.1 * 255.).round(),
                        (rgba.2 * 255.).round()
                    ))))
                    .into_any_element()
            }
            ItemKind::File => div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(14.))
                .p(px(24.))
                .children(item.thumb_path.clone().map(|p| img(p).max_w(px(220.)).max_h(px(220.)).object_fit(ObjectFit::Contain)))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .items_center()
                        .children(item.files.iter().take(12).map(|f| {
                            div().text_size(px(13.)).text_color(theme.text).child(SharedString::from(f.to_string_lossy().to_string()))
                        }))
                        .when(item.files.len() > 12, |d| {
                            d.child(div().text_size(px(12.)).text_color(theme.text_secondary).child(SharedString::from(format!("and {} more", item.files.len() - 12))))
                        }),
                )
                .into_any_element(),
            ItemKind::Link => div()
                .size_full()
                .p(px(28.))
                .flex()
                .flex_col()
                .gap(px(12.))
                .children(item.link_image_path.clone().map(|p| {
                    let t = *theme;
                    let h = (viewport_h * 0.42).clamp(120., 360.);
                    div().h(px(h)).w_full().flex_shrink_0().rounded(px(10.)).overflow_hidden().child(
                        img(p)
                            .size_full()
                            .object_fit(ObjectFit::Cover)
                            .with_loading(move || crate::ui::card::image_placeholder(&t, "image", true)),
                    )
                }))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(8.))
                        .children(item.favicon_path.clone().map(|p| img(p).size(px(20.)).rounded(px(4.))))
                        .child(div().text_size(px(13.)).text_color(theme.text_secondary).child(SharedString::from(item.domain().unwrap_or_default()))),
                )
                .child(
                    div()
                        .text_size(px(22.))
                        .line_height(px(28.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text)
                        .child(SharedString::from(item.title.clone().unwrap_or_else(|| item.preview.clone()))),
                )
                .child(div().text_size(px(13.)).line_height(px(18.)).text_color(theme.accent).child(SharedString::from(item.preview.clone())))
                .child(div().flex_1())
                .child(div().text_size(px(12.)).text_color(theme.text_tertiary).child("⌘O opens the link in your browser"))
                .into_any_element(),
            ItemKind::Text | ItemKind::RichText => div()
                .id("preview-scroll")
                .size_full()
                .overflow_y_scroll()
                .track_scroll(&self.scroll)
                .p(px(24.))
                .child(
                    // No explicit line height: gpui measures scroll content with the font's
                    // default spacing, and a custom value would make the scroll range too short.
                    div()
                        .text_size(px(14.))
                        .text_color(theme.text)
                        .child(SharedString::from(self.payload.text.clone())),
                )
                .into_any_element(),
        }
    }
}

impl Render for Preview {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pref = cx.global::<Core>().settings.read().unwrap().theme;
        let theme = Theme::for_window(window, pref);
        let item = self.item.clone();
        let meta = match item.kind {
            ItemKind::Text | ItemKind::RichText => util::count_label(item.char_count, "character", "characters"),
            ItemKind::Image => format!("{} × {} · {}", item.width, item.height, util::format_bytes(item.byte_size)),
            ItemKind::File => util::format_bytes(item.byte_size),
            ItemKind::Link => item.domain().unwrap_or_default(),
            ItemKind::Color => item.color.clone().unwrap_or_default(),
        };
        let bg = if theme.dark { hsla(0., 0., 0.11, 0.97) } else { hsla(0., 0., 0.98, 0.98) };
        div()
            .id("preview-root")
            .size_full()
            .track_focus(&self.focus_handle)
            .key_context("Preview")
            .on_key_down(cx.listener(Self::on_key_down))
            .p(px(10.))
            .child(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .rounded(px(16.))
                    .overflow_hidden()
                    .bg(bg)
                    .border_1()
                    .border_color(theme.menu_border)
                    .shadow(vec![BoxShadow {
                        color: hsla(0., 0., 0., 0.45),
                        offset: point(px(0.), px(12.)),
                        blur_radius: px(40.),
                        spread_radius: px(0.),
                    }])
                    .child(
                        div()
                            .h(px(52.))
                            .flex_shrink_0()
                            .px(px(16.))
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(10.))
                            .border_b_1()
                            .border_color(theme.separator)
                            .child(div().size(px(28.)).rounded(px(7.)).bg(self.app_icon.tint).flex().items_center().justify_center().child(app_icon_view(&self.app_icon, 26.)))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .child(div().text_size(px(13.5)).font_weight(FontWeight::SEMIBOLD).text_color(theme.text).child(SharedString::from(
                                        if item.app_name.is_empty() { item.kind.label().to_string() } else { format!("{} · {}", item.kind.label(), item.app_name) },
                                    )))
                                    .child(div().text_size(px(11.5)).text_color(theme.text_secondary).child(SharedString::from(format!(
                                        "{} · {}",
                                        util::absolute_time(item.created_at),
                                        meta
                                    )))),
                            )
                            .child(div().flex_1())
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .text_color(theme.text_tertiary)
                                    .child(match item.kind {
                                        ItemKind::Text | ItemKind::RichText => "← → Browse   ↑ ↓ Scroll   ↩ Paste   Space Close",
                                        _ => "← → Browse   ↩ Paste   Space Close",
                                    }),
                            ),
                    )
                    .child(div().flex_1().min_h_0().child(self.render_content(&theme, f32::from(window.viewport_size().height)))),
            )
    }
}
