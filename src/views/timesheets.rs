use crate::components::{
    Avatar, Button, Chip, DatePicker, Floating, MenuHeader, MenuRow, Variant, icon,
};
use crate::date;
use crate::format::{hours, parse_hours, person_tone, plural, short_name};
use crate::input::{InputEvent, TextInput};
use crate::odoo::{Task, Timesheet, WorkItem};
use crate::scale::px;
use crate::store::{Load, Store};
use crate::theme::*;
use gpui::{
    App, ClickEvent, Context, Div, Entity, EventEmitter, FocusHandle, Focusable, FontWeight,
    IntoElement, KeyDownEvent, MouseDownEvent, Render, SharedString, Window, div, prelude::*, rgb,
};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Clone, Copy, PartialEq)]
enum Period {
    All,
    Today,
    ThisWeek,
    ThisMonth,
}

const PERIODS: [(Period, &str); 4] = [
    (Period::All, "All"),
    (Period::Today, "Today"),
    (Period::ThisWeek, "This week"),
    (Period::ThisMonth, "This month"),
];

pub enum TimesheetsEvent {
    OpenTask(i64),
}

pub struct TimesheetsView {
    store: Entity<Store>,
    root: FocusHandle,
    filter: Entity<TextInput>,
    period: Period,
    person: Option<String>,
    project: Option<String>,
    person_menu: bool,
    project_menu: bool,
    composing: bool,
    task_menu: bool,
    picked_task: Option<i64>,
    task_query: Entity<TextInput>,
    work_name: Entity<TextInput>,
    work_hours: Entity<TextInput>,
    work_day: Entity<DatePicker>,
    notice: Option<SharedString>,
}

impl EventEmitter<TimesheetsEvent> for TimesheetsView {}

