mod ai;
mod assets;
mod connect;
mod date;
mod input;
mod odoo;
mod ollama;
mod settings;
mod settings_view;
mod task_page;
mod theme;
mod tree;
mod ui;

use connect::{ConnectView, Connected};
use gpui::{
    Anchor, App, Bounds, ClickEvent, Context, Div, Entity, FocusHandle, Focusable, FontWeight,
    KeyDownEvent, MouseButton, MouseDownEvent, ScrollStrategy, SharedString, Stateful,
    TitlebarOptions, UniformListScrollHandle, Window, WindowBounds, WindowOptions, anchored,
    deferred, div, point, prelude::*, px, rgb, size, uniform_list, white,
};
use input::{InputEvent, TextInput};
use odoo::{Login, Odoo, Stage, Task};
use serde_json::json;
use settings_view::{SettingsEvent, SettingsView};
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    sync::Arc,
};
use task_page::{Detail, Draft, Tab};
use theme::*;
use ui::{icon, kbd, popover, tag};

struct BetterOdoo {
    focus: FocusHandle,
    filter: Entity<TextInput>,
    odoo: Option<Arc<Odoo>>,
    workspace: SharedString,
    version: SharedString,
    tasks: Vec<Task>,
    assignee: Option<String>,
    project: Option<String>,
    selected: usize,
    status: SharedString,
    proposed: Vec<Draft>,
    draft_ai: bool,
    composer: Option<Entity<TextInput>>,
    drafting: bool,
    ai_error: Option<SharedString>,
    viewing: Option<i64>,
    tab: Tab,
    notice: Option<SharedString>,
    quick: Entity<TextInput>,
    ts_name: Entity<TextInput>,
    ts_hours: Entity<TextInput>,
    ts_date: Entity<TextInput>,
    stages: HashMap<i64, Vec<Stage>>,
    people_menu: bool,
    shortcuts: bool,
    settings: Option<Entity<SettingsView>>,
    expanded: HashSet<i64>,
    stage_filter: HashSet<String>,
    stage_menu: bool,
    people_header_menu: bool,
    today: i64,
    connect: Option<Entity<ConnectView>>,
    rows_scroll: UniformListScrollHandle,
    details: HashMap<i64, Option<Result<Detail, SharedString>>>,
}

const KEYCHAIN: &str = "BetterOdoo";

