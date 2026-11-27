use super::{icon, popover};
use crate::theme::*;
use gpui::{
    Anchor, AnyElement, App, ClickEvent, ElementId, FontWeight, IntoElement, MouseDownEvent,
    ParentElement, RenderOnce, SharedString, Window, anchored, deferred, div, point, prelude::*,
    px, rgb,
};

type Dismiss = Box<dyn Fn(&MouseDownEvent, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Floating {
    id: ElementId,
    anchor: Option<Anchor>,
    offset: f32,
    width: f32,
    max_height: Option<f32>,
    padding: f32,
    gap: f32,
    on_dismiss: Option<Dismiss>,
    children: Vec<AnyElement>,
}

impl Floating {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            anchor: None,
            offset: 6.,
            width: 260.,
            max_height: None,
            padding: 4.,
            gap: 0.,
            on_dismiss: None,
            children: Vec::new(),
        }
    }

    pub fn above(mut self, offset: f32) -> Self {
        self.anchor = Some(Anchor::BottomLeft);
        self.offset = -offset;
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    pub fn max_height(mut self, height: f32) -> Self {
        self.max_height = Some(height);
        self
    }

    pub fn padded(mut self, padding: f32, gap: f32) -> Self {
        self.padding = padding;
        self.gap = gap;
        self
    }

    pub fn on_dismiss(
        mut self,
        f: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_dismiss = Some(Box::new(f));
        self
    }
}

impl ParentElement for Floating {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Floating {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let mut anchor = anchored()
            .snap_to_window()
            .offset(point(px(0.), px(self.offset)));
        if let Some(a) = self.anchor {
            anchor = anchor.anchor(a);
        }
        deferred(
            anchor.child(
                popover()
                    .id(self.id)
                    .occlude()
                    .w(px(self.width))
                    .when_some(self.max_height, |d, h| d.max_h(px(h)).overflow_y_scroll())
                    .p(px(self.padding))
                    .flex()
                    .flex_col()
                    .gap(px(self.gap))
                    .when_some(self.on_dismiss, |d, f| d.on_mouse_down_out(f))
                    .children(self.children),
            ),
        )
        .with_priority(1)
    }
}

#[derive(IntoElement)]
pub struct MenuHeader {
    title: SharedString,
    on_clear: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>>,
}

impl MenuHeader {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            on_clear: None,
        }
    }

    pub fn on_clear(
        mut self,
        show: bool,
        f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_clear = show.then(|| Box::new(f) as _);
        self
    }
}

impl RenderOnce for MenuHeader {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .px(px(10.))
            .pt(px(8.))
            .pb(px(6.))
            .text_size(px(12.))
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgb(MUTED))
            .child(self.title)
            .children(self.on_clear.map(|f| {
                div()
                    .id("clear")
                    .cursor_pointer()
                    .text_color(rgb(ACCENT))
                    .hover(|s| s.text_color(rgb(TEXT)))
                    .child("Clear")
                    .on_click(f)
            }))
    }
}

#[derive(IntoElement)]
pub struct MenuRow {
    id: ElementId,
    leading: AnyElement,
    label: SharedString,
    count: usize,
    selected: bool,
    on_click: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>>,
}

impl MenuRow {
    pub fn new(
        id: impl Into<ElementId>,
        leading: impl IntoElement,
        label: impl Into<SharedString>,
        count: usize,
    ) -> Self {
        Self {
            id: id.into(),
            leading: leading.into_any_element(),
            label: label.into(),
            count,
            selected: false,
            on_click: None,
        }
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Box::new(f));
        self
    }
}

impl RenderOnce for MenuRow {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div()
            .id(self.id)
            .flex()
            .items_center()
            .gap(px(10.))
            .h(px(32.))
            .px(px(10.))
            .rounded(px(6.))
            .cursor_pointer()
            .text_size(px(13.))
            .hover(|s| s.bg(rgb(HOVER)))
            .child(self.leading)
            .child(div().flex_1().min_w_0().truncate().child(self.label))
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(MUTED))
                    .child(self.count.to_string()),
            )
            .child(
                div()
                    .flex_none()
                    .size(px(14.))
                    .when(self.selected, |d| d.child(icon("check", 14., ACCENT))),
            )
            .when_some(self.on_click, |d, f| d.on_click(f))
    }
}
