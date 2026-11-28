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
            height: 34.,
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
            .px(px(11.))
            .h(px(self.height))
            .rounded(px(8.))
            .border_1()
            .border_color(rgb(if focused { ACCENT } else { BORDER }))
            .bg(rgb(if focused { PAGE } else { SIDEBAR }))
            .when(!focused, |d| d.hover(|s| s.border_color(rgb(BORDER_STRONG))))
            .text_size(px(13.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .child(self.input),
            )
    }
}
