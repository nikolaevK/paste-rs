use crate::assets::icon;
use crate::ui::theme::Theme;
use gpui::{prelude::*, *};

pub fn icon_button(id: impl Into<ElementId>, name: &str, size: f32, theme: &Theme) -> Stateful<Div> {
    let hover = theme.pill_hover;
    div()
        .id(id)
        .size(px(size + 12.0))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .text_color(theme.text_secondary)
        .hover(move |s| s.bg(hover))
        .child(svg().path(icon(name)).size(px(size)).text_color(theme.text_secondary))
}

pub fn toggle(id: impl Into<ElementId>, on: bool, theme: &Theme) -> Stateful<Div> {
    let track = if on { theme.accent } else if theme.dark { hsla(0., 0., 0.32, 1.) } else { hsla(0., 0., 0.82, 1.) };
    div()
        .id(id)
        .w(px(38.))
        .h(px(22.))
        .rounded_full()
        .bg(track)
        .cursor_pointer()
        .relative()
        .child(
            div()
                .absolute()
                .top(px(2.))
                .left(px(if on { 18. } else { 2. }))
                .size(px(18.))
                .rounded_full()
                .bg(white())
                .shadow(vec![BoxShadow {
                    color: hsla(0., 0., 0., 0.25),
                    offset: point(px(0.), px(1.)),
                    blur_radius: px(2.),
                    spread_radius: px(0.),
                }]),
        )
}

pub fn button(id: impl Into<ElementId>, label: impl Into<SharedString>, theme: &Theme, primary: bool) -> Stateful<Div> {
    let (bg, fg, border) = if primary {
        (theme.accent, white(), theme.accent)
    } else {
        (theme.control_bg, theme.text, theme.control_border)
    };
    let hover_bg = if primary { theme.accent.opacity(0.85) } else { theme.pill_hover.blend(theme.control_bg) };
    div()
        .id(id)
        .h(px(26.))
        .px(px(12.))
        .rounded(px(7.))
        .bg(bg)
        .border_1()
        .border_color(border)
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(12.5))
        .text_color(fg)
        .cursor_pointer()
        .hover(move |s| s.bg(hover_bg))
        .child(label.into())
}

pub fn segmented<F>(id_prefix: &'static str, options: &[&str], selected: usize, theme: &Theme, on_select: F) -> Div
where
    F: Fn(usize, &mut Window, &mut App) + 'static + Clone,
{
    let mut root = div()
        .flex()
        .flex_row()
        .rounded(px(7.))
        .bg(theme.control_border.opacity(0.35))
        .p(px(2.))
        .gap(px(2.));
    for (i, opt) in options.iter().enumerate() {
        let is_sel = i == selected;
        let cb = on_select.clone();
        root = root.child(
            div()
                .id((id_prefix, i))
                .px(px(10.))
                .h(px(22.))
                .rounded(px(5.))
                .flex()
                .items_center()
                .text_size(px(12.))
                .cursor_pointer()
                .when(is_sel, |d| d.bg(theme.control_bg).shadow_sm())
                .text_color(if is_sel { theme.text } else { theme.text_secondary })
                .on_click(move |_, window, cx| cb(i, window, cx))
                .child(SharedString::from(opt.to_string())),
        );
    }
    root
}

pub fn pref_row(label: impl Into<SharedString>, description: Option<&str>, control: impl IntoElement, theme: &Theme) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .py(px(10.))
        .gap(px(16.))
        .border_b_1()
        .border_color(theme.separator)
        .child(
            // The label column takes the remaining width and wraps, so controls always stay visible.
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(div().text_size(px(13.)).text_color(theme.text).child(label.into()))
                .when_some(description, |d, desc| {
                    d.child(
                        div()
                            .text_size(px(11.5))
                            .line_height(px(15.))
                            .text_color(theme.text_secondary)
                            .child(SharedString::from(desc.to_string())),
                    )
                }),
        )
        .child(div().flex_shrink_0().child(control))
}

pub fn section_title(title: &'static str, theme: &Theme) -> Div {
    div()
        .pt(px(6.))
        .pb(px(4.))
        .text_size(px(11.5))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme.text_tertiary)
        .child(title.to_uppercase())
}
