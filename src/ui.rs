use crate::theme::Tone;
use crate::theme::{FONT, PAGE, TEXT};
use gpui::{
    BoxShadow, Div, FontWeight, SharedString, Svg, black, div, point, prelude::*, px, rgb, svg,
};

pub fn icon(name: &str, size: f32, color: u32) -> Svg {
    svg()
        .path(format!("icons/{name}.svg"))
        .size(px(size))
        .flex_none()
        .text_color(rgb(color))
}

pub fn tag(text: impl Into<SharedString>, tone: Tone, text_size: f32) -> Div {
    div()
        .flex_none()
        .px(px(8.))
        .py(px(2.))
        .rounded(px(4.))
        .text_size(px(text_size))
        .font_weight(FontWeight::MEDIUM)
        .bg(rgb(tone.bg))
        .text_color(rgb(tone.fg))
        .child(text.into())
}

pub fn popover() -> Div {
    let shadow = |alpha: f32, y: f32, blur: f32, spread: f32| BoxShadow {
        color: black().opacity(alpha),
        offset: point(px(0.), px(y)),
        blur_radius: px(blur),
        spread_radius: px(spread),
        inset: false,
    };
    div()
        .rounded(px(8.))
        .bg(rgb(PAGE))
        .text_size(px(14.))
        .text_color(rgb(TEXT))
        .font_family(FONT)
        .shadow(vec![
            shadow(0.05, 0., 0., 1.),
            shadow(0.1, 3., 6., 0.),
            shadow(0.2, 9., 24., 0.),
        ])
}
