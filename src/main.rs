mod ai;
mod assets;
mod connect;
mod date;
mod input;
mod odoo;
mod theme;
mod ui;

use connect::{ConnectView, Connected};
use gpui::{
    App, Bounds, BoxShadow, ClickEvent, Context, Div, Entity, FocusHandle, Focusable, FontWeight,
    HighlightStyle, KeyDownEvent, ScrollHandle, SharedString, Stateful, StyledText,
    TitlebarOptions, Window, WindowBounds, WindowOptions, black, div, point, prelude::*, px, rgb,
    size, white,
};
use input::{InputEvent, TextInput};
use odoo::{Login, Odoo, SubTask, Task};
use serde_json::json;
use std::{collections::HashMap, sync::Arc};
use theme::*;
use ui::{icon, tag};

struct Proposal {
    name: String,
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
    today: i64,
    connect: Option<Entity<ConnectView>>,
    rows_scroll: ScrollHandle,
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
            today: date::today(),
            connect: None,
            rows_scroll: ScrollHandle::new(),
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
                    anyhow::Ok((version, o.tasks(json!([["stage_id.fold", "=", false]]))?))
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
                self.rows_scroll.scroll_to_item(0);
                self.proposed.clear();
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
            .filter(|t| {
                q.is_empty()
                    || t.name.to_lowercase().contains(&q)
                    || t.project.to_lowercase().contains(&q)
            })
            .collect()
    }

    fn people(&self) -> Vec<String> {
        let mut p: Vec<String> = self
            .tasks
            .iter()
            .flat_map(|t| t.assignees.clone())
            .collect();
        p.sort_by_key(|n| (n != "Me", n.clone()));
        p.dedup();
        p
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

    fn suggest_subtasks(&mut self, cx: &mut Context<Self>) {
        let Some(t) = self.selected_task(cx).map(|t| t.name) else {
            return;
        };
        self.status = "Drafting sub-tasks…".into();
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move { ai::subtasks(&t) })
                .await;
            this.update(cx, |this, cx| {
                match r {
                    Ok(s) => {
                        this.status = "Synced just now".into();
                        this.proposed = s
                            .into_iter()
                            .map(|name| Proposal { name, keep: true })
                            .collect();
                    }
                    Err(e) => this.status = format!("AI: {e}").into(),
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

    fn commit_subtasks(&mut self, cx: &mut Context<Self>) {
        let (Some(o), Some(t)) = (self.odoo.clone(), self.selected_task(cx).map(|t| t.id)) else {
            return;
        };
        let names: Vec<String> = std::mem::take(&mut self.proposed)
            .into_iter()
            .filter(|p| p.keep)
            .map(|p| p.name)
            .collect();
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move { o.create_subtasks(t, &names) })
                .await;
            this.update(cx, |this, cx| match r {
                Ok(()) => this.reload(cx),
                Err(e) => {
                    this.status = e.to_string().into();
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn set_assignee(&mut self, who: Option<String>) {
        self.assignee = who;
        self.selected = 0;
        self.rows_scroll.scroll_to_item(0);
        self.proposed.clear();
    }

    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.filter.focus_handle(cx).is_focused(window) {
            return;
        }
        let n = self.visible(cx).len();
        let secondary = ev.keystroke.modifiers.secondary(cx);
        match ev.keystroke.key.as_str() {
            "k" if secondary => window.focus(&self.filter.focus_handle(cx), cx),
            "/" => window.focus(&self.filter.focus_handle(cx), cx),
            "j" | "down" if self.selected + 1 < n => {
                self.selected += 1;
                self.proposed.clear();
            }
            "k" | "up" => {
                self.selected = self.selected.saturating_sub(1);
                self.proposed.clear();
            }
            "s" => self.suggest_subtasks(cx),
            "enter" if !self.proposed.is_empty() => self.commit_subtasks(cx),
            "escape" => self.proposed.clear(),
            "0" => self.set_assignee(None),
            d if d.len() == 1 && d.as_bytes()[0].is_ascii_digit() => {
                let p = self.people().get(d.parse::<usize>().unwrap() - 1).cloned();
                if p.is_some() {
                    self.set_assignee(p)
                }
            }
            _ => return,
        }
        self.rows_scroll.scroll_to_item(self.selected);
        cx.notify();
    }
}

fn stage_tone(stage: &str) -> Tone {
    let s = stage.to_lowercase();
    if s.contains("progress") {
        BLUE
    } else if s.contains("review") || s.contains("test") {
        PURPLE
    } else if s.contains("done") {
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

const COLS: [f32; 4] = [120., 92., 130., 96.];

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
        let tone_of =
            |name: &str| PEOPLE[people.iter().position(|p| p == name).unwrap_or(0) % PEOPLE.len()];
        let projects = self.projects();
        let overdue = self
            .tasks
            .iter()
            .filter(|t| date::due(t.deadline.as_deref(), self.today).overdue)
            .count();
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
                        this.selected = 0;
                        this.rows_scroll.scroll_to_item(0);
                        this.proposed.clear();
                        cx.notify()
                    })),
            );
        }
        let synced = self.odoo.is_some() && self.status.as_ref() == "Synced just now";
        sidebar = sidebar.child(project_list).child(
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
                .child(self.status.clone()),
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
        for (i, p) in people.iter().enumerate() {
            let who = p.clone();
            chips = chips.child(
                chip(
                    format!("chip-{i}").into(),
                    p.clone(),
                    self.assignee.as_ref() == Some(p),
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.set_assignee(Some(who.clone()));
                    cx.notify()
                })),
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
            .children(
                ["Status", "Assignees", "Project", "Due"]
                    .iter()
                    .zip(COLS)
                    .map(|(l, w)| div().flex_none().w(px(w)).child(*l)),
            );

        let rows = visible.iter().enumerate().map(|(i, t)| {
            let on = i == self.selected;
            let due = date::due(t.deadline.as_deref(), self.today);
            div()
                .id(("row", i))
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
                    this.proposed.clear();
                    window.focus(&this.focus, cx);
                    cx.notify()
                }))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(160.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .font_weight(FontWeight::MEDIUM)
                                .child(t.name.clone()),
                        )
                        .when(t.priority_high, |d| {
                            d.child(tag("High", ORANGE, 11.).px(px(6.)).py(px(1.)))
                        }),
                )
                .child(div().flex_none().flex().w(px(COLS[0])).child(tag(
                    t.stage.clone(),
                    stage_tone(&t.stage),
                    12.,
                )))
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
        });

        let key = |k: String, label: &str| {
            div()
                .flex()
                .gap(px(4.))
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(TEXT))
                        .child(k),
                )
                .child(label.to_string())
        };
        let main = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .h_full()
            .pt(px(40.))
            .px(px(48.))
            .pb(px(20.))
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
                    .child(div().text_color(rgb(MUTED)).child(format!(
                        "Open tasks across {} projects · {} overdue",
                        projects.len(),
                        overdue
                    ))),
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
                        div()
                            .id("rows")
                            .track_scroll(&self.rows_scroll)
                            .flex()
                            .flex_col()
                            .flex_1()
                            .overflow_y_scroll()
                            .children(rows),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(18.))
                    .text_size(px(12.))
                    .text_color(rgb(MUTED))
                    .child(key("j k".into(), "move"))
                    .child(key(format!("1–{}", people.len().clamp(1, 9)), "assignee"))
                    .child(key("s".into(), "AI sub-tasks")),
            );

        if let Some(id) = visible.get(self.selected).map(|t| t.id) {
            self.ensure_subtasks(id, cx);
        }
        let detail = self.detail(visible.get(self.selected), cx);

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
            .child(main)
            .child(detail)
            .into_any_element()
    }
}

