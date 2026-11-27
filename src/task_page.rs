use crate::components::{icon, kbd, tag};
use crate::input::{InputEvent, TextInput};
use crate::odoo::{Stage, Task, Timesheet, WorkItem};
use crate::theme::*;
use crate::{BetterOdoo, ai, date, hours, stage_tone};
use gpui::{
    ClickEvent, Context, Div, Entity, Focusable, FontWeight, SharedString, Stateful, Window, div,
    prelude::*, px, rgb, white,
};

#[derive(Clone, Copy, PartialEq)]
pub enum Tab {
    Description,
    Timesheets,
    SubTasks,
}

pub struct Draft {
    done: bool,
    name: Entity<TextInput>,
    hours: Entity<TextInput>,
    date: Entity<TextInput>,
}

#[derive(Clone)]
pub struct Detail {
    description: String,
    timesheets: Vec<Timesheet>,
}

#[derive(Clone)]
struct DragSub {
    id: i64,
    name: SharedString,
}

struct DragPreview(SharedString);

impl Render for DragPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(10.))
            .py(px(6.))
            .rounded(px(6.))
            .bg(rgb(PAGE))
            .border_1()
            .border_color(rgb(BORDER))
            .shadow_md()
            .text_size(px(13.))
            .text_color(rgb(TEXT))
            .child(self.0.clone())
    }
}

fn closed(state: &str) -> bool {
    matches!(state, "1_done" | "1_canceled")
}

fn parse_hours(text: &str) -> Option<f64> {
    let t = text.trim().to_lowercase();
    if t.is_empty() {
        return Some(0.);
    }
    let value = if let Some((h, m)) = t.split_once(':') {
        h.trim().parse::<f64>().ok()? + m.trim().parse::<f64>().ok()? / 60.
    } else if t.contains('h') || t.contains('m') {
        let (h, rest) = t.split_once('h').unwrap_or(("", &t));
        let h = if h.is_empty() {
            0.
        } else {
            h.trim().parse::<f64>().ok()?
        };
        let rest = rest.trim().trim_end_matches('m').trim();
        let m = if rest.is_empty() {
            0.
        } else {
            rest.parse::<f64>().ok()?
        };
        h + m / 60.
    } else {
        t.parse::<f64>().ok()?
    };
    (0. ..=24.).contains(&value).then_some(value)
}

