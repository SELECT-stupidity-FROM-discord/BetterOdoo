use crate::scale::px;
use crate::theme::*;
use gpui::{
    App, ClickEvent, ElementId, FontWeight, IntoElement, MouseButton, MouseDownEvent, RenderOnce,
    SharedString, Window, div, prelude::*, rgb,
};

#[derive(IntoElement)]
pub struct Chip {
    id: ElementId,
    label: SharedString,
    selected: bool,
    open: bool,
    on_click: Option<super::OnClick>,
    on_press: Option<super::OnPress>,
}

impl Chip {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            selected: false,
            open: false,
            on_click: None,
            on_press: None,
        }
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Box::new(f));
        self
    }

    pub fn on_press(
        mut self,
        f: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_press = Some(Box::new(f));
        self
    }
}

impl RenderOnce for Chip {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let on = self.selected;
        div()
            .id(self.id)
            .px(px(12.))
            .py(px(5.))
            .rounded_full()
            .border_1()
            .cursor_pointer()
            .text_size(px(13.))
            .font_weight(FontWeight::MEDIUM)
            .border_color(rgb(if on { TEXT } else { BORDER }))
            .bg(rgb(if on { TEXT } else { PAGE }))
            .text_color(rgb(if on { PAGE } else { TEXT_SOFT }))
            .when(!on, |d| d.hover(|s| s.bg(rgb(HOVER))))
            .when(self.open, |d| d.bg(rgb(HOVER)))
            .child(self.label)
            .when_some(self.on_click, |d, f| d.on_click(f))
            .when_some(self.on_press, |d, f| d.on_mouse_down(MouseButton::Left, f))
    }
}
