use super::ChildEvent;
use crate::components::{Button, Card, CheckCircle, Field, Tint, Variant, icon, kbd};
use crate::format::{hours, hours_text, parse_hours};
use crate::input::{InputEvent, TextInput};
use crate::odoo::WorkItem;
use crate::store::Store;
use crate::theme::*;
use crate::{ai, date};
use gpui::{
    App, ClickEvent, Context, Entity, EventEmitter, FocusHandle, Focusable, SharedString, Window,
    div, prelude::*, px, rgb,
};

struct Draft {
    done: bool,
    name: Entity<TextInput>,
    hours: Entity<TextInput>,
    date: Entity<TextInput>,
}

pub struct EntryEditor {
    store: Entity<Store>,
    root: FocusHandle,
    task: i64,
    composer: Option<Entity<TextInput>>,
    drafts: Vec<Draft>,
    from_ai: bool,
    busy: bool,
    error: Option<SharedString>,
}

impl EventEmitter<ChildEvent> for EntryEditor {}

impl EntryEditor {
    pub fn new(store: Entity<Store>, root: FocusHandle) -> Self {
        Self {
            store,
            root,
            task: 0,
            composer: None,
            drafts: Vec::new(),
            from_ai: false,
            busy: false,
            error: None,
        }
    }

    pub fn set_task(&mut self, id: i64, cx: &mut Context<Self>) {
        self.task = id;
        self.reset(cx);
    }

    pub fn is_open(&self) -> bool {
        self.composer.is_some() || !self.drafts.is_empty()
    }

    pub fn has_drafts(&self) -> bool {
        !self.drafts.is_empty()
    }

    pub fn reset(&mut self, cx: &mut Context<Self>) {
        self.composer = None;
        self.drafts.clear();
        self.busy = false;
        self.error = None;
        cx.notify();
    }

