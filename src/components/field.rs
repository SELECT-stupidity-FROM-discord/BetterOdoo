use crate::input::TextInput;
use crate::scale::px;
use crate::theme::*;
use gpui::{App, Entity, Focusable, IntoElement, RenderOnce, Window, div, prelude::*, rgb};

#[derive(IntoElement)]
pub struct Field {
    input: Entity<TextInput>,
    height: f32,
}

impl Field {
    pub fn new(input: &Entity<TextInput>) -> Self {
        Self {
            input: input.clone(),
            height: 30.,
        }
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }
}

impl RenderOnce for Field {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let focused = self.input.focus_handle(cx).is_focused(window);
        div()
            .flex()
            .items_center()
            .px(px(10.))
            .min_h(px(self.height))
            .rounded(px(6.))
            .border_1()
            .border_color(rgb(if focused { MUTED } else { BORDER }))
            .bg(rgb(PAGE))
            .child(self.input)
    }
}
