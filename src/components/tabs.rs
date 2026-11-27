use crate::scale::px;
use crate::theme::*;
use gpui::{
    App, ClickEvent, FontWeight, IntoElement, PathBuilder, RenderOnce, SharedString, Window,
    canvas, div, point, prelude::*, rgb,
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

struct Step {
    label: SharedString,
    active: bool,
    note: Option<String>,
    on_click: super::OnClick,
}

#[derive(IntoElement, Default)]
pub struct Segmented {
    items: Vec<Step>,
}

impl Segmented {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn item(
        mut self,
        label: impl Into<SharedString>,
        active: bool,
        note: Option<String>,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.items.push(Step {
            label: label.into(),
            active,
            note,
            on_click: Box::new(on_click),
        });
        self
    }
}

const ARROW: f32 = 11.;

fn arrow(color: u32) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let mut path = PathBuilder::fill();
            path.move_to(bounds.origin);
            path.line_to(point(
                bounds.origin.x + bounds.size.width,
                bounds.origin.y + bounds.size.height / 2.,
            ));
            path.line_to(point(bounds.origin.x, bounds.origin.y + bounds.size.height));
            if let Ok(path) = path.build() {
                window.paint_path(path, rgb(color));
            }
        },
    )
    .flex_none()
    .w(px(ARROW))
}

impl RenderOnce for Segmented {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let strip = div().flex().flex_wrap().items_stretch().children(
            self.items.into_iter().enumerate().map(|(i, t)| {
                let step = div()
                    .id(("segment", i))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .px(px(14.))
                    .py(px(7.))
                    .cursor_pointer()
                    .text_size(px(13.))
                    .when(t.active, |d| {
                        d.rounded_l(px(7.))
                            .bg(rgb(ACTIVE_STEP))
                            .text_color(rgb(ACTIVE_STEP_TEXT))
                            .font_weight(FontWeight::SEMIBOLD)
                    })
                    .when(!t.active, |d| {
                        d.rounded(px(7.))
                            .text_color(rgb(TEXT_SOFT))
                            .font_weight(FontWeight::MEDIUM)
                            .hover(|s| s.bg(rgb(HOVER)))
                    })
                    .child(t.label)
                    .children(t.note.map(|n| {
                        div()
                            .text_size(px(12.))
                            .font_weight(FontWeight::NORMAL)
                            .text_color(rgb(MUTED))
                            .child(n)
                    }))
                    .on_click(t.on_click);
                div()
                    .flex()
                    .items_stretch()
                    .child(step)
                    .when(t.active, |d| d.child(arrow(ACTIVE_STEP)))
            }),
        );
        div().flex().child(strip)
    }
}