impl TimesheetsView {
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
        let filter = input("Filter timesheets…", cx);
        let task_query = input("Search tasks…", cx);
        let work_name = input("What did you work on?", cx);
        let work_hours = input("1h 30m", cx);
        let root_for_cal = root.clone();
        let work_day = cx
            .new(|cx| DatePicker::new("ts-day", "Date", "Today", today, root_for_cal, window, cx));
        cx.subscribe_in(&filter, window, Self::on_filter).detach();
        cx.subscribe_in(&task_query, window, Self::on_task_query)
            .detach();
        for i in [&work_name, &work_hours] {
            cx.subscribe_in(i, window, Self::on_work_input).detach();
        }
        Self {
            store,
            root,
            filter,
            period: Period::All,
            person: None,
            project: None,
            person_menu: false,
            project_menu: false,
            composing: false,
            task_menu: false,
            picked_task: None,
            task_query,
            work_name,
            work_hours,
            work_day,
            notice: None,
        }
    }

    pub fn focus_filter(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.filter.focus_handle(cx), cx);
    }

    pub fn dismiss_menu(&mut self, cx: &mut Context<Self>) -> bool {
        let cal = self.work_day.update(cx, |d, cx| d.dismiss(cx));
        let open = self.person_menu || self.project_menu || self.task_menu || cal;
        self.close_menus();
        cx.notify();
        open
    }

    fn close_menus(&mut self) {
        self.person_menu = false;
        self.project_menu = false;
        self.task_menu = false;
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
            "r" => self.store.update(cx, |s, cx| s.reload_timesheets(cx)),
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

    fn on_task_query(
        &mut self,
        _: &Entity<TextInput>,
        ev: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match ev {
            InputEvent::Changed => {}
            InputEvent::Cancel => {
                self.task_menu = false;
                window.focus(&self.root, cx);
            }
            InputEvent::Submit => {
                if let Some(task) = self.filtered_tasks(cx).first() {
                    self.picked_task = Some(task.id);
                    self.task_menu = false;
                    window.focus(&self.work_name.focus_handle(cx), cx);
                }
            }
        }
        cx.notify();
    }

    fn on_work_input(
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

    fn filtered_tasks(&self, cx: &App) -> Vec<Task> {
        let q = self.task_query.read(cx).text().trim().to_lowercase();
        self.store
            .read(cx)
            .tasks
            .iter()
            .filter(|t| {
                q.is_empty()
                    || t.name.to_lowercase().contains(&q)
                    || t.project.to_lowercase().contains(&q)
            })
            .take(40)
            .cloned()
            .collect()
    }

    fn log(&mut self, cx: &mut Context<Self>) {
        let store = self.store.read(cx);
        let today = store.today;
        let Some(task) = self.picked_task.and_then(|id| store.task(id).cloned()) else {
            self.notice = Some("Pick a task first.".into());
            cx.notify();
            return;
        };
        let Some(h) = parse_hours(self.work_hours.read(cx).text()).filter(|h| *h > 0.) else {
            self.notice = Some("Enter the hours, like 1.5 or 1h 30m.".into());
            cx.notify();
            return;
        };
        let day = self
            .work_day
            .read(cx)
            .value()
            .map(String::from)
            .unwrap_or_else(|| date::iso(today));
        let name = self.work_name.read(cx).text().trim().to_string();
        if name.is_empty() {
            self.notice = Some("Describe the work.".into());
            cx.notify();
            return;
        }
        let item = WorkItem {
            name,
            hours: h,
            date: day,
            done: false,
        };
        self.notice = None;
        self.work_name.update(cx, |i, cx| i.set_text("", cx));
        self.work_hours.update(cx, |i, cx| i.set_text("", cx));
        let job = self
            .store
            .update(cx, |s, cx| s.add_timesheet(task, item, cx));
        cx.spawn(async move |this, cx| {
            if let Err(e) = job.await {
                this.update(cx, |this, cx| {
                    this.notice = Some(format!("Couldn’t log the time: {e}").into());
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    fn matches(&self, t: &Timesheet, today: i64, q: &str) -> bool {
        let in_period = match self.period {
            Period::All => true,
            Period::Today => date::days_from_iso(&t.date) == Some(today),
            Period::ThisWeek => {
                let start = date::week_start(today);
                date::days_from_iso(&t.date).is_some_and(|d| d >= start && d <= today)
            }
            Period::ThisMonth => {
                let start = date::month_start(today);
                date::days_from_iso(&t.date).is_some_and(|d| d >= start && d <= today)
            }
        };
        in_period
            && self.person.as_ref().is_none_or(|p| &t.user == p)
            && self.project.as_ref().is_none_or(|p| &t.project == p)
            && (q.is_empty()
                || t.name.to_lowercase().contains(q)
                || t.project.to_lowercase().contains(q)
                || t.user.to_lowercase().contains(q)
                || t.task
                    .as_ref()
                    .is_some_and(|(_, name)| name.to_lowercase().contains(q)))
    }

    fn person_menu(
        &self,
        entries: &[Timesheet],
        all_people: &[String],
        cx: &mut Context<Self>,
    ) -> Floating {
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for t in entries {
            *counts.entry(t.user.as_str()).or_default() += 1;
        }
        let mut people: Vec<(&str, usize)> = counts.into_iter().collect();
        people.sort_by_key(|&(n, c)| (n != "Me", std::cmp::Reverse(c), n.to_lowercase()));
        let rows = people.into_iter().enumerate().map(|(i, (name, count))| {
            let selected = self.person.as_deref() == Some(name);
            let who = name.to_string();
            MenuRow::new(
                ("ts-person-row", i),
                Avatar::new(name, person_tone(all_people, name)).size(22.),
                short_name(name).to_string(),
                count,
            )
            .selected(selected)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.person = (!selected).then(|| who.clone());
                this.close_menus();
                cx.notify()
            }))
        });
        Floating::new("ts-person-menu")
            .max_height(380.)
            .on_dismiss(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.close_menus();
                cx.notify()
            }))
            .child(MenuHeader::new("Filter by person").on_clear(
                self.person.is_some(),
                cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.person = None;
                    this.close_menus();
                    cx.notify()
                }),
            ))
            .children(rows)
    }

    fn project_menu(&self, entries: &[Timesheet], cx: &mut Context<Self>) -> Floating {
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for t in entries {
            *counts.entry(t.project.as_str()).or_default() += 1;
        }
        let mut projects: Vec<(&str, usize)> = counts.into_iter().collect();
        projects.sort_by_key(|&(n, c)| (std::cmp::Reverse(c), n.to_lowercase()));
        let rows = projects.into_iter().enumerate().map(|(i, (name, count))| {
            let selected = self.project.as_deref() == Some(name);
            let proj = name.to_string();
            let dot = div()
                .flex_none()
                .size(px(8.))
                .rounded(px(2.))
                .bg(rgb(PEOPLE[i % PEOPLE.len()].fg));
            MenuRow::new(("ts-project-row", i), dot, name.to_string(), count)
                .selected(selected)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.project = (!selected).then(|| proj.clone());
                    this.close_menus();
                    cx.notify()
                }))
        });
        Floating::new("ts-project-menu")
            .max_height(380.)
            .on_dismiss(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.close_menus();
                cx.notify()
            }))
            .child(MenuHeader::new("Filter by project").on_clear(
                self.project.is_some(),
                cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.project = None;
                    this.close_menus();
                    cx.notify()
                }),
            ))
            .children(rows)
    }

    fn task_picker(&self, cx: &mut Context<Self>) -> Floating {
        let tasks = self.filtered_tasks(cx);
        let mut menu = Floating::new("ts-task-picker")
            .width(360.)
            .max_height(380.)
            .on_dismiss(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.task_menu = false;
                cx.notify()
            }))
            .child(MenuHeader::new("Select task"))
            .child(
                div()
                    .flex_none()
                    .mx(px(4.))
                    .mb(px(4.))
                    .px(px(10.))
                    .py(px(5.))
                    .rounded(px(6.))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .child(self.task_query.clone()),
            );
        if tasks.is_empty() {
            menu = menu.child(
                div()
                    .flex_none()
                    .px(px(10.))
                    .py(px(8.))
                    .text_size(px(13.))
                    .text_color(rgb(MUTED))
                    .child("No matching tasks"),
            );
        } else {
            menu = menu.children(tasks.into_iter().enumerate().map(|(i, t)| {
                let id = t.id;
                let on = self.picked_task == Some(id);
                div()
                    .id(("ts-pick-task", i))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .min_h(px(36.))
                    .py(px(6.))
                    .px(px(10.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .text_size(px(13.))
                    .hover(|s| s.bg(rgb(HOVER)))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(div().truncate().child(t.name))
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(11.))
                                    .text_color(rgb(MUTED))
                                    .child(t.project),
                            ),
                    )
                    .child(
                        div()
                            .flex_none()
                            .size(px(14.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .when(on, |d| d.child(icon("check", 14., ACCENT))),
                    )
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.picked_task = Some(id);
                        this.task_menu = false;
                        this.notice = None;
                        window.focus(&this.work_name.focus_handle(cx), cx);
                        cx.notify()
                    }))
            }));
        }
        menu
    }

    fn composer(&self, cx: &mut Context<Self>) -> Div {
        let pill = |icon_name: &'static str, width: f32, input: &Entity<TextInput>| {
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
        };
        let picked_label = self
            .picked_task
            .and_then(|id| self.store.read(cx).task(id).map(|t| t.name.clone()));
        let task_open = self.task_menu;
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .p(px(14.))
            .rounded(px(10.))
            .border_1()
            .border_color(rgb(BORDER))
            .bg(rgb(PAGE))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(
                        div()
                            .child(
                                div()
                                    .id("ts-pick-trigger")
                                    .flex()
                                    .items_center()
                                    .gap(px(6.))
                                    .max_w(px(260.))
                                    .px(px(10.))
                                    .py(px(5.))
                                    .rounded(px(7.))
                                    .bg(rgb(SIDEBAR))
                                    .border_1()
                                    .border_color(rgb(BORDER))
                                    .cursor_pointer()
                                    .text_size(px(13.))
                                    .hover(|s| s.bg(rgb(HOVER)))
                                    .child(icon("tasks", 14., MUTED))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .truncate()
                                            .text_color(rgb(if picked_label.is_some() {
                                                TEXT
                                            } else {
                                                MUTED
                                            }))
                                            .child(
                                                picked_label
                                                    .unwrap_or_else(|| "Select task".into()),
                                            ),
                                    )
                                    .child(icon("chevron-down", 12., MUTED))
                                    .on_mouse_down(
                                        gpui::MouseButton::Left,
                                        cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                                            this.close_menus();
                                            this.task_menu = !task_open;
                                            if this.task_menu {
                                                this.task_query
                                                    .update(cx, |i, cx| i.set_text("", cx));
                                                window.focus(&this.task_query.focus_handle(cx), cx);
                                            }
                                            cx.notify()
                                        }),
                                    ),
                            )
                            .when(task_open, |d| d.child(self.task_picker(cx))),
                    )
                    .child(div().flex_1().min_w_0().child(self.work_name.clone())),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(pill("clock", 120., &self.work_hours))
                    .child(self.work_day.clone())
                    .when_some(self.notice.clone(), |d, msg| {
                        d.child(div().text_size(px(12.)).text_color(rgb(DANGER)).child(msg))
                    })
                    .child(div().flex_1())
                    .child(Button::new("ts-cancel-log", "Cancel").on_click(cx.listener(
                        |this, _: &ClickEvent, _, cx| {
                            this.composing = false;
                            this.task_menu = false;
                            this.notice = None;
                            cx.notify()
                        },
                    )))
                    .child(
                        Button::new("ts-submit-log", "Log time")
                            .icon("plus")
                            .variant(Variant::Primary)
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.log(cx))),
                    ),
            )
    }

    fn toolbar(
        &self,
        all_entries: &[Timesheet],
        all_people: &[String],
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let focused = self.filter.focus_handle(cx).is_focused(window);
        let person_open = self.person_menu;
        let project_open = self.project_menu;
        let person_label = match &self.person {
            Some(who) => format!("Person: {}", short_name(who)),
            None => "Person".into(),
        };
        let project_label = match &self.project {
            Some(proj) => format!("Project: {proj}"),
            None => "Project".into(),
        };

        let mut chips = div().flex().flex_wrap().items_center().gap(px(6.));
        for (i, (period, label)) in PERIODS.into_iter().enumerate() {
            chips = chips.child(
                Chip::new(("ts-period", i), label)
                    .selected(self.period == period)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.period = period;
                        this.close_menus();
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
                div()
                    .child(
                        Chip::new("ts-person-chip", person_label)
                            .selected(self.person.is_some())
                            .open(person_open)
                            .dropdown()
                            .on_press(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                                this.close_menus();
                                this.person_menu = !person_open;
                                cx.notify()
                            })),
                    )
                    .when(person_open, |d| {
                        d.child(self.person_menu(all_entries, all_people, cx))
                    }),
            )
            .child(
                div()
                    .child(
                        Chip::new("ts-project-chip", project_label)
                            .selected(self.project.is_some())
                            .open(project_open)
                            .dropdown()
                            .on_press(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                                this.close_menus();
                                this.project_menu = !project_open;
                                cx.notify()
                            })),
                    )
                    .when(project_open, |d| {
                        d.child(self.project_menu(all_entries, cx))
                    }),
            );

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

