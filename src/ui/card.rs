use crate::assets::icon;
use crate::core::AppIcon;
use crate::model::{ClipItem, ItemKind};
use crate::ui::theme::{contrast_text, hex_to_hsla, white_alpha, Theme};
use crate::util;
use gpui::{prelude::*, *};

pub const CARD_W: f32 = 196.0;
pub const CARD_H: f32 = 228.0;
pub const CARD_GAP: f32 = 14.0;
pub const HEADER_H: f32 = 44.0;
pub const FOOTER_H: f32 = 24.0;
pub const CARD_RADIUS: f32 = 12.0;

pub fn app_icon_view(app_icon: &AppIcon, size: f32) -> AnyElement {
    match &app_icon.path {
        Some(path) => img(path.clone()).size(px(size)).flex_shrink_0().into_any_element(),
        None => svg()
            .path(icon("clipboard"))
            .size(px(size - 4.0))
            .text_color(white_alpha(0.9))
            .flex_shrink_0()
            .into_any_element(),
    }
}

pub fn caption(item: &ClipItem) -> String {
    match item.kind {
        ItemKind::Text | ItemKind::RichText => util::count_label(item.char_count, "character", "characters"),
        ItemKind::Link => item.domain().unwrap_or_else(|| item.preview.clone()),
        ItemKind::Color => item
            .color
            .as_deref()
            .and_then(util::hex_to_rgba)
            .map(|(r, g, b, _)| {
                format!("RGB {}, {}, {}", (r * 255.0).round(), (g * 255.0).round(), (b * 255.0).round())
            })
            .unwrap_or_default(),
        ItemKind::Image => format!("{} × {}", item.width, item.height),
        ItemKind::File => {
            if item.files.len() == 1 {
                util::format_bytes(item.byte_size)
            } else {
                util::count_label(item.files.len() as i64, "item", "items")
            }
        }
    }
}

/// Neutral block shown while an image decodes (or if it failed to load).
pub fn image_placeholder(theme: &Theme, icon_name: &'static str, pulse: bool) -> AnyElement {
    let bg = if theme.dark { hsla(0., 0., 1., 0.06) } else { hsla(0., 0., 0., 0.05) };
    let base = div()
        .size_full()
        .bg(bg)
        .flex()
        .items_center()
        .justify_center()
        .child(svg().path(icon(icon_name)).size(px(26.)).text_color(theme.text_tertiary));
    if pulse {
        base.with_animation(
            "placeholder-pulse",
            Animation::new(std::time::Duration::from_millis(1400)).repeat().with_easing(pulsating_between(0.45, 1.0)),
            |d, t| d.opacity(t),
        )
        .into_any_element()
    } else {
        base.into_any_element()
    }
}

fn link_meta_rows(item: &ClipItem, theme: &Theme, compact: bool) -> Div {
    let title = item.title.clone().unwrap_or_else(|| item.preview.clone());
    let domain = item.domain().unwrap_or_default();
    div()
        .flex()
        .flex_col()
        .gap(px(4.))
        .min_h_0()
        .overflow_hidden()
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(6.))
                .child(match &item.favicon_path {
                    Some(p) => img(p.clone()).size(px(14.)).rounded(px(3.)).flex_shrink_0().into_any_element(),
                    None => svg().path(icon("link")).size(px(13.)).text_color(theme.text_secondary).flex_shrink_0().into_any_element(),
                })
                .child(div().text_size(px(11.)).text_color(theme.text_secondary).truncate().child(SharedString::from(domain))),
        )
        .child(
            div()
                .text_size(px(12.5))
                .line_height(px(16.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.text)
                .line_clamp(if compact { 2 } else { 4 })
                .overflow_hidden()
                .child(SharedString::from(title)),
        )
        .when(!compact && item.title.is_some(), |d| {
            d.child(
                div()
                    .text_size(px(11.))
                    .line_height(px(14.))
                    .text_color(theme.text_tertiary)
                    .line_clamp(2)
                    .overflow_hidden()
                    .child(SharedString::from(item.preview.clone())),
            )
        })
}

