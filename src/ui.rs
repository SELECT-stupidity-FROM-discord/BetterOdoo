use crate::theme::Tone;
use gpui::{Div, FontWeight, SharedString, Svg, div, prelude::*, px, rgb, svg};

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