impl BetterOdoo {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter = cx.new(|cx| TextInput::new("Filter or ask in plain words", cx));
        cx.subscribe_in(&filter, window, Self::on_filter).detach();
        let entry = |placeholder: &'static str, cx: &mut Context<Self>| {
            cx.new(|cx| TextInput::new(placeholder, cx))
        };
        let quick = entry("Add a sub-task and press Enter", cx);
        let ts_name = entry("What did you work on?", cx);
        let ts_hours = entry("Hours", cx);
        let ts_date = entry("YYYY-MM-DD", cx);
        for input in [&quick, &ts_name, &ts_hours, &ts_date] {
            cx.subscribe_in(input, window, Self::on_entry).detach();
        }
        let mut me = Self {
            focus: cx.focus_handle(),
            filter,
            odoo: None,
            workspace: "".into(),
            version: "".into(),
            tasks: vec![],
            assignee: None,
            project: None,
            selected: 0,
            status: "Connecting…".into(),
            proposed: vec![],
            composer: None,
            drafting: false,
            ai_error: None,
            viewing: None,
            tab: Tab::SubTasks,
            notice: None,
            quick,
            ts_name,
            ts_hours,
            ts_date,
            stages: HashMap::new(),
            draft_ai: false,
            people_menu: false,
            shortcuts: false,
            settings: None,
            expanded: HashSet::new(),
            stage_filter: HashSet::new(),
            stage_menu: false,
            people_header_menu: false,
            today: date::today(),
            connect: None,
            rows_scroll: UniformListScrollHandle::new(),
            details: HashMap::new(),
        };
        me.restore(window, cx);
        me
    }

    fn restore(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let read = cx.read_credentials(KEYCHAIN);
        cx.spawn_in(window, async move |this, cx| {
            let saved = read
                .await
                .ok()
                .flatten()
                .and_then(|(user, secret)| Login::from_credentials(&user, secret));
            this.update_in(cx, |this, window, cx| match saved {
                Some(login) => this.sign_in(login, window, cx),
                None => this.show_connect(None, window, cx),
            })
            .ok();
        })
        .detach();
    }

    fn sign_in(&mut self, login: Login, window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move { Odoo::connect(&login) })
                .await;
            this.update_in(cx, |this, window, cx| match r {
                Ok(o) => this.open(Arc::new(o), window, cx),
                Err(e) => this.show_connect(Some(e.to_string()), window, cx),
            })
            .ok();
        })
        .detach();
    }

    fn show_connect(&mut self, error: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.new(|cx| ConnectView::new(error, window, cx));
        cx.subscribe_in(&view, window, Self::on_connected).detach();
        self.connect = Some(view);
        cx.notify();
    }

    fn on_connected(
        &mut self,
        _: &Entity<ConnectView>,
        ev: &Connected,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (user, secret) = ev.login.to_credentials();
        cx.write_credentials(KEYCHAIN, &user, &secret).detach();
        self.open(ev.odoo.clone(), window, cx);
    }

    fn open(&mut self, odoo: Arc<Odoo>, window: &mut Window, cx: &mut Context<Self>) {
        self.workspace = odoo.workspace().into();
        self.odoo = Some(odoo);
        self.connect = None;
        self.status = "Syncing…".into();
        window.focus(&self.focus, cx);
        self.reload(cx);
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        let Some(o) = self.odoo.clone() else { return };
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move {
                    let version = o.version().unwrap_or_default();
                    anyhow::Ok((version, o.tasks(json!([]))?))
                })
                .await;
            this.update(cx, |this, cx| {
                match r {
                    Ok((version, t)) => {
                        this.version = version.into();
                        this.tasks = t;
                        this.details.clear();
                        this.today = date::today();
                        this.status = "Synced just now".into();
                    }
                    Err(e) => this.status = e.to_string().into(),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
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
                self.selected = 0;
                self.rows_scroll.scroll_to_item(0, ScrollStrategy::Top);
                self.reset_ai();
            }
            InputEvent::Submit => window.focus(&self.focus, cx),
            InputEvent::Cancel => {
                input.update(cx, |i, cx| i.clear(window, cx));
                window.focus(&self.focus, cx);
            }
        }
        cx.notify();
    }

    fn matches(&self, t: &Task, q: &str) -> bool {
        self.assignee
            .as_ref()
            .is_none_or(|a| t.assignees.contains(a))
            && self.project.as_ref().is_none_or(|p| &t.project == p)
            && (self.stage_filter.is_empty() || self.stage_filter.contains(&t.stage))
            && (q.is_empty()
                || t.name.to_lowercase().contains(q)
                || t.project.to_lowercase().contains(q))
    }

    fn visible(&self, cx: &App) -> Vec<&Task> {
        let q = self.filter.read(cx).text().to_lowercase();
        self.tasks.iter().filter(|t| self.matches(t, &q)).collect()
    }

    fn rows(&self, cx: &App) -> Vec<tree::Row> {
        let q = self.filter.read(cx).text().to_lowercase();
        let nodes: Vec<(i64, Option<i64>)> = self
            .tasks
            .iter()
            .map(|t| (t.id, t.parent.as_ref().map(|p| p.0)))
            .collect();
        let own: Vec<bool> = self.tasks.iter().map(|t| self.matches(t, &q)).collect();
        tree::rows(&nodes, &own, &self.expanded)
    }

    fn people(&self) -> Vec<String> {
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for name in self.tasks.iter().flat_map(|t| &t.assignees) {
            *counts.entry(name).or_default() += 1;
        }
        let mut p: Vec<(&str, usize)> = counts.into_iter().collect();
        p.sort_by_key(|&(n, c)| (n != "Me", std::cmp::Reverse(c), n.to_lowercase()));
        p.into_iter().map(|(n, _)| n.to_string()).collect()
    }

    fn projects(&self) -> Vec<String> {
        let mut p: Vec<String> = self.tasks.iter().map(|t| t.project.clone()).collect();
        p.sort();
        p.dedup();
        p
    }

    fn follow(&mut self, rows: &[tree::Row], window: &mut Window, cx: &mut Context<Self>) {
        match (self.viewing, rows.get(self.selected)) {
            (Some(_), Some(row)) => {
                let id = self.tasks[row.idx].id;
                self.open_task(id, window, cx)
            }
            _ => self.reset_ai(),
        }
    }

    fn set_assignee(&mut self, who: Option<String>) {
        self.assignee = who;
        self.selected = 0;
        self.rows_scroll.scroll_to_item(0, ScrollStrategy::Top);
        self.reset_ai();
    }

    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.focus.is_focused(window) {
            return;
        }
        if self.settings.is_some() {
            if ev.keystroke.key == "escape" {
                self.close_settings(window, cx);
            }
            return;
        }
        let rows = self.rows(cx);
        let n = rows.len();
        let secondary = ev.keystroke.modifiers.secondary(cx);
        if ev.keystroke.key_char.as_deref() == Some("?") {
            self.shortcuts = !self.shortcuts;
            cx.notify();
            return;
        }
        match ev.keystroke.key.as_str() {
            "k" if secondary => window.focus(&self.filter.focus_handle(cx), cx),
            "/" => window.focus(&self.filter.focus_handle(cx), cx),
            "j" | "down" if self.selected + 1 < n => {
                self.selected += 1;
                self.follow(&rows, window, cx);
            }
            "k" | "up" => {
                self.selected = self.selected.saturating_sub(1);
                self.follow(&rows, window, cx);
            }
            "right" | "left" => {
                let Some(row) = rows.get(self.selected) else {
                    return;
                };
                let id = self.tasks[row.idx].id;
                if row.forced || row.kids == 0 {
                    return;
                }
                if ev.keystroke.key == "right" {
                    self.expanded.insert(id);
                } else {
                    self.expanded.remove(&id);
                }
            }
            "a" => self.open_composer(window, cx),
            "enter" if !self.proposed.is_empty() => self.commit_work(cx),
            "enter" if n > 0 && self.viewing.is_none() => {
                let id = self.tasks[rows[self.selected].idx].id;
                self.open_task(id, window, cx)
            }
            "escape" if self.people_menu => self.people_menu = false,
            "escape" if self.shortcuts => self.shortcuts = false,
            "escape" if self.stage_menu => self.stage_menu = false,
            "escape" if self.people_header_menu => self.people_header_menu = false,
            "escape" if !self.proposed.is_empty() => self.proposed.clear(),
            "escape" if self.composer.is_some() => self.reset_ai(),
            "escape" => self.back(window, cx),
            "0" => self.set_assignee(None),
            d if d.len() == 1 && d.as_bytes()[0].is_ascii_digit() => {
                let p = self
                    .people()
                    .into_iter()
                    .take(TOP_PEOPLE)
                    .nth(d.parse::<usize>().unwrap() - 1);
                if p.is_some() {
                    self.set_assignee(p)
                }
            }
            _ => return,
        }
        self.rows_scroll
            .scroll_to_item(self.selected, ScrollStrategy::Nearest);
        cx.notify();
    }
}