pub fn render_body(item: &ClipItem, theme: &Theme, link_pending: bool) -> AnyElement {
    match item.kind {
        ItemKind::Text | ItemKind::RichText => div()
            .size_full()
            .p(px(12.))
            .text_size(px(12.))
            .line_height(px(16.))
            .text_color(theme.text)
            .overflow_hidden()
            .child(SharedString::from(item.preview.clone()))
            .into_any_element(),
        ItemKind::Link => {
            let t = *theme;
            if let Some(image) = &item.link_image_path {
                // Rich preview: image on top, title + domain below (like Paste's link cards).
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .child(
                        div().h(px(92.)).w_full().flex_shrink_0().overflow_hidden().child(
                            img(image.clone())
                                .size_full()
                                .object_fit(ObjectFit::Cover)
                                .with_loading(move || image_placeholder(&t, "image", true))
                                .with_fallback(move || image_placeholder(&t, "link", false)),
                        ),
                    )
                    .child(div().flex_1().min_h_0().px(px(12.)).pt(px(8.)).child(link_meta_rows(item, theme, true)))
                    .into_any_element()
            } else if link_pending {
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .child(div().h(px(92.)).w_full().flex_shrink_0().child(image_placeholder(theme, "link", true)))
                    .child(div().flex_1().min_h_0().px(px(12.)).pt(px(8.)).child(link_meta_rows(item, theme, true)))
                    .into_any_element()
            } else {
                div().size_full().p(px(12.)).child(link_meta_rows(item, theme, false)).into_any_element()
            }
        }
        ItemKind::Color => {
            let hex = item.color.clone().unwrap_or_default();
            let color = hex_to_hsla(&hex);
            div()
                .size_full()
                .bg(color)
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .px(px(10.))
                        .py(px(4.))
                        .rounded(px(7.))
                        .bg(if theme.dark { hsla(0., 0., 0., 0.35) } else { hsla(0., 0., 1., 0.6) })
                        .text_size(px(13.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(contrast_text(color))
                        .child(SharedString::from(hex)),
                )
                .into_any_element()
        }
        ItemKind::Image => {
            let t = *theme;
            match &item.thumb_path {
                Some(p) => div()
                    .size_full()
                    .bg(theme.card_bg)
                    .child(
                        img(p.clone())
                            .size_full()
                            .object_fit(ObjectFit::Cover)
                            .with_loading(move || image_placeholder(&t, "image", true))
                            .with_fallback(move || image_placeholder(&t, "image", false)),
                    )
                    .into_any_element(),
                None => image_placeholder(theme, "image", false),
            }
        }
        ItemKind::File => {
            let name = item.file_label();
            div()
                .size_full()
                .p(px(12.))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(8.))
                .child(match &item.thumb_path {
                    Some(p) => img(p.clone())
                        .max_w(px(96.))
                        .max_h(px(84.))
                        .object_fit(ObjectFit::Contain)
                        .into_any_element(),
                    None => svg().path(icon("file")).size(px(48.)).text_color(theme.text_secondary).into_any_element(),
                })
                .child(
                    div()
                        .text_size(px(12.))
                        .line_height(px(15.))
                        .text_color(theme.text)
                        .text_center()
                        .line_clamp(2)
                        .overflow_hidden()
                        .child(SharedString::from(name)),
                )
                .into_any_element()
        }
    }
}

pub fn render_card(
    idx: usize,
    item: &ClipItem,
    app_icon: &AppIcon,
    theme: &Theme,
    selected: bool,
    hovered: bool,
    now: i64,
    link_pending: bool,
) -> Stateful<Div> {
    let tint = app_icon.tint;
    let header = div()
        .h(px(HEADER_H))
        .flex_shrink_0()
        .rounded_t(px(CARD_RADIUS))
        .bg(tint)
        .px(px(12.))
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap(px(8.))
        .child(
            div()
                .flex()
                .flex_col()
                .min_w_0()
                .child(
                    div()
                        .text_size(px(13.))
                        .line_height(px(16.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(white())
                        .child(item.kind.label()),
                )
                .child(
                    div()
                        .text_size(px(10.5))
                        .line_height(px(13.))
                        .text_color(white_alpha(0.78))
                        .truncate()
                        .child(SharedString::from(util::relative_time(item.created_at, now))),
                ),
        )
        .child(app_icon_view(app_icon, 24.));

    let body = div().flex_1().min_h_0().w_full().overflow_hidden().child(render_body(item, theme, link_pending));

    let footer = div()
        .h(px(FOOTER_H))
        .flex_shrink_0()
        .rounded_b(px(CARD_RADIUS))
        .px(px(10.))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(10.5))
        .text_color(theme.text_tertiary)
        .child(div().truncate().child(SharedString::from(caption(item))));

    // No border: a soft drop shadow separates the card from the shelf, and a hairline ring keeps
    // it crisp on dark backgrounds. Selection draws a solid accent ring outside the card, so the
    // header and body always reach the card's rounded edge.
    let mut shadows = vec![
        BoxShadow {
            color: theme.card_border,
            offset: point(px(0.), px(0.)),
            blur_radius: px(0.),
            spread_radius: px(0.5),
        },
        BoxShadow {
            color: theme.shadow,
            offset: point(px(0.), px(if hovered { 5. } else { 2. })),
            blur_radius: px(if hovered { 16. } else { 6. }),
            spread_radius: px(0.),
        },
    ];
    if selected {
        shadows.push(BoxShadow {
            color: theme.accent,
            offset: point(px(0.), px(0.)),
            blur_radius: px(0.),
            spread_radius: px(2.5),
        });
        shadows.push(BoxShadow {
            color: theme.accent.opacity(0.25),
            offset: point(px(0.), px(0.)),
            blur_radius: px(0.),
            spread_radius: px(5.),
        });
    }

    div()
        .id(("card", idx))
        .w(px(CARD_W))
        .h(px(CARD_H))
        .flex_shrink_0()
        .flex()
        .flex_col()
        .rounded(px(CARD_RADIUS))
        .overflow_hidden()
        .bg(theme.card_bg)
        .shadow(shadows)
        .cursor_pointer()
        .child(header)
        .child(body)
        .child(footer)
}

/// Small ghost rendered while dragging a card onto a pinboard.
pub struct DragGhost {
    pub label: SharedString,
    pub tint: Hsla,
}

impl Render for DragGhost {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(12.))
            .py(px(6.))
            .rounded(px(8.))
            .bg(self.tint)
            .text_size(px(12.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(white())
            .shadow_md()
            .child(self.label.clone())
    }
}
