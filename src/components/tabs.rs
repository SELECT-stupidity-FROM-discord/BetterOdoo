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
    trailing: Option<gpui::AnyElement>,
}

impl TabBar {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn trailing(mut self, element: Option<impl IntoElement>) -> Self {
        self.trailing = element.map(IntoElement::into_any_element);
        self
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
            .items_end()
            .justify_between()
            .gap(px(22.))
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .flex()
                    .gap(px(22.))
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
                    })),
            )
            .children(self.trailing.map(|t| div().pb(px(6.)).child(t)))
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
        let current = self.items.iter().position(|t| t.active);
        div()
            .flex()
            .flex_wrap()
            .gap(px(6.))
            .children(self.items.into_iter().enumerate().map(|(i, t)| {
                let reached = current.is_some_and(|c| i < c);
                div()
                    .id(("segment", i))
                    .px(px(14.))
                    .py(px(7.))
                    .rounded(px(6.))
                    .border_1()
                    .cursor_pointer()
                    .text_size(px(13.))
                    .font_weight(FontWeight::MEDIUM)
                    .when(t.active, |d| {
                        d.bg(rgb(ACCENT))
                            .border_color(rgb(ACCENT))
                            .text_color(white())
                    })
                    .when(reached, |d| {
                        d.bg(rgb(BLUE.bg))
                            .border_color(rgb(BLUE.bg))
                            .text_color(rgb(BLUE.fg))
                            .hover(|s| s.bg(rgb(0xd6e6f6)))
                    })
                    .when(!t.active && !reached, |d| {
                        d.bg(rgb(PAGE))
                            .border_color(rgb(BORDER))
                            .text_color(rgb(TEXT_SOFT))
                            .hover(|s| s.bg(rgb(HOVER)))
                    })
                    .child(t.label)
                    .on_click(t.on_click)
            }))
    }
}