    pub fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let open = self.is_open();
        self.reset(cx);
        if open {
            window.focus(&self.root, cx);
        }
        open
    }

    pub fn open_ai(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.drafts.clear();
        self.from_ai = true;
        if self.composer.is_none() {
            let input = cx.new(|cx| {
                TextInput::new(
                    "Fixed the ETA card and added driver contact, 2h. Push notifications yesterday, 3h.",
                    cx,
                )
                .multiline()
            });
            cx.subscribe_in(&input, window, Self::on_composer).detach();
            self.composer = Some(input);
        }
        if let Some(c) = &self.composer {
            window.focus(&c.focus_handle(cx), cx);
        }
        cx.notify();
    }

    pub fn open_manual(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.composer = None;
        self.from_ai = false;
        let draft = self.blank(cx);
        window.focus(&draft.name.focus_handle(cx), cx);
        self.drafts = vec![draft];
        cx.notify();
    }

    fn on_composer(
        &mut self,
        _: &Entity<TextInput>,
        ev: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match ev {
            InputEvent::Submit => self.draft(cx),
            InputEvent::Cancel => {
                self.dismiss(window, cx);
            }
            InputEvent::Changed => {
                self.error = None;
                cx.notify();
            }
        }
    }

    fn make_draft(&self, item: &WorkItem, cx: &mut Context<Self>) -> Draft {
        let field = |placeholder: &'static str, value: String, cx: &mut Context<Self>| {
            cx.new(|cx| {
                let mut input = TextInput::new(placeholder, cx);
                input.set_text(&value, cx);
                input
            })
        };
        Draft {
            done: item.done,
            name: field("What did you do?", item.name.clone(), cx),
            hours: field("Hours", hours_text(item.hours), cx),
            date: field("YYYY-MM-DD", item.date.clone(), cx),
        }
    }

    fn blank(&self, cx: &mut Context<Self>) -> Draft {
        let today = self.store.read(cx).today;
        let item = WorkItem {
            name: String::new(),
            hours: 0.,
            date: date::iso(today),
            done: true,
        };
        self.make_draft(&item, cx)
    }

    fn draft(&mut self, cx: &mut Context<Self>) {
        let Some(composer) = &self.composer else {
            return;
        };
        let log = composer.read(cx).text().trim().to_string();
        let store = self.store.read(cx);
        let (Some(task), today) = (store.task(self.task).cloned(), store.today) else {
            return;
        };
        if log.is_empty() || self.busy {
            return;
        }
        self.busy = true;
        self.error = None;
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move { ai::work_items(&task.name, &log, today) })
                .await;
            this.update(cx, |this, cx| {
                this.busy = false;
                match r {
                    Ok(items) => {
                        this.drafts = items.iter().map(|i| this.make_draft(i, cx)).collect()
                    }
                    Err(e) => this.error = Some(format!("AI: {e}").into()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn read(&self, cx: &App) -> Result<Vec<WorkItem>, String> {
        let today = self.store.read(cx).today;
        let mut items = Vec::new();
        for (n, d) in self.drafts.iter().enumerate() {
            let row = n + 1;
            let name = d.name.read(cx).text().trim().to_string();
            if name.is_empty() {
                return Err(format!("Row {row} needs a description"));
            }
            let hours = parse_hours(d.hours.read(cx).text())
                .ok_or(format!("Row {row}: write hours like 1.5 or 1h 30m"))?;
            let day = d.date.read(cx).text().trim().to_string();
            let day = if day.is_empty() {
                date::iso(today)
            } else {
                day
            };
            if !date::is_valid(&day) {
                return Err(format!("Row {row}: write the date as YYYY-MM-DD"));
            }
            items.push(WorkItem {
                name,
                hours,
                date: day,
                done: d.done,
            });
        }
        Ok(items)
    }

    pub fn commit(&mut self, cx: &mut Context<Self>) {
        if self.drafts.is_empty() || self.busy {
            return;
        }
        let Some(task) = self.store.read(cx).task(self.task).cloned() else {
            return;
        };
        let items = match self.read(cx) {
            Ok(items) => items,
            Err(e) => {
                self.error = Some(e.into());
                cx.notify();
                return;
            }
        };
        self.error = None;
        self.busy = true;
        let summary = (items.len(), items.iter().map(|i| i.hours).sum::<f64>());
        let job = self.store.update(cx, |s, cx| s.add_work(task, items, cx));
        cx.spawn(async move |this, cx| {
            let r = job.await;
            this.update(cx, |this, cx| {
                this.busy = false;
                match r {
                    Ok(()) => {
                        let (n, logged) = summary;
                        this.reset(cx);
                        this.store.update(cx, |s, cx| {
                            s.status = format!(
                                "Added {} · logged {}",
                                crate::format::plural(n, "sub-task"),
                                hours(logged)
                            )
                            .into();
                            cx.notify();
                        });
                    }
                    Err(e) => this.error = Some(e),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn composer_card(
        &self,
        composer: &Entity<TextInput>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Card {
        let focused = composer.focus_handle(cx).is_focused(window);
        let submit = if cfg!(target_os = "macos") {
            "⌘"
        } else {
            "Ctrl"
        };
        Card::new("Describe what you did")
            .tint(Tint::Ai)
            .error(self.error.clone())
            .child(
                div()
                    .min_h(px(88.))
                    .px(px(12.))
                    .py(px(10.))
                    .rounded(px(6.))
                    .border_1()
                    .border_color(rgb(if focused { PURPLE.fg } else { AI_BORDER }))
                    .bg(rgb(PAGE))
                    .line_height(px(21.))
                    .child(composer.clone()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .items_center()
                            .gap(px(4.))
                            .text_size(px(12.))
                            .text_color(rgb(MUTED))
                            .child(kbd(submit))
                            .child(kbd("↵"))
                            .child("to draft · Esc to cancel"),
                    )
                    .child(
                        Button::new("draft", if self.busy { "Drafting…" } else { "Draft" })
                            .variant(Variant::Primary)
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.draft(cx))),
                    ),
            )
    }

    fn draft_card(&self, cx: &mut Context<Self>) -> Card {
        let items = self.read(cx).unwrap_or_default();
        let logged: f64 = items.iter().map(|i| i.hours).sum();
        let n = self.drafts.len();
        let action = format!(
            "Add {}{}",
            crate::format::plural(n, "sub-task"),
            if logged > 0. {
                format!(" · log {}", hours(logged))
            } else {
                String::new()
            }
        );
        let head = |w: Option<f32>, label: &'static str| {
            let d = div().text_size(px(11.)).text_color(rgb(MUTED)).child(label);
            match w {
                Some(w) => d.flex_none().w(px(w)),
                None => d.flex_1(),
            }
        };
        let (title, tint) = if self.from_ai {
            ("Drafted by AI · edit anything, nothing saved yet", Tint::Ai)
        } else {
            ("New entries · nothing saved yet", Tint::Neutral)
        };
        Card::new(title)
            .tint(tint)
            .error(self.error.clone())
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(head(None, "Sub-task"))
                    .child(head(Some(70.), "Hours"))
                    .child(head(Some(108.), "Date"))
                    .child(head(Some(36.), "Done"))
                    .child(div().flex_none().w(px(20.))),
            )
            .children(self.drafts.iter().enumerate().map(|(i, d)| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(div().flex_1().min_w_0().child(Field::new(&d.name)))
                    .child(div().flex_none().w(px(70.)).child(Field::new(&d.hours)))
                    .child(div().flex_none().w(px(108.)).child(Field::new(&d.date)))
                    .child(
                        div()
                            .id(("draft-done", i))
                            .flex_none()
                            .w(px(36.))
                            .cursor_pointer()
                            .child(CheckCircle::new(d.done))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.drafts[i].done = !this.drafts[i].done;
                                cx.notify()
                            })),
                    )
                    .child(
                        div()
                            .id(("draft-del", i))
                            .flex_none()
                            .w(px(20.))
                            .cursor_pointer()
                            .text_color(rgb(MUTED))
                            .hover(|s| s.text_color(rgb(DANGER)))
                            .child("×")
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.drafts.remove(i);
                                cx.notify()
                            })),
                    )
            }))
            .child(
                div()
                    .id("draft-add")
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .cursor_pointer()
                    .text_size(px(13.))
                    .text_color(rgb(TEXT_SOFT))
                    .hover(|s| s.text_color(rgb(TEXT)))
                    .child(icon("plus", 13., MUTED))
                    .child("Add another row")
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        let blank = this.blank(cx);
                        this.drafts.push(blank);
                        cx.notify()
                    })),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .mt(px(4.))
                    .child(
                        Button::new("create", action)
                            .variant(Variant::Primary)
                            .disabled(n == 0 || self.busy)
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.commit(cx))),
                    )
                    .child(Button::new("discard", "Discard").on_click(cx.listener(
                        |this, _: &ClickEvent, window, cx| {
                            this.dismiss(window, cx);
                        },
                    ))),
            )
    }
}

impl Render for EntryEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let composer = self.composer.clone().filter(|_| self.drafts.is_empty());
        div()
            .children(composer.map(|c| self.composer_card(&c, window, cx)))
            .when(!self.drafts.is_empty(), |d| d.child(self.draft_card(cx)))
    }
}