fn stage_tone(stage: &str) -> Tone {
    let s = stage.to_lowercase();
    if s.contains("progress") {
        BLUE
    } else if s.contains("review") || s.contains("test") {
        PURPLE
    } else if s.contains("done") || s.contains("complet") {
        GREEN
    } else {
        GRAY
    }
}

fn hours(h: f64) -> String {
    if h <= 0. {
        return "—".into();
    }
    let total = (h * 60.).round() as i64;
    match (total / 60, total % 60) {
        (h, 0) => format!("{h}h"),
        (0, m) => format!("{m}m"),
        (h, m) => format!("{h}h {m}m"),
    }
}

fn nav_item(
    id: impl Into<SharedString>,
    icon_name: &str,
    label: String,
    active: bool,
) -> Stateful<Div> {
    div()
        .id(id.into())
        .flex()
        .items_center()
        .gap(px(10.))
        .px(px(10.))
        .py(px(6.))
        .rounded(px(6.))
        .cursor_pointer()
        .text_color(rgb(if active { TEXT } else { TEXT_SOFT }))
        .when(active, |d| d.bg(rgb(HOVER)).font_weight(FontWeight::MEDIUM))
        .hover(|s| s.bg(rgb(HOVER)))
        .child(icon(icon_name, 16., if active { TEXT } else { MUTED }))
        .child(div().flex_1().child(label))
}

