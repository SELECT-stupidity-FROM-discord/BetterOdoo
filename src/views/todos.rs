use crate::components::{
    Button, Checkbox, Chip, DatePicker, Floating, MenuHeader, MenuRow, Variant, icon, tag,
};
use crate::date;
use crate::format::{plural, stage_dot, stage_tone};
use crate::input::{InputEvent, TextInput};
use crate::odoo::Task;
use crate::scale::px;
use crate::store::{Load, Store};
use crate::theme::*;
use gpui::{
    App, ClickEvent, Context, Div, Entity, EventEmitter, FocusHandle, Focusable, FontWeight,
    IntoElement, KeyDownEvent, MouseDownEvent, Render, SharedString, Window, div, prelude::*, rgb,
};
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq)]
enum StatusFilter {
    Open,
    Done,
    All,
}

const STATUSES: [(StatusFilter, &str); 3] = [
    (StatusFilter::Open, "Open"),
    (StatusFilter::Done, "Done"),
    (StatusFilter::All, "All"),
];

pub enum TodosEvent {
    OpenTask(i64),
}

pub struct TodosView {
    store: Entity<Store>,
    root: FocusHandle,
    filter: Entity<TextInput>,
    new_title: Entity<TextInput>,
    new_day: Entity<DatePicker>,
    status: StatusFilter,
    high_only: bool,
    stage: Option<String>,
    stage_menu: bool,
    adding: bool,
    notice: Option<SharedString>,
}

impl EventEmitter<TodosEvent> for TodosView {}

