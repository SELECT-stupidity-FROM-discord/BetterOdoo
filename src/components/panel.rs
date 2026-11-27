use crate::scale::px;
use crate::theme::*;
use gpui::{
    AnyElement, App, FontWeight, IntoElement, ParentElement, RenderOnce, SharedString, Window, div,
    prelude::*, rgb,
};

use super::icon;

#[derive(Clone, Copy, PartialEq)]
pub enum Tint {
    Neutral,
    Ai,
}

#[derive(IntoElement)]
pub struct Card {
    title: SharedString,
    tint: Tint,
    error: Option<SharedString>,
    children: Vec<AnyElement>,
}

impl Card {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            tint: Tint::Neutral,
            error: None,
            children: Vec::new(),
        }
    }

    pub fn tint(mut self, tint: Tint) -> Self {
        self.tint = tint;
        self
    }

    pub fn error(mut self, error: Option<SharedString>) -> Self {
        self.error = error;
        self
    }
}

impl ParentElement for Card {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Card {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let ai = self.tint == Tint::Ai;
        let fg = if ai { PURPLE.fg } else { TEXT_SOFT };
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .p(px(14.))
            .rounded(px(8.))
            .bg(rgb(if ai { PURPLE.bg } else { SIDEBAR }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(fg))
                    .when(ai, |d| d.child(icon("sparkle", 13., fg)))
                    .child(self.title),
            )
            .when_some(self.error, |d, e| {
                d.child(div().text_size(px(13.)).text_color(rgb(DANGER)).child(e))
            })
            .children(self.children)
    }
}

#[derive(IntoElement, Default)]
pub struct Panel {
    children: Vec<AnyElement>,
}

impl Panel {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ParentElement for Panel {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Panel {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .rounded(px(8.))
            .border_1()
            .border_color(rgb(BORDER))
            .overflow_hidden()
            .children(self.children)
    }
}

#[derive(IntoElement)]
pub struct Section {
    title: SharedString,
    note: SharedString,
    children: Vec<AnyElement>,
}

impl Section {
    pub fn new(title: impl Into<SharedString>, note: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            note: note.into(),
            children: Vec::new(),
        }
    }
}

impl ParentElement for Section {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Section {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .text_size(px(18.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(self.title),
                    )
                    .child(div().text_color(rgb(MUTED)).child(self.note)),
            )
            .children(self.children)
    }
}
