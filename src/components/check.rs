use super::icon;
use crate::theme::*;
use gpui::{App, IntoElement, RenderOnce, Window, div, prelude::*, px, rgb};

#[derive(IntoElement)]
pub struct Checkbox {
    on: bool,
}

impl Checkbox {
    pub fn new(on: bool) -> Self {
        Self { on }
    }
}

impl RenderOnce for Checkbox {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div()
            .flex_none()
            .size(px(18.))
            .rounded(px(5.))
            .flex()
            .items_center()
            .justify_center()
            .when(self.on, |d| {
                d.bg(rgb(CHECK)).child(icon("check", 12., 0xffffff))
            })
            .when(!self.on, |d| {
                d.border_1().border_color(rgb(BORDER_STRONG)).bg(rgb(PAGE))
            })
    }
}

#[derive(IntoElement)]
pub struct Radio {
    on: bool,
}

impl Radio {
    pub fn new(on: bool) -> Self {
        Self { on }
    }
}

impl RenderOnce for Radio {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let colour = if self.on { PURPLE.fg } else { MUTED };
        div()
            .flex_none()
            .size(px(16.))
            .rounded_full()
            .border_1()
            .border_color(rgb(colour))
            .flex()
            .items_center()
            .justify_center()
            .when(self.on, |d| {
                d.child(div().size(px(8.)).rounded_full().bg(rgb(colour)))
            })
    }
}