impl BetterOdoo {
    fn detail(&self, task: Option<&Task>, cx: &mut Context<Self>) -> Stateful<Div> {
        let panel = div()
            .id("detail")
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(380.))
            .h_full()
            .py(px(40.))
            .px(px(32.))
            .gap(px(18.))
            .border_l_1()
            .border_color(rgb(BORDER))
            .shadow(vec![BoxShadow {
                color: black().opacity(0.03),
                offset: point(px(-8.), px(0.)),
                blur_radius: px(24.),
                spread_radius: px(0.),
                inset: false,
            }]);
        let Some(t) = task else {
            return panel.child(div().text_color(rgb(MUTED)).child("No task selected"));
        };
        let due = date::due(t.deadline.as_deref(), self.today);
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

        panel
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(MUTED))
                    .child(format!("{} / #{}", t.project, t.id)),
            )
            .child(
                div()
                    .text_size(px(26.))
                    .font_weight(FontWeight::BOLD)
                    .line_height(px(32.))
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
                            .id("suggest")
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
                            .child("Suggest with AI")
                            .on_click(
                                cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.suggest_subtasks(cx)
                                }),
                            ),
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
                self.proposed.is_empty() && existing.is_some_and(|s| s.is_empty()),
                |d| {
                    let text =
                        "No sub-tasks yet. Press s and AI will draft a few for you to review.";
                    let key = text.find(" s ").map_or(0..0, |i| i + 1..i + 2);
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
            .when(!self.proposed.is_empty(), |d| {
                d.child(self.proposal(kept, cx))
            })
    }

    fn proposal(&self, kept: usize, cx: &mut Context<Self>) -> Div {
        let button = |id: &'static str| {
            div()
                .id(id)
                .px(px(14.))
                .py(px(7.))
                .min_h(px(34.))
                .rounded(px(6.))
                .cursor_pointer()
        };
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .p(px(14.))
            .rounded(px(8.))
            .bg(rgb(PURPLE.bg))
            .child(
                div()
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(PURPLE.fg))
                    .child("Drafted by AI · nothing saved yet"),
            )
            .children(self.proposed.iter().enumerate().map(|(i, p)| {
                let check = div()
                    .flex_none()
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
                div()
                    .id(("proposal", i))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .cursor_pointer()
                    .child(check)
                    .child(div().flex_1().min_w_0().child(p.name.clone()))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.proposed[i].keep = !this.proposed[i].keep;
                        cx.notify()
                    }))
            }))
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .mt(px(6.))
                    .text_size(px(13.))
                    .child(
                        button("create")
                            .bg(rgb(ACCENT))
                            .text_color(white())
                            .font_weight(FontWeight::MEDIUM)
                            .when(kept == 0, |d| d.opacity(0.5))
                            .child(format!(
                                "Create {kept} sub-task{}",
                                if kept == 1 { "" } else { "s" }
                            ))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                if kept > 0 {
                                    this.commit_subtasks(cx)
                                }
                            })),
                    )
                    .child(
                        button("discard")
                            .border_1()
                            .border_color(rgb(BORDER))
                            .bg(rgb(PAGE))
                            .text_color(rgb(TEXT_SOFT))
                            .child("Discard")
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.proposed.clear();
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
