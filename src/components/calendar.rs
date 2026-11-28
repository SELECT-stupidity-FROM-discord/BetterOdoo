use super::{Floating, icon};
use crate::date;
use crate::input::{InputEvent, TextInput};
use crate::scale::px;
use crate::theme::*;
use gpui::{
    ClickEvent, Context, ElementId, Entity, EventEmitter, FocusHandle, Focusable, FontWeight,
    IntoElement, MouseButton, MouseDownEvent, Render, SharedString, Window, div, prelude::*, rgb,
};

const WEEKDAY_HEADERS: [&str; 7] = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];

pub enum DatePickerEvent {
    Changed(Option<String>),
    Closed,
}

pub struct DatePicker {
    id: ElementId,
    title: SharedString,
    placeholder: SharedString,
    today: i64,
    value: Option<String>,
    view_year: i64,
    view_month: i64,
    preview_day: Option<i64>,
    open: bool,
    clearable: bool,
    headless: bool,
    align_right: bool,
    full_width: bool,
    height: Option<f32>,
    input: Entity<TextInput>,
    root: FocusHandle,
}

impl EventEmitter<DatePickerEvent> for DatePicker {}

impl DatePicker {
    pub fn new(
        id: impl Into<ElementId>,
        title: impl Into<SharedString>,
        placeholder: impl Into<SharedString>,
        today: i64,
        root: FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (view_year, view_month, _) = date::parts(today);
        let input = cx.new(|cx| TextInput::new("Type a date (e.g. Dec 15)", cx));
        cx.subscribe_in(&input, window, Self::on_input).detach();
        Self {
            id: id.into(),
            title: title.into(),
            placeholder: placeholder.into(),
            today,
            value: None,
            view_year,
            view_month,
            preview_day: None,
            open: false,
            clearable: true,
            headless: false,
            align_right: false,
            full_width: false,
            height: None,
            input,
            root,
        }
    }

    pub fn required(mut self) -> Self {
        self.clearable = false;
        self
    }

    pub fn full_width(mut self) -> Self {
        self.full_width = true;
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    pub fn headless(mut self) -> Self {
        self.headless = true;
        self
    }

    pub fn align_right(mut self) -> Self {
        self.align_right = true;
        self
    }

    pub fn value(&self) -> Option<&str> {
        self.value.as_deref()
    }

    pub fn set_value(&mut self, iso: Option<String>, today: i64, cx: &mut Context<Self>) {
        self.today = today;
        self.value = iso;
        self.sync_view_to_value();
        cx.notify();
    }

    pub fn open_popover(
        &mut self,
        value: Option<String>,
        today: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.today = today;
        self.value = value;
        self.sync_view_to_value();
        self.preview_day = None;
        self.open = true;
        let text = self.value.clone().unwrap_or_default();
        self.input.update(cx, |i, cx| i.set_text(&text, cx));
        window.focus(&self.input.focus_handle(cx), cx);
        cx.notify();
    }

    pub fn dismiss(&mut self, cx: &mut Context<Self>) -> bool {
        let was_open = self.open;
        self.open = false;
        self.preview_day = None;
        if was_open {
            cx.notify();
        }
        was_open
    }

    fn sync_view_to_value(&mut self) {
        let anchor = self
            .value
            .as_deref()
            .and_then(date::days_from_iso)
            .unwrap_or(self.today);
        let (y, m, _) = date::parts(anchor);
        self.view_year = y;
        self.view_month = m;
    }

    fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.open {
            self.close(window, cx);
        } else {
            let value = self.value.clone();
            let today = self.today;
            self.open_popover(value, today, window, cx);
        }
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.open {
            self.open = false;
            self.preview_day = None;
            window.focus(&self.root, cx);
            cx.emit(DatePickerEvent::Closed);
            cx.notify();
        }
    }

    fn select_day(&mut self, days: i64, window: &mut Window, cx: &mut Context<Self>) {
        let iso = date::iso(days);
        let (y, m, _) = date::parts(days);
        self.view_year = y;
        self.view_month = m;
        self.value = Some(iso.clone());
        self.preview_day = None;
        self.open = false;
        window.focus(&self.root, cx);
        cx.emit(DatePickerEvent::Changed(Some(iso)));
        cx.notify();
    }

    fn clear_value(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.value = None;
        self.preview_day = None;
        self.open = false;
        self.input.update(cx, |i, cx| i.set_text("", cx));
        window.focus(&self.root, cx);
        cx.emit(DatePickerEvent::Changed(None));
        cx.notify();
    }

