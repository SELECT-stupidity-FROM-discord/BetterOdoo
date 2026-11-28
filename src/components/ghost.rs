use super::icon;
use crate::input::TextInput;
use crate::scale::px;
use crate::theme::*;
use gpui::{App, Entity, Focusable, IntoElement, RenderOnce, Window, div, prelude::*, rgb};

#[derive(IntoElement)]
pub struct GhostInput {
    input: Entity<TextInput>,
    icon: &'static str,
}

impl GhostInput {
    pub fn new(input: &Entity<TextInput>, icon: &'static str) -> Self {
        Self {
            input: input.clone(),
            icon,
        }
    }
}

impl RenderOnce for GhostInput {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let focused = self.input.focus_handle(cx).is_focused(window);
        div()
            .flex()
            .items_center()
            .gap(px(10.))
            .px(px(14.))
            .min_h(px(42.))
            .rounded(px(10.))
            .border_1()
            .when(!focused, |d| {
                d.border_color(rgb(BORDER))
                    .bg(rgb(SIDEBAR))
                    .hover(|s| s.border_color(rgb(BORDER_STRONG)))
            })
            .when(focused, |d| d.border_color(rgb(ACCENT)).bg(rgb(PAGE)))
            .child(icon(self.icon, 15., if focused { ACCENT } else { MUTED }))
            .child(div().flex_1().min_w_0().overflow_hidden().child(self.input))
    }
}