impl TodosView {
    pub fn new(
        store: Entity<Store>,
        root: FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        let today = store.read(cx).today;
        let input = |placeholder: &'static str, cx: &mut Context<Self>| {
            cx.new(|cx| TextInput::new(placeholder, cx))
        };
        let filter = input("Filter to-dos…", cx);
        let new_title = input("Add a to-do…", cx);
        let root_for_cal = root.clone();
        let new_day = cx.new(|cx| {
            DatePicker::new(
                "todo-due",
                "Due date",
                "Due date",
                today,
                root_for_cal,
                window,
                cx,
            )
            .align_right()
        });
        cx.subscribe_in(&filter, window, Self::on_filter).detach();
        cx.subscribe_in(&new_title, window, Self::on_add_input)
            .detach();
        Self {
            store,
            root,
            filter,
            new_title,
            new_day,
            status: StatusFilter::Open,
            high_only: false,
            stage: None,
            stage_menu: false,
            adding: false,
            notice: None,
        }
    }

    pub fn focus_filter(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.filter.focus_handle(cx), cx);
    }

    pub fn dismiss_menu(&mut self, cx: &mut Context<Self>) -> bool {
        let cal = self.new_day.update(cx, |d, cx| d.dismiss(cx));
        let open = self.stage_menu || cal;
        self.stage_menu = false;
        cx.notify();
        open
    }

    pub fn handle_key(
        &mut self,
        ev: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let secondary = ev.keystroke.modifiers.secondary(cx);
        match ev.keystroke.key.as_str() {
            "k" if secondary => self.focus_filter(window, cx),
            "/" => self.focus_filter(window, cx),
            "n" => window.focus(&self.new_title.focus_handle(cx), cx),
            "r" => self.store.update(cx, |s, cx| s.reload_todos(cx)),
            _ => return false,
        }
        true
    }

    fn on_filter(
        &mut self,
        input: &Entity<TextInput>,
        ev: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match ev {
            InputEvent::Changed => {}
            InputEvent::Submit => window.focus(&self.root, cx),
            InputEvent::Cancel => {
                input.update(cx, |i, cx| i.clear(window, cx));
                window.focus(&self.root, cx);
            }
        }
        cx.notify();
    }

    fn on_add_input(
        &mut self,
        _: &Entity<TextInput>,
        ev: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match ev {
            InputEvent::Submit => self.add_todo(cx),
            InputEvent::Cancel => window.focus(&self.root, cx),
            InputEvent::Changed => {}
        }
    }

    fn add_todo(&mut self, cx: &mut Context<Self>) {
        if self.adding {
            return;
        }
        let name = self.new_title.read(cx).text().trim().to_string();
        if name.is_empty() {
            self.notice = Some("Enter a title for the to-do.".into());
            cx.notify();
            return;
        }
        let today = self.store.read(cx).today;
        let deadline = self.new_day.read(cx).value().map(String::from);
        self.adding = true;
        self.notice = None;
        self.new_title.update(cx, |i, cx| i.set_text("", cx));
        self.new_day.update(cx, |d, cx| d.set_value(None, today, cx));
        cx.notify();
        let job = self
            .store
            .update(cx, |s, cx| s.create_todo(name, deadline, cx));
        cx.spawn(async move |this, cx| {
            let r = job.await;
            this.update(cx, |this, cx| {
                this.adding = false;
                if let Err(e) = r {
                    this.notice = Some(format!("Couldn’t create to-do: {e}").into());
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn matches(&self, t: &Task, q: &str) -> bool {
        let status_ok = match self.status {
            StatusFilter::Open => !t.done,
            StatusFilter::Done => t.done,
            StatusFilter::All => true,
        };
        status_ok
            && (!self.high_only || t.priority_high)
            && self
                .stage
                .as_deref()
                .is_none_or(|s| t.personal_stage.as_deref() == Some(s))
            && (q.is_empty()
                || t.name.to_lowercase().contains(q)
                || t.personal_stage
                    .as_deref()
                    .is_some_and(|s| s.to_lowercase().contains(q))
                || t.tags
                    .iter()
                    .any(|(_, tag_name)| tag_name.to_lowercase().contains(q)))
    }

    fn stage_menu(&self, todos: &[Task], cx: &mut Context<Self>) -> Floating {
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for t in todos {
            if let Some(s) = t.personal_stage.as_deref() {
                *counts.entry(s).or_default() += 1;
            }
        }
        let mut stages: Vec<(&str, usize)> = counts.into_iter().collect();
        stages.sort_by_key(|&(s, n)| (std::cmp::Reverse(n), s.to_lowercase()));
        let rows = stages.into_iter().enumerate().map(|(i, (name, count))| {
            let selected = self.stage.as_deref() == Some(name);
            let s = name.to_string();
            let dot = div()
                .flex_none()
                .size(px(9.))
                .rounded_full()
                .bg(rgb(stage_dot(name)));
            MenuRow::new(("todo-stage", i), dot, name.to_string(), count)
                .selected(selected)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.stage = (!selected).then(|| s.clone());
                    this.stage_menu = false;
                    cx.notify()
                }))
        });
        Floating::new("todo-stage-menu")
            .max_height(380.)
            .on_dismiss(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.stage_menu = false;
                cx.notify()
            }))
            .child(MenuHeader::new("Personal stage").on_clear(
                self.stage.is_some(),
                cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.stage = None;
                    this.stage_menu = false;
                    cx.notify()
                }),
            ))
            .children(rows)
    }

    fn quick_add(&self, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .p(px(10.))
                    .rounded(px(10.))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .bg(rgb(PAGE))
                    .child(icon("todo", 15., MUTED))
                    .child(div().flex_1().min_w_0().child(self.new_title.clone()))
                    .child(self.new_day.clone())
                    .child(
                        Button::new("todo-add", if self.adding { "Adding…" } else { "Add" })
                            .icon("plus")
                            .variant(Variant::Primary)
                            .disabled(self.adding)
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.add_todo(cx))),
                    ),
            )
            .when_some(self.notice.clone(), |d, msg| {
                d.child(div().text_size(px(12.)).text_color(rgb(DANGER)).child(msg))
            })
    }

    fn toolbar(&self, todos: &[Task], window: &Window, cx: &mut Context<Self>) -> Div {
        let focused = self.filter.focus_handle(cx).is_focused(window);
        let has_stages = todos.iter().any(|t| t.personal_stage.is_some());
        let stage_open = self.stage_menu;
        let stage_label = match &self.stage {
            Some(s) => format!("Stage: {s}"),
            None => "Stage".into(),
        };
        let mut chips = div().flex().flex_wrap().items_center().gap(px(6.));
        for (i, (status, label)) in STATUSES.into_iter().enumerate() {
            chips = chips.child(
                Chip::new(("todo-status", i), label)
                    .selected(self.status == status)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.status = status;
                        this.stage_menu = false;
                        cx.notify()
                    })),
            );
        }
        chips = chips
            .child(
                div()
                    .flex_none()
                    .w(px(1.))
                    .h(px(18.))
                    .mx(px(2.))
                    .bg(rgb(BORDER)),
            )
            .child(
                Chip::new("todo-high", "High priority")
                    .selected(self.high_only)
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.high_only = !this.high_only;
                        this.stage_menu = false;
                        cx.notify()
                    })),
            );
        if has_stages {
            chips = chips.child(
                div()
                    .child(
                        Chip::new("todo-stage-chip", stage_label)
                            .selected(self.stage.is_some())
                            .open(stage_open)
                            .dropdown()
                            .on_press(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                                this.stage_menu = !stage_open;
                                cx.notify()
                            })),
                    )
                    .when(stage_open, |d| d.child(self.stage_menu(todos, cx))),
            );
        }

        div()
            .flex()
            .items_start()
            .gap(px(12.))
            .pb(px(10.))
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(chips.flex_1().min_w_0())
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .w(px(240.))
                    .px(px(10.))
                    .py(px(5.))
                    .rounded(px(6.))
                    .border_1()
                    .border_color(rgb(if focused { ACCENT } else { BORDER }))
                    .text_size(px(13.))
                    .child(icon("search", 14., if focused { ACCENT } else { MUTED }))
                    .child(self.filter.clone()),
            )
    }
}

