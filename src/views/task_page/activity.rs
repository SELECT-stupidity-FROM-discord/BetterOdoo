use super::ChildEvent;
use crate::components::{Avatar, Button, Field, RichText, Variant};
use crate::date;
use crate::format::person_tone;
use crate::html;
use crate::input::{InputEvent, TextInput};
use crate::store::{Load, Store};
use crate::theme::*;
use gpui::{
    ClickEvent, Context, Entity, EventEmitter, FocusHandle, FontWeight, IntoElement, Render,
    Window, div, prelude::*, px, rgb,
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

    fn composer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let empty = self.input.read(cx).text().trim().is_empty();
        let mode = |id: &'static str,
                    label: &'static str,
                    on: bool,
                    note: bool,
                    cx: &mut Context<Self>| {
            Button::new(id, label)
                .variant(if on {
                    Variant::Primary
                } else {
                    Variant::Secondary
                })
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.note = note;
                    cx.notify()
                }))
        };
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(mode("send-message", "Send message", !self.note, false, cx))
                    .child(mode("log-note", "Log note", self.note, true, cx)),
            )
            .child(Field::new(&self.input).height(72.))
            .child(
                div().flex().child(
                    Button::new("post", if self.note { "Log" } else { "Send" })
                        .variant(Variant::Primary)
                        .disabled(empty)
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.post(cx))),
                ),
            )
    }
}

impl Render for ActivityTab {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let store = self.store.read(cx);
        let today = store.today;
        let base = "";
        let feed = match store.detail(self.task) {
            Load::Loading => div().text_color(rgb(MUTED)).child("Loading…"),
            Load::Failed(e) => div().text_color(rgb(DANGER)).child(e.clone()),
            Load::Ready(d) if d.messages.is_empty() => {
                div().text_color(rgb(MUTED)).child("Nothing here yet.")
            }
            Load::Ready(d) => div()
                .flex()
                .flex_col()
                .gap(px(18.))
                .children(d.messages.iter().map(|m| {
                    let when = date::due(m.date.get(..10), today).label;
                    let blocks = html::parse(&m.body, base);
                    div()
                        .flex()
                        .gap(px(12.))
                        .child(Avatar::new(&m.author, person_tone(&[], &m.author)).size(28.))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap(px(4.))
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
                                        .child(
                                            div()
                                                .text_size(px(12.))
                                                .text_color(rgb(MUTED))
                                                .child(when),
                                        ),
                                )
                                .when(!blocks.is_empty(), |d| d.child(RichText::new(blocks)))
                                .children(m.changes.iter().map(|c| {
                                    div()
                                        .flex()
                                        .flex_wrap()
                                        .gap(px(6.))
                                        .text_color(rgb(TEXT_SOFT))
                                        .child(c.old.clone())
                                        .child("→")
                                        .child(
                                            div()
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(rgb(ACCENT))
                                                .child(c.new.clone()),
                                        )
                                        .child(
                                            div()
                                                .text_color(rgb(MUTED))
                                                .child(format!("({})", c.field)),
                                        )
                                })),
                        )
                })),
        };
        div()
            .flex()
            .flex_col()
            .gap(px(22.))
            .child(self.composer(cx))
            .child(feed)
    }
}
