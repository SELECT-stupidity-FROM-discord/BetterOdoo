mod ai;
mod assets;
mod connect;
mod date;
mod input;
mod odoo;
mod ollama;
mod settings;
mod settings_view;
mod theme;
mod ui;

use connect::{ConnectView, Connected};
use gpui::{
    Anchor, App, Bounds, ClickEvent, Context, Div, Entity, FocusHandle, Focusable, FontWeight,
    HighlightStyle, KeyDownEvent, MouseButton, MouseDownEvent, ScrollStrategy, SharedString,
    Stateful, StyledText, TitlebarOptions, UniformListScrollHandle, Window, WindowBounds,
    WindowOptions, anchored, deferred, div, point, prelude::*, px, rgb, size, uniform_list, white,
};
use input::{InputEvent, TextInput};
use odoo::{Login, Odoo, SubTask, Task, WorkItem};
use serde_json::json;
use settings_view::{SettingsEvent, SettingsView};
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    sync::Arc,
};
use theme::*;
use ui::{icon, kbd, popover, tag};

struct Proposal {
    item: WorkItem,
    keep: bool,
}

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
    proposed: Vec<Proposal>,
    composer: Option<Entity<TextInput>>,
    drafting: bool,
    ai_error: Option<SharedString>,
    page: bool,
    people_menu: bool,
    shortcuts: bool,
    settings: Option<Entity<SettingsView>>,
    stage_filter: HashSet<String>,
    stage_menu: bool,
    people_header_menu: bool,
    today: i64,
    connect: Option<Entity<ConnectView>>,
    rows_scroll: UniformListScrollHandle,
    subtasks: HashMap<i64, Option<Result<Vec<SubTask>, SharedString>>>,
}

const KEYCHAIN: &str = "BetterOdoo";

impl BetterOdoo {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter = cx.new(|cx| TextInput::new("Filter or ask in plain words", cx));
        cx.subscribe_in(&filter, window, Self::on_filter).detach();
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
            page: false,
            people_menu: false,
            shortcuts: false,
            settings: None,
            stage_filter: HashSet::new(),
            stage_menu: false,
            people_header_menu: false,
            today: date::today(),
            connect: None,
            rows_scroll: UniformListScrollHandle::new(),
            subtasks: HashMap::new(),
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
                        this.subtasks.clear();
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