const TOP_PEOPLE: usize = 5;

fn header_cell(id: &'static str, label: &'static str, active: bool, open: bool) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(6.))
        .px(px(6.))
        .py(px(3.))
        .ml(px(-6.))
        .rounded(px(5.))
        .cursor_pointer()
        .text_color(rgb(if active { TEXT } else { MUTED }))
        .when(active, |d| d.font_weight(FontWeight::MEDIUM))
        .when(open, |d| d.bg(rgb(HOVER)))
        .hover(|s| s.bg(rgb(HOVER)))
        .child(label)
        .child(icon("filter", 13., if active { ACCENT } else { MUTED }))
}

fn menu_header(title: &'static str, clear: Option<Stateful<Div>>) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .px(px(10.))
        .pt(px(8.))
        .pb(px(6.))
        .text_size(px(12.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(MUTED))
        .child(title)
        .children(clear.map(|c| {
            c.cursor_pointer()
                .text_color(rgb(ACCENT))
                .hover(|s| s.text_color(rgb(TEXT)))
                .child("Clear")
        }))
}

fn menu_row(
    id: impl Into<gpui::ElementId>,
    leading: Div,
    label: String,
    count: usize,
    selected: bool,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(10.))
        .h(px(32.))
        .px(px(10.))
        .rounded(px(6.))
        .cursor_pointer()
        .text_size(px(13.))
        .hover(|s| s.bg(rgb(HOVER)))
        .child(leading)
        .child(div().flex_1().min_w_0().truncate().child(label))
        .child(
            div()
                .text_size(px(12.))
                .text_color(rgb(MUTED))
                .child(count.to_string()),
        )
        .child(
            div()
                .flex_none()
                .size(px(14.))
                .when(selected, |d| d.child(icon("check", 14., ACCENT))),
        )
}

fn short_name(name: &str) -> &str {
    name.split('@').next().unwrap_or(name)
}

const COLS: [f32; 4] = [170., 92., 170., 110.];

impl Render for BetterOdoo {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(connect) = &self.connect {
            return div()
                .size_full()
                .text_color(rgb(TEXT))
                .font_family(FONT)
                .text_size(px(14.))
                .child(connect.clone())
                .into_any_element();
        }
        let visible: Vec<Task> = self.visible(cx).into_iter().cloned().collect();
        let rows = self.rows(cx);
        let people = self.people();
        let projects = self.projects();
        let overdue = visible
            .iter()
            .filter(|t| !t.done && date::due(t.deadline.as_deref(), self.today).overdue)
            .count();
        let known: HashSet<i64> = self.tasks.iter().map(|t| t.id).collect();
        let subs = visible
            .iter()
            .filter(|t| t.parent.as_ref().is_some_and(|p| known.contains(&p.0)))
            .count();
        let roots = visible.len() - subs;
        let mut summary = format!("{roots} task{}", if roots == 1 { "" } else { "s" });
        if subs > 0 {
            summary += &format!(" · {subs} sub-task{}", if subs == 1 { "" } else { "s" });
        }
        if self.project.is_none() {
            let spread = visible
                .iter()
                .map(|t| &t.project)
                .collect::<std::collections::HashSet<_>>()
                .len();
            summary += &format!(
                " across {spread} project{}",
                if spread == 1 { "" } else { "s" }
            );
        }
        if overdue > 0 {
            summary += &format!(" · {overdue} overdue");
        }
        let filter_focused = self.filter.focus_handle(cx).is_focused(window);

