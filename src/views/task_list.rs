use crate::components::{Avatar, Chip, ColumnHeader, Floating, MenuHeader, MenuRow, icon, tag};
use crate::format::{person_tone, plural, short_name, stage_tone};
use crate::input::{InputEvent, TextInput};
use crate::odoo::Task;
use crate::store::Store;
use crate::theme::*;
use crate::{date, tree};
use gpui::{
    App, ClickEvent, Context, Div, Entity, EventEmitter, FocusHandle, Focusable, FontWeight,
    KeyDownEvent, MouseDownEvent, ScrollStrategy, Stateful, UniformListScrollHandle, Window, div,
    prelude::*, px, rgb, uniform_list,
};
use std::{collections::HashSet, ops::Range};

pub const TOP_PEOPLE: usize = 5;
const COLS: [f32; 4] = [170., 92., 170., 110.];

pub enum ListEvent {
    Open(i64),
}

pub struct TaskList {
    store: Entity<Store>,
    root: FocusHandle,
    filter: Entity<TextInput>,
    assignee: Option<String>,
    project: Option<String>,
    stages: HashSet<String>,
    selected: usize,
    expanded: HashSet<i64>,
    stage_menu: bool,
    people_menu: bool,
    people_header_menu: bool,
    scroll: UniformListScrollHandle,
}

impl EventEmitter<ListEvent> for TaskList {}

impl TaskList {
    pub fn new(
        store: Entity<Store>,
        root: FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        let filter = cx.new(|cx| TextInput::new("Filter or ask in plain words", cx));
        cx.subscribe_in(&filter, window, Self::on_filter).detach();
        Self {
            store,
            root,
            filter,
            assignee: None,
            project: None,
            stages: HashSet::new(),
            selected: 0,
            expanded: HashSet::new(),
            stage_menu: false,
            people_menu: false,
            people_header_menu: false,
            scroll: UniformListScrollHandle::new(),
        }
    }

    pub fn project(&self) -> Option<&String> {
        self.project.as_ref()
    }

    pub fn reset(&mut self) {
        self.assignee = None;
        self.project = None;
        self.stages.clear();
        self.selected = 0;
        self.close_menus();
    }

    pub fn set_project(&mut self, project: Option<String>, cx: &mut Context<Self>) {
        self.project = project;
        self.jump_to_top();
        cx.notify();
    }

