use super::ChildEvent;
use crate::components::{Avatar, Button, RichText, Variant, icon};
use crate::date;
use crate::format::person_tone;
use crate::input::{InputEvent, TextInput};
use crate::odoo::{Change, Message};
use crate::scale::px;
use crate::store::{Load, Store};
use crate::theme::*;
use gpui::{
    ClickEvent, Context, Entity, EventEmitter, FocusHandle, Focusable, FontWeight, IntoElement,
    Render, Window, div, prelude::*, rgb,
};

pub struct ActivityTab {
    store: Entity<Store>,
    root: FocusHandle,
    task: i64,
    input: Entity<TextInput>,
    note: bool,
}

impl EventEmitter<ChildEvent> for ActivityTab {}

impl ActivityTab {
    pub fn new(
        store: Entity<Store>,
        root: FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        let input = cx.new(|cx| TextInput::new("Write something…", cx).multiline());
        cx.subscribe_in(&input, window, Self::on_input).detach();
        Self {
            store,
            root,
            task: 0,
            input,
            note: true,
        }
    }

    pub fn set_task(&mut self, id: i64, cx: &mut Context<Self>) {
        self.task = id;
        self.input.update(cx, |i, cx| i.set_text("", cx));
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
            InputEvent::Submit => self.post(cx),
            InputEvent::Cancel => window.focus(&self.root, cx),
            InputEvent::Changed => cx.notify(),
        }
    }

    fn post(&mut self, cx: &mut Context<Self>) {
        let text = self.input.read(cx).text().trim().to_string();
        let Some(task) = self.store.read(cx).task(self.task).cloned() else {
            return;
        };
        if text.is_empty() {
            return;
        }
        self.input.update(cx, |i, cx| i.set_text("", cx));
        let note = self.note;
        let job = self
            .store
            .update(cx, |s, cx| s.post_message(task, text, note, cx));
        cx.spawn(async move |this, cx| {
            if let Err(e) = job.await {
                this.update(cx, |_, cx| {
                    cx.emit(ChildEvent::Notice(format!("Couldn’t post: {e}").into()))
                })
                .ok();
            }
        })
        .detach();
    }

    fn mode_toggle(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let option = |id: &'static str, label: &'static str, note: bool, cx: &mut Context<Self>| {
            let on = self.note == note;
            div()
                .id(id)
                .px(px(10.))
                .py(px(4.))
                .rounded(px(5.))
                .cursor_pointer()
                .text_size(px(13.))
                .font_weight(FontWeight::MEDIUM)
                .when(on, |d| d.bg(rgb(PAGE)).text_color(rgb(TEXT)).shadow_sm())
                .when(!on, |d| {
                    d.text_color(rgb(MUTED)).hover(|s| s.text_color(rgb(TEXT)))
                })
                .child(label)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.note = note;
                    cx.notify()
                }))
        };
        div()
            .flex()
            .p(px(2.))
            .gap(px(2.))
            .rounded(px(7.))
            .bg(rgb(HOVER))
            .child(option("send-message", "Send message", false, cx))
            .child(option("log-note", "Log note", true, cx))
    }

    fn composer(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let empty = self.input.read(cx).text().trim().is_empty();
        let focused = self.input.focus_handle(cx).is_focused(window);
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .p(px(12.))
            .rounded(px(10.))
            .border_1()
            .border_color(rgb(if focused { ACCENT } else { BORDER }))
            .bg(rgb(if self.note { SIDEBAR } else { PAGE }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(self.mode_toggle(cx))
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(MUTED))
                            .child(if self.note {
                                "Only visible to your team"
                            } else {
                                "Followers will be notified"
                            }),
                    ),
            )
            .child(div().min_h(px(64.)).child(self.input.clone()))
            .child(
                div().flex().justify_end().child(
                    Button::new("post", if self.note { "Log note" } else { "Send" })
                        .icon("plus")
                        .variant(Variant::Primary)
                        .disabled(empty)
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.post(cx))),
                ),
            )
    }
}

fn day_divider(label: String) -> impl IntoElement {
    let line = || div().flex_1().h(px(1.)).bg(rgb(BORDER));
    div()
        .flex()
        .items_center()
        .gap(px(12.))
        .child(line())
        .child(div().text_size(px(12.)).text_color(rgb(MUTED)).child(label))
        .child(line())
}

fn change_row(c: &Change) -> impl IntoElement {
    let pill = |text: String, bg: u32, fg: u32| {
        div()
            .px(px(8.))
            .py(px(2.))
            .rounded(px(5.))
            .bg(rgb(bg))
            .text_color(rgb(fg))
            .text_size(px(13.))
            .child(text)
    };
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(8.))
        .child(
            div()
                .text_size(px(12.))
                .text_color(rgb(MUTED))
                .child(c.field.clone()),
        )
        .child(pill(c.old.clone(), HOVER, MUTED))
        .child(icon("chevron-right", 12., MUTED))
        .child(pill(c.new.clone(), BLUE.bg, BLUE.fg))
}

fn entry(m: &Message, time: String) -> impl IntoElement {
    div()
        .flex()
        .gap(px(12.))
        .child(
            Avatar::new(&m.author, person_tone(&[], &m.author))
                .for_me(m.is_me)
                .size(30.),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(
                    div()
                        .flex()
                        .items_baseline()
                        .gap(px(8.))
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(m.author.clone()),
                        )
                        .child(div().text_size(px(12.)).text_color(rgb(MUTED)).child(time)),
                )
                .when(!m.body.is_empty(), |d| {
                    d.child(RichText::new(m.body.clone()))
                })
                .children(m.changes.iter().map(change_row)),
        )
}

impl Render for ActivityTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let note = |text: String, color: u32| div().text_color(rgb(color)).child(text);
        let feed = match self.store.read(cx).detail(self.task) {
            Load::Loading => note("Loading…".into(), MUTED),
            Load::Failed(e) => note(e.to_string(), DANGER),
            Load::Ready(d) => match &d.messages {
                Err(e) => note(format!("Couldn’t load activity: {e}"), DANGER),
                Ok(list) if list.is_empty() => note("Nothing here yet.".into(), MUTED),
                Ok(list) => {
                    let mut out = Vec::new();
                    let mut day = String::new();
                    for m in list {
                        let (d, time) = date::stamp(&m.date);
                        if d != day {
                            out.push(day_divider(d.clone()).into_any_element());
                            day = d;
                        }
                        out.push(entry(m, time).into_any_element());
                    }
                    div().flex().flex_col().gap(px(20.)).children(out)
                }
            },
        };
        div()
            .flex()
            .flex_col()
            .gap(px(24.))
            .child(self.composer(window, cx))
            .child(feed)
    }
}