        let mut sidebar = div()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(240.))
            .h_full()
            .px(px(8.))
            .py(px(10.))
            .gap(px(1.))
            .bg(rgb(SIDEBAR))
            .border_r_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .px(px(10.))
                    .pt(px(6.))
                    .pb(px(14.))
                    .child(
                        div()
                            .flex_none()
                            .size(px(22.))
                            .rounded(px(5.))
                            .bg(rgb(TEXT))
                            .text_color(white())
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                self.workspace
                                    .chars()
                                    .next()
                                    .map_or("·".into(), |c| c.to_uppercase().to_string()),
                            ),
                    )
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .truncate()
                            .child(self.workspace.clone()),
                    )
                    .child(
                        div()
                            .ml_auto()
                            .text_size(px(12.))
                            .text_color(rgb(MUTED))
                            .child(self.version.clone()),
                    ),
            )
            .child(
                nav_item("nav-search", "search", "Search".into(), false)
                    .child(div().text_size(px(12.)).text_color(rgb(MUTED)).child(
                        if cfg!(target_os = "macos") {
                            "⌘K"
                        } else {
                            "Ctrl K"
                        },
                    ))
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        window.focus(&this.filter.focus_handle(cx), cx)
                    })),
            )
            .child(
                nav_item(
                    "nav-tasks",
                    "tasks",
                    "My Tasks".into(),
                    self.project.is_none(),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .font_weight(FontWeight::NORMAL)
                        .text_color(rgb(MUTED))
                        .child(self.tasks.len().to_string()),
                )
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.project = None;
                    this.viewing = None;
                    this.settings = None;
                    this.selected = 0;
                    cx.notify()
                })),
            )
            .child(nav_item(
                "nav-timesheets",
                "clock",
                "Timesheets".into(),
                false,
            ))
            .child(
                div()
                    .px(px(10.))
                    .pt(px(18.))
                    .pb(px(6.))
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(MUTED))
                    .child("Projects"),
            );
        let mut project_list = div()
            .id("projects")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap(px(1.))
            .overflow_y_scroll();
        for (i, p) in projects.iter().enumerate() {
            let active = self.project.as_ref() == Some(p);
            let name = p.clone();
            project_list = project_list.child(
                div()
                    .id(("project", i))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .px(px(10.))
                    .py(px(6.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .text_color(rgb(if active { TEXT } else { TEXT_SOFT }))
                    .when(active, |d| d.bg(rgb(HOVER)).font_weight(FontWeight::MEDIUM))
                    .hover(|s| s.bg(rgb(HOVER)))
                    .child(
                        div()
                            .flex_none()
                            .size(px(8.))
                            .rounded(px(2.))
                            .bg(rgb(PEOPLE[i % PEOPLE.len()].fg)),
                    )
                    .child(div().truncate().child(p.clone()))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.project = Some(name.clone());
                        this.viewing = None;
                        this.settings = None;
                        this.selected = 0;
                        this.rows_scroll.scroll_to_item(0, ScrollStrategy::Top);
                        this.reset_ai();
                        cx.notify()
                    })),
            );
        }
        let synced = self.odoo.is_some() && self.status.as_ref() == "Synced just now";
        let was_open = self.shortcuts;
        let settings_open = self.settings.is_some();
        sidebar = sidebar
            .child(project_list)
            .child(
                div()
                    .child(
                        nav_item("nav-shortcuts", "keyboard", "Shortcuts".into(), was_open)
                            .child(kbd("?"))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                                    this.shortcuts = !was_open;
                                    cx.notify()
                                }),
                            ),
                    )
                    .when(was_open, |d| {
                        d.child(self.shortcuts_popover(people.len(), cx))
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .p(px(10.))
                    .text_size(px(12.))
                    .text_color(rgb(MUTED))
                    .child(
                        div()
                            .flex_none()
                            .size(px(6.))
                            .rounded_full()
                            .bg(rgb(if synced { SUCCESS } else { MUTED })),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(self.status.clone()),
                    )
                    .child(
                        div()
                            .id("cog")
                            .flex_none()
                            .p(px(5.))
                            .rounded(px(6.))
                            .cursor_pointer()
                            .when(settings_open, |d| d.bg(rgb(HOVER)))
                            .hover(|s| s.bg(rgb(HOVER)))
                            .child(icon("cog", 16., MUTED))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                                    this.shortcuts = false;
                                    if settings_open {
                                        this.close_settings(window, cx);
                                    } else {
                                        this.open_settings(window, cx);
                                    }
                                }),
                            ),
                    ),
            );

        let chip = |id: SharedString, label: String, on: bool| {
            div()
                .id(id)
                .px(px(12.))
                .py(px(5.))
                .rounded_full()
                .border_1()
                .cursor_pointer()
                .text_size(px(13.))
                .font_weight(FontWeight::MEDIUM)
                .border_color(rgb(if on { TEXT } else { BORDER }))
                .bg(rgb(if on { TEXT } else { PAGE }))
                .text_color(rgb(if on { PAGE } else { TEXT_SOFT }))
                .when(!on, |d| d.hover(|s| s.bg(rgb(HOVER))))
                .child(label)
        };
        let mut chips = div().flex().flex_wrap().items_center().gap(px(6.)).child(
            chip(
                "chip-all".into(),
                "Everyone".into(),
                self.assignee.is_none(),
            )
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.set_assignee(None);
                cx.notify()
            })),
        );
        let (top, rest) = people.split_at(people.len().min(TOP_PEOPLE));
        let picked_from_rest = self.assignee.as_ref().filter(|a| rest.contains(a));
        for (i, p) in top.iter().chain(picked_from_rest).enumerate() {
            let who = p.clone();
            chips = chips.child(
                chip(
                    format!("chip-{i}").into(),
                    short_name(p).to_string(),
                    self.assignee.as_ref() == Some(p),
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.set_assignee(Some(who.clone()));
                    cx.notify()
                })),
            );
        }
        if !rest.is_empty() {
            let was_open = self.people_menu;
            chips = chips.child(
                div()
                    .child(
                        chip("chip-more".into(), format!("+{} more", rest.len()), false)
                            .when(was_open, |d| d.bg(rgb(HOVER)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                                    this.people_menu = !was_open;
                                    cx.notify()
                                }),
                            ),
                    )
                    .when(was_open, |d| d.child(self.people_menu(rest, cx))),
            );
        }

        let toolbar = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(6.))
            .pb(px(10.))
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(chips)
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
                    .border_color(rgb(if filter_focused { MUTED } else { BORDER }))
                    .text_size(px(13.))
                    .child(icon("sparkle", 15., MUTED))
                    .child(self.filter.clone()),
            );

        let header_row = div()
            .flex()
            .gap(px(12.))
            .px(px(10.))
            .pb(px(8.))
            .text_size(px(12.))
            .text_color(rgb(MUTED))
            .child(div().flex_1().min_w(px(160.)).child("Name"))
            .child({
                let open = self.stage_menu;
                let active = !self.stage_filter.is_empty();
                div()
                    .flex_none()
                    .w(px(COLS[0]))
                    .child(
                        header_cell("h-status", "Status", active, open).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                                this.stage_menu = !open;
                                cx.notify()
                            }),
                        ),
                    )
                    .when(open, |d| d.child(self.stage_menu(cx)))
            })
            .child({
                let open = self.people_header_menu;
                div()
                    .flex_none()
                    .flex()
                    .w(px(COLS[1]))
                    .child(
                        header_cell("h-people", "Assignees", self.assignee.is_some(), open)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                                    this.people_header_menu = !open;
                                    cx.notify()
                                }),
                            ),
                    )
                    .when(open, |d| d.child(self.people_menu(&people, cx)))
            })
            .child(div().flex_none().w(px(COLS[2])).child("Project"))
            .child(div().flex_none().w(px(COLS[3])).child("Due"));

        let main = div()
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
            .child(toolbar)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .child(header_row)
                    .child(
                        uniform_list(
                            "rows",
                            rows.len(),
                            cx.processor(|this, range: Range<usize>, _, cx| {
                                this.render_rows(range, cx)
                            }),
                        )
                        .track_scroll(&self.rows_scroll)
                        .flex_1(),
                    ),
            );

        let selected: Option<Task> = self
            .viewing
            .and_then(|id| self.tasks.iter().find(|t| t.id == id))
            .cloned();
        let content = match (&self.settings, selected) {
            (Some(view), _) => view.clone().into_any_element(),
            (None, Some(t)) => self.detail(&t, window, cx).into_any_element(),
            (None, None) => main.into_any_element(),
        };

        div()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .size_full()
            .flex()
            .bg(rgb(PAGE))
            .text_color(rgb(TEXT))
            .font_family(FONT)
            .text_size(px(14.))
            .child(sidebar)
            .child(content)
            .into_any_element()
    }
}

