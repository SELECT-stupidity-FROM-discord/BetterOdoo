use crate::components::{Button, Chip, DatePicker, Floating, Variant, icon};
use crate::input::{InputEvent, TextInput};
use crate::scale::px;
use crate::store::Store;
use crate::theme::*;
use gpui::{
    ClickEvent, Context, Entity, FocusHandle, Focusable, FontWeight, IntoElement, MouseDownEvent,
    Render, SharedString, Window, div, prelude::*, rgb,
};

pub struct NewTask {
    store: Entity<Store>,
    root: FocusHandle,
    title: Entity<TextInput>,
    due: Entity<DatePicker>,
    projects: Vec<(i64, String)>,
    project: Option<i64>,
    open: bool,
    busy: bool,
    notice: Option<SharedString>,
}

impl NewTask {
    pub fn new(
        store: Entity<Store>,
        root: FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let today = store.read(cx).today;
        let title = cx.new(|cx| TextInput::new("Task title…", cx));
        let due = cx.new(|cx| {
            DatePicker::new(
                "new-task-due",
                "Due date",
                "Due date",
                today,
                root.clone(),
                window,
                cx,
            )
        });
        cx.subscribe_in(&title, window, Self::on_title).detach();
        Self {
            store,
            root,
            title,
            due,
            projects: Vec::new(),
            project: None,
            open: false,
            busy: false,
            notice: None,
        }
    }

    pub fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.open {
            self.dismiss(cx);
            return;
        }
        self.open = true;
        window.focus(&self.title.focus_handle(cx), cx);
        let job = self.store.update(cx, |s, cx| {
            s.search_options("project.project", String::new(), cx)
        });
        cx.spawn(async move |this, cx| {
            if let Ok(list) = job.await {
                this.update(cx, |this, cx| {
                    this.projects = list;
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
        cx.notify();
    }

    pub fn dismiss(&mut self, cx: &mut Context<Self>) -> bool {
        let was = std::mem::take(&mut self.open);
        if was {
            self.due.update(cx, |d, cx| d.dismiss(cx));
            cx.notify();
        }
        was
    }

    fn on_title(
        &mut self,
        _: &Entity<TextInput>,
        ev: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match ev {
            InputEvent::Submit => self.create(cx),
            InputEvent::Cancel => {
                self.dismiss(cx);
                window.focus(&self.root, cx);
            }
            InputEvent::Changed => {}
        }
    }

    fn create(&mut self, cx: &mut Context<Self>) {
        let name = self.title.read(cx).text().trim().to_string();
        if self.busy || name.is_empty() {
            self.notice = name.is_empty().then(|| "Enter a title.".into());
            cx.notify();
            return;
        }
        let deadline = self.due.read(cx).value().map(String::from);
        let project = self.project;
        self.busy = true;
        self.notice = None;
        cx.notify();
        let job = self
            .store
            .update(cx, |s, cx| s.create_todo(name, deadline, project, cx));
        cx.spawn(async move |this, cx| {
            let r = job.await;
            this.update(cx, |this, cx| {
                this.busy = false;
                match r {
                    Ok(()) => {
                        let today = this.store.read(cx).today;
                        this.title.update(cx, |i, cx| i.set_text("", cx));
                        this.due.update(cx, |d, cx| d.set_value(None, today, cx));
                        this.project = None;
                        this.open = false;
                    }
                    Err(e) => this.notice = Some(format!("Couldn’t create task: {e}").into()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn form(&self, cx: &mut Context<Self>) -> Floating {
        let chips = self.projects.iter().map(|&(id, ref name)| {
            Chip::new(("new-task-project", id as usize), name.clone())
                .selected(self.project == Some(id))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.project = (this.project != Some(id)).then_some(id);
                    cx.notify()
                }))
        });
        Floating::new("new-task-form")
            .above(10.)
            .width(340.)
            .padded(12., 10.)
            .on_dismiss(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.dismiss(cx);
            }))
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("New task"),
            )
            .child(
                div()
                    .px(px(10.))
                    .py(px(6.))
                    .rounded(px(8.))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .child(self.title.clone()),
            )
            .child(div().flex().flex_wrap().gap(px(6.)).children(chips))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(self.due.clone())
                    .child(
                        Button::new(
                            "new-task-create",
                            if self.busy { "Creating…" } else { "Create" },
                        )
                        .variant(Variant::Primary)
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.create(cx))),
                    ),
            )
            .when_some(self.notice.clone(), |f, msg| {
                f.child(div().text_size(px(12.)).text_color(rgb(DANGER)).child(msg))
            })
    }
}

impl Render for NewTask {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .absolute()
            .right(px(24.))
            .bottom(px(24.))
            .child(
                div()
                    .id("new-task-fab")
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(48.))
                    .rounded_full()
                    .bg(rgb(ACCENT))
                    .cursor_pointer()
                    .hover(|s| s.opacity(0.92))
                    .shadow_lg()
                    .child(icon("plus", 20., 0xffffff))
                    .on_click(
                        cx.listener(|this, _: &ClickEvent, window, cx| this.toggle(window, cx)),
                    ),
            )
            .when(self.open, |d| d.child(self.form(cx)))
    }
}