    fn step_month(&mut self, delta: i64, cx: &mut Context<Self>) {
        let (y, m) = date::add_months(self.view_year, self.view_month, delta);
        self.view_year = y;
        self.view_month = m;
        cx.notify();
    }

    fn on_input(
        &mut self,
        _: &Entity<TextInput>,
        ev: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match ev {
            InputEvent::Changed => {
                let text = self.input.read(cx).text().to_string();
                if let Some(p) =
                    date::parse_partial(&text, self.today, self.view_year, self.view_month)
                {
                    self.view_year = p.year;
                    self.view_month = p.month;
                    self.preview_day = p.day;
                } else {
                    self.preview_day = None;
                }
                cx.notify();
            }
            InputEvent::Submit => {
                let text = self.input.read(cx).text().trim().to_string();
                if text.is_empty() && self.clearable {
                    self.clear_value(window, cx);
                } else if let Some(p) =
                    date::parse_partial(&text, self.today, self.view_year, self.view_month)
                    && let Some(days) = p.day
                {
                    self.select_day(days, window, cx);
                }
            }
            InputEvent::Cancel => self.close(window, cx),
        }
    }

    fn popover(&self, window: &Window, cx: &mut Context<Self>) -> Floating {
        let selected_day = self.value.as_deref().and_then(date::days_from_iso);
        let focused = self.input.focus_handle(cx).is_focused(window);
        let cells = date::calendar_cells(self.view_year, self.view_month);
        let quick = |id: &'static str, label: &'static str, offset: i64, cx: &mut Context<Self>| {
            let target = self.today + offset;
            let active = selected_day == Some(target);
            div()
                .id(id)
                .flex_1()
                .py(px(5.))
                .px(px(8.))
                .rounded_full()
                .border_1()
                .border_color(rgb(if active { ACCENT } else { BORDER }))
                .bg(rgb(if active { ACCENT } else { SIDEBAR }))
                .text_color(rgb(if active { PAGE } else { TEXT_SOFT }))
                .text_size(px(12.))
                .font_weight(FontWeight::MEDIUM)
                .flex()
                .justify_center()
                .cursor_pointer()
                .when(!active, |d| d.hover(|s| s.bg(rgb(HOVER))))
                .child(label)
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.select_day(target, window, cx)
                }))
        };

        let mut rows = Vec::with_capacity(6);
        for week in 0..6 {
            let mut row = div().flex().items_center().gap(px(2.));
            for col in 0..7 {
                let days = cells[week * 7 + col];
                let (_, m, d) = date::parts(days);
                let in_month = m == self.view_month;
                let is_selected = selected_day == Some(days);
                let is_preview = !is_selected && self.preview_day == Some(days);
                let is_today = days == self.today;
                row = row.child(
                    div().flex_1().flex().justify_center().child(
                        div()
                            .id(("cal-day", week * 7 + col))
                            .size(px(34.))
                            .rounded_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .text_size(px(13.))
                            .when(is_selected, |el| {
                                el.bg(rgb(ACCENT))
                                    .text_color(rgb(PAGE))
                                    .font_weight(FontWeight::SEMIBOLD)
                            })
                            .when(is_preview, |el| {
                                el.bg(rgb(ACTIVE_STEP))
                                    .border_1()
                                    .border_color(rgb(ACCENT))
                                    .text_color(rgb(ACTIVE_STEP_TEXT))
                                    .font_weight(FontWeight::SEMIBOLD)
                            })
                            .when(!is_selected && !is_preview && is_today, |el| {
                                el.bg(rgb(ACTIVE_STEP))
                                    .text_color(rgb(ACTIVE_STEP_TEXT))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .hover(|s| s.bg(rgb(HOVER)))
                            })
                            .when(!is_selected && !is_preview && !is_today, |el| {
                                el.text_color(rgb(if in_month { TEXT } else { BORDER_STRONG }))
                                    .hover(|s| s.bg(rgb(if in_month { HOVER } else { SIDEBAR })))
                            })
                            .child(d.to_string())
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                this.select_day(days, window, cx)
                            })),
                    ),
                );
            }
            rows.push(row);
        }

        Floating::new((self.id.clone(), "popover"))
            .width(304.)
            .radius(16.)
            .offset(8.)
            .padded(16., 12.)
            .on_dismiss(cx.listener(|this, _: &MouseDownEvent, window, cx| this.close(window, cx)))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(2.))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(TEXT))
                            .child(self.title.clone()),
                    )
                    .when(self.clearable && self.value.is_some(), |d| {
                        d.child(
                            div()
                                .id("cal-clear")
                                .px(px(7.))
                                .py(px(2.))
                                .rounded_full()
                                .cursor_pointer()
                                .text_size(px(12.))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(ACCENT))
                                .hover(|s| s.bg(rgb(ACTIVE_STEP)))
                                .child("Clear")
                                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                    this.clear_value(window, cx)
                                })),
                        )
                    }),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .px(px(11.))
                    .py(px(7.))
                    .rounded(px(10.))
                    .border_1()
                    .border_color(rgb(if focused { ACCENT } else { BORDER }))
                    .bg(rgb(if focused { PAGE } else { SIDEBAR }))
                    .text_size(px(13.))
                    .child(icon("calendar", 14., if focused { ACCENT } else { MUTED }))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .child(self.input.clone()),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(quick("cal-today", "Today", 0, cx))
                    .child(quick("cal-tomorrow", "Tomorrow", 1, cx))
                    .child(quick("cal-week", "+1 week", 7, cx)),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_between()
                    .pt(px(2.))
                    .child(
                        div()
                            .id("cal-prev")
                            .flex_none()
                            .size(px(28.))
                            .rounded_full()
                            .bg(rgb(SIDEBAR))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(HOVER)))
                            .child(icon("chevron-left", 14., TEXT_SOFT))
                            .on_click(
                                cx.listener(|this, _: &ClickEvent, _, cx| this.step_month(-1, cx)),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(TEXT))
                            .child(format!(
                                "{} {}",
                                date::month_name(self.view_month),
                                self.view_year
                            )),
                    )
                    .child(
                        div()
                            .id("cal-next")
                            .flex_none()
                            .size(px(28.))
                            .rounded_full()
                            .bg(rgb(SIDEBAR))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(HOVER)))
                            .child(icon("chevron-right", 14., TEXT_SOFT))
                            .on_click(
                                cx.listener(|this, _: &ClickEvent, _, cx| this.step_month(1, cx)),
                            ),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(2.))
                            .pb(px(6.))
                            .mb(px(2.))
                            .border_b_1()
                            .border_color(rgb(ROW_RULE))
                            .children(WEEKDAY_HEADERS.into_iter().map(|w| {
                                div()
                                    .flex_1()
                                    .h(px(20.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_size(px(11.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgb(MUTED))
                                    .child(w)
                            })),
                    )
                    .children(rows),
            )
    }
}

