use super::ChildEvent;
use super::entries::EntryEditor;
use crate::components::{Checkbox, GhostInput, icon, tag};
use crate::date;
use crate::format::stage_tone;
use crate::input::{InputEvent, TextInput};
use crate::odoo::{Task, WorkItem};
use crate::store::Store;
use crate::theme::*;
use gpui::{
    ClickEvent, Context, Div, Entity, EventEmitter, FocusHandle, IntoElement, Render, SharedString,
    Stateful, Window, div, prelude::*, px, rgb,
};

#[derive(Clone)]
struct DragSub {
    id: i64,
    name: SharedString,
}

struct DragPreview(SharedString);

impl Render for DragPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(10.))
            .py(px(6.))
            .rounded(px(6.))
            .bg(rgb(PAGE))
            .border_1()
            .border_color(rgb(BORDER))
            .shadow_md()
            .text_size(px(13.))
            .text_color(rgb(TEXT))
            .child(self.0.clone())
    }
}

pub struct SubtaskList {
    store: Entity<Store>,
    root: FocusHandle,
    task: i64,
    quick: Entity<TextInput>,
    entries: Entity<EntryEditor>,
}

impl EventEmitter<ChildEvent> for SubtaskList {}

impl SubtaskList {
    pub fn new(
        store: Entity<Store>,
        root: FocusHandle,
        entries: Entity<EntryEditor>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        cx.observe(&entries, |_, _, cx| cx.notify()).detach();
        let quick = cx.new(|cx| TextInput::new("Add a sub-task…", cx));
        cx.subscribe_in(&quick, window, Self::on_quick).detach();
        Self {
            store,
            root,
            task: 0,
            quick,
            entries,
        }
    }

    pub fn set_task(&mut self, id: i64, cx: &mut Context<Self>) {
        self.task = id;
        self.quick.update(cx, |q, cx| q.set_text("", cx));
        cx.notify();
    }

    fn on_quick(
        &mut self,
        _: &Entity<TextInput>,
        ev: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match ev {
            InputEvent::Submit => self.add(cx),
            InputEvent::Cancel => window.focus(&self.root, cx),
            InputEvent::Changed => {}
        }
    }

    fn add(&mut self, cx: &mut Context<Self>) {
        let name = self.quick.read(cx).text().trim().to_string();
        let Some(task) = self.store.read(cx).task(self.task).cloned() else {
            return;
        };
        if name.is_empty() {
            return;
        }
        let today = self.store.read(cx).today;
        let item = WorkItem {
            name,
            hours: 0.,
            date: date::iso(today),
            done: false,
        };
        self.quick.update(cx, |q, cx| q.set_text("", cx));
        let job = self
            .store
            .update(cx, |s, cx| s.add_work(task, vec![item], cx));
        cx.spawn(async move |this, cx| {
            if let Err(e) = job.await {
                this.update(cx, |_, cx| {
                    cx.emit(ChildEvent::Notice(
                        format!("Couldn’t add the sub-task: {e}").into(),
                    ))
                })
                .ok();
            }
        })
        .detach();
    }

    fn row(&self, i: usize, s: &Task, cx: &mut Context<Self>) -> Stateful<Div> {
        let parent = self.task;
        let id = s.id;
        let name: SharedString = s.name.clone().into();
        let drag = DragSub {
            id,
            name: name.clone(),
        };
        div()
            .id(("sub", i))
            .flex()
            .items_center()
            .gap(px(10.))
            .px(px(6.))
            .py(px(7.))
            .rounded(px(6.))
            .border_t_2()
            .border_color(rgb(PAGE))
            .hover(|s| s.bg(rgb(SIDEBAR)))
            .drag_over::<DragSub>(|d, _, _, _| d.border_color(rgb(ACCENT)))
            .on_drop(cx.listener(move |this, d: &DragSub, _, cx| {
                this.store
                    .update(cx, |s, cx| s.reorder(parent, d.id, id, cx))
            }))
            .child(
                div()
                    .id(("grip", i))
                    .flex_none()
                    .cursor_grab()
                    .child(icon("grip", 14., MUTED))
                    .on_drag(drag, |d, _, _, cx| cx.new(|_| DragPreview(d.name.clone()))),
            )
            .child(
                div()
                    .id(("done", i))
                    .flex_none()
                    .cursor_pointer()
                    .child(Checkbox::new(s.done))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.store.update(cx, |s, cx| s.toggle_done(id, cx))
                    })),
            )
            .child(
                div()
                    .id(("open", i))
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .cursor_pointer()
                    .text_color(rgb(if s.done { MUTED } else { TEXT }))
                    .child(name)
                    .on_click(
                        cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(ChildEvent::Open(id))),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(px(12.))
                    .text_color(rgb(MUTED))
                    .child(s.assignees.join(", ")),
            )
            .child(tag(s.stage.clone(), stage_tone(&s.stage), 11.))
    }
}

impl Render for SubtaskList {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let subs = self.store.read(cx).children_of(self.task);
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(self.entries.clone())
            .child(
                div().flex().flex_col().children(
                    subs.iter()
                        .enumerate()
                        .map(|(i, s)| self.row(i, s, cx))
                        .collect::<Vec<_>>(),
                ),
            )
            .when(subs.is_empty(), |d| {
                d.child(div().text_color(rgb(MUTED)).child("No sub-tasks yet."))
            })
            .child(GhostInput::new(&self.quick, "plus"))
    }
}
