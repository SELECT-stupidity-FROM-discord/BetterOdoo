use super::icon;
use crate::theme::*;
use gpui::{
    AnyElement, App, ClickEvent, ElementId, FontWeight, IntoElement, MouseButton, MouseDownEvent,
    ParentElement, RenderOnce, SharedString, Window, div, prelude::*, px, rgb,
};

#[derive(IntoElement)]
pub struct NavItem {
    id: ElementId,
    icon: Option<&'static str>,
    dot: Option<u32>,
    label: SharedString,
    active: bool,
    trailing: Vec<AnyElement>,
    on_click: Option<super::OnClick>,
    on_press: Option<super::OnPress>,
}

impl NavItem {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            icon: None,
            dot: None,
            label: label.into(),
            active: false,
            trailing: Vec::new(),
            on_click: None,
            on_press: None,
        }
    }

    pub fn icon(mut self, name: &'static str) -> Self {
        self.icon = Some(name);
        self
    }

    pub fn dot(mut self, color: u32) -> Self {
        self.dot = Some(color);
        self
    }

    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
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

impl ParentElement for NavItem {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.trailing.extend(elements);
    }
}

impl RenderOnce for NavItem {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let on = self.active;
        div()
            .id(self.id)
            .flex()
            .items_center()
            .gap(px(10.))
            .px(px(10.))
            .py(px(6.))
            .rounded(px(6.))
            .cursor_pointer()
            .text_color(rgb(if on { TEXT } else { TEXT_SOFT }))
            .when(on, |d| d.bg(rgb(HOVER)).font_weight(FontWeight::MEDIUM))
            .hover(|s| s.bg(rgb(HOVER)))
            .children(
                self.icon
                    .map(|name| icon(name, 16., if on { TEXT } else { MUTED })),
            )
            .children(
                self.dot
                    .map(|c| div().flex_none().size(px(8.)).rounded(px(2.)).bg(rgb(c))),
            )
            .child(div().flex_1().truncate().child(self.label))
            .children(self.trailing)
            .when_some(self.on_click, |d, f| d.on_click(f))
            .when_some(self.on_press, |d, f| d.on_mouse_down(MouseButton::Left, f))
    }
}

#[derive(IntoElement)]
pub struct ColumnHeader {
    id: ElementId,
    label: SharedString,
    active: bool,
    open: bool,
    on_press: Option<super::OnPress>,
}

impl ColumnHeader {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            active: false,
            open: false,
            on_press: None,
        }
    }

    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
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

impl RenderOnce for ColumnHeader {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div()
            .id(self.id)
            .flex()
            .items_center()
            .gap(px(6.))
            .px(px(6.))
            .py(px(3.))
            .ml(px(-6.))
            .rounded(px(5.))
            .cursor_pointer()
            .text_color(rgb(if self.active { TEXT } else { MUTED }))
            .when(self.active, |d| d.font_weight(FontWeight::MEDIUM))
            .when(self.open, |d| d.bg(rgb(HOVER)))
            .hover(|s| s.bg(rgb(HOVER)))
            .child(self.label)
            .child(icon(
                "filter",
                13.,
                if self.active { ACCENT } else { MUTED },
            ))
            .when_some(self.on_press, |d, f| d.on_mouse_down(MouseButton::Left, f))
    }
}

#[derive(IntoElement)]
pub struct IconButton {
    id: ElementId,
    icon: &'static str,
    active: bool,
    on_press: Option<super::OnPress>,
}

impl IconButton {
    pub fn new(id: impl Into<ElementId>, icon: &'static str) -> Self {
        Self {
            id: id.into(),
            icon,
            active: false,
            on_press: None,
        }
    }

    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
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

impl RenderOnce for IconButton {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div()
            .id(self.id)
            .flex_none()
            .p(px(5.))
            .rounded(px(6.))
            .cursor_pointer()
            .when(self.active, |d| d.bg(rgb(HOVER)))
            .hover(|s| s.bg(rgb(HOVER)))
            .child(icon(self.icon, 16., MUTED))
            .when_some(self.on_press, |d, f| d.on_mouse_down(MouseButton::Left, f))
    }
}
