use crate::components::{Avatar, Button, Chip, Floating, MenuHeader, MenuRow, icon, tag};
use crate::format::{person_tone, plural, short_name};
use crate::input::{InputEvent, TextInput};
use crate::odoo::Employee;
use crate::scale::px;
use crate::store::{Load, Store};
use crate::theme::*;
use gpui::{
    App, ClickEvent, Context, Div, Entity, EventEmitter, FocusHandle, Focusable, FontWeight,
    IntoElement, KeyDownEvent, MouseDownEvent, Render, Window, div, prelude::*, rgb,
};
use std::collections::{HashMap, HashSet};

const TOP_DEPARTMENTS: usize = 5;

pub enum EmployeesEvent {
    FilterTasks(String),
}

pub struct EmployeesView {
    store: Entity<Store>,
    root: FocusHandle,
    filter: Entity<TextInput>,
    department: Option<String>,
    dept_menu: bool,
    expanded: Option<i64>,
}

impl EventEmitter<EmployeesEvent> for EmployeesView {}

impl EmployeesView {
    pub fn new(
        store: Entity<Store>,
        root: FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        let filter = cx.new(|cx| TextInput::new("Filter employees…", cx));
        cx.subscribe_in(&filter, window, Self::on_filter).detach();
        Self {
            store,
            root,
            filter,
            department: None,
            dept_menu: false,
            expanded: None,
        }
    }

    pub fn focus_filter(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.filter.focus_handle(cx), cx);
    }

    pub fn dismiss_menu(&mut self, cx: &mut Context<Self>) -> bool {
        let open = self.dept_menu;
        self.dept_menu = false;
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
            "r" => self.store.update(cx, |s, cx| s.reload_employees(cx)),
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

    fn departments(employees: &[Employee]) -> Vec<(String, usize)> {
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for e in employees {
            if !e.department.is_empty() {
                *counts.entry(e.department.as_str()).or_default() += 1;
            }
        }
        let mut list: Vec<(&str, usize)> = counts.into_iter().collect();
        list.sort_by_key(|&(d, n)| (std::cmp::Reverse(n), d.to_lowercase()));
        list.into_iter().map(|(d, n)| (d.to_string(), n)).collect()
    }

    fn matches(&self, e: &Employee, q: &str) -> bool {
        self.department.as_deref().is_none_or(|d| e.department == d)
            && (q.is_empty()
                || e.name.to_lowercase().contains(q)
                || e.job_title.to_lowercase().contains(q)
                || e.department.to_lowercase().contains(q)
                || e.email.to_lowercase().contains(q)
                || e.manager
                    .as_deref()
                    .is_some_and(|m| m.to_lowercase().contains(q)))
    }

    fn dept_menu(&self, rest: &[(String, usize)], cx: &mut Context<Self>) -> Floating {
        let rows = rest.iter().enumerate().map(|(i, (name, count))| {
            let selected = self.department.as_ref() == Some(name);
            let dept = name.clone();
            let dot = div()
                .flex_none()
                .size(px(8.))
                .rounded(px(2.))
                .bg(rgb(PEOPLE[i % PEOPLE.len()].fg));
            MenuRow::new(("emp-dept", i), dot, name.clone(), *count)
                .selected(selected)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.department = (!selected).then(|| dept.clone());
                    this.dept_menu = false;
                    cx.notify()
                }))
        });
        Floating::new("emp-dept-menu")
            .max_height(380.)
            .on_dismiss(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.dept_menu = false;
                cx.notify()
            }))
            .child(MenuHeader::new("Filter by department").on_clear(
                self.department.is_some(),
                cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.department = None;
                    this.dept_menu = false;
                    cx.notify()
                }),
            ))
            .children(rows)
    }

    fn toolbar(
        &self,
        departments: &[(String, usize)],
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let focused = self.filter.focus_handle(cx).is_focused(window);
        let (top, rest) = departments.split_at(departments.len().min(TOP_DEPARTMENTS));
        let picked_from_rest = self
            .department
            .as_ref()
            .and_then(|d| rest.iter().find(|(name, _)| name == d));
        let mut chips = div().flex().flex_wrap().items_center().gap(px(6.)).child(
            Chip::new("emp-dept-all", "All")
                .selected(self.department.is_none())
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.department = None;
                    this.dept_menu = false;
                    cx.notify()
                })),
        );
        for (i, (dept, _)) in top.iter().chain(picked_from_rest).enumerate() {
            let name = dept.clone();
            chips = chips.child(
                Chip::new(("emp-dept-chip", i), dept.clone())
                    .selected(self.department.as_ref() == Some(dept))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.department = Some(name.clone());
                        this.dept_menu = false;
                        cx.notify()
                    })),
            );
        }
        if !rest.is_empty() {
            let open = self.dept_menu;
            chips = chips.child(
                div()
                    .child(
                        Chip::new("emp-dept-more", format!("+{} more", rest.len()))
                            .open(open)
                            .dropdown()
                            .on_press(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                                this.dept_menu = !open;
                                cx.notify()
                            })),
                    )
                    .when(open, |d| d.child(self.dept_menu(rest, cx))),
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

    fn task_link_for(e: &Employee, store: &Store) -> Option<(String, usize)> {
        let count_by_uid = e.user_id.map(|uid| {
            store
                .tasks
                .iter()
                .filter(|t| t.user_ids.contains(&uid))
                .count()
        });
        if let (Some(name), Some(n)) = (&e.user_name, count_by_uid)
            && n > 0
        {
            return Some((name.clone(), n));
        }
        let n = store
            .tasks
            .iter()
            .filter(|t| t.assignees.iter().any(|a| a.eq_ignore_ascii_case(&e.name)))
            .count();
        (n > 0).then(|| (e.name.clone(), n))
    }
}

