use super::icon;
use crate::theme::*;
use gpui::{
    App, ClickEvent, ElementId, FontWeight, IntoElement, RenderOnce, SharedString, Window, div,
    prelude::*, px, rgb, white,
};

#[derive(Clone, Copy, PartialEq)]
pub enum Variant {
    Primary,
    Secondary,
    Ai,
    Danger,
}

#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    label: SharedString,
    icon: Option<&'static str>,
    variant: Variant,
    disabled: bool,
    wide: bool,
    on_click: Option<super::OnClick>,
}

impl Button {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
            variant: Variant::Secondary,
            disabled: false,
            wide: false,
            on_click: None,
        }
    }

    pub fn variant(mut self, variant: Variant) -> Self {
        self.variant = variant;
        self
    }

    pub fn icon(mut self, name: &'static str) -> Self {
        self.icon = Some(name);
        self
    }

    pub fn wide(mut self) -> Self {
        self.wide = true;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Box::new(f));
        self
    }
}

impl RenderOnce for Button {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let (fg, glyph) = match self.variant {
            Variant::Primary => (0xffffff, 0xffffff),
            Variant::Secondary => (TEXT_SOFT, MUTED),
            Variant::Ai => (PURPLE.fg, PURPLE.fg),
            Variant::Danger => (DANGER, DANGER),
        };
        let variant = self.variant;
        let disabled = self.disabled;
        div()
            .id(self.id)
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .gap(px(6.))
            .when(self.wide, |d| d.w_full().min_h(px(40.)))
            .px(px(12.))
            .min_h(px(32.))
            .rounded(px(6.))
            .text_size(px(13.))
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgb(fg))
            .when(variant == Variant::Primary, |d| {
                d.bg(rgb(ACCENT)).text_color(white())
            })
            .when(variant == Variant::Ai, |d| {
                d.bg(rgb(PURPLE.bg)).border_1().border_color(rgb(AI_BORDER))
            })
            .when(
                matches!(variant, Variant::Secondary | Variant::Danger),
                |d| {
                    d.bg(rgb(PAGE))
                        .border_1()
                        .border_color(rgb(BORDER))
                        .hover(|s| s.bg(rgb(HOVER)))
                },
            )
            .when(disabled, |d| d.opacity(0.5))
            .when(!disabled, |d| d.cursor_pointer())
            .children(self.icon.map(|name| icon(name, 14., glyph)))
            .child(self.label)
            .when_some(self.on_click.filter(|_| !disabled), |d, f| d.on_click(f))
    }
}