fn hours_text(h: f64) -> String {
    if h <= 0. {
        return String::new();
    }
    let s = format!("{h:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn field(input: &Entity<TextInput>) -> Div {
    div()
        .flex()
        .items_center()
        .px(px(8.))
        .min_h(px(30.))
        .rounded(px(6.))
        .border_1()
        .border_color(rgb(BORDER))
        .bg(rgb(PAGE))
        .child(input.clone())
}

fn check_circle(on: bool) -> Div {
    let c = div()
        .flex_none()
        .size(px(16.))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center();
    if on {
        c.bg(rgb(SUCCESS)).child(icon("check", 11., PAGE))
    } else {
        c.border_1().border_color(rgb(MUTED))
    }
}

impl BetterOdoo {
    pub fn children_of(&self, id: i64) -> Vec<Task> {
        let mut subs: Vec<Task> = self
            .tasks
            .iter()
            .filter(|t| t.parent.as_ref().is_some_and(|p| p.0 == id))
            .cloned()
            .collect();
        subs.sort_by_key(|t| (t.sequence, t.id));
        subs
    }

    pub fn open_task(&mut self, id: i64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.tasks.iter().find(|t| t.id == id).map(|t| t.project_id) else {
            return;
        };
        self.tab = if self.children_of(id).is_empty() {
            Tab::Description
        } else {
            Tab::SubTasks
        };
        self.viewing = Some(id);
        self.notice = None;
        self.reset_ai();
        window.focus(&self.focus, cx);
        self.ensure_detail(id, cx);
        self.ensure_stages(project, cx);
        cx.notify();
    }

    pub fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let parent = self
            .viewing
            .and_then(|id| self.tasks.iter().find(|t| t.id == id))
            .and_then(|t| t.parent.as_ref().map(|p| p.0))
            .filter(|p| self.tasks.iter().any(|t| t.id == *p));
        match parent {
            Some(p) => self.open_task(p, window, cx),
            None => self.close_task(window, cx),
        }
    }

    pub fn close_task(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.viewing = None;
        self.reset_ai();
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub fn selected_task(&self, cx: &gpui::App) -> Option<Task> {
        let id = match self.viewing {
            Some(id) => id,
            None => self.tasks[self.rows(cx).get(self.selected)?.idx].id,
        };
        self.tasks.iter().find(|t| t.id == id).cloned()
    }

    fn ensure_detail(&mut self, id: i64, cx: &mut Context<Self>) {
        let Some(o) = self.odoo.clone() else { return };
        if self.details.contains_key(&id) {
            return;
        }
        self.details.insert(id, None);
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move {
                    anyhow::Ok(Detail {
                        description: o.description(id)?,
                        timesheets: o.timesheets(id).unwrap_or_default(),
                    })
                })
                .await;
            this.update(cx, |this, cx| {
                this.details
                    .insert(id, Some(r.map_err(|e| e.to_string().into())));
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn refresh_detail(&mut self, id: i64, cx: &mut Context<Self>) {
        self.details.remove(&id);
        self.ensure_detail(id, cx);
    }

    fn ensure_stages(&mut self, project: Option<i64>, cx: &mut Context<Self>) {
        let (Some(o), Some(project)) = (self.odoo.clone(), project) else {
            return;
        };
        if self.stages.contains_key(&project) {
            return;
        }
        self.stages.insert(project, vec![]);
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move { o.stages(project) })
                .await;
            this.update(cx, |this, cx| {
                if let Ok(s) = r {
                    this.stages.insert(project, s);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn fail(&mut self, what: &str, e: impl std::fmt::Display, cx: &mut Context<Self>) {
        self.notice = Some(format!("{what}: {e}").into());
        cx.notify();
    }

    fn set_stage(&mut self, id: i64, stage: Stage, cx: &mut Context<Self>) {
        let Some(o) = self.odoo.clone() else { return };
        if let Some(t) = self.tasks.iter_mut().find(|t| t.id == id) {
            t.stage = stage.name.clone();
            t.stage_id = Some(stage.id);
            t.done = stage.fold || closed(&t.state);
        }
        cx.notify();
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move { o.set_stage(id, stage.id) })
                .await;
            this.update(cx, |this, cx| {
                if let Err(e) = r {
                    this.fail("Couldn’t change the status", e, cx);
                    this.reload(cx);
                }
            })
            .ok();
        })
        .detach();
    }

    fn toggle_done(&mut self, id: i64, cx: &mut Context<Self>) {
        let Some(o) = self.odoo.clone() else { return };
        let Some(t) = self.tasks.iter_mut().find(|t| t.id == id) else {
            return;
        };
        let done = !t.done;
        t.done = done;
        t.state = if done { "1_done" } else { "01_in_progress" }.into();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move { o.set_done(id, done) })
                .await;
            this.update(cx, |this, cx| {
                if let Err(e) = r {
                    this.fail("Couldn’t update the sub-task", e, cx);
                    this.reload(cx);
                }
            })
            .ok();
        })
        .detach();
    }

    fn reorder(&mut self, parent: i64, dragged: i64, onto: i64, cx: &mut Context<Self>) {
        let Some(o) = self.odoo.clone() else { return };
        let mut ids: Vec<i64> = self.children_of(parent).iter().map(|t| t.id).collect();
        let (Some(from), Some(to)) = (
            ids.iter().position(|&i| i == dragged),
            ids.iter().position(|&i| i == onto),
        ) else {
            return;
        };
        if from == to {
            return;
        }
        ids.remove(from);
        let at = ids.iter().position(|&i| i == onto).unwrap_or(0);
        ids.insert(if from < to { at + 1 } else { at }, dragged);
        for (i, id) in ids.iter().enumerate() {
            if let Some(t) = self.tasks.iter_mut().find(|t| t.id == *id) {
                t.sequence = (i as i64 + 1) * 10;
            }
        }
        cx.notify();
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move { o.resequence(&ids) })
                .await;
            this.update(cx, |this, cx| {
                if let Err(e) = r {
                    this.fail("Couldn’t save the new order", e, cx);
                    this.reload(cx);
                }
            })
            .ok();
        })
        .detach();
    }

    pub fn on_entry(
        &mut self,
        input: &Entity<TextInput>,
        ev: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match ev {
            InputEvent::Submit if input == &self.quick => self.add_quick(cx),
            InputEvent::Submit => self.add_timesheet_entry(cx),
            InputEvent::Cancel => window.focus(&self.focus, cx),
            InputEvent::Changed => {}
        }
    }

    fn add_quick(&mut self, cx: &mut Context<Self>) {
        let name = self.quick.read(cx).text().trim().to_string();
        let (Some(o), Some(task)) = (self.odoo.clone(), self.selected_task(cx)) else {
            return;
        };
        if name.is_empty() {
            return;
        }
        let item = WorkItem {
            name,
            hours: 0.,
            date: date::iso(self.today),
            done: false,
        };
        self.quick.update(cx, |q, cx| q.set_text("", cx));
        let parent = task.id;
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move { o.add_work(&task, &[item]) })
                .await;
            this.update(cx, |this, cx| {
                match r {
                    Ok(()) => {
                        this.reload(cx);
                        this.refresh_detail(parent, cx);
                    }
                    Err(e) => this.fail("Couldn’t add the sub-task", e, cx),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn add_timesheet_entry(&mut self, cx: &mut Context<Self>) {
        let (Some(o), Some(task)) = (self.odoo.clone(), self.selected_task(cx)) else {
            return;
        };
        let name = self.ts_name.read(cx).text().trim().to_string();
        let Some(h) = parse_hours(self.ts_hours.read(cx).text()).filter(|h| *h > 0.) else {
            return self.fail("Timesheet", "enter the hours, like 1.5 or 1h 30m", cx);
        };
        let day = self.ts_date.read(cx).text().trim().to_string();
        let day = if day.is_empty() {
            date::iso(self.today)
        } else {
            day
        };
        if !date::is_valid(&day) {
            return self.fail("Timesheet", "write the date as YYYY-MM-DD", cx);
        }
        if name.is_empty() {
            return self.fail("Timesheet", "describe the work", cx);
        }
        let item = WorkItem {
            name,
            hours: h,
            date: day,
            done: false,
        };
        self.notice = None;
        self.ts_name.update(cx, |i, cx| i.set_text("", cx));
        self.ts_hours.update(cx, |i, cx| i.set_text("", cx));
        let parent = task.id;
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move { o.add_timesheet(&task, &item) })
                .await;
            this.update(cx, |this, cx| {
                match r {
                    Ok(()) => {
                        this.reload(cx);
                        this.refresh_detail(parent, cx);
                    }
                    Err(e) => this.fail("Couldn’t log the time", e, cx),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn make_draft(&self, item: &WorkItem, cx: &mut Context<Self>) -> Draft {
        let text = |placeholder: &'static str, value: String, cx: &mut Context<Self>| {
            cx.new(|cx| {
                let mut input = TextInput::new(placeholder, cx);
                input.set_text(&value, cx);
                input
            })
        };
        Draft {
            done: item.done,
            name: text("What did you do?", item.name.clone(), cx),
            hours: text("Hours", hours_text(item.hours), cx),
            date: text("YYYY-MM-DD", item.date.clone(), cx),
        }
    }

    fn blank_item(&self) -> WorkItem {
        WorkItem {
            name: String::new(),
            hours: 0.,
            date: date::iso(self.today),
            done: true,
        }
    }

    pub fn open_manual(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.viewing.is_none() {
            let Some(t) = self.selected_task(cx) else {
                return;
            };
            self.open_task(t.id, window, cx);
        }
        self.tab = Tab::SubTasks;
        self.composer = None;
        self.draft_ai = false;
        let blank = self.blank_item();
        let d = self.make_draft(&blank, cx);
        window.focus(&d.name.focus_handle(cx), cx);
        self.proposed = vec![d];
        cx.notify();
    }

    pub fn open_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.viewing.is_none() {
            let Some(t) = self.selected_task(cx) else {
                return;
            };
            self.open_task(t.id, window, cx);
        }
        self.tab = Tab::SubTasks;
        self.proposed.clear();
        self.draft_ai = true;
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

    pub fn reset_ai(&mut self) {
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
                        this.proposed = items.iter().map(|i| this.make_draft(i, cx)).collect()
                    }
                    Err(e) => this.ai_error = Some(format!("AI: {e}").into()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn read_drafts(&self, cx: &gpui::App) -> Result<Vec<WorkItem>, String> {
        let mut items = Vec::new();
        for (n, d) in self.proposed.iter().enumerate() {
            let name = d.name.read(cx).text().trim().to_string();
            let row = n + 1;
            if name.is_empty() {
                return Err(format!("Row {row} needs a description"));
            }
            let hours = parse_hours(d.hours.read(cx).text())
                .ok_or(format!("Row {row}: write hours like 1.5 or 1h 30m"))?;
            let day = d.date.read(cx).text().trim().to_string();
            let day = if day.is_empty() {
                date::iso(self.today)
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

    pub fn commit_work(&mut self, cx: &mut Context<Self>) {
        let (Some(o), Some(task)) = (self.odoo.clone(), self.selected_task(cx)) else {
            return;
        };
        if self.proposed.is_empty() || self.drafting {
            return;
        }
        let items = match self.read_drafts(cx) {
            Ok(i) => i,
            Err(e) => {
                self.ai_error = Some(e.into());
                cx.notify();
                return;
            }
        };
        self.ai_error = None;
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
                        this.reload(cx);
                        this.refresh_detail(parent, cx);
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

    pub fn detail(&self, t: &Task, window: &Window, cx: &mut Context<Self>) -> Stateful<Div> {
        let id = t.id;
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
        let parent = t
            .parent
            .clone()
            .filter(|p| self.tasks.iter().any(|x| x.id == p.0));
        let subs = self.children_of(id);
        let done_count = subs.iter().filter(|s| s.done).count();

        let crumb = div()
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
                    .on_click(
                        cx.listener(|this, _: &ClickEvent, window, cx| this.close_task(window, cx)),
                    ),
            )
            .when_some(parent, |d, (pid, pname)| {
                d.child("/").child(
                    div()
                        .id("parent")
                        .cursor_pointer()
                        .hover(|s| s.text_color(rgb(TEXT)))
                        .child(pname)
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            this.open_task(pid, window, cx)
                        })),
                )
            })
            .child(format!("/ #{id}"));

        let stages = t
            .project_id
            .and_then(|p| self.stages.get(&p))
            .filter(|s| !s.is_empty());
        let status = match stages {
            Some(stages) => div()
                .flex()
                .flex_wrap()
                .gap(px(2.))
                .p(px(3.))
                .rounded(px(8.))
                .bg(rgb(SIDEBAR))
                .border_1()
                .border_color(rgb(BORDER))
                .children(stages.iter().enumerate().map(|(i, s)| {
                    let on = t.stage_id == Some(s.id);
                    let stage = s.clone();
                    div()
                        .id(("stage", i))
                        .px(px(12.))
                        .py(px(5.))
                        .rounded(px(6.))
                        .cursor_pointer()
                        .text_size(px(13.))
                        .when(on, |d| {
                            d.bg(rgb(ACCENT))
                                .text_color(white())
                                .font_weight(FontWeight::MEDIUM)
                        })
                        .when(!on, |d| {
                            d.text_color(rgb(TEXT_SOFT)).hover(|s| s.bg(rgb(HOVER)))
                        })
                        .child(s.name.clone())
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.set_stage(id, stage.clone(), cx)
                        }))
                })),
            None => div()
                .flex()
                .child(tag(t.stage.clone(), stage_tone(&t.stage), 12.)),
        };

        let tab = |which: Tab, label: String| {
            let on = self.tab == which;
            div()
                .id(SharedString::from(label.clone()))
                .px(px(2.))
                .pb(px(8.))
                .cursor_pointer()
                .border_b_2()
                .border_color(rgb(if on { TEXT } else { PAGE }))
                .text_color(rgb(if on { TEXT } else { MUTED }))
                .font_weight(if on {
                    FontWeight::SEMIBOLD
                } else {
                    FontWeight::NORMAL
                })
                .child(label)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.tab = which;
                    cx.notify()
                }))
        };
        let tabs = div()
            .flex()
            .gap(px(22.))
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(tab(Tab::Description, "Description".into()))
            .child(tab(Tab::Timesheets, "Timesheets".into()))
            .child(tab(
                Tab::SubTasks,
                if subs.is_empty() {
                    "Sub-tasks".into()
                } else {
                    format!("Sub-tasks {done_count}/{}", subs.len())
                },
            ));

        let body = match self.tab {
            Tab::Description => self.description_tab(id),
            Tab::Timesheets => self.timesheets_tab(id, cx),
            Tab::SubTasks => self.subtasks_tab(id, &subs, window, cx),
        };

        let inner = div()
            .flex()
            .flex_col()
            .w_full()
            .max_w(px(760.))
            .pt(px(40.))
            .pb(px(64.))
            .gap(px(18.))
            .child(crumb)
            .child(
                div()
                    .text_size(px(32.))
                    .font_weight(FontWeight::BOLD)
                    .line_height(px(40.))
                    .child(t.name.clone()),
            )
            .child(status)
            .when_some(self.notice.clone(), |d, n| {
                d.child(div().text_size(px(13.)).text_color(rgb(DANGER)).child(n))
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(row(
                        prop("status", "Project"),
                        div().child(t.project.clone()),
                    ))
                    .child(row(
                        prop("user", "Assignees"),
                        div().child(if t.assignees.is_empty() {
                            "Unassigned".to_string()
                        } else {
                            t.assignees.join(", ")
                        }),
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
            .child(tabs)
            .child(body);
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

    fn description_tab(&self, id: i64) -> Div {
        let text = match self.details.get(&id) {
            None | Some(None) => "Loading…".to_string(),
            Some(Some(Err(e))) => format!("Couldn’t load: {e}"),
            Some(Some(Ok(d))) if d.description.is_empty() => "No description.".to_string(),
            Some(Some(Ok(d))) => d.description.clone(),
        };
        div()
            .line_height(px(22.))
            .text_color(rgb(TEXT_SOFT))
            .child(text)
    }

    fn timesheets_tab(&self, id: i64, cx: &mut Context<Self>) -> Div {
        let cell = |w: f32, v: String| div().flex_none().w(px(w)).truncate().child(v);
        let list = match self.details.get(&id) {
            None | Some(None) => div().text_color(rgb(MUTED)).child("Loading…"),
            Some(Some(Err(e))) => div().text_color(rgb(DANGER)).child(e.clone()),
            Some(Some(Ok(d))) => {
                let total: f64 = d.timesheets.iter().map(|t| t.hours).sum();
                div()
                    .flex()
                    .flex_col()
                    .children(d.timesheets.iter().map(|t| {
                        div()
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .py(px(8.))
                            .border_b_1()
                            .border_color(rgb(ROW_RULE))
                            .child(
                                cell(92., date::due(Some(&t.date), self.today).label)
                                    .text_color(rgb(MUTED)),
                            )
                            .child(div().flex_1().min_w_0().truncate().child(t.name.clone()))
                            .child(cell(60., t.user.clone()).text_color(rgb(MUTED)))
                            .child(cell(60., hours(t.hours)).text_color(rgb(TEXT_SOFT)))
                    }))
                    .when(d.timesheets.is_empty(), |d| {
                        d.child(div().text_color(rgb(MUTED)).child("No time logged yet."))
                    })
                    .when(!d.timesheets.is_empty(), |d| {
                        d.child(
                            div()
                                .flex()
                                .justify_end()
                                .pt(px(8.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(format!("Total {}", hours(total))),
                        )
                    })
            }
        };
        div().flex().flex_col().gap(px(14.)).child(list).child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(div().flex_1().child(field(&self.ts_name)))
                .child(div().w(px(80.)).child(field(&self.ts_hours)))
                .child(div().w(px(116.)).child(field(&self.ts_date)))
                .child(
                    self.ai_button("add-time", "Log time".into(), true)
                        .on_click(
                            cx.listener(|this, _: &ClickEvent, _, cx| this.add_timesheet_entry(cx)),
                        ),
                ),
        )
    }

    fn subtasks_tab(
        &self,
        parent: i64,
        subs: &[Task],
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let toolbar =
            div()
                .flex()
                .items_center()
                .justify_end()
                .gap(px(8.))
                .child(
                    div()
                        .id("add-entry")
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .px(px(10.))
                        .py(px(6.))
                        .min_h(px(32.))
                        .rounded(px(6.))
                        .border_1()
                        .border_color(rgb(BORDER))
                        .text_color(rgb(TEXT_SOFT))
                        .text_size(px(13.))
                        .font_weight(FontWeight::MEDIUM)
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(HOVER)))
                        .child(icon("plus", 14., MUTED))
                        .child("Add entries")
                        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                            this.open_manual(window, cx)
                        })),
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
                );

        let list = div()
            .flex()
            .flex_col()
            .children(subs.iter().enumerate().map(|(i, s)| {
                let sid = s.id;
                let name: SharedString = s.name.clone().into();
                let drag_name = name.clone();
                div()
                    .id(("sub", i))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .px(px(6.))
                    .py(px(7.))
                    .rounded(px(6.))
                    .border_t_2()
                    .border_color(rgb(PAGE))
                    .hover(|s| s.bg(rgb(SIDEBAR)))
                    .drag_over::<DragSub>(|d, _, _, _| d.border_color(rgb(ACCENT)))
                    .on_drop(cx.listener(move |this, d: &DragSub, _, cx| {
                        this.reorder(parent, d.id, sid, cx)
                    }))
                    .child(
                        div()
                            .id(("grip", i))
                            .flex_none()
                            .cursor_grab()
                            .child(icon("grip", 14., MUTED))
                            .on_drag(
                                DragSub {
                                    id: sid,
                                    name: drag_name,
                                },
                                |d, _, _, cx| cx.new(|_| DragPreview(d.name.clone())),
                            ),
                    )
                    .child(
                        div()
                            .id(("done", i))
                            .flex_none()
                            .cursor_pointer()
                            .child(check_circle(s.done))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.toggle_done(sid, cx)
                            })),
                    )
                    .child(
                        div()
                            .id(("open", i))
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .cursor_pointer()
                            .text_color(rgb(if s.done { MUTED } else { TEXT }))
                            .child(name)
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                this.open_task(sid, window, cx)
                            })),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(12.))
                            .text_color(rgb(MUTED))
                            .child(if s.assignees.is_empty() {
                                String::new()
                            } else {
                                s.assignees.join(", ")
                            }),
                    )
                    .child(tag(s.stage.clone(), stage_tone(&s.stage), 11.))
            }));

        let add = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(icon("plus", 14., MUTED))
            .child(div().flex_1().child(field(&self.quick)));

        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(toolbar)
            .when(self.composer.is_some() && self.proposed.is_empty(), |d| {
                d.child(self.composer_card(window, cx))
            })
            .when(!self.proposed.is_empty(), |d| d.child(self.draft_card(cx)))
            .child(list)
            .when(subs.is_empty(), |d| {
                d.child(div().text_color(rgb(MUTED)).child("No sub-tasks yet."))
            })
            .child(add)
    }

    fn ai_button(&self, id: &'static str, label: String, primary: bool) -> Stateful<Div> {
        div()
            .id(id)
            .flex()
            .flex_none()
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

    fn ai_card(&self, title: &'static str, ai: bool) -> Div {
        let fg = if ai { PURPLE.fg } else { TEXT_SOFT };
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .p(px(14.))
            .rounded(px(8.))
            .bg(rgb(if ai { PURPLE.bg } else { SIDEBAR }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(fg))
                    .when(ai, |d| d.child(icon("sparkle", 13., fg)))
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
        self.ai_card("Describe what you did", true)
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

    fn draft_card(&self, cx: &mut Context<Self>) -> Div {
        let items = self.read_drafts(cx).unwrap_or_default();
        let logged: f64 = items.iter().map(|i| i.hours).sum();
        let n = self.proposed.len();
        let action = format!(
            "Add {n} sub-task{}{}",
            if n == 1 { "" } else { "s" },
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
        let title = if self.draft_ai {
            "Drafted by AI · edit anything, nothing saved yet"
        } else {
            "New entries · nothing saved yet"
        };
        self.ai_card(title, self.draft_ai)
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
            .children(self.proposed.iter().enumerate().map(|(i, d)| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(div().flex_1().min_w_0().child(field(&d.name)))
                    .child(div().flex_none().w(px(70.)).child(field(&d.hours)))
                    .child(div().flex_none().w(px(108.)).child(field(&d.date)))
                    .child(
                        div()
                            .id(("draft-done", i))
                            .flex_none()
                            .w(px(36.))
                            .cursor_pointer()
                            .child(check_circle(d.done))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.proposed[i].done = !this.proposed[i].done;
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
                                this.proposed.remove(i);
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
                        let blank = this.blank_item();
                        let d = this.make_draft(&blank, cx);
                        this.proposed.push(d);
                        cx.notify()
                    })),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .mt(px(4.))
                    .child(
                        self.ai_button("create", action, true)
                            .when(n == 0, |d| d.opacity(0.5))
                            .on_click(
                                cx.listener(|this, _: &ClickEvent, _, cx| this.commit_work(cx)),
                            ),
                    )
                    .child(self.ai_button("discard", "Discard".into(), false).on_click(
                        cx.listener(|this, _: &ClickEvent, window, cx| {
                            this.reset_ai();
                            window.focus(&this.focus, cx);
                            cx.notify()
                        }),
                    )),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hours_are_read_leniently() {
        assert_eq!(parse_hours("1.5"), Some(1.5));
        assert_eq!(parse_hours("2h"), Some(2.));
        assert_eq!(parse_hours("1h 30m"), Some(1.5));
        assert_eq!(parse_hours("45m"), Some(0.75));
        assert_eq!(parse_hours("1:30"), Some(1.5));
        assert_eq!(parse_hours(""), Some(0.));
        assert_eq!(parse_hours("abc"), None);
        assert_eq!(parse_hours("-1"), None);
        assert_eq!(parse_hours("30"), None);
    }
}