impl BetterOdoo {
    fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (workspace, version) = (self.workspace.clone(), self.version.clone());
        let view = cx.new(|cx| SettingsView::new(workspace, version, window, cx));
        cx.subscribe_in(&view, window, Self::on_settings).detach();
        self.settings = Some(view);
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn on_settings(
        &mut self,
        _: &Entity<SettingsView>,
        ev: &SettingsEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match ev {
            SettingsEvent::Close => self.close_settings(window, cx),
            SettingsEvent::SignOut => self.sign_out(window, cx),
        }
    }

    fn sign_out(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.delete_credentials(KEYCHAIN).detach();
        self.odoo = None;
        self.tasks.clear();
        self.details.clear();
        self.assignee = None;
        self.project = None;
        self.stage_filter.clear();
        self.viewing = None;
        self.settings = None;
        self.selected = 0;
        self.reset_ai();
        self.show_connect(None, window, cx);
    }

    fn shortcuts_popover(&self, people: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let filter = if cfg!(target_os = "macos") {
            "/  ⌘K"
        } else {
            "/  Ctrl K"
        };
        let people = format!("1–{}", people.clamp(1, TOP_PEOPLE));
        let rows = [
            ("j  k", "Move between tasks"),
            ("↵", "Open task"),
            ("←  →", "Collapse or expand sub-tasks"),
            ("Esc", "Back to list"),
            (people.as_str(), "Filter by person"),
            ("0", "Everyone"),
            (filter, "Filter tasks"),
            ("a", "Autofill sub-tasks with AI"),
            ("?", "Show shortcuts"),
        ];
        deferred(
            anchored()
                .anchor(Anchor::BottomLeft)
                .snap_to_window()
                .offset(point(px(0.), px(-38.)))
                .child(
                    popover()
                        .id("shortcuts")
                        .occlude()
                        .w(px(300.))
                        .p(px(14.))
                        .flex()
                        .flex_col()
                        .gap(px(10.))
                        .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                            this.shortcuts = false;
                            cx.notify()
                        }))
                        .child(
                            div()
                                .text_size(px(12.))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(MUTED))
                                .child("Keyboard shortcuts"),
                        )
                        .children(rows.into_iter().map(|(k, label)| {
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .text_size(px(13.))
                                .child(label)
                                .child(div().flex().gap(px(4.)).children(k.split("  ").map(kbd)))
                        })),
                ),
        )
        .with_priority(1)
    }

    fn stage_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut stages: Vec<(String, usize)> = Vec::new();
        for t in &self.tasks {
            match stages.iter_mut().find(|(s, _)| s == &t.stage) {
                Some((_, n)) => *n += 1,
                None => stages.push((t.stage.clone(), 1)),
            }
        }
        stages.sort_by_key(|(s, n)| (std::cmp::Reverse(*n), s.to_lowercase()));
        let items = stages.into_iter().enumerate().map(|(i, (stage, n))| {
            let dot = div()
                .flex_none()
                .size(px(8.))
                .rounded_full()
                .bg(rgb(stage_tone(&stage).fg));
            let selected = self.stage_filter.contains(&stage);
            menu_row(("stage", i), dot, stage.clone(), n, selected).on_click(cx.listener(
                move |this, _: &ClickEvent, _, cx| {
                    if !this.stage_filter.remove(&stage) {
                        this.stage_filter.insert(stage.clone());
                    }
                    this.selected = 0;
                    this.rows_scroll.scroll_to_item(0, ScrollStrategy::Top);
                    cx.notify()
                },
            ))
        });
        let clear = (!self.stage_filter.is_empty()).then(|| {
            div()
                .id("stages-clear")
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.stage_filter.clear();
                    cx.notify()
                }))
        });
        deferred(
            anchored()
                .snap_to_window()
                .offset(point(px(0.), px(6.)))
                .child(
                    popover()
                        .id("stage-menu")
                        .occlude()
                        .w(px(260.))
                        .max_h(px(380.))
                        .overflow_y_scroll()
                        .p(px(4.))
                        .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                            this.stage_menu = false;
                            cx.notify()
                        }))
                        .child(menu_header("Show stages", clear))
                        .children(items),
                ),
        )
        .with_priority(1)
    }

    fn people_menu(&self, rest: &[String], cx: &mut Context<Self>) -> impl IntoElement {
        let all = self.people();
        let counts: HashMap<&String, usize> = rest
            .iter()
            .map(|p| {
                (
                    p,
                    self.tasks
                        .iter()
                        .filter(|t| t.assignees.contains(p))
                        .count(),
                )
            })
            .collect();
        let items = rest.iter().enumerate().map(|(i, p)| {
            let tone = PEOPLE[all.iter().position(|x| x == p).unwrap_or(i) % PEOPLE.len()];
            let avatar = div()
                .flex_none()
                .size(px(20.))
                .rounded_full()
                .bg(rgb(tone.bg))
                .text_color(rgb(tone.fg))
                .text_size(px(10.))
                .font_weight(FontWeight::SEMIBOLD)
                .flex()
                .items_center()
                .justify_center()
                .child(p.chars().next().unwrap_or('?').to_uppercase().to_string());
            let who = p.clone();
            menu_row(
                ("person", i),
                avatar,
                short_name(p).to_string(),
                counts.get(p).copied().unwrap_or(0),
                self.assignee.as_ref() == Some(p),
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.set_assignee(Some(who.clone()));
                this.people_menu = false;
                this.people_header_menu = false;
                cx.notify()
            }))
        });
        let clear = self.assignee.is_some().then(|| {
            div()
                .id("people-clear")
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.set_assignee(None);
                    this.people_menu = false;
                    this.people_header_menu = false;
                    cx.notify()
                }))
        });
        deferred(
            anchored()
                .snap_to_window()
                .offset(point(px(0.), px(6.)))
                .child(
                    popover()
                        .id("people-menu")
                        .occlude()
                        .w(px(260.))
                        .max_h(px(340.))
                        .overflow_y_scroll()
                        .p(px(4.))
                        .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                            this.people_menu = false;
                            this.people_header_menu = false;
                            cx.notify()
                        }))
                        .child(menu_header("Filter by assignee", clear))
                        .children(items),
                ),
        )
        .with_priority(1)
    }

    fn render_rows(&mut self, range: Range<usize>, cx: &mut Context<Self>) -> Vec<Stateful<Div>> {
        let rows = self.rows(cx);
        let people = self.people();
        let tone_of =
            |name: &str| PEOPLE[people.iter().position(|p| p == name).unwrap_or(0) % PEOPLE.len()];
        let range = range.start.min(rows.len())..range.end.min(rows.len());
        rows[range.clone()]
            .iter()
            .zip(range)
            .map(|(row, i)| {
                let t = &self.tasks[row.idx];
                let id = t.id;
                let on = i == self.selected;
                let mut due = date::due(t.deadline.as_deref(), self.today);
                due.overdue &= !t.done;
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
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.selected = i;
                        this.open_task(id, window, cx)
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(160.))
                            .overflow_hidden()
                            .flex()
                            .items_center()
                            .gap(px(4.))
                            .pl(px(row.depth as f32 * 22.))
                            .child(
                                div()
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
                                                MouseButton::Left,
                                                cx.listener(
                                                    move |this, _: &MouseDownEvent, _, cx| {
                                                        if !this.expanded.remove(&id) {
                                                            this.expanded.insert(id);
                                                        }
                                                        cx.stop_propagation();
                                                        cx.notify()
                                                    },
                                                ),
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
                                    }),
                            )
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
                            }),
                    )
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
                            let tone = tone_of(p);
                            div()
                                .size(px(24.))
                                .rounded_full()
                                .border_2()
                                .border_color(white())
                                .bg(rgb(tone.bg))
                                .text_color(rgb(tone.fg))
                                .text_size(px(11.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .flex()
                                .items_center()
                                .justify_center()
                                .when(j > 0, |d| d.ml(px(-8.)))
                                .child(p.chars().next().unwrap_or('?').to_string())
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

fn main() {
    gpui_platform::application()
        .with_assets(assets::Assets)
        .run(|cx: &mut App| {
            assets::load_fonts(cx).expect("bundled fonts load");
            input::bind_keys(cx);
            let bounds = Bounds::centered(None, size(px(1440.), px(900.)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("BetterOdoo".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |window, cx| {
                    let view = cx.new(|cx| BetterOdoo::new(window, cx));
                    let focus = view.read(cx).focus.clone();
                    window.focus(&focus, cx);
                    view
                },
            )
            .unwrap();
            cx.activate(true);
        });
}
