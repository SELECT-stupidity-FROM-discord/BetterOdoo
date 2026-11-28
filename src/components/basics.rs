use crate::scale::px;
use crate::theme::Tone;
use crate::theme::{BORDER, FONT, PAGE, SIDEBAR, TEXT, TEXT_SOFT};
use gpui::{
    App, BoxShadow, ClickEvent, Div, FontWeight, MouseDownEvent, SharedString, Svg, Window, black,
    div, point, prelude::*, rgb, svg,
};

pub type OnClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
pub type OnPress = Box<dyn Fn(&MouseDownEvent, &mut Window, &mut App)>;

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

pub fn kbd(key: &str) -> Div {
    div()
        .px(px(6.))
        .py(px(1.))
        .min_w(px(20.))
        .flex()
        .justify_center()
        .rounded(px(4.))
        .border_1()
        .border_color(rgb(BORDER))
        .bg(rgb(SIDEBAR))
        .text_size(px(11.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(TEXT_SOFT))
        .child(key.to_string())
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
        .rounded(px(12.))
        .bg(rgb(PAGE))
        .border_1()
        .border_color(rgb(BORDER))
        .text_size(px(14.))
        .text_color(rgb(TEXT))
        .font_family(FONT)
        .shadow(vec![
            shadow(0.04, 1., 2., 0.),
            shadow(0.08, 8., 24., -4.),
            shadow(0.08, 16., 36., -8.),
        ])
}
