use crate::scale::px;
use crate::theme::*;
use gpui::{
    AnyElement, App, BoxShadow, FontWeight, IntoElement, ParentElement, RenderOnce, SharedString,
    Window, black, div, point, prelude::*, rgb,
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
    badge: Option<SharedString>,
    trailing: Option<AnyElement>,
    tint: Tint,
    error: Option<SharedString>,
    children: Vec<AnyElement>,
}

impl Card {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            badge: None,
            trailing: None,
            tint: Tint::Neutral,
            error: None,
            children: Vec::new(),
        }
    }

    pub fn badge(mut self, badge: impl Into<SharedString>) -> Self {
        self.badge = Some(badge.into());
        self
    }

    pub fn trailing(mut self, el: impl IntoElement) -> Self {
        self.trailing = Some(el.into_any_element());
        self
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
        let fg = if ai { PURPLE.fg } else { TEXT };
        let shadow = |alpha: f32, y: f32, blur: f32, spread: f32| BoxShadow {
            color: black().opacity(alpha),
            offset: point(px(0.), px(y)),
            blur_radius: px(blur),
            spread_radius: px(spread),
            inset: false,
        };
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .p(px(18.))
            .rounded(px(12.))
            .border_1()
            .border_color(rgb(if ai { AI_BORDER } else { BORDER }))
            .bg(rgb(if ai { PURPLE.bg } else { PAGE }))
            .shadow(vec![
                shadow(0.03, 1., 3., 0.),
                shadow(0.04, 6., 16., -4.),
            ])
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(10.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .min_w_0()
                            .when(ai, |d| d.child(icon("sparkle", 14., PURPLE.fg)))
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(fg))
                                    .child(self.title),
                            )
                            .when_some(self.badge, |d, badge| {
                                d.child(
                                    div()
                                        .flex_none()
                                        .px(px(8.))
                                        .py(px(2.))
                                        .rounded_full()
                                        .bg(rgb(if ai { PAGE } else { SIDEBAR }))
                                        .border_1()
                                        .border_color(rgb(if ai { AI_BORDER } else { BORDER }))
                                        .text_size(px(11.))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(rgb(if ai { PURPLE.fg } else { MUTED }))
                                        .child(badge),
                                )
                            }),
                    )
                    .children(self.trailing),
            )
            .when_some(self.error, |d, e| {
                d.child(
                    div()
                        .px(px(12.))
                        .py(px(8.))
                        .rounded(px(8.))
                        .bg(rgb(STAGE_RED.bg))
                        .text_size(px(12.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(DANGER))
                        .child(e),
                )
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
