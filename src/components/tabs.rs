use crate::theme::*;
use gpui::{
    App, ClickEvent, FontWeight, IntoElement, RenderOnce, SharedString, Window, div, prelude::*,
    px, rgb, white,
};

struct Item {
    label: SharedString,
    active: bool,
    on_click: super::OnClick,
}

#[derive(IntoElement, Default)]
pub struct TabBar {
    items: Vec<Item>,
}

impl TabBar {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tab(
        mut self,
        label: impl Into<SharedString>,
        active: bool,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.items.push(Item {
            label: label.into(),
            active,
            on_click: Box::new(on_click),
        });
        self
    }
}

impl RenderOnce for TabBar {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div()
            .flex()
            .gap(px(22.))
            .border_b_1()
            .border_color(rgb(BORDER))
            .children(self.items.into_iter().enumerate().map(|(i, t)| {
                div()
                    .id(("tab", i))
                    .px(px(2.))
                    .pb(px(8.))
                    .cursor_pointer()
                    .border_b_2()
                    .border_color(rgb(if t.active { TEXT } else { PAGE }))
                    .text_color(rgb(if t.active { TEXT } else { MUTED }))
                    .font_weight(if t.active {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::NORMAL
                    })
                    .child(t.label)
                    .on_click(t.on_click)
            }))
    }
}

#[derive(IntoElement, Default)]
pub struct Segmented {
    items: Vec<Item>,
}

impl Segmented {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn item(
        mut self,
        label: impl Into<SharedString>,
        active: bool,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.items.push(Item {
            label: label.into(),
            active,
            on_click: Box::new(on_click),
        });
        self
    }
}

impl RenderOnce for Segmented {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div()
            .flex()
            .flex_wrap()
            .gap(px(2.))
            .p(px(3.))
            .rounded(px(8.))
            .bg(rgb(SIDEBAR))
            .border_1()
            .border_color(rgb(BORDER))
            .children(self.items.into_iter().enumerate().map(|(i, t)| {
                div()
                    .id(("segment", i))
                    .px(px(12.))
                    .py(px(5.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .text_size(px(13.))
                    .when(t.active, |d| {
                        d.bg(rgb(ACCENT))
                            .text_color(white())
                            .font_weight(FontWeight::MEDIUM)
                    })
                    .when(!t.active, |d| {
                        d.text_color(rgb(TEXT_SOFT)).hover(|s| s.bg(rgb(HOVER)))
                    })
                    .child(t.label)
                    .on_click(t.on_click)
            }))
    }
}
