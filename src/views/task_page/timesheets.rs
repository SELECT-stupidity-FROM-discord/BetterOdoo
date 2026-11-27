use super::ChildEvent;
use crate::components::{Button, Variant, icon};
use crate::date;
use crate::format::{hours, parse_hours};
use crate::input::{InputEvent, TextInput};
use crate::odoo::WorkItem;
use crate::store::{Load, Store};
use crate::theme::*;
use gpui::{
    ClickEvent, Context, Entity, EventEmitter, FocusHandle, FontWeight, IntoElement, Render,
    Window, div, prelude::*, px, rgb,
};

pub struct TimesheetTab {
    store: Entity<Store>,
    root: FocusHandle,
    task: i64,
    name: Entity<TextInput>,
    hours: Entity<TextInput>,
    day: Entity<TextInput>,
}

impl EventEmitter<ChildEvent> for TimesheetTab {}

impl TimesheetTab {
    pub fn new(
        store: Entity<Store>,
        root: FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        let input = |placeholder: &'static str, cx: &mut Context<Self>| {
            cx.new(|cx| TextInput::new(placeholder, cx))
        };
        let (name, hours, day) = (
            input("What did you work on?", cx),
            input("1h 30m", cx),
            input("Today", cx),
        );
        for i in [&name, &hours, &day] {
            cx.subscribe_in(i, window, Self::on_input).detach();
        }
        Self {
            store,
            root,
            task: 0,
            name,
            hours,
            day,
        }
    }

    pub fn set_task(&mut self, id: i64, cx: &mut Context<Self>) {
        self.task = id;
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
            InputEvent::Submit => self.log(cx),
            InputEvent::Cancel => window.focus(&self.root, cx),
            InputEvent::Changed => {}
        }
    }

    fn notice(&self, msg: &str, cx: &mut Context<Self>) {
        cx.emit(ChildEvent::Notice(format!("Timesheet: {msg}").into()));
    }

    fn log(&mut self, cx: &mut Context<Self>) {
        let store = self.store.read(cx);
        let (Some(task), today) = (store.task(self.task).cloned(), store.today) else {
            return;
        };
        let Some(h) = parse_hours(self.hours.read(cx).text()).filter(|h| *h > 0.) else {
            return self.notice("enter the hours, like 1.5 or 1h 30m", cx);
        };
        let day = self.day.read(cx).text().trim().to_string();
        let day = if day.is_empty() {
            date::iso(today)
        } else {
            day
        };
        if !date::is_valid(&day) {
            return self.notice("write the date as YYYY-MM-DD", cx);
        }
        let name = self.name.read(cx).text().trim().to_string();
        if name.is_empty() {
            return self.notice("describe the work", cx);
        }
        let item = WorkItem {
            name,
            hours: h,
            date: day,
            done: false,
        };
        cx.emit(ChildEvent::Notice("".into()));
        self.name.update(cx, |i, cx| i.set_text("", cx));
        self.hours.update(cx, |i, cx| i.set_text("", cx));
        let job = self
            .store
            .update(cx, |s, cx| s.add_timesheet(task, item, cx));
        cx.spawn(async move |this, cx| {
            if let Err(e) = job.await {
                this.update(cx, |this, cx| {
                    this.notice(&format!("couldn’t log the time: {e}"), cx)
                })
                .ok();
            }
        })
        .detach();
    }
}

fn pill(icon_name: &'static str, width: f32, input: &Entity<TextInput>) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(px(8.))
        .w(px(width))
        .px(px(10.))
        .py(px(5.))
        .rounded(px(7.))
        .bg(rgb(SIDEBAR))
        .border_1()
        .border_color(rgb(BORDER))
        .child(icon(icon_name, 14., MUTED))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .child(input.clone()),
        )
}

impl TimesheetTab {
    fn composer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .p(px(12.))
            .rounded(px(10.))
            .border_1()
            .border_color(rgb(BORDER))
            .child(div().px(px(2.)).child(self.name.clone()))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(pill("clock", 120., &self.hours))
                    .child(pill("calendar", 150., &self.day))
                    .child(div().flex_1())
                    .child(
                        Button::new("log-time", "Log time")
                            .icon("plus")
                            .variant(Variant::Primary)
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.log(cx))),
                    ),
            )
    }
}

impl Render for TimesheetTab {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let store = self.store.read(cx);
        let today = store.today;
        let cell = |w: f32, v: String| div().flex_none().w(px(w)).truncate().child(v);
        let list = match store.detail(self.task) {
            Load::Loading => div().text_color(rgb(MUTED)).child("Loading…"),
            Load::Failed(e) => div().text_color(rgb(DANGER)).child(e.clone()),
            Load::Ready(d) if d.timesheets.is_empty() => {
                div().text_color(rgb(MUTED)).child("No time logged yet.")
            }
            Load::Ready(d) => {
                let total: f64 = d.timesheets.iter().map(|t| t.hours).sum();
                div()
                    .flex()
                    .flex_col()
                    .children(d.timesheets.iter().map(|t| {
                        div()
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .py(px(8.))
                            .border_b_1()
                            .border_color(rgb(ROW_RULE))
                            .child(
                                cell(92., date::due(Some(&t.date), today).label)
                                    .text_color(rgb(MUTED)),
                            )
                            .child(div().flex_1().min_w_0().truncate().child(t.name.clone()))
                            .child(cell(60., t.user.clone()).text_color(rgb(MUTED)))
                            .child(cell(60., hours(t.hours)).text_color(rgb(TEXT_SOFT)))
                    }))
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .pt(px(8.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("Total {}", hours(total))),
                    )
            }
        };
        div()
            .flex()
            .flex_col()
            .gap(px(20.))
            .child(self.composer(cx))
            .child(list)
    }
}