impl Render for TodosView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let store = self.store.read(cx);
        let today = store.today;
        let q = self.filter.read(cx).text().trim().to_lowercase();

        let (summary, all_todos, body) = match store.todos() {
            Load::Loading => (
                "Loading to-dos…".into(),
                Vec::new(),
                div()
                    .pt(px(12.))
                    .text_color(rgb(MUTED))
                    .child("Loading…")
                    .into_any_element(),
            ),
            Load::Failed(err) => (
                "Couldn’t load to-dos".into(),
                Vec::new(),
                div()
                    .pt(px(16.))
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap(px(10.))
                    .child(div().text_color(rgb(DANGER)).child(err.clone()))
                    .child(
                        Button::new("todos-retry", "Try again").on_click(cx.listener(
                            |this, _: &ClickEvent, _, cx| {
                                this.store.update(cx, |s, cx| s.reload_todos(cx));
                            },
                        )),
                    )
                    .into_any_element(),
            ),
            Load::Ready(todos) => {
                let all_todos = todos.clone();
                let open_count = todos.iter().filter(|t| !t.done).count();
                let done_count = todos.len() - open_count;
                let high_count = todos.iter().filter(|t| !t.done && t.priority_high).count();
                let mut summary = format!("{open_count} open · {done_count} done");
                if high_count > 0 {
                    summary += &format!(" · {high_count} high priority");
                }

                let visible: Vec<&Task> = todos.iter().filter(|t| self.matches(t, &q)).collect();
                let body = if visible.is_empty() {
                    div()
                        .pt(px(24.))
                        .text_color(rgb(MUTED))
                        .child(if todos.is_empty() {
                            "No to-dos yet. Add one above to get started."
                        } else {
                            "No to-dos match these filters."
                        })
                        .into_any_element()
                } else {
                    let rows = visible.into_iter().map(|t| {
                        let id = t.id;
                        let mut due = date::due(t.deadline.as_deref(), today);
                        due.overdue &= !t.done;
                        div()
                            .id(("todo-row", id as usize))
                            .flex_none()
                            .w_full()
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .px(px(10.))
                            .py(px(9.))
                            .min_h(px(44.))
                            .border_t_1()
                            .border_color(rgb(ROW_RULE))
                            .rounded(px(6.))
                            .hover(|s| s.bg(rgb(SIDEBAR)))
                            .child(
                                div()
                                    .id(("todo-check", id as usize))
                                    .flex_none()
                                    .cursor_pointer()
                                    .child(Checkbox::new(t.done))
                                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                        this.store.update(cx, |s, cx| s.toggle_done(id, cx));
                                    })),
                            )
                            .child(
                                div()
                                    .id(("todo-open", id as usize))
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .cursor_pointer()
                                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                                        cx.emit(TodosEvent::OpenTask(id))
                                    }))
                                    .child(
                                        div()
                                            .min_w_0()
                                            .truncate()
                                            .font_weight(if t.done {
                                                FontWeight::NORMAL
                                            } else {
                                                FontWeight::MEDIUM
                                            })
                                            .when(t.done, |d| {
                                                d.text_color(rgb(MUTED)).line_through()
                                            })
                                            .child(t.name.clone()),
                                    )
                                    .children(t.tags.iter().take(2).map(|(_, label)| {
                                        tag(label.clone(), BLUE, 11.).px(px(6.)).py(px(1.))
                                    })),
                            )
                            .when_some(t.personal_stage.clone(), |d, stage| {
                                d.child(
                                    tag(stage.clone(), stage_tone(&stage), 11.)
                                        .px(px(7.))
                                        .py(px(2.)),
                                )
                            })
                            .child(
                                div()
                                    .id(("todo-prio", id as usize))
                                    .flex_none()
                                    .cursor_pointer()
                                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                        this.store.update(cx, |s, cx| s.toggle_priority(id, cx));
                                    }))
                                    .child(if t.priority_high {
                                        tag("High", ORANGE, 11.).px(px(7.)).py(px(2.))
                                    } else {
                                        div()
                                            .px(px(7.))
                                            .py(px(2.))
                                            .rounded(px(4.))
                                            .text_size(px(11.))
                                            .text_color(rgb(MUTED))
                                            .hover(|s| s.bg(rgb(HOVER)))
                                            .child("Priority")
                                    }),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .w(px(96.))
                                    .flex()
                                    .justify_end()
                                    .text_size(px(13.))
                                    .when(due.overdue, |d| {
                                        d.text_color(rgb(DANGER)).font_weight(FontWeight::MEDIUM)
                                    })
                                    .when(!due.overdue, |d| d.text_color(rgb(TEXT_SOFT)))
                                    .child(due.label),
                            )
                    });
                    div()
                        .id("todos-scroll")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .w_full()
                                .flex_none()
                                .flex()
                                .flex_col()
                                .pb(px(32.))
                                .children(rows),
                        )
                        .into_any_element()
                };
                (
                    if todos.is_empty() {
                        plural(0, "to-do")
                    } else {
                        summary
                    },
                    all_todos,
                    body,
                )
            }
        };

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .h_full()
            .pt(px(40.))
            .px(px(48.))
            .gap(px(18.))
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap(px(12.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(
                                div()
                                    .text_size(px(32.))
                                    .font_weight(FontWeight::BOLD)
                                    .child("To-Dos"),
                            )
                            .child(div().text_color(rgb(MUTED)).child(summary)),
                    )
                    .child(
                        div()
                            .pt(px(6.))
                            .child(Button::new("todos-refresh", "Refresh").on_click(cx.listener(
                                |this, _: &ClickEvent, _, cx| {
                                    this.store.update(cx, |s, cx| s.reload_todos(cx));
                                },
                            ))),
                    ),
            )
            .child(self.quick_add(cx))
            .child(self.toolbar(&all_todos, window, cx))
            .child(body)
    }
}