impl Render for TimesheetsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let store = self.store.read(cx);
        let today = store.today;
        let all_people = store.people();
        let q = self.filter.read(cx).text().trim().to_lowercase();

        let (summary, all_entries, body) = match store.timesheets() {
            Load::Loading => (
                "Loading timesheets…".into(),
                Vec::new(),
                div()
                    .pt(px(12.))
                    .text_color(rgb(MUTED))
                    .child("Loading…")
                    .into_any_element(),
            ),
            Load::Failed(err) => (
                "Couldn’t load timesheets".into(),
                Vec::new(),
                div()
                    .pt(px(16.))
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap(px(10.))
                    .child(div().text_color(rgb(DANGER)).child(err.clone()))
                    .child(Button::new("ts-retry", "Try again").on_click(cx.listener(
                        |this, _: &ClickEvent, _, cx| {
                            this.store.update(cx, |s, cx| s.reload_timesheets(cx));
                        },
                    )))
                    .into_any_element(),
            ),
            Load::Ready(entries) => {
                let all_entries = entries.clone();
                let visible: Vec<&Timesheet> = entries
                    .iter()
                    .filter(|t| self.matches(t, today, &q))
                    .collect();
                let total_hours: f64 = visible.iter().map(|t| t.hours).sum();
                let projects: HashSet<&str> = visible.iter().map(|t| t.project.as_str()).collect();
                let summary = if visible.is_empty() {
                    "No matching entries".into()
                } else {
                    let entries_label = if visible.len() == 1 {
                        "1 entry".to_string()
                    } else {
                        format!("{} entries", visible.len())
                    };
                    format!(
                        "{} · {entries_label} across {}",
                        hours(total_hours),
                        plural(projects.len(), "project")
                    )
                };

                let body = if visible.is_empty() {
                    div()
                        .pt(px(24.))
                        .text_color(rgb(MUTED))
                        .child(if entries.is_empty() {
                            "No time logged yet."
                        } else {
                            "No timesheet entries match these filters."
                        })
                        .into_any_element()
                } else {
                    let mut groups: BTreeMap<String, Vec<&Timesheet>> = BTreeMap::new();
                    for t in visible {
                        groups.entry(t.date.clone()).or_default().push(t);
                    }
                    let sections = groups.into_iter().rev().map(|(day, items)| {
                        let day_total: f64 = items.iter().map(|t| t.hours).sum();
                        let day_label = date::due(Some(&day), today).label;
                        div()
                            .flex_none()
                            .flex()
                            .flex_col()
                            .mb(px(18.))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .px(px(10.))
                                    .pb(px(8.))
                                    .text_size(px(12.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgb(MUTED))
                                    .child(day_label)
                                    .child(hours(day_total)),
                            )
                            .children(items.into_iter().map(|t| {
                                let task_id = t.task.as_ref().map(|(id, _)| *id);
                                let task_name = t
                                    .task
                                    .as_ref()
                                    .map(|(_, name)| name.clone())
                                    .unwrap_or_else(|| "—".into());
                                let desc = if t.name.is_empty() || t.name == "/" {
                                    "No description".into()
                                } else {
                                    t.name.clone()
                                };
                                let no_desc = t.name.is_empty() || t.name == "/";
                                div()
                                    .id(("ts-row", t.id as usize))
                                    .flex_none()
                                    .w_full()
                                    .flex()
                                    .items_center()
                                    .gap(px(12.))
                                    .px(px(10.))
                                    .py(px(9.))
                                    .min_h(px(42.))
                                    .border_t_1()
                                    .border_color(rgb(ROW_RULE))
                                    .rounded(px(6.))
                                    .when(task_id.is_some(), |d| {
                                        d.cursor_pointer().hover(|s| s.bg(rgb(SIDEBAR)))
                                    })
                                    .when_some(task_id, |d, id| {
                                        d.on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                                            cx.emit(TimesheetsEvent::OpenTask(id))
                                        }))
                                    })
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w(px(160.))
                                            .truncate()
                                            .font_weight(if no_desc {
                                                FontWeight::NORMAL
                                            } else {
                                                FontWeight::MEDIUM
                                            })
                                            .when(no_desc, |d| d.text_color(rgb(MUTED)))
                                            .child(desc),
                                    )
                                    .child(
                                        div()
                                            .flex_none()
                                            .w(px(210.))
                                            .truncate()
                                            .text_size(px(13.))
                                            .text_color(rgb(if task_id.is_some() {
                                                TEXT_SOFT
                                            } else {
                                                MUTED
                                            }))
                                            .child(task_name),
                                    )
                                    .child(
                                        div()
                                            .flex_none()
                                            .w(px(150.))
                                            .truncate()
                                            .text_size(px(13.))
                                            .text_color(rgb(MUTED))
                                            .child(t.project.clone()),
                                    )
                                    .child(
                                        div()
                                            .flex_none()
                                            .w(px(130.))
                                            .flex()
                                            .items_center()
                                            .gap(px(6.))
                                            .overflow_hidden()
                                            .child(
                                                Avatar::new(
                                                    &t.user,
                                                    person_tone(&all_people, &t.user),
                                                )
                                                .size(20.),
                                            )
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .truncate()
                                                    .text_size(px(13.))
                                                    .text_color(rgb(TEXT_SOFT))
                                                    .child(short_name(&t.user).to_string()),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .flex_none()
                                            .w(px(64.))
                                            .flex()
                                            .justify_end()
                                            .text_size(px(13.))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(rgb(TEXT))
                                            .child(hours(t.hours)),
                                    )
                            }))
                    });
                    div()
                        .id("ts-scroll")
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
                                .children(sections),
                        )
                        .into_any_element()
                };
                (summary, all_entries, body)
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
                                    .child("Timesheets"),
                            )
                            .child(div().text_color(rgb(MUTED)).child(summary)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .pt(px(6.))
                            .child(Button::new("ts-refresh", "Refresh").on_click(cx.listener(
                                |this, _: &ClickEvent, _, cx| {
                                    this.store.update(cx, |s, cx| s.reload_timesheets(cx));
                                },
                            )))
                            .child(
                                Button::new("ts-new", "Log time")
                                    .icon("plus")
                                    .variant(Variant::Primary)
                                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                        this.composing = !this.composing;
                                        this.notice = None;
                                        if this.composing && this.picked_task.is_none() {
                                            this.task_menu = true;
                                            window.focus(&this.task_query.focus_handle(cx), cx);
                                        }
                                        cx.notify()
                                    })),
                            ),
                    ),
            )
            .when(self.composing, |d| d.child(self.composer(cx)))
            .child(self.toolbar(&all_entries, &all_people, window, cx))
            .child(body)
    }
}
