use super::ChildEvent;
use crate::components::{Button, Card, Checkbox, DatePicker, Field, Tint, Variant, icon, kbd};
use crate::format::{hours, hours_text, parse_hours};
use crate::input::{InputEvent, TextInput};
use crate::odoo::WorkItem;
use crate::scale::px;
use crate::store::Store;
use crate::theme::*;
use crate::{ai, date};
use gpui::{
    App, ClickEvent, Context, Entity, EventEmitter, FocusHandle, Focusable, FontWeight,
    SharedString, Window, div, prelude::*, rgb,
};

struct Draft {
    uid: usize,
    done: bool,
    name: Entity<TextInput>,
    hours: Entity<TextInput>,
    date: Entity<DatePicker>,
}

pub struct EntryEditor {
    store: Entity<Store>,
    root: FocusHandle,
    task: i64,
    next_id: usize,
    composer: Option<Entity<TextInput>>,
    refine: Option<Entity<TextInput>>,
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
            next_id: 0,
            composer: None,
            refine: None,
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
        self.refine = None;
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
        if self.refine.is_none() {
            let input = cx.new(|cx| {
                TextInput::new(
                    "Ask the AI to change this, e.g. merge the first two, make each 1h",
                    cx,
                )
            });
            cx.subscribe_in(&input, window, Self::on_refine).detach();
            self.refine = Some(input);
        }
        if let Some(c) = &self.composer {
            window.focus(&c.focus_handle(cx), cx);
        }
        cx.notify();
    }

    pub fn open_manual(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.composer = None;
        self.from_ai = false;
        let draft = self.blank(window, cx);
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
            InputEvent::Submit => self.draft(window, cx),
            InputEvent::Cancel => {
                self.dismiss(window, cx);
            }
            InputEvent::Changed => {
                self.error = None;
                cx.notify();
            }
        }
    }

    fn on_refine(
        &mut self,
        _: &Entity<TextInput>,
        ev: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match ev {
            InputEvent::Submit => self.revise(window, cx),
            InputEvent::Cancel => {
                self.dismiss(window, cx);
            }
            InputEvent::Changed => {
                self.error = None;
                cx.notify();
            }
        }
    }

    fn snapshot(&self, cx: &App) -> Vec<WorkItem> {
        let today = date::iso(self.store.read(cx).today);
        self.drafts
            .iter()
            .map(|d| WorkItem {
                name: d.name.read(cx).text().trim().to_string(),
                hours: parse_hours(d.hours.read(cx).text()).unwrap_or(0.),
                date: d
                    .date
                    .read(cx)
                    .value()
                    .map(str::to_string)
                    .unwrap_or_else(|| today.clone()),
                done: d.done,
            })
            .filter(|i| !i.name.is_empty())
            .collect()
    }

    fn revise(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(refine) = &self.refine else {
            return;
        };
        let instruction = refine.read(cx).text().trim().to_string();
        let store = self.store.read(cx);
        let (Some(task), today) = (store.task(self.task).cloned(), store.today) else {
            return;
        };
        if instruction.is_empty() || self.busy {
            return;
        }
        let current = self.snapshot(cx);
        self.busy = true;
        self.error = None;
        cx.spawn_in(window, async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move { ai::revise_items(&task.name, &current, &instruction, today) })
                .await;
            this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match r {
                    Ok(items) => {
                        this.drafts = items
                            .iter()
                            .map(|i| this.make_draft(i, window, cx))
                            .collect();
                        if let Some(r) = &this.refine {
                            r.update(cx, |i, cx| i.set_text("", cx));
                        }
                    }
                    Err(e) => this.error = Some(format!("AI: {e}").into()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn refine_row(&self, window: &Window, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let input = self.refine.as_ref()?;
        let focused = input.focus_handle(cx).is_focused(window);
        Some(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(
                    div()
                        .flex_1()
                        .h(px(34.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .px(px(11.))
                        .rounded(px(8.))
                        .border_1()
                        .border_color(rgb(if focused { PURPLE.fg } else { AI_BORDER }))
                        .bg(rgb(PAGE))
                        .child(icon("sparkle", 14., PURPLE.fg))
                        .child(div().flex_1().min_w_0().overflow_hidden().child(input.clone())),
                )
                .child(
                    Button::new("revise", if self.busy { "Revising…" } else { "Revise" })
                        .variant(Variant::Ai)
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                            this.revise(window, cx)
                        })),
                )
                .into_any_element(),
        )
    }

    fn make_draft(
        &mut self,
        item: &WorkItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Draft {
        let uid = self.next_id;
        self.next_id += 1;
        let today = self.store.read(cx).today;
        let root = self.root.clone();
        let field = |placeholder: &'static str, value: String, cx: &mut Context<Self>| {
            cx.new(|cx| {
                let mut input = TextInput::new(placeholder, cx);
                input.set_text(&value, cx);
                input
            })
        };
        let initial_day = if date::is_valid(&item.date) {
            item.date.clone()
        } else {
            date::iso(today)
        };
        let picker = cx.new(|cx| {
            let mut p = DatePicker::new(
                ("draft-date", uid),
                "Date",
                "Today",
                today,
                root,
                window,
                cx,
            )
            .required()
            .align_right()
            .full_width()
            .height(34.);
            p.set_value(Some(initial_day), today, cx);
            p
        });
        cx.observe(&picker, |_, _, cx| cx.notify()).detach();
        let name = field("What did you do?", item.name.clone(), cx);
        let hours = field("0h", hours_text(item.hours), cx);
        cx.observe(&hours, |_, _, cx| cx.notify()).detach();
        Draft {
            uid,
            done: item.done,
            name,
            hours,
            date: picker,
        }
    }

    fn blank(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Draft {
        let today = self.store.read(cx).today;
        let item = WorkItem {
            name: String::new(),
            hours: 0.,
            date: date::iso(today),
            done: true,
        };
        self.make_draft(&item, window, cx)
    }

    fn draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
        cx.spawn_in(window, async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move { ai::work_items(&task.name, &log, today) })
                .await;
            this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match r {
                    Ok(items) => {
                        this.drafts = items
                            .iter()
                            .map(|i| this.make_draft(i, window, cx))
                            .collect()
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
            let day = d
                .date
                .read(cx)
                .value()
                .map(str::to_string)
                .unwrap_or_else(|| date::iso(today));
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
            .badge("AI autofill")
            .tint(Tint::Ai)
            .trailing(
                div()
                    .id("composer-close")
                    .size(px(26.))
                    .rounded_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .text_size(px(15.))
                    .text_color(rgb(MUTED))
                    .hover(|s| s.bg(rgb(PAGE)).text_color(rgb(TEXT)))
                    .child("×")
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.dismiss(window, cx);
                    })),
            )
            .error(self.error.clone())
            .child(
                div()
                    .min_h(px(88.))
                    .px(px(12.))
                    .py(px(10.))
                    .rounded(px(8.))
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
                    .pt(px(10.))
                    .border_t_1()
                    .border_color(rgb(AI_BORDER))
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
                            .icon("sparkle")
                            .variant(Variant::Primary)
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.draft(window, cx)
                            })),
                    ),
            )
    }

    fn draft_card(&self, window: &Window, cx: &mut Context<Self>) -> Card {
        let logged: f64 = self
            .drafts
            .iter()
            .filter_map(|d| parse_hours(d.hours.read(cx).text()))
            .sum();
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
        let head = |w: Option<f32>, label: &'static str, center: bool| {
            let d = div()
                .text_size(px(11.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(MUTED))
                .when(center, |el| el.flex().justify_center())
                .child(label);
            match w {
                Some(w) => d.flex_none().w(px(w)),
                None => d.flex_1().pl(px(2.)),
            }
        };
        let (title, badge, tint) = if self.from_ai {
            ("AI draft", "Unsaved · review & edit", Tint::Ai)
        } else {
            ("New sub-tasks", "Unsaved draft", Tint::Neutral)
        };
        Card::new(title)
            .badge(badge)
            .tint(tint)
            .trailing(
                div()
                    .id("draft-close")
                    .size(px(26.))
                    .rounded_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .text_size(px(15.))
                    .text_color(rgb(MUTED))
                    .hover(|s| s.bg(rgb(HOVER)).text_color(rgb(TEXT)))
                    .child("×")
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.dismiss(window, cx);
                    })),
            )
            .error(self.error.clone())
            .children(self.from_ai.then(|| self.refine_row(window, cx)).flatten())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .child(head(None, "Sub-task", false))
                            .child(head(Some(84.), "Hours", false))
                            .child(head(Some(148.), "Date", false))
                            .child(head(Some(40.), "Done", true))
                            .child(div().flex_none().w(px(28.))),
                    )
                    .children(self.drafts.iter().enumerate().map(|(i, d)| {
                        let uid = d.uid;
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .child(div().flex_1().min_w_0().child(Field::new(&d.name)))
                            .child(div().flex_none().w(px(84.)).child(Field::new(&d.hours)))
                            .child(div().flex_none().w(px(148.)).child(d.date.clone()))
                            .child(
                                div()
                                    .id(("draft-done", uid))
                                    .flex_none()
                                    .w(px(40.))
                                    .h(px(34.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .child(Checkbox::new(d.done))
                                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                        if let Some(row) = this.drafts.get_mut(i) {
                                            row.done = !row.done;
                                            cx.notify();
                                        }
                                    })),
                            )
                            .child(
                                div()
                                    .id(("draft-del", uid))
                                    .flex_none()
                                    .size(px(28.))
                                    .rounded_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .text_size(px(15.))
                                    .text_color(rgb(MUTED))
                                    .hover(|s| s.bg(rgb(STAGE_RED.bg)).text_color(rgb(DANGER)))
                                    .child("×")
                                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                        if i < this.drafts.len() {
                                            this.drafts.remove(i);
                                            cx.notify();
                                        }
                                    })),
                            )
                    })),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(10.))
                    .pt(px(12.))
                    .border_t_1()
                    .border_color(rgb(if self.from_ai { AI_BORDER } else { ROW_RULE }))
                    .child(
                        div()
                            .id("draft-add")
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .px(px(10.))
                            .py(px(6.))
                            .rounded(px(7.))
                            .cursor_pointer()
                            .text_size(px(13.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(TEXT_SOFT))
                            .hover(|s| s.bg(rgb(SIDEBAR)).text_color(rgb(TEXT)))
                            .child(icon("plus", 13., MUTED))
                            .child("Add another row")
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                let blank = this.blank(window, cx);
                                window.focus(&blank.name.focus_handle(cx), cx);
                                this.drafts.push(blank);
                                cx.notify()
                            })),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(Button::new("discard", "Discard").on_click(cx.listener(
                                |this, _: &ClickEvent, window, cx| {
                                    this.dismiss(window, cx);
                                },
                            )))
                            .child(
                                Button::new("create", action)
                                    .icon("plus")
                                    .variant(Variant::Primary)
                                    .disabled(n == 0 || self.busy)
                                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                        this.commit(cx)
                                    })),
                            ),
                    ),
            )
    }
}

impl Render for EntryEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let composer = self.composer.clone().filter(|_| self.drafts.is_empty());
        div()
            .children(composer.map(|c| self.composer_card(&c, window, cx)))
            .when(!self.drafts.is_empty(), |d| {
                d.child(self.draft_card(window, cx))
            })
    }
}
