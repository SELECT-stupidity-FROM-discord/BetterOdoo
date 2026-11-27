use super::icon;
use crate::theme::*;
use gpui::{
    AnyElement, App, FontWeight, IntoElement, ParentElement, RenderOnce, SharedString, Window, div,
    prelude::*, px, rgb, white,
};

#[derive(IntoElement)]
pub struct Avatar {
    initial: String,
    tone: Tone,
    size: f32,
    overlap: bool,
}

impl Avatar {
    pub fn new(name: &str, tone: Tone) -> Self {
        Self {
            initial: name
                .chars()
                .next()
                .map_or("?".into(), |c| c.to_uppercase().to_string()),
            tone,
            size: 24.,
            overlap: false,
        }
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    pub fn overlapping(mut self, overlap: bool) -> Self {
        self.overlap = overlap;
        self
    }
}

impl RenderOnce for Avatar {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div()
            .flex_none()
            .size(px(self.size))
            .rounded_full()
            .bg(rgb(self.tone.bg))
            .text_color(rgb(self.tone.fg))
            .text_size(px((self.size * 0.45).round()))
            .font_weight(FontWeight::SEMIBOLD)
            .flex()
            .items_center()
            .justify_center()
            .when(self.overlap, |d| {
                d.ml(px(-8.)).border_2().border_color(white())
            })
            .child(self.initial)
    }
}

#[derive(IntoElement)]
pub struct Property {
    icon: &'static str,
    label: SharedString,
    children: Vec<AnyElement>,
}

impl Property {
    pub fn new(icon: &'static str, label: impl Into<SharedString>) -> Self {
        Self {
            icon,
            label: label.into(),
            children: Vec::new(),
        }
    }
}

impl ParentElement for Property {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Property {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .min_h(px(26.))
            .child(
                div()
                    .flex_none()
                    .w(px(120.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_color(rgb(MUTED))
                    .child(icon(self.icon, 15., MUTED))
                    .child(self.label),
            )
            .child(div().children(self.children))
    }
}
