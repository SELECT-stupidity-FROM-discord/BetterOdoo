use super::task_filters::{
    Aggregate, DoneFilter, DueFilter, GroupBy, HoursFilter, KindFilter, SortBy, TaskFilters,
};
use crate::components::{Avatar, Chip, ColumnHeader, Floating, MenuHeader, MenuRow, icon, tag};
use crate::format::{hours, person_tone, plural, short_name, stage_dot, stage_tone};
use crate::input::{InputEvent, TextInput};
use crate::odoo::Task;
use crate::scale::px;
use crate::store::Store;
use crate::theme::*;
use crate::{date, tree};
use gpui::{
    App, ClickEvent, Context, Div, Entity, EventEmitter, FocusHandle, Focusable, FontWeight,
    KeyDownEvent, MouseButton, MouseDownEvent, ScrollStrategy, Stateful, UniformListScrollHandle,
    Window, div, prelude::*, rgb, uniform_list,
};
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
};

pub const TOP_PEOPLE: usize = 5;
const COLS: [f32; 4] = [170., 92., 170., 110.];

pub enum ListEvent {
    Open(i64),
}

enum ListItem {
    Group {
        key: String,
        open: bool,
        agg: Aggregate,
    },
    Task(tree::Row),
}

pub struct TaskList {
    store: Entity<Store>,
    root: FocusHandle,
    filter: Entity<TextInput>,
    assignee: Option<String>,
    assigned_by: Option<String>,
    project: Option<String>,
    stages: HashSet<String>,
    filters: TaskFilters,
    selected: usize,
    expanded: HashSet<i64>,
    collapsed: HashSet<i64>,
    collapsed_groups: HashSet<String>,
    stage_menu: bool,
    people_menu: bool,
    people_header_menu: bool,
    assigned_by_menu: bool,
    filter_menu: bool,
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
            assigned_by: None,
            project: None,
            stages: HashSet::new(),
            filters: TaskFilters::default(),
            selected: 0,
            expanded: HashSet::new(),
            collapsed: HashSet::new(),
            collapsed_groups: HashSet::new(),
            stage_menu: false,
            people_menu: false,
            people_header_menu: false,
            assigned_by_menu: false,
            filter_menu: false,
            scroll: UniformListScrollHandle::new(),
        }
    }

    pub fn project(&self) -> Option<&String> {
        self.project.as_ref()
    }

    pub fn reset(&mut self) {
        self.assignee = None;
        self.assigned_by = None;
        self.project = None;
        self.stages.clear();
        self.filters.clear();
        self.collapsed.clear();
        self.collapsed_groups.clear();
        self.selected = 0;
        self.close_menus();
    }

    pub fn set_project(&mut self, project: Option<String>, cx: &mut Context<Self>) {
        self.project = project;
        self.collapsed.clear();
        self.jump_to_top();
        cx.notify();
    }

    pub fn set_assignee_filter(&mut self, who: Option<String>, cx: &mut Context<Self>) {
        self.project = None;
        self.assigned_by = None;
        self.set_assignee(who);
        cx.notify();
    }

    pub fn focus_filter(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.filter.focus_handle(cx), cx);
    }

    pub fn selected_id(&self, cx: &App) -> Option<i64> {
        match self.items(cx).into_iter().nth(self.selected)? {
            ListItem::Task(row) => Some(self.store.read(cx).tasks[row.idx].id),
            ListItem::Group { .. } => None,
        }
    }

    pub fn select(&mut self, id: i64, cx: &App) {
        let store = self.store.read(cx);
        if let Some(i) = self
            .items(cx)
            .iter()
            .position(|item| matches!(item, ListItem::Task(r) if store.tasks[r.idx].id == id))
        {
            self.selected = i;
        }
    }

    pub fn dismiss_menu(&mut self, cx: &mut Context<Self>) -> bool {
        let open = self.stage_menu
            || self.people_menu
            || self.people_header_menu
            || self.assigned_by_menu
            || self.filter_menu;
        self.close_menus();
        cx.notify();
        open
    }

    fn close_menus(&mut self) {
        self.stage_menu = false;
        self.people_menu = false;
        self.people_header_menu = false;
        self.assigned_by_menu = false;
        self.filter_menu = false;
    }

    fn jump_to_top(&mut self) {
        self.selected = 0;
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
    }

    fn set_assignee(&mut self, who: Option<String>) {
        self.assignee = who;
        self.collapsed.clear();
        self.close_menus();
        self.jump_to_top();
    }

    fn set_assigned_by(&mut self, who: Option<String>) {
        self.assigned_by = who;
        if let Some(who) = &self.assigned_by
            && self.assignee.as_ref() == Some(who)
        {
            self.assignee = None;
        }
        self.collapsed.clear();
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
            InputEvent::Changed => {
                self.collapsed.clear();
                self.jump_to_top();
            }
            InputEvent::Submit => window.focus(&self.root, cx),
            InputEvent::Cancel => {
                input.update(cx, |i, cx| i.clear(window, cx));
                self.collapsed.clear();
                window.focus(&self.root, cx);
            }
        }
        cx.notify();
    }

    fn matches(&self, t: &Task, q: &str, today: i64, known_ids: &HashSet<i64>) -> bool {
        self.assigned_by
            .as_deref()
            .is_none_or(|who| t.assigned_by(who))
            && self
                .assignee
                .as_ref()
                .is_none_or(|a| t.assignees.contains(a))
            && self.project.as_ref().is_none_or(|p| &t.project == p)
            && (self.stages.is_empty() || self.stages.contains(&t.stage))
            && self.filters.matches(t, today, known_ids)
            && (q.is_empty()
                || t.name.to_lowercase().contains(q)
                || t.project.to_lowercase().contains(q)
                || t.partner
                    .as_ref()
                    .is_some_and(|(_, p)| p.to_lowercase().contains(q))
                || t.tags
                    .iter()
                    .any(|(_, tag_name)| tag_name.to_lowercase().contains(q)))
    }

    fn query(&self, cx: &App) -> String {
        self.filter.read(cx).text().to_lowercase()
    }

    fn visible<'a>(&self, tasks: &'a [Task], today: i64, cx: &App) -> Vec<&'a Task> {
        let q = self.query(cx);
        let known: HashSet<i64> = tasks.iter().map(|t| t.id).collect();
        tasks
            .iter()
            .filter(|t| self.matches(t, &q, today, &known))
            .collect()
    }

    fn items(&self, cx: &App) -> Vec<ListItem> {
        let q = self.query(cx);
        let store = self.store.read(cx);
        let today = store.today;
        let tasks = &store.tasks;
        let known: HashSet<i64> = tasks.iter().map(|t| t.id).collect();

        if self.filters.group == GroupBy::None && self.filters.sort == SortBy::Default {
            let nodes: Vec<(i64, Option<i64>)> = tasks
                .iter()
                .map(|t| (t.id, t.parent.as_ref().map(|p| p.0)))
                .collect();
            let own: Vec<bool> = tasks
                .iter()
                .map(|t| self.matches(t, &q, today, &known))
                .collect();
            return tree::rows(&nodes, &own, &self.expanded, &self.collapsed)
                .into_iter()
                .map(ListItem::Task)
                .collect();
        }

        let mut matched: Vec<usize> = tasks
            .iter()
            .enumerate()
            .filter(|(_, t)| self.matches(t, &q, today, &known))
            .map(|(i, _)| i)
            .collect();

        if self.filters.group == GroupBy::None {
            self.filters.sort_indices(&mut matched, tasks, today);
            return matched
                .into_iter()
                .map(|idx| {
                    ListItem::Task(tree::Row {
                        idx,
                        depth: 0,
                        kids: 0,
                        open: false,
                        forced: false,
                    })
                })
                .collect();
        }

        let mut grouped: HashMap<String, Vec<usize>> = HashMap::new();
        for idx in matched {
            for key in self.filters.group_keys(&tasks[idx], today) {
                grouped.entry(key).or_default().push(idx);
            }
        }
        let mut groups: Vec<(String, Vec<usize>)> = grouped.into_iter().collect();
        groups.sort_by_key(|(label, list)| self.filters.group_order(label, list.len()));

        let mut out = Vec::new();
        for (key, mut list) in groups {
            self.filters.sort_indices(&mut list, tasks, today);
            let refs: Vec<&Task> = list.iter().map(|&i| &tasks[i]).collect();
            let agg = Aggregate::compute(&refs, today);
            let open = !self.collapsed_groups.contains(&key);
            out.push(ListItem::Group {
                key: key.clone(),
                open,
                agg,
            });
            if open {
                for idx in list {
                    out.push(ListItem::Task(tree::Row {
                        idx,
                        depth: 0,
                        kids: 0,
                        open: false,
                        forced: false,
                    }));
                }
            }
        }
        out
    }

    fn toggle(&mut self, id: i64, forced: bool) {
        let set = if forced {
            &mut self.collapsed
        } else {
            &mut self.expanded
        };
        if !set.remove(&id) {
            set.insert(id);
        }
    }

    fn set_all_open(&mut self, open: bool, cx: &mut Context<Self>) {
        let ids: HashSet<i64> = self.store.read(cx).tasks.iter().map(|t| t.id).collect();
        let groups: Vec<String> = self
            .items(cx)
            .into_iter()
            .filter_map(|i| match i {
                ListItem::Group { key, .. } => Some(key),
                _ => None,
            })
            .collect();
        self.collapsed_groups.clear();
        if open {
            self.expanded = ids;
            self.collapsed.clear();
        } else {
            self.expanded.clear();
            self.collapsed = ids;
            self.collapsed_groups.extend(groups);
        }
        cx.notify();
    }

    fn toggle_group(&mut self, key: &str) {
        if !self.collapsed_groups.remove(key) {
            self.collapsed_groups.insert(key.to_string());
        }
    }

    pub fn handle_key(
        &mut self,
        ev: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let items = self.items(cx);
        let secondary = ev.keystroke.modifiers.secondary(cx);
        let key = ev.keystroke.key.as_str();
        match key {
            "k" if secondary => self.focus_filter(window, cx),
            "/" => self.focus_filter(window, cx),
            "j" | "down" => self.selected = (self.selected + 1).min(items.len().saturating_sub(1)),
            "k" | "up" => self.selected = self.selected.saturating_sub(1),
            "right" | "left" => match items.get(self.selected) {
                Some(ListItem::Group { key: gkey, .. }) => {
                    if key == "right" {
                        self.collapsed_groups.remove(gkey);
                    } else {
                        self.collapsed_groups.insert(gkey.clone());
                    }
                }
                Some(ListItem::Task(row)) if row.kids > 0 => {
                    let id = self.store.read(cx).tasks[row.idx].id;
                    if row.forced {
                        if key == "right" {
                            self.collapsed.remove(&id);
                        } else {
                            self.collapsed.insert(id);
                        }
                    } else if key == "right" {
                        self.expanded.insert(id);
                    } else {
                        self.expanded.remove(&id);
                    }
                }
                _ => return false,
            },
            "enter" => match items.get(self.selected) {
                Some(ListItem::Group { key: gkey, .. }) => self.toggle_group(gkey),
                Some(ListItem::Task(row)) => {
                    let id = self.store.read(cx).tasks[row.idx].id;
                    cx.emit(ListEvent::Open(id));
                }
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

    fn summary(&self, visible: &[&Task], known: &HashSet<i64>, agg: Aggregate) -> String {
        let subs = visible
            .iter()
            .filter(|t| t.parent.as_ref().is_some_and(|p| known.contains(&p.0)))
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
        if agg.overdue > 0 {
            out += &format!(" · {} overdue", agg.overdue);
        }
        if agg.hours > 0. {
            out += &format!(" · {} logged", hours(agg.hours));
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
        let open = self.assigned_by_menu;
        let label = match &self.assigned_by {
            Some(who) => format!("Assigned by: {}", short_name(who)),
            None => "Assigned by".into(),
        };
        chips
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
                        Chip::new("chip-assigned-by", label)
                            .selected(self.assigned_by.is_some())
                            .open(open)
                            .dropdown()
                            .on_press(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                                this.close_menus();
                                this.assigned_by_menu = !open;
                                cx.notify()
                            })),
                    )
                    .when(open, |d| d.child(self.assigned_by_menu(people, cx))),
            )
    }

    fn filter_menu(&self, cx: &mut Context<Self>) -> Floating {
        let (all_tags, all_customers) = {
            let tasks = &self.store.read(cx).tasks;
            let mut tag_counts: HashMap<&str, usize> = HashMap::new();
            let mut cust_counts: HashMap<&str, usize> = HashMap::new();
            for t in tasks {
                for (_, name) in &t.tags {
                    *tag_counts.entry(name.as_str()).or_default() += 1;
                }
                if let Some((_, c)) = &t.partner {
                    *cust_counts.entry(c.as_str()).or_default() += 1;
                }
            }
            let mut tags: Vec<(&str, usize)> = tag_counts.into_iter().collect();
            tags.sort_by_key(|&(n, c)| (std::cmp::Reverse(c), n.to_lowercase()));
            let mut custs: Vec<(&str, usize)> = cust_counts.into_iter().collect();
            custs.sort_by_key(|&(n, c)| (std::cmp::Reverse(c), n.to_lowercase()));
            (
                tags.into_iter()
                    .take(12)
                    .map(|(n, _)| n.to_string())
                    .collect::<Vec<_>>(),
                custs
                    .into_iter()
                    .take(10)
                    .map(|(n, _)| n.to_string())
                    .collect::<Vec<_>>(),
            )
        };

        let section = |title: &'static str, body: Div| {
            div()
                .flex_none()
                .flex()
                .flex_col()
                .gap(px(7.))
                .child(
                    div()
                        .text_size(px(11.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(MUTED))
                        .child(title),
                )
                .child(body)
        };
        let pill = |id: (&'static str, usize), label: String, on: bool| {
            div()
                .id(id)
                .px(px(10.))
                .py(px(4.))
                .rounded_full()
                .border_1()
                .border_color(rgb(if on { TEXT } else { BORDER }))
                .bg(rgb(if on { TEXT } else { SIDEBAR }))
                .text_color(rgb(if on { PAGE } else { TEXT_SOFT }))
                .text_size(px(12.))
                .font_weight(FontWeight::MEDIUM)
                .cursor_pointer()
                .when(!on, |d| d.hover(|s| s.bg(rgb(HOVER))))
                .child(label)
        };

        let mut menu =
            Floating::new("filter-menu")
                .width(360.)
                .radius(16.)
                .offset(8.)
                .max_height(520.)
                .padded(16., 14.)
                .on_dismiss(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                    this.close_menus();
                    cx.notify()
                }))
                .child(
                    div()
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(TEXT))
                                .child("Filters & Aggregations"),
                        )
                        .when(self.filters.active_count() > 0, |d| {
                            d.child(
                                div()
                                    .id("filters-reset-all")
                                    .px(px(8.))
                                    .py(px(2.))
                                    .rounded_full()
                                    .cursor_pointer()
                                    .text_size(px(12.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgb(ACCENT))
                                    .hover(|s| s.bg(rgb(ACTIVE_STEP)))
                                    .child("Reset all")
                                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                        this.filters.clear();
                                        this.collapsed.clear();
                                        this.collapsed_groups.clear();
                                        this.jump_to_top();
                                        cx.notify()
                                    })),
                            )
                        }),
                )
                .child(section(
                    "Completion",
                    div().flex().flex_wrap().gap(px(6.)).children(
                        DoneFilter::ALL
                            .into_iter()
                            .enumerate()
                            .map(|(i, (val, label))| {
                                pill(("f-done", i), label.into(), self.filters.done == val)
                                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                        this.filters.done = val;
                                        this.collapsed.clear();
                                        this.jump_to_top();
                                        cx.notify()
                                    }))
                            }),
                    ),
                ))
                .child(section(
                    "Due date",
                    div().flex().flex_wrap().gap(px(6.)).children(
                        DueFilter::ALL
                            .into_iter()
                            .enumerate()
                            .map(|(i, (val, label))| {
                                pill(("f-due", i), label.into(), self.filters.due == val).on_click(
                                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                                        this.filters.due = val;
                                        this.collapsed.clear();
                                        this.jump_to_top();
                                        cx.notify()
                                    }),
                                )
                            }),
                    ),
                ))
                .child(section(
                    "Quick filters",
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(px(6.))
                        .child(
                            pill(
                                ("f-quick", 0),
                                "High priority".into(),
                                self.filters.high_only,
                            )
                            .on_click(cx.listener(
                                |this, _: &ClickEvent, _, cx| {
                                    this.filters.high_only = !this.filters.high_only;
                                    this.collapsed.clear();
                                    this.jump_to_top();
                                    cx.notify()
                                },
                            )),
                        )
                        .child(
                            pill(
                                ("f-quick", 1),
                                "Unassigned".into(),
                                self.filters.unassigned_only,
                            )
                            .on_click(cx.listener(
                                |this, _: &ClickEvent, _, cx| {
                                    this.filters.unassigned_only = !this.filters.unassigned_only;
                                    if this.filters.unassigned_only {
                                        this.assignee = None;
                                    }
                                    this.collapsed.clear();
                                    this.jump_to_top();
                                    cx.notify()
                                },
                            )),
                        )
                        .child(
                            pill(
                                ("f-quick", 2),
                                "Summary stats".into(),
                                self.filters.show_stats,
                            )
                            .on_click(cx.listener(
                                |this, _: &ClickEvent, _, cx| {
                                    this.filters.show_stats = !this.filters.show_stats;
                                    cx.notify()
                                },
                            )),
                        ),
                ))
                .child(section(
                    "Logged time",
                    div().flex().flex_wrap().gap(px(6.)).children(
                        HoursFilter::ALL
                            .into_iter()
                            .enumerate()
                            .map(|(i, (val, label))| {
                                pill(("f-hours", i), label.into(), self.filters.hours == val)
                                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                        this.filters.hours = val;
                                        this.collapsed.clear();
                                        this.jump_to_top();
                                        cx.notify()
                                    }))
                            }),
                    ),
                ))
                .child(section(
                    "Hierarchy",
                    div().flex().flex_wrap().gap(px(6.)).children(
                        KindFilter::ALL
                            .into_iter()
                            .enumerate()
                            .map(|(i, (val, label))| {
                                pill(("f-kind", i), label.into(), self.filters.kind == val)
                                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                        this.filters.kind = val;
                                        this.collapsed.clear();
                                        this.jump_to_top();
                                        cx.notify()
                                    }))
                            }),
                    ),
                ));

        if !all_tags.is_empty() {
            menu = menu.child(section(
                "Tags",
                div().flex().flex_wrap().gap(px(6.)).children(
                    all_tags.into_iter().enumerate().map(|(i, name)| {
                        let on = self.filters.tags.contains(&name);
                        let key = name.clone();
                        pill(("f-tag", i), name, on).on_click(cx.listener(
                            move |this, _: &ClickEvent, _, cx| {
                                if !this.filters.tags.remove(&key) {
                                    this.filters.tags.insert(key.clone());
                                }
                                this.collapsed.clear();
                                this.jump_to_top();
                                cx.notify()
                            },
                        ))
                    }),
                ),
            ));
        }

        if !all_customers.is_empty() {
            menu = menu.child(section(
                "Customer",
                div().flex().flex_wrap().gap(px(6.)).children(
                    all_customers.into_iter().enumerate().map(|(i, name)| {
                        let on = self.filters.customer.as_deref() == Some(name.as_str());
                        let key = name.clone();
                        pill(("f-cust", i), name, on).on_click(cx.listener(
                            move |this, _: &ClickEvent, _, cx| {
                                this.filters.customer = (!on).then(|| key.clone());
                                this.collapsed.clear();
                                this.jump_to_top();
                                cx.notify()
                            },
                        ))
                    }),
                ),
            ));
        }

        menu.child(div().flex_none().h(px(1.)).bg(rgb(ROW_RULE)))
            .child(section(
                "Group by (aggregations)",
                div().flex().flex_wrap().gap(px(6.)).children(
                    GroupBy::ALL
                        .into_iter()
                        .enumerate()
                        .map(|(i, (val, label))| {
                            pill(("f-group", i), label.into(), self.filters.group == val).on_click(
                                cx.listener(move |this, _: &ClickEvent, _, cx| {
                                    this.filters.group = val;
                                    this.collapsed_groups.clear();
                                    this.jump_to_top();
                                    cx.notify()
                                }),
                            )
                        }),
                ),
            ))
            .child(section(
                "Sort by",
                div().flex().flex_wrap().gap(px(6.)).children(
                    SortBy::ALL
                        .into_iter()
                        .enumerate()
                        .map(|(i, (val, label))| {
                            pill(("f-sort", i), label.into(), self.filters.sort == val).on_click(
                                cx.listener(move |this, _: &ClickEvent, _, cx| {
                                    this.filters.sort = val;
                                    this.jump_to_top();
                                    cx.notify()
                                }),
                            )
                        }),
                ),
            ))
    }

    fn search_controls(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let focused = self.filter.focus_handle(cx).is_focused(window);
        let open = self.filter_menu;
        let active = self.filters.active_count();
        div()
            .flex_none()
            .flex()
            .flex_col()
            .items_end()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .id("task-filter-trigger")
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .px(px(10.))
                            .py(px(7.))
                            .rounded(px(8.))
                            .border_1()
                            .border_color(rgb(if open || active > 0 { ACCENT } else { BORDER }))
                            .bg(rgb(if active > 0 { ACTIVE_STEP } else { PAGE }))
                            .cursor_pointer()
                            .when(active == 0, |d| d.hover(|s| s.bg(rgb(SIDEBAR))))
                            .child(icon(
                                "filter",
                                14.,
                                if open || active > 0 { ACCENT } else { MUTED },
                            ))
                            .when(active > 0, |d| {
                                d.child(
                                    div()
                                        .text_size(px(12.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(rgb(ACTIVE_STEP_TEXT))
                                        .child(active.to_string()),
                                )
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                                    this.close_menus();
                                    this.filter_menu = !open;
                                    cx.notify()
                                }),
                            ),
                    )
                    .child(
                        div()
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .w(px(280.))
                            .px(px(12.))
                            .py(px(7.))
                            .rounded(px(8.))
                            .bg(rgb(PAGE))
                            .border_1()
                            .border_color(rgb(if focused { ACCENT } else { BORDER }))
                            .text_size(px(13.))
                            .child(icon("sparkle", 15., if focused { ACCENT } else { MUTED }))
                            .child(self.filter.clone()),
                    ),
            )
            .when(open, |d| d.child(self.filter_menu(cx)))
    }

    fn active_filter_pills(&self, cx: &mut Context<Self>) -> Option<Div> {
        if self.filters.active_count() == 0 {
            return None;
        }
        let mut row = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(6.))
            .pt(px(2.));
        let badge = |id: (&'static str, usize), text: String| {
            div()
                .id(id)
                .flex()
                .items_center()
                .gap(px(5.))
                .px(px(9.))
                .py(px(3.))
                .rounded_full()
                .bg(rgb(SIDEBAR))
                .border_1()
                .border_color(rgb(BORDER))
                .text_size(px(12.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(TEXT_SOFT))
                .cursor_pointer()
                .hover(|s| s.bg(rgb(HOVER)))
                .child(text)
                .child(div().text_color(rgb(MUTED)).child("×"))
        };
        if self.filters.done != DoneFilter::All {
            row = row.child(badge(("af", 0), self.filters.done.label().into()).on_click(
                cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.filters.done = DoneFilter::All;
                    this.jump_to_top();
                    cx.notify()
                }),
            ));
        }
        if self.filters.due != DueFilter::Any {
            row = row.child(badge(("af", 1), self.filters.due.label().into()).on_click(
                cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.filters.due = DueFilter::Any;
                    this.jump_to_top();
                    cx.notify()
                }),
            ));
        }
        if self.filters.high_only {
            row = row.child(
                badge(("af", 2), "High priority".into()).on_click(cx.listener(
                    |this, _: &ClickEvent, _, cx| {
                        this.filters.high_only = false;
                        this.jump_to_top();
                        cx.notify()
                    },
                )),
            );
        }
        if self.filters.unassigned_only {
            row = row.child(badge(("af", 3), "Unassigned".into()).on_click(cx.listener(
                |this, _: &ClickEvent, _, cx| {
                    this.filters.unassigned_only = false;
                    this.jump_to_top();
                    cx.notify()
                },
            )));
        }
        if self.filters.hours != HoursFilter::Any {
            row = row.child(
                badge(("af", 4), self.filters.hours.label().into()).on_click(cx.listener(
                    |this, _: &ClickEvent, _, cx| {
                        this.filters.hours = HoursFilter::Any;
                        this.jump_to_top();
                        cx.notify()
                    },
                )),
            );
        }
        if self.filters.kind != KindFilter::All {
            row = row.child(badge(("af", 5), self.filters.kind.label().into()).on_click(
                cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.filters.kind = KindFilter::All;
                    this.jump_to_top();
                    cx.notify()
                }),
            ));
        }
        let mut tags: Vec<String> = self.filters.tags.iter().cloned().collect();
        tags.sort();
        for (i, t) in tags.into_iter().enumerate() {
            let key = t.clone();
            row = row.child(
                badge(("af-tag", i), format!("Tag: {t}")).on_click(cx.listener(
                    move |this, _: &ClickEvent, _, cx| {
                        this.filters.tags.remove(&key);
                        this.jump_to_top();
                        cx.notify()
                    },
                )),
            );
        }
        if let Some(c) = &self.filters.customer {
            row = row.child(
                badge(("af", 6), format!("Customer: {c}")).on_click(cx.listener(
                    |this, _: &ClickEvent, _, cx| {
                        this.filters.customer = None;
                        this.jump_to_top();
                        cx.notify()
                    },
                )),
            );
        }
        if self.filters.group != GroupBy::None {
            row = row.child(
                badge(("af", 7), format!("Group: {}", self.filters.group.label())).on_click(
                    cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.filters.group = GroupBy::None;
                        this.collapsed_groups.clear();
                        this.jump_to_top();
                        cx.notify()
                    }),
                ),
            );
        }
        if self.filters.sort != SortBy::Default {
            row = row.child(
                badge(("af", 8), format!("Sort: {}", self.filters.sort.label())).on_click(
                    cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.filters.sort = SortBy::Default;
                        this.jump_to_top();
                        cx.notify()
                    }),
                ),
            );
        }
        row = row.child(
            div()
                .id("af-clear-all")
                .px(px(6.))
                .py(px(3.))
                .cursor_pointer()
                .text_size(px(12.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(ACCENT))
                .hover(|s| s.text_color(rgb(TEXT)))
                .child("Reset all")
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.filters.clear();
                    this.collapsed.clear();
                    this.collapsed_groups.clear();
                    this.jump_to_top();
                    cx.notify()
                })),
        );
        Some(row)
    }

    fn stats_strip(&self, agg: Aggregate) -> Div {
        let card = |label: &'static str, value: String, sub: String, accent: Option<u32>| {
            div()
                .flex_1()
                .min_w(px(140.))
                .flex()
                .flex_col()
                .gap(px(3.))
                .px(px(14.))
                .py(px(10.))
                .rounded(px(10.))
                .bg(rgb(SIDEBAR))
                .border_1()
                .border_color(rgb(BORDER))
                .child(
                    div()
                        .text_size(px(11.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(MUTED))
                        .child(label),
                )
                .child(
                    div()
                        .text_size(px(20.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(accent.unwrap_or(TEXT)))
                        .child(value),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(rgb(MUTED))
                        .truncate()
                        .child(sub),
                )
        };
        let avg = if agg.total > 0 && agg.hours > 0. {
            format!("avg {} / task", hours(agg.hours / agg.total as f64))
        } else {
            "No time logged".into()
        };
        div()
            .flex()
            .flex_wrap()
            .gap(px(10.))
            .child(card(
                "Tasks",
                agg.total.to_string(),
                format!(
                    "{} open · {} done ({}%)",
                    agg.open,
                    agg.done,
                    agg.completion_pct()
                ),
                None,
            ))
            .child(card(
                "Logged time",
                if agg.hours > 0. {
                    hours(agg.hours)
                } else {
                    "0h".into()
                },
                avg,
                None,
            ))
            .child(card(
                "Overdue",
                agg.overdue.to_string(),
                format!("{} due today", agg.due_today),
                (agg.overdue > 0).then_some(DANGER),
            ))
            .child(card(
                "Needs attention",
                format!("{} high", agg.high),
                format!("{} unassigned", agg.unassigned),
                None,
            ))
    }

    fn toolbar(&self, people: &[String], cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .pb(px(12.))
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(self.people_chips(people, cx))
            .children(self.active_filter_pills(cx))
    }

    fn fold_link(
        &self,
        id: &'static str,
        label: &'static str,
        open: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .id(id)
            .cursor_pointer()
            .hover(|s| s.text_color(rgb(TEXT)))
            .child(label)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.set_all_open(open, cx)))
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
            .child(
                div()
                    .flex_1()
                    .min_w(px(160.))
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child("Name")
                    .child(self.fold_link("expand-all", "Expand all", true, cx))
                    .child(self.fold_link("collapse-all", "Collapse all", false, cx)),
            )
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
                .size(px(9.))
                .rounded_full()
                .bg(rgb(stage_dot(&stage)));
            let selected = self.stages.contains(&stage);
            MenuRow::new(("stage", i), dot, stage.clone(), n)
                .selected(selected)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    if !this.stages.remove(&stage) {
                        this.stages.insert(stage.clone());
                    }
                    this.collapsed.clear();
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
                Avatar::new(p, person_tone(all, p)).size(22.),
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
            .max_height(380.)
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

    fn assigned_by_menu(&self, all: &[String], cx: &mut Context<Self>) -> Floating {
        let assigners = self.store.read(cx).assigners();
        let mut menu = Floating::new("assigned-by-menu")
            .max_height(380.)
            .on_dismiss(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.close_menus();
                cx.notify()
            }))
            .child(MenuHeader::new("Assigned by").on_clear(
                self.assigned_by.is_some(),
                cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.set_assigned_by(None);
                    cx.notify()
                }),
            ));
        if assigners.is_empty() {
            menu = menu.child(
                div()
                    .flex_none()
                    .px(px(10.))
                    .py(px(8.))
                    .text_size(px(13.))
                    .text_color(rgb(MUTED))
                    .child("No delegated tasks"),
            );
        } else {
            menu = menu.children(
                assigners
                    .into_iter()
                    .enumerate()
                    .map(|(i, (person, count))| {
                        let selected = self.assigned_by.as_ref() == Some(&person);
                        let who = person.clone();
                        MenuRow::new(
                            ("assigner", i),
                            Avatar::new(&person, person_tone(all, &person)).size(22.),
                            short_name(&person).to_string(),
                            count,
                        )
                        .selected(selected)
                        .on_click(cx.listener(
                            move |this, _: &ClickEvent, _, cx| {
                                this.set_assigned_by((!selected).then(|| who.clone()));
                                cx.notify()
                            },
                        ))
                    }),
            );
        }
        menu
    }

    fn render_rows(&mut self, range: Range<usize>, cx: &mut Context<Self>) -> Vec<Stateful<Div>> {
        let items = self.items(cx);
        let range = range.start.min(items.len())..range.end.min(items.len());
        let (people, today, slice) = {
            let store = self.store.read(cx);
            let slice: Vec<(Option<Task>, &ListItem)> = items[range.clone()]
                .iter()
                .map(|item| match item {
                    ListItem::Task(r) => (Some(store.tasks[r.idx].clone()), item),
                    ListItem::Group { .. } => (None, item),
                })
                .collect();
            (store.people(), store.today, slice)
        };
        let group_by = self.filters.group;
        slice
            .into_iter()
            .zip(range)
            .map(|((task, item), i)| {
                let on = i == self.selected;
                let (row, t) = match (item, task) {
                    (ListItem::Group { key, open, agg }, _) => {
                        let gkey = key.clone();
                        let leading = match group_by {
                            GroupBy::Status => div()
                                .flex_none()
                                .size(px(9.))
                                .rounded_full()
                                .bg(rgb(stage_dot(key)))
                                .into_any_element(),
                            GroupBy::Assignee | GroupBy::AssignedBy
                                if key != "Unassigned" && key != "Unknown" =>
                            {
                                Avatar::new(key, person_tone(&people, key))
                                    .size(20.)
                                    .into_any_element()
                            }
                            GroupBy::Due if key == "Overdue" => div()
                                .flex_none()
                                .size(px(9.))
                                .rounded_full()
                                .bg(rgb(DANGER))
                                .into_any_element(),
                            GroupBy::Priority if key == "High priority" => div()
                                .flex_none()
                                .size(px(9.))
                                .rounded_full()
                                .bg(rgb(ORANGE.fg))
                                .into_any_element(),
                            _ => div().into_any_element(),
                        };
                        let label = if matches!(group_by, GroupBy::Assignee | GroupBy::AssignedBy) {
                            short_name(key).to_string()
                        } else {
                            key.clone()
                        };
                        return div()
                            .id(("group", i))
                            .w_full()
                            .h(px(44.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .px(px(10.))
                            .rounded(px(6.))
                            .bg(rgb(if on { HOVER } else { SIDEBAR }))
                            .border_t_1()
                            .border_color(rgb(BORDER))
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(HOVER)))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.selected = i;
                                this.toggle_group(&gkey);
                                cx.notify()
                            }))
                            .child(icon(
                                if *open {
                                    "chevron-down"
                                } else {
                                    "chevron-right"
                                },
                                14.,
                                MUTED,
                            ))
                            .child(leading)
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_size(px(13.))
                                    .text_color(rgb(TEXT))
                                    .truncate()
                                    .child(label),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .px(px(7.))
                                    .py(px(1.))
                                    .rounded_full()
                                    .bg(rgb(PAGE))
                                    .border_1()
                                    .border_color(rgb(BORDER))
                                    .text_size(px(11.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgb(TEXT_SOFT))
                                    .child(plural(agg.total, "task")),
                            )
                            .child(div().flex_1())
                            .child(
                                div()
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .gap(px(12.))
                                    .text_size(px(12.))
                                    .text_color(rgb(MUTED))
                                    .when(agg.done > 0, |d| {
                                        d.child(format!(
                                            "{}/{} done ({}%)",
                                            agg.done,
                                            agg.total,
                                            agg.completion_pct()
                                        ))
                                    })
                                    .when(agg.overdue > 0, |d| {
                                        d.child(
                                            div()
                                                .text_color(rgb(DANGER))
                                                .font_weight(FontWeight::MEDIUM)
                                                .child(format!("{} overdue", agg.overdue)),
                                        )
                                    })
                                    .when(agg.high > 0, |d| d.child(format!("{} high", agg.high)))
                                    .when(agg.hours > 0., |d| {
                                        d.child(
                                            div()
                                                .text_color(rgb(TEXT_SOFT))
                                                .font_weight(FontWeight::MEDIUM)
                                                .child(hours(agg.hours)),
                                        )
                                    }),
                            );
                    }
                    (ListItem::Task(row), Some(t)) => (row, t),
                    _ => unreachable!(),
                };
                let id = t.id;
                let mut due = date::due(t.deadline.as_deref(), today);
                due.overdue &= !t.done;
                let forced = row.forced;
                let fold = div()
                    .id(("fold", i))
                    .flex_none()
                    .size(px(20.))
                    .rounded(px(4.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(row.kids > 0, |d| {
                        d.cursor_pointer()
                            .hover(|s| s.bg(rgb(BORDER)))
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                                    this.toggle(id, forced);
                                    cx.stop_propagation();
                                    cx.notify()
                                }),
                            )
                            .on_click(|_, _, cx| cx.stop_propagation())
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
                    .h(px(44.))
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .px(px(10.))
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
        let (people, summary, agg, count) = {
            let store = self.store.read(cx);
            let today = store.today;
            let known: HashSet<i64> = store.tasks.iter().map(|t| t.id).collect();
            let visible = self.visible(&store.tasks, today, cx);
            let agg = Aggregate::compute(&visible, today);
            let summary = self.summary(&visible, &known, agg);
            (store.people(), summary, agg, self.items(cx).len())
        };
        let show_stats = self.filters.show_stats || self.filters.group != GroupBy::None;
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .h_full()
            .pt(px(40.))
            .px(px(48.))
            .gap(px(16.))
            .child(
                div()
                    .flex()
                    .items_end()
                    .justify_between()
                    .gap(px(16.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(32.))
                                    .font_weight(FontWeight::BOLD)
                                    .child(self.project.clone().unwrap_or("My Tasks".into())),
                            )
                            .child(div().text_color(rgb(MUTED)).child(summary)),
                    )
                    .child(self.search_controls(window, cx)),
            )
            .child(self.toolbar(&people, cx))
            .when(show_stats, |d| d.child(self.stats_strip(agg)))
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