    fn visible(&self, cx: &App) -> Vec<&Task> {
        let q = self.filter.read(cx).text().to_lowercase();
        self.tasks
            .iter()
            .filter(|t| {
                self.assignee
                    .as_ref()
                    .is_none_or(|a| t.assignees.contains(a))
            })
            .filter(|t| self.project.as_ref().is_none_or(|p| &t.project == p))
            .filter(|t| self.stage_filter.is_empty() || self.stage_filter.contains(&t.stage))
            .filter(|t| {
                q.is_empty()
                    || t.name.to_lowercase().contains(&q)
                    || t.project.to_lowercase().contains(&q)
            })
            .collect()
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

    fn selected_task(&self, cx: &App) -> Option<Task> {
        self.visible(cx).get(self.selected).map(|t| (*t).clone())
    }

    fn open_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_task(cx).is_none() {
            return;
        }
        self.page = true;
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
        if let Some(composer) = &self.composer {
            window.focus(&composer.focus_handle(cx), cx);
        }
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
                self.reset_ai();
                window.focus(&self.focus, cx);
            }
            InputEvent::Changed => self.ai_error = None,
        }
        cx.notify();
    }

    fn reset_ai(&mut self) {
        self.proposed.clear();
        self.composer = None;
        self.drafting = false;
        self.ai_error = None;
    }

    fn draft(&mut self, cx: &mut Context<Self>) {
        let (Some(task), Some(composer)) = (self.selected_task(cx), &self.composer) else {
            return;
        };
        let log = composer.read(cx).text().trim().to_string();
        if log.is_empty() || self.drafting {
            return;
        }
        self.drafting = true;
        self.ai_error = None;
        let today = self.today;
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move { ai::work_items(&task.name, &log, today) })
                .await;
            this.update(cx, |this, cx| {
                this.drafting = false;
                match r {
                    Ok(items) => {
                        this.proposed = items
                            .into_iter()
                            .map(|item| Proposal { item, keep: true })
                            .collect()
                    }
                    Err(e) => this.ai_error = Some(format!("AI: {e}").into()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn ensure_subtasks(&mut self, parent: i64, cx: &mut Context<Self>) {
        let Some(o) = self.odoo.clone() else { return };
        if self.subtasks.contains_key(&parent) {
            return;
        }
        self.subtasks.insert(parent, None);
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move { o.subtasks(parent) })
                .await;
            this.update(cx, |this, cx| {
                this.subtasks
                    .insert(parent, Some(r.map_err(|e| e.to_string().into())));
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn commit_work(&mut self, cx: &mut Context<Self>) {
        let (Some(o), Some(task)) = (self.odoo.clone(), self.selected_task(cx)) else {
            return;
        };
        let items: Vec<WorkItem> = self
            .proposed
            .iter()
            .filter(|p| p.keep)
            .map(|p| p.item.clone())
            .collect();
        if items.is_empty() || self.drafting {
            return;
        }
        self.drafting = true;
        cx.spawn(async move |this, cx| {
            let parent = task.id;
            let r = cx
                .background_executor()
                .spawn(async move { o.add_work(&task, &items).map(|_| items) })
                .await;
            this.update(cx, |this, cx| {
                this.drafting = false;
                match r {
                    Ok(items) => {
                        let logged: f64 = items.iter().map(|i| i.hours).sum();
                        this.reset_ai();
                        this.subtasks.remove(&parent);
                        this.reload(cx);
                        this.status = format!(
                            "Added {} sub-task{} · logged {}",
                            items.len(),
                            if items.len() == 1 { "" } else { "s" },
                            hours(logged)
                        )
                        .into();
                    }
                    Err(e) => this.ai_error = Some(e.to_string().into()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
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
        let n = self.visible(cx).len();
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
                self.reset_ai();
            }
            "k" | "up" => {
                self.selected = self.selected.saturating_sub(1);
                self.reset_ai();
            }
            "a" => self.open_composer(window, cx),
            "enter" if !self.proposed.is_empty() => self.commit_work(cx),
            "enter" if n > 0 => self.page = true,
            "escape" if self.people_menu => self.people_menu = false,
            "escape" if self.shortcuts => self.shortcuts = false,
            "escape" if self.stage_menu => self.stage_menu = false,
            "escape" if self.people_header_menu => self.people_header_menu = false,
            "escape" if !self.proposed.is_empty() => self.proposed.clear(),
            "escape" if self.composer.is_some() => self.reset_ai(),
            "escape" => self.page = false,
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
        let people = self.people();
        let projects = self.projects();
        let overdue = visible
            .iter()
            .filter(|t| !t.done && date::due(t.deadline.as_deref(), self.today).overdue)
            .count();
        let mut summary = format!(
            "{} task{}",
            visible.len(),
            if visible.len() == 1 { "" } else { "s" }
        );
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
                    this.page = false;
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
                        this.page = false;
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
                            visible.len(),
                            cx.processor(|this, range: Range<usize>, _, cx| {
                                this.render_rows(range, cx)
                            }),
                        )
                        .track_scroll(&self.rows_scroll)
                        .flex_1(),
                    ),
            );

        let selected = visible.get(self.selected).filter(|_| self.page);
        if let Some(id) = selected.map(|t| t.id) {
            self.ensure_subtasks(id, cx);
        }
        let content = match (&self.settings, selected) {
            (Some(view), _) => view.clone().into_any_element(),
            (None, Some(t)) => self.detail(t, window, cx).into_any_element(),
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
        self.subtasks.clear();
        self.assignee = None;
        self.project = None;
        self.stage_filter.clear();
        self.page = false;
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
        let visible: Vec<Task> = self.visible(cx).into_iter().cloned().collect();
        let people = self.people();
        let tone_of =
            |name: &str| PEOPLE[people.iter().position(|p| p == name).unwrap_or(0) % PEOPLE.len()];
        let range = range.start.min(visible.len())..range.end.min(visible.len());
        visible[range.clone()]
            .iter()
            .zip(range)
            .map(|(t, i)| {
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
                        this.page = true;
                        this.reset_ai();
                        window.focus(&this.focus, cx);
                        cx.notify()
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(160.))
                            .overflow_hidden()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .font_weight(FontWeight::MEDIUM)
                                    .when(t.done, |d| d.text_color(rgb(MUTED)))
                                    .child(t.name.clone()),
                            )
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

    fn detail(&self, t: &Task, window: &Window, cx: &mut Context<Self>) -> Stateful<Div> {
        let panel = div()
            .flex()
            .flex_col()
            .w_full()
            .max_w(px(760.))
            .pt(px(40.))
            .pb(px(64.))
            .gap(px(18.));
        let mut due = date::due(t.deadline.as_deref(), self.today);
        due.overdue &= !t.done;
        let prop = |icon_name: &str, label: &str| {
            div()
                .flex_none()
                .w(px(120.))
                .flex()
                .items_center()
                .gap(px(8.))
                .text_color(rgb(MUTED))
                .child(icon(icon_name, 15., MUTED))
                .child(label.to_string())
        };
        let row = |l: Div, v: Div| div().flex().items_center().min_h(px(26.)).child(l).child(v);
        let kept = self.proposed.iter().filter(|p| p.keep).count();
        let loaded = self.subtasks.get(&t.id).cloned().flatten();
        let failed = loaded.clone().and_then(|r| r.err());
        let existing = loaded.as_ref().and_then(|r| r.as_ref().ok());
        let count = existing
            .filter(|s| !s.is_empty())
            .map(|s| (s.iter().filter(|x| x.done).count(), s.len()));

        let inner =
            panel
                .child(
                    div()
                        .flex()
                        .gap(px(6.))
                        .text_size(px(12.))
                        .text_color(rgb(MUTED))
                        .child(
                            div()
                                .id("back")
                                .cursor_pointer()
                                .hover(|s| s.text_color(rgb(TEXT)))
                                .child(self.project.clone().unwrap_or("My Tasks".into()))
                                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.page = false;
                                    this.reset_ai();
                                    cx.notify()
                                })),
                        )
                        .child(format!("/ {} / #{}", t.project, t.id)),
                )
                .child(
                    div()
                        .text_size(px(32.))
                        .font_weight(FontWeight::BOLD)
                        .line_height(px(40.))
                        .child(t.name.clone()),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(10.))
                        .child(row(
                            prop("status", "Status"),
                            div()
                                .flex()
                                .child(tag(t.stage.clone(), stage_tone(&t.stage), 12.)),
                        ))
                        .child(row(
                            prop("user", "Assignees"),
                            div().child(t.assignees.join(", ")),
                        ))
                        .child(row(
                            prop("calendar", "Due"),
                            div()
                                .when(due.overdue, |d| d.text_color(rgb(DANGER)))
                                .child(due.label),
                        ))
                        .child(row(
                            prop("clock", "Logged"),
                            div().child(hours(t.logged_hours)),
                        )),
                )
                .child(div().h(px(1.)).bg(rgb(BORDER)))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .flex()
                                .items_baseline()
                                .gap(px(8.))
                                .child(div().font_weight(FontWeight::SEMIBOLD).child("Sub-tasks"))
                                .when_some(count, |d, (done, total)| {
                                    d.child(
                                        div()
                                            .text_size(px(12.))
                                            .text_color(rgb(MUTED))
                                            .child(format!("{done}/{total} done")),
                                    )
                                }),
                        )
                        .child(
                            div()
                                .id("autofill")
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .px(px(10.))
                                .py(px(6.))
                                .min_h(px(32.))
                                .rounded(px(6.))
                                .border_1()
                                .border_color(rgb(AI_BORDER))
                                .bg(rgb(PURPLE.bg))
                                .text_color(rgb(PURPLE.fg))
                                .text_size(px(13.))
                                .font_weight(FontWeight::MEDIUM)
                                .cursor_pointer()
                                .child(icon("sparkle", 14., PURPLE.fg))
                                .child("Autofill with AI")
                                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                    this.open_composer(window, cx)
                                })),
                        ),
                )
                .when_some(failed, |d, e| {
                    d.child(
                        div()
                            .text_size(px(13.))
                            .text_color(rgb(DANGER))
                            .child(format!("Couldn’t load sub-tasks: {e}")),
                    )
                })
                .when(loaded.is_none(), |d| {
                    d.child(
                        div()
                            .text_size(px(13.))
                            .text_color(rgb(MUTED))
                            .child("Loading sub-tasks…"),
                    )
                })
                .when_some(existing.filter(|s| !s.is_empty()), |d, subs| {
                    d.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .children(subs.iter().map(|s| {
                                let check = div()
                                    .flex_none()
                                    .mt(px(2.))
                                    .size(px(15.))
                                    .rounded(px(3.))
                                    .flex()
                                    .items_center()
                                    .justify_center();
                                let check = if s.done {
                                    check.bg(rgb(SUCCESS)).child(icon("check", 11., PAGE))
                                } else {
                                    check.border_1().border_color(rgb(MUTED))
                                };
                                div().flex().items_start().gap(px(10.)).child(check).child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .text_color(rgb(if s.done { MUTED } else { TEXT }))
                                        .child(s.name.clone()),
                                )
                            })),
                    )
                })
                .when(
                    self.composer.is_none() && existing.is_some_and(|s| s.is_empty()),
                    |d| {
                        let text =
                            "No sub-tasks yet. Press a, describe what you did, and AI fills them in with timesheets.";
                        let key = text.find(" a,").map_or(0..0, |i| i + 1..i + 2);
                        d.child(div().text_size(px(13.)).text_color(rgb(MUTED)).child(
                            StyledText::new(text).with_highlights([(
                                key,
                                HighlightStyle {
                                    color: Some(rgb(TEXT).into()),
                                    font_weight: Some(FontWeight::SEMIBOLD),
                                    ..Default::default()
                                },
                            )]),
                        ))
                    },
                )
                .when(self.composer.is_some() && self.proposed.is_empty(), |d| {
                    d.child(self.composer_card(window, cx))
                })
                .when(!self.proposed.is_empty(), |d| {
                    d.child(self.proposal(kept, cx))
                });
        div()
            .id("detail")
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .items_center()
            .px(px(48.))
            .child(inner)
    }

    fn ai_button(&self, id: &'static str, label: String, primary: bool) -> Stateful<Div> {
        div()
            .id(id)
            .flex()
            .items_center()
            .px(px(14.))
            .py(px(7.))
            .min_h(px(34.))
            .rounded(px(6.))
            .cursor_pointer()
            .text_size(px(13.))
            .when(primary, |d| {
                d.bg(rgb(ACCENT))
                    .text_color(white())
                    .font_weight(FontWeight::MEDIUM)
            })
            .when(!primary, |d| {
                d.border_1()
                    .border_color(rgb(BORDER))
                    .bg(rgb(PAGE))
                    .text_color(rgb(TEXT_SOFT))
            })
            .when(self.drafting, |d| d.opacity(0.6))
            .child(label)
    }

    fn ai_card(&self, title: &'static str) -> Div {
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .p(px(14.))
            .rounded(px(8.))
            .bg(rgb(PURPLE.bg))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(PURPLE.fg))
                    .child(icon("sparkle", 13., PURPLE.fg))
                    .child(title),
            )
            .when_some(self.ai_error.clone(), |d, e| {
                d.child(div().text_size(px(13.)).text_color(rgb(DANGER)).child(e))
            })
    }

    fn composer_card(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let Some(composer) = &self.composer else {
            return div();
        };
        let focused = composer.focus_handle(cx).is_focused(window);
        let submit = if cfg!(target_os = "macos") {
            "⌘"
        } else {
            "Ctrl"
        };
        self.ai_card("Describe what you did")
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
                        self.ai_button(
                            "draft",
                            if self.drafting {
                                "Drafting…"
                            } else {
                                "Draft"
                            }
                            .into(),
                            true,
                        )
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.draft(cx))),
                    ),
            )
    }

    fn proposal(&self, kept: usize, cx: &mut Context<Self>) -> Div {
        let logged: f64 = self
            .proposed
            .iter()
            .filter(|p| p.keep)
            .map(|p| p.item.hours)
            .sum();
        let action = format!(
            "Add {kept} sub-task{}{}",
            if kept == 1 { "" } else { "s" },
            if logged > 0. {
                format!(" · log {}", hours(logged))
            } else {
                String::new()
            }
        );
        self.ai_card("Drafted by AI · nothing saved yet")
            .children(self.proposed.iter().enumerate().map(|(i, p)| {
                let check = div()
                    .flex_none()
                    .mt(px(2.))
                    .size(px(15.))
                    .rounded(px(3.))
                    .flex()
                    .items_center()
                    .justify_center();
                let check = if p.keep {
                    check.bg(rgb(PURPLE.fg)).child(icon("check", 11., PAGE))
                } else {
                    check.bg(rgb(PAGE)).border_1().border_color(rgb(MUTED))
                };
                let when = date::due(Some(&p.item.date), self.today).label;
                div()
                    .id(("proposal", i))
                    .flex()
                    .items_start()
                    .gap(px(10.))
                    .cursor_pointer()
                    .child(check)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_color(rgb(if p.keep { TEXT } else { MUTED }))
                            .child(p.item.name.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .flex()
                            .gap(px(10.))
                            .text_size(px(12.))
                            .text_color(rgb(MUTED))
                            .child(hours(p.item.hours))
                            .child(div().w(px(76.)).flex().justify_end().child(when)),
                    )
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.proposed[i].keep = !this.proposed[i].keep;
                        cx.notify()
                    }))
            }))
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .mt(px(4.))
                    .child(
                        self.ai_button("create", action, true)
                            .when(kept == 0, |d| d.opacity(0.5))
                            .on_click(
                                cx.listener(|this, _: &ClickEvent, _, cx| this.commit_work(cx)),
                            ),
                    )
                    .child(
                        self.ai_button("discard", "Edit description".into(), false)
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.proposed.clear();
                                if let Some(c) = &this.composer {
                                    window.focus(&c.focus_handle(cx), cx);
                                }
                                cx.notify()
                            })),
                    ),
            )
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