impl Render for DatePicker {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.headless {
            return div().when(self.open, |d| d.child(self.popover(window, cx)));
        }
        let label = match &self.value {
            Some(iso) => date::due(Some(iso), self.today).label,
            None => self.placeholder.to_string(),
        };
        let has_value = self.value.is_some();
        let fill_surface = self.full_width;
        div()
            .flex()
            .flex_col()
            .when(self.full_width, |d| d.w_full())
            .when(self.align_right, |d| d.items_end())
            .child(
                div()
                    .id(self.id.clone())
                    .flex_none()
                    .when(self.full_width, |d| d.w_full().justify_between())
                    .when_some(self.height, |d, h| d.h(px(h)))
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .px(px(11.))
                    .when(self.height.is_none(), |d| d.py(px(6.)))
                    .rounded(px(8.))
                    .bg(rgb(if self.open || !fill_surface {
                        PAGE
                    } else {
                        SIDEBAR
                    }))
                    .border_1()
                    .border_color(rgb(if self.open { ACCENT } else { BORDER }))
                    .cursor_pointer()
                    .text_size(px(13.))
                    .hover(|s| {
                        if fill_surface && !self.open {
                            s.border_color(rgb(BORDER_STRONG))
                        } else {
                            s.bg(rgb(SIDEBAR))
                        }
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(7.))
                            .min_w_0()
                            .child(icon(
                                "calendar",
                                14.,
                                if self.open {
                                    ACCENT
                                } else if has_value {
                                    TEXT_SOFT
                                } else {
                                    MUTED
                                },
                            ))
                            .child(
                                div()
                                    .truncate()
                                    .text_color(rgb(if has_value { TEXT } else { TEXT_SOFT }))
                                    .when(has_value, |d| d.font_weight(FontWeight::MEDIUM))
                                    .child(label),
                            ),
                    )
                    .child(icon("chevron-down", 12., MUTED))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _: &MouseDownEvent, window, cx| this.toggle(window, cx)),
                    ),
            )
            .when(self.open, |d| d.child(self.popover(window, cx)))
    }
}
