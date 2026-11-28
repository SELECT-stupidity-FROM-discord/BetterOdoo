use super::{icon, popover};
use crate::scale::px;
use crate::theme::*;
use gpui::{
    Anchor, AnyElement, App, Bounds, ClickEvent, Corners, ElementId, FontWeight, IntoElement,
    MouseDownEvent, ParentElement, RenderOnce, ScrollHandle, SharedString, Window, anchored, black,
    canvas, deferred, div, fill, point, prelude::*, rgb, size,
};

fn scrollbar_overlay(handle: ScrollHandle) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let max_y = f32::from(handle.max_offset().y);
            if max_y <= 0.5 {
                return;
            }
            let vh = f32::from(handle.bounds().size.height);
            if vh <= 0. {
                return;
            }
            let content_h = vh + max_y;
            let scroll_y = (-f32::from(handle.offset().y)).clamp(0., max_y);
            let inset = f32::from(px(12.));
            let track_h = (f32::from(bounds.size.height) - inset * 2.).max(0.);
            if track_h <= 0. {
                return;
            }
            let thumb_h = ((vh / content_h) * track_h).clamp(f32::from(px(24.)), track_h);
            let bar_w = f32::from(px(2.5));
            let bar_x =
                f32::from(bounds.origin.x) + f32::from(bounds.size.width) - f32::from(px(6.));
            let track_y = f32::from(bounds.origin.y) + inset;
            let thumb_y = track_y + (scroll_y / max_y) * (track_h - thumb_h);
            let radius = Corners::all(gpui::px(bar_w * 0.5));

            let thumb = Bounds::new(
                point(gpui::px(bar_x), gpui::px(thumb_y)),
                size(gpui::px(bar_w), gpui::px(thumb_h)),
            );
            window.paint_quad(fill(thumb, black().opacity(0.16)).corner_radii(radius));
        },
    )
    .absolute()
    .top_0()
    .right_0()
    .bottom_0()
    .w(px(8.))
}

#[derive(IntoElement)]
pub struct Floating {
    id: ElementId,
    anchor: Option<Anchor>,
    offset: f32,
    width: f32,
    max_height: Option<f32>,
    padding: f32,
    gap: f32,
    radius: Option<f32>,
    on_dismiss: Option<super::OnPress>,
    children: Vec<AnyElement>,
}

impl Floating {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            anchor: None,
            offset: 6.,
            width: 280.,
            max_height: None,
            padding: 6.,
            gap: 2.,
            radius: None,
            on_dismiss: None,
            children: Vec::new(),
        }
    }

    pub fn above(mut self, offset: f32) -> Self {
        self.anchor = Some(Anchor::BottomLeft);
        self.offset = -offset;
        self
    }

    pub fn offset(mut self, offset: f32) -> Self {
        self.offset = offset;
        self
    }

    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = Some(radius);
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
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let scroll = self.max_height.map(|_| {
            window
                .use_keyed_state((self.id.clone(), "scroll"), cx, |_, _| ScrollHandle::new())
                .read(cx)
                .clone()
        });
        let mut anchor = anchored()
            .snap_to_window_with_margin(px(16.))
            .offset(point(px(0.), px(self.offset)));
        if let Some(a) = self.anchor {
            anchor = anchor.anchor(a);
        }
        let right_pad = if self.max_height.is_some() {
            self.padding.max(12.)
        } else {
            self.padding
        };
        let body = div()
            .id((self.id.clone(), "viewport"))
            .w_full()
            .flex()
            .flex_col()
            .when_some(self.max_height, |d, h| {
                d.max_h(px(h))
                    .overflow_y_scroll()
                    .rounded(px(self.radius.unwrap_or(12.)))
            })
            .when_some(scroll.clone(), |d, handle| d.track_scroll(&handle))
            .child(
                div()
                    .w_full()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .pt(px(self.padding))
                    .pb(px(self.padding))
                    .pl(px(self.padding))
                    .pr(px(right_pad))
                    .gap(px(self.gap))
                    .children(self.children),
            );
        deferred(
            anchor.child(
                popover()
                    .id(self.id)
                    .relative()
                    .occlude()
                    .w(px(self.width))
                    .when_some(self.radius, |d, r| d.rounded(px(r)))
                    .flex()
                    .flex_col()
                    .when_some(self.on_dismiss, |d, f| d.on_mouse_down_out(f))
                    .child(body)
                    .when_some(scroll, |d, handle| d.child(scrollbar_overlay(handle))),
            ),
        )
        .with_priority(1)
    }
}

#[derive(IntoElement)]
pub struct MenuHeader {
    title: SharedString,
    on_clear: Option<super::OnClick>,
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
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .px(px(10.))
            .pt(px(6.))
            .pb(px(8.))
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
    on_click: Option<super::OnClick>,
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
            .flex_none()
            .flex()
            .items_center()
            .gap(px(10.))
            .min_h(px(36.))
            .py(px(6.))
            .px(px(10.))
            .rounded(px(6.))
            .cursor_pointer()
            .text_size(px(13.))
            .hover(|s| s.bg(rgb(HOVER)))
            .child(self.leading)
            .child(div().flex_1().min_w_0().truncate().child(self.label))
            .child(
                div()
                    .flex_none()
                    .text_size(px(12.))
                    .text_color(rgb(MUTED))
                    .child(self.count.to_string()),
            )
            .child(
                div()
                    .flex_none()
                    .size(px(14.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(self.selected, |d| d.child(icon("check", 14., ACCENT))),
            )
            .when_some(self.on_click, |d, f| d.on_click(f))
    }
}