impl Render for EmployeesView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let store = self.store.read(cx);
        let all_people = store.people();
        let q = self.filter.read(cx).text().trim().to_lowercase();

        let (summary, departments, body) = match store.employees() {
            Load::Loading => (
                "Loading directory…".into(),
                Vec::new(),
                div()
                    .pt(px(12.))
                    .text_color(rgb(MUTED))
                    .child("Loading…")
                    .into_any_element(),
            ),
            Load::Failed(err) => (
                "Couldn’t load employees".into(),
                Vec::new(),
                div()
                    .pt(px(16.))
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap(px(10.))
                    .child(div().text_color(rgb(DANGER)).child(err.clone()))
                    .child(Button::new("emp-retry", "Try again").on_click(cx.listener(
                        |this, _: &ClickEvent, _, cx| {
                            this.store.update(cx, |s, cx| s.reload_employees(cx));
                        },
                    )))
                    .into_any_element(),
            ),
            Load::Ready(employees) => {
                let departments = Self::departments(employees);
                let visible: Vec<&Employee> =
                    employees.iter().filter(|e| self.matches(e, &q)).collect();
                let dept_count = visible
                    .iter()
                    .filter(|e| !e.department.is_empty())
                    .map(|e| e.department.as_str())
                    .collect::<HashSet<_>>()
                    .len();
                let mut summary = plural(visible.len(), "employee");
                if dept_count > 0 && self.department.is_none() {
                    summary += &format!(" across {}", plural(dept_count, "department"));
                }

                let body = if visible.is_empty() {
                    div()
                        .pt(px(24.))
                        .text_color(rgb(MUTED))
                        .child(if employees.is_empty() {
                            "No employees found."
                        } else {
                            "No employees match these filters."
                        })
                        .into_any_element()
                } else {
                    let rows = visible.into_iter().enumerate().map(|(i, e)| {
                        let id = e.id;
                        let open = self.expanded == Some(id);
                        let present = e.presence.as_deref() == Some("present");
                        let tone = if let Some(u) = &e.user_name {
                            person_tone(&all_people, u)
                        } else {
                            PEOPLE[i % PEOPLE.len()]
                        };
                        let task_link = Self::task_link_for(e, store);
                        let phone_text = if !e.phone.is_empty() {
                            e.phone.clone()
                        } else if !e.mobile.is_empty() {
                            e.mobile.clone()
                        } else {
                            "—".into()
                        };
                        let detail_field = |label: &'static str, value: String| {
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .min_w(px(150.))
                                .child(
                                    div()
                                        .text_size(px(11.))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(rgb(MUTED))
                                        .child(label),
                                )
                                .child(div().text_size(px(13.)).text_color(rgb(TEXT)).child(value))
                        };

                        div()
                            .flex_none()
                            .flex()
                            .flex_col()
                            .border_t_1()
                            .border_color(rgb(ROW_RULE))
                            .child(
                                div()
                                    .id(("emp-row", id as usize))
                                    .w_full()
                                    .flex()
                                    .items_center()
                                    .gap(px(12.))
                                    .px(px(10.))
                                    .py(px(10.))
                                    .min_h(px(50.))
                                    .rounded(px(6.))
                                    .cursor_pointer()
                                    .when(open, |d| d.bg(rgb(SIDEBAR)))
                                    .when(!open, |d| d.hover(|s| s.bg(rgb(SIDEBAR))))
                                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                        this.expanded = if open { None } else { Some(id) };
                                        cx.notify()
                                    }))
                                    .child(
                                        div()
                                            .flex_none()
                                            .relative()
                                            .child(
                                                Avatar::new(&e.name, tone)
                                                    .for_me(e.user_name.as_deref() == Some("Me"))
                                                    .size(28.),
                                            )
                                            .when(present, |d| {
                                                d.child(
                                                    div()
                                                        .absolute()
                                                        .bottom_0()
                                                        .right_0()
                                                        .size(px(8.))
                                                        .rounded_full()
                                                        .bg(rgb(CHECK))
                                                        .border_1()
                                                        .border_color(rgb(PAGE)),
                                                )
                                            }),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w(px(160.))
                                            .flex()
                                            .flex_col()
                                            .child(
                                                div()
                                                    .truncate()
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .child(e.name.clone()),
                                            )
                                            .when(!e.job_title.is_empty(), |d| {
                                                d.child(
                                                    div()
                                                        .truncate()
                                                        .text_size(px(12.))
                                                        .text_color(rgb(MUTED))
                                                        .child(e.job_title.clone()),
                                                )
                                            }),
                                    )
                                    .child(
                                        div()
                                            .flex_none()
                                            .w(px(155.))
                                            .overflow_hidden()
                                            .flex()
                                            .child(if e.department.is_empty() {
                                                div()
                                                    .text_size(px(13.))
                                                    .text_color(rgb(MUTED))
                                                    .child("—")
                                            } else {
                                                tag(e.department.clone(), GRAY, 12.)
                                                    .flex_shrink_1()
                                                    .min_w_0()
                                                    .truncate()
                                            }),
                                    )
                                    .child(
                                        div()
                                            .flex_none()
                                            .w(px(210.))
                                            .truncate()
                                            .text_size(px(13.))
                                            .text_color(rgb(TEXT_SOFT))
                                            .child(if e.email.is_empty() {
                                                "—".into()
                                            } else {
                                                e.email.clone()
                                            }),
                                    )
                                    .child(
                                        div()
                                            .flex_none()
                                            .w(px(125.))
                                            .truncate()
                                            .text_size(px(13.))
                                            .text_color(rgb(MUTED))
                                            .child(phone_text),
                                    )
                                    .child(
                                        div()
                                            .flex_none()
                                            .w(px(90.))
                                            .flex()
                                            .justify_end()
                                            .when_some(task_link.clone(), |d, (who, count)| {
                                                d.child(
                                                    div()
                                                        .id(("emp-tasks", id as usize))
                                                        .px(px(8.))
                                                        .py(px(3.))
                                                        .rounded(px(5.))
                                                        .text_size(px(12.))
                                                        .font_weight(FontWeight::MEDIUM)
                                                        .text_color(rgb(ACCENT))
                                                        .cursor_pointer()
                                                        .hover(|s| s.bg(rgb(HOVER)))
                                                        .child(plural(count, "task"))
                                                        .on_mouse_down(
                                                            gpui::MouseButton::Left,
                                                            cx.listener(
                                                                move |_,
                                                                      _: &MouseDownEvent,
                                                                      _,
                                                                      cx| {
                                                                    cx.stop_propagation();
                                                                    cx.emit(
                                                                        EmployeesEvent::FilterTasks(
                                                                            who.clone(),
                                                                        ),
                                                                    );
                                                                },
                                                            ),
                                                        ),
                                                )
                                            }),
                                    ),
                            )
                            .when(open, |d| {
                                let mut details = div()
                                    .flex()
                                    .flex_wrap()
                                    .items_center()
                                    .gap(px(24.))
                                    .px(px(50.))
                                    .py(px(12.))
                                    .mb(px(6.))
                                    .rounded(px(6.))
                                    .bg(rgb(SIDEBAR));
                                if !e.email.is_empty() {
                                    details =
                                        details.child(detail_field("Work email", e.email.clone()));
                                }
                                if !e.phone.is_empty() {
                                    details =
                                        details.child(detail_field("Work phone", e.phone.clone()));
                                }
                                if !e.mobile.is_empty() && e.mobile != e.phone {
                                    details =
                                        details.child(detail_field("Mobile", e.mobile.clone()));
                                }
                                if let Some(mgr) = &e.manager {
                                    details = details.child(detail_field(
                                        "Manager",
                                        short_name(mgr).to_string(),
                                    ));
                                }
                                if let Some(loc) = &e.location {
                                    details = details.child(detail_field("Location", loc.clone()));
                                }
                                if let Some((who, count)) = task_link {
                                    details = details.child(
                                        div().ml_auto().child(
                                            Button::new(
                                                ("emp-open-tasks", id as usize),
                                                format!("View {}", plural(count, "task")),
                                            )
                                            .icon("tasks")
                                            .on_click(
                                                cx.listener(move |_, _: &ClickEvent, _, cx| {
                                                    cx.emit(EmployeesEvent::FilterTasks(
                                                        who.clone(),
                                                    ))
                                                }),
                                            ),
                                        ),
                                    );
                                }
                                d.child(details)
                            })
                    });

                    div()
                        .id("employees-scroll")
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
                (summary, departments, body)
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
                                    .child("Employees"),
                            )
                            .child(div().text_color(rgb(MUTED)).child(summary)),
                    )
                    .child(
                        div()
                            .pt(px(6.))
                            .child(Button::new("emp-refresh", "Refresh").on_click(cx.listener(
                                |this, _: &ClickEvent, _, cx| {
                                    this.store.update(cx, |s, cx| s.reload_employees(cx));
                                },
                            ))),
                    ),
            )
            .child(self.toolbar(&departments, window, cx))
            .child(
                div()
                    .flex()
                    .gap(px(12.))
                    .px(px(10.))
                    .text_size(px(12.))
                    .text_color(rgb(MUTED))
                    .child(div().flex_none().w(px(28.)))
                    .child(div().flex_1().min_w(px(160.)).child("Name"))
                    .child(div().flex_none().w(px(155.)).child("Department"))
                    .child(div().flex_none().w(px(210.)).child("Email"))
                    .child(div().flex_none().w(px(125.)).child("Phone"))
                    .child(div().flex_none().w(px(90.))),
            )
            .child(body)
    }
}