    pub fn focus_filter(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.filter.focus_handle(cx), cx);
    }

    pub fn selected_id(&self, cx: &App) -> Option<i64> {
        let row = self.rows(cx).into_iter().nth(self.selected)?;
        Some(self.store.read(cx).tasks[row.idx].id)
    }

    pub fn select(&mut self, id: i64, cx: &App) {
        if let Some(i) = self
            .rows(cx)
            .iter()
            .position(|r| self.store.read(cx).tasks[r.idx].id == id)
        {
            self.selected = i;
        }
    }

    pub fn dismiss_menu(&mut self, cx: &mut Context<Self>) -> bool {
        let open = self.stage_menu || self.people_menu || self.people_header_menu;
        self.close_menus();
        cx.notify();
        open
    }

    fn close_menus(&mut self) {
        self.stage_menu = false;
        self.people_menu = false;
        self.people_header_menu = false;
    }

    fn jump_to_top(&mut self) {
        self.selected = 0;
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
    }

    fn set_assignee(&mut self, who: Option<String>) {
        self.assignee = who;
        self.close_menus();
        self.jump_to_top();
    }

    fn on_filter(
        &mut self,
        input: &Entity<TextInput>,
        ev: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match ev {
            InputEvent::Changed => self.jump_to_top(),
            InputEvent::Submit => window.focus(&self.root, cx),
            InputEvent::Cancel => {
                input.update(cx, |i, cx| i.clear(window, cx));
                window.focus(&self.root, cx);
            }
        }
        cx.notify();
    }

    fn matches(&self, t: &Task, q: &str) -> bool {
        self.assignee
            .as_ref()
            .is_none_or(|a| t.assignees.contains(a))
            && self.project.as_ref().is_none_or(|p| &t.project == p)
            && (self.stages.is_empty() || self.stages.contains(&t.stage))
            && (q.is_empty()
                || t.name.to_lowercase().contains(q)
                || t.project.to_lowercase().contains(q))
    }

    fn query(&self, cx: &App) -> String {
        self.filter.read(cx).text().to_lowercase()
    }

    fn visible<'a>(&self, tasks: &'a [Task], cx: &App) -> Vec<&'a Task> {
        let q = self.query(cx);
        tasks.iter().filter(|t| self.matches(t, &q)).collect()
    }

    fn rows(&self, cx: &App) -> Vec<tree::Row> {
        let q = self.query(cx);
        let tasks = &self.store.read(cx).tasks;
        let nodes: Vec<(i64, Option<i64>)> = tasks
            .iter()
            .map(|t| (t.id, t.parent.as_ref().map(|p| p.0)))
            .collect();
        let own: Vec<bool> = tasks.iter().map(|t| self.matches(t, &q)).collect();
        tree::rows(&nodes, &own, &self.expanded)
    }

    fn toggle(&mut self, id: i64) {
        if !self.expanded.remove(&id) {
            self.expanded.insert(id);
        }
    }

    pub fn handle_key(
        &mut self,
        ev: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let rows = self.rows(cx);
        let secondary = ev.keystroke.modifiers.secondary(cx);
        let key = ev.keystroke.key.as_str();
        match key {
            "k" if secondary => self.focus_filter(window, cx),
            "/" => self.focus_filter(window, cx),
            "j" | "down" => self.selected = (self.selected + 1).min(rows.len().saturating_sub(1)),
            "k" | "up" => self.selected = self.selected.saturating_sub(1),
            "right" | "left" => {
                let Some(row) = rows.get(self.selected) else {
                    return false;
                };
                if row.forced || row.kids == 0 {
                    return false;
                }
                let id = self.store.read(cx).tasks[row.idx].id;
                if key == "right" {
                    self.expanded.insert(id);
                } else {
                    self.expanded.remove(&id);
                }
            }
            "enter" => match self.selected_id(cx) {
                Some(id) => cx.emit(ListEvent::Open(id)),
                None => return false,
            },
            "0" => self.set_assignee(None),
            d if d.len() == 1 && d.as_bytes()[0].is_ascii_digit() => {
                let people = self.store.read(cx).people();
                match people
                    .into_iter()
                    .take(TOP_PEOPLE)
                    .nth(d.parse::<usize>().unwrap() - 1)
                {
                    Some(p) => self.set_assignee(Some(p)),
                    None => return false,
                }
            }
            _ => return false,
        }
        self.scroll
            .scroll_to_item(self.selected, ScrollStrategy::Nearest);
        cx.notify();
        true
    }

    fn summary(&self, tasks: &[Task], cx: &App) -> String {
        let visible = self.visible(tasks, cx);
        let known: HashSet<i64> = tasks.iter().map(|t| t.id).collect();
        let subs = visible
            .iter()
            .filter(|t| t.parent.as_ref().is_some_and(|p| known.contains(&p.0)))
            .count();
        let today = self.store.read(cx).today;
        let overdue = visible
            .iter()
            .filter(|t| !t.done && date::due(t.deadline.as_deref(), today).overdue)
            .count();
        let mut out = plural(visible.len() - subs, "task");
        if subs > 0 {
            out += &format!(" · {}", plural(subs, "sub-task"));
        }
        if self.project.is_none() {
            let spread = visible
                .iter()
                .map(|t| &t.project)
                .collect::<HashSet<_>>()
                .len();
            out += &format!(" across {}", plural(spread, "project"));
        }
        if overdue > 0 {
            out += &format!(" · {overdue} overdue");
        }
        out
    }

    fn people_chips(&self, people: &[String], cx: &mut Context<Self>) -> Div {
        let (top, rest) = people.split_at(people.len().min(TOP_PEOPLE));
        let picked_from_rest = self.assignee.as_ref().filter(|a| rest.contains(a));
        let mut chips = div().flex().flex_wrap().items_center().gap(px(6.)).child(
            Chip::new("chip-all", "Everyone")
                .selected(self.assignee.is_none())
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.set_assignee(None);
                    cx.notify()
                })),
        );
        for (i, p) in top.iter().chain(picked_from_rest).enumerate() {
            let who = p.clone();
            chips = chips.child(
                Chip::new(("chip", i), short_name(p).to_string())
                    .selected(self.assignee.as_ref() == Some(p))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.set_assignee(Some(who.clone()));
                        cx.notify()
                    })),
            );
        }
        if !rest.is_empty() {
            let open = self.people_menu;
            chips = chips.child(
                div()
                    .child(
                        Chip::new("chip-more", format!("+{} more", rest.len()))
                            .open(open)
                            .on_press(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                                this.people_menu = !open;
                                cx.notify()
                            })),
                    )
                    .when(open, |d| d.child(self.people_menu(rest, people, cx))),
            );
        }
        chips
    }

    fn toolbar(&self, people: &[String], window: &Window, cx: &mut Context<Self>) -> Div {
        let focused = self.filter.focus_handle(cx).is_focused(window);
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(6.))
            .pb(px(10.))
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(self.people_chips(people, cx))
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .w(px(280.))
                    .px(px(10.))
                    .py(px(5.))
                    .rounded(px(6.))
                    .border_1()
                    .border_color(rgb(if focused { MUTED } else { BORDER }))
                    .text_size(px(13.))
                    .child(icon("sparkle", 15., MUTED))
                    .child(self.filter.clone()),
            )
    }

    fn header_row(&self, people: &[String], cx: &mut Context<Self>) -> Div {
        let stage_open = self.stage_menu;
        let people_open = self.people_header_menu;
        div()
            .flex()
            .gap(px(12.))
            .px(px(10.))
            .pb(px(8.))
            .text_size(px(12.))
            .text_color(rgb(MUTED))
            .child(div().flex_1().min_w(px(160.)).child("Name"))
            .child(
                div()
                    .flex_none()
                    .w(px(COLS[0]))
                    .child(
                        ColumnHeader::new("h-status", "Status")
                            .active(!self.stages.is_empty())
                            .open(stage_open)
                            .on_press(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                                this.stage_menu = !stage_open;
                                cx.notify()
                            })),
                    )
                    .when(stage_open, |d| d.child(self.stage_menu(cx))),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .w(px(COLS[1]))
                    .child(
                        ColumnHeader::new("h-people", "Assignees")
                            .active(self.assignee.is_some())
                            .open(people_open)
                            .on_press(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                                this.people_header_menu = !people_open;
                                cx.notify()
                            })),
                    )
                    .when(people_open, |d| {
                        d.child(self.people_menu(people, people, cx))
                    }),
            )
            .child(div().flex_none().w(px(COLS[2])).child("Project"))
            .child(div().flex_none().w(px(COLS[3])).child("Due"))
    }

    fn stage_menu(&self, cx: &mut Context<Self>) -> Floating {
        let mut counts: Vec<(String, usize)> = Vec::new();
        for t in &self.store.read(cx).tasks {
            match counts.iter_mut().find(|(s, _)| s == &t.stage) {
                Some((_, n)) => *n += 1,
                None => counts.push((t.stage.clone(), 1)),
            }
        }
        counts.sort_by_key(|(s, n)| (std::cmp::Reverse(*n), s.to_lowercase()));
        let rows = counts.into_iter().enumerate().map(|(i, (stage, n))| {
            let dot = div()
                .flex_none()
                .size(px(8.))
                .rounded_full()
                .bg(rgb(stage_tone(&stage).fg));
            let selected = self.stages.contains(&stage);
            MenuRow::new(("stage", i), dot, stage.clone(), n)
                .selected(selected)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    if !this.stages.remove(&stage) {
                        this.stages.insert(stage.clone());
                    }
                    this.jump_to_top();
                    cx.notify()
                }))
        });
        Floating::new("stage-menu")
            .max_height(380.)
            .on_dismiss(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.stage_menu = false;
                cx.notify()
            }))
            .child(MenuHeader::new("Show stages").on_clear(
                !self.stages.is_empty(),
                cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.stages.clear();
                    cx.notify()
                }),
            ))
            .children(rows)
    }

    fn people_menu(&self, shown: &[String], all: &[String], cx: &mut Context<Self>) -> Floating {
        let tasks = &self.store.read(cx).tasks;
        let rest = if shown.len() == all.len() { all } else { shown };
        let rows = rest.iter().enumerate().map(|(i, p)| {
            let count = tasks.iter().filter(|t| t.assignees.contains(p)).count();
            let who = p.clone();
            MenuRow::new(
                ("person", i),
                Avatar::new(p, person_tone(all, p)).size(20.),
                short_name(p).to_string(),
                count,
            )
            .selected(self.assignee.as_ref() == Some(p))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.set_assignee(Some(who.clone()));
                cx.notify()
            }))
        });
        Floating::new("people-menu")
            .max_height(340.)
            .on_dismiss(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.close_menus();
                cx.notify()
            }))
            .child(MenuHeader::new("Filter by assignee").on_clear(
                self.assignee.is_some(),
                cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.set_assignee(None);
                    cx.notify()
                }),
            ))
            .children(rows.collect::<Vec<_>>())
    }

    fn render_rows(&mut self, range: Range<usize>, cx: &mut Context<Self>) -> Vec<Stateful<Div>> {
        let rows = self.rows(cx);
        let range = range.start.min(rows.len())..range.end.min(rows.len());
        let (people, today, items) = {
            let store = self.store.read(cx);
            let items: Vec<(tree::Row, Task)> = rows[range.clone()]
                .iter()
                .map(|r| (*r, store.tasks[r.idx].clone()))
                .collect();
            (store.people(), store.today, items)
        };
        items
            .iter()
            .zip(range)
            .map(|((row, t), i)| {
                let id = t.id;
                let on = i == self.selected;
                let mut due = date::due(t.deadline.as_deref(), today);
                due.overdue &= !t.done;
                let fold = div()
                    .id(("fold", i))
                    .flex_none()
                    .size(px(20.))
                    .rounded(px(4.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(row.kids > 0 && !row.forced, |d| {
                        d.cursor_pointer()
                            .hover(|s| s.bg(rgb(BORDER)))
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                                    this.toggle(id);
                                    cx.stop_propagation();
                                    cx.notify()
                                }),
                            )
                    })
                    .when(row.kids > 0, |d| {
                        d.child(icon(
                            if row.open {
                                "chevron-down"
                            } else {
                                "chevron-right"
                            },
                            14.,
                            MUTED,
                        ))
                    });
                let name = div()
                    .flex_1()
                    .min_w(px(160.))
                    .overflow_hidden()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .pl(px(row.depth as f32 * 22.))
                    .child(fold)
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .font_weight(if row.depth == 0 {
                                FontWeight::MEDIUM
                            } else {
                                FontWeight::NORMAL
                            })
                            .when(t.done || row.forced, |d| d.text_color(rgb(MUTED)))
                            .child(t.name.clone()),
                    )
                    .when(row.kids > 0, |d| {
                        d.child(
                            div()
                                .flex_none()
                                .ml(px(4.))
                                .text_size(px(12.))
                                .text_color(rgb(MUTED))
                                .child(row.kids.to_string()),
                        )
                    })
                    .when(t.priority_high, |d| {
                        d.child(tag("High", ORANGE, 11.).px(px(6.)).py(px(1.)))
                    });
                div()
                    .id(("row", i))
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .px(px(10.))
                    .py(px(8.))
                    .min_h(px(44.))
                    .border_t_1()
                    .border_color(rgb(ROW_RULE))
                    .cursor_pointer()
                    .when(on, |d| d.bg(rgb(HOVER)).rounded(px(6.)))
                    .when(!on, |d| d.hover(|s| s.bg(rgb(SIDEBAR))))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.selected = i;
                        cx.emit(ListEvent::Open(id));
                        cx.notify()
                    }))
                    .child(name)
                    .child(
                        div()
                            .flex_none()
                            .flex()
                            .w(px(COLS[0]))
                            .overflow_hidden()
                            .child(
                                tag(t.stage.clone(), stage_tone(&t.stage), 12.)
                                    .flex_shrink_1()
                                    .min_w_0()
                                    .truncate(),
                            ),
                    )
                    .child(div().flex_none().w(px(COLS[1])).flex().children(
                        t.assignees.iter().enumerate().map(|(j, p)| {
                            Avatar::new(p, person_tone(&people, p)).overlapping(j > 0)
                        }),
                    ))
                    .child(
                        div()
                            .flex_none()
                            .w(px(COLS[2]))
                            .truncate()
                            .text_size(px(13.))
                            .text_color(rgb(TEXT_SOFT))
                            .child(t.project.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(COLS[3]))
                            .text_size(px(13.))
                            .when(due.overdue, |d| {
                                d.text_color(rgb(DANGER)).font_weight(FontWeight::MEDIUM)
                            })
                            .when(!due.overdue, |d| d.text_color(rgb(TEXT_SOFT)))
                            .child(due.label),
                    )
            })
            .collect()
    }
}

impl Render for TaskList {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let store = self.store.read(cx);
        let people = store.people();
        let summary = self.summary(&store.tasks, cx);
        let count = self.rows(cx).len();
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
                    .flex_col()
                    .gap(px(6.))
                    .child(
                        div()
                            .text_size(px(32.))
                            .font_weight(FontWeight::BOLD)
                            .child(self.project.clone().unwrap_or("My Tasks".into())),
                    )
                    .child(div().text_color(rgb(MUTED)).child(summary)),
            )
            .child(self.toolbar(&people, window, cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .child(self.header_row(&people, cx))
                    .child(
                        uniform_list(
                            "rows",
                            count,
                            cx.processor(|this, range: Range<usize>, _, cx| {
                                this.render_rows(range, cx)
                            }),
                        )
                        .track_scroll(&self.scroll)
                        .flex_1(),
                    ),
            )
    }
}
