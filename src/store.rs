use crate::date;
use crate::html::{self, Block};
use crate::odoo::{Employee, Message, NewTaskInput, Odoo, Stage, Task, Timesheet, WorkItem};
use gpui::{Context, EventEmitter, SharedString};
use std::{collections::HashMap, sync::Arc};

#[derive(Clone)]
pub struct Detail {
    pub description: Vec<Block>,
    pub timesheets: Vec<Timesheet>,
    pub messages: Result<Vec<Message>, String>,
}

pub enum Load<T> {
    Loading,
    Ready(T),
    Failed(SharedString),
}

pub enum StoreEvent {
    Failed(SharedString),
}

pub struct Store {
    odoo: Option<Arc<Odoo>>,
    pub workspace: SharedString,
    pub version: SharedString,
    pub status: SharedString,
    pub today: i64,
    pub tasks: Vec<Task>,
    stages: HashMap<i64, Vec<Stage>>,
    details: HashMap<i64, Load<Detail>>,
    timesheets: Option<Load<Vec<Timesheet>>>,
    todos: Option<Load<Vec<Task>>>,
    employees: Option<Load<Vec<Employee>>>,
}

impl EventEmitter<StoreEvent> for Store {}

fn closed(state: &str) -> bool {
    matches!(state, "1_done" | "1_canceled")
}

impl Store {
    pub fn new() -> Self {
        Self {
            odoo: None,
            workspace: "".into(),
            version: "".into(),
            status: "Connecting…".into(),
            today: date::today(),
            tasks: Vec::new(),
            stages: HashMap::new(),
            details: HashMap::new(),
            timesheets: None,
            todos: None,
            employees: None,
        }
    }

    pub fn connect(&mut self, odoo: Arc<Odoo>, cx: &mut Context<Self>) {
        self.workspace = odoo.workspace().into();
        self.odoo = Some(odoo);
        self.status = "Syncing…".into();
        self.reload(cx);
    }

    pub fn disconnect(&mut self, cx: &mut Context<Self>) {
        self.odoo = None;
        self.tasks.clear();
        self.stages.clear();
        self.details.clear();
        self.timesheets = None;
        self.todos = None;
        self.employees = None;
        cx.notify();
    }

    pub fn synced(&self) -> bool {
        self.odoo.is_some() && self.status.as_ref() == "Synced just now"
    }

    pub fn task(&self, id: i64) -> Option<&Task> {
        self.tasks.iter().find(|t| t.id == id).or_else(|| {
            let Some(Load::Ready(todos)) = &self.todos else {
                return None;
            };
            todos.iter().find(|t| t.id == id)
        })
    }

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

    pub fn people(&self) -> Vec<String> {
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for name in self.tasks.iter().flat_map(|t| &t.assignees) {
            *counts.entry(name).or_default() += 1;
        }
        let mut p: Vec<(&str, usize)> = counts.into_iter().collect();
        p.sort_by_key(|&(n, c)| (n != "Me", std::cmp::Reverse(c), n.to_lowercase()));
        p.into_iter().map(|(n, _)| n.to_string()).collect()
    }

    pub fn assigners(&self) -> Vec<(String, usize)> {
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for t in &self.tasks {
            if let Some(who) = t.creator.as_deref()
                && t.assigned_by(who)
            {
                *counts.entry(who).or_default() += 1;
            }
        }
        let mut list: Vec<(&str, usize)> = counts.into_iter().collect();
        list.sort_by_key(|&(n, c)| (n != "Me", std::cmp::Reverse(c), n.to_lowercase()));
        list.into_iter().map(|(n, c)| (n.to_string(), c)).collect()
    }

    pub fn projects(&self) -> Vec<String> {
        let mut p: Vec<String> = self
            .tasks
            .iter()
            .filter(|t| t.project_id.is_some())
            .map(|t| t.project.clone())
            .collect();
        p.sort();
        p.dedup();
        p
    }

    pub fn stages_of(&self, project: Option<i64>) -> &[Stage] {
        project
            .and_then(|p| self.stages.get(&p))
            .map_or(&[], Vec::as_slice)
    }

    pub fn detail(&self, id: i64) -> &Load<Detail> {
        self.details.get(&id).unwrap_or(&Load::Loading)
    }

    pub fn timesheets(&self) -> &Load<Vec<Timesheet>> {
        self.timesheets.as_ref().unwrap_or(&Load::Loading)
    }

    pub fn ensure_timesheets(&mut self, cx: &mut Context<Self>) {
        if self.timesheets.is_none() {
            self.reload_timesheets(cx);
        }
    }

    pub fn reload_timesheets(&mut self, cx: &mut Context<Self>) {
        let Some(o) = self.odoo.clone() else { return };
        if self.timesheets.is_none() {
            self.timesheets = Some(Load::Loading);
            cx.notify();
        }
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move { o.all_timesheets() })
                .await;
            this.update(cx, |this, cx| {
                this.timesheets = Some(match r {
                    Ok(list) => Load::Ready(list),
                    Err(e) => Load::Failed(e.to_string().into()),
                });
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn todos(&self) -> &Load<Vec<Task>> {
        self.todos.as_ref().unwrap_or(&Load::Loading)
    }

    pub fn ensure_todos(&mut self, cx: &mut Context<Self>) {
        if self.todos.is_none() {
            self.reload_todos(cx);
        }
    }

    pub fn reload_todos(&mut self, cx: &mut Context<Self>) {
        let Some(o) = self.odoo.clone() else { return };
        if self.todos.is_none() {
            self.todos = Some(Load::Loading);
            cx.notify();
        }
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move { o.todos() })
                .await;
            this.update(cx, |this, cx| {
                this.todos = Some(match r {
                    Ok(list) => Load::Ready(list),
                    Err(e) => Load::Failed(e.to_string().into()),
                });
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn create_todo(
        &mut self,
        name: String,
        deadline: Option<String>,
        project: Option<i64>,
        cx: &mut Context<Self>,
    ) -> gpui::Task<Result<(), SharedString>> {
        self.create_task(
            NewTaskInput {
                name,
                deadline,
                project_id: project,
                ..Default::default()
            },
            cx,
        )
    }

    pub fn create_task(
        &mut self,
        input: NewTaskInput,
        cx: &mut Context<Self>,
    ) -> gpui::Task<Result<(), SharedString>> {
        let Some(o) = self.odoo.clone() else {
            return gpui::Task::ready(Err("Not connected".into()));
        };
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move { o.create_task(&input) })
                .await
                .map_err(|e| SharedString::from(e.to_string()));
            this.update(cx, |this, cx| {
                if r.is_ok() {
                    this.reload_todos(cx);
                    this.reload(cx);
                }
            })
            .ok();
            r
        })
    }

    pub fn toggle_priority(&mut self, id: i64, cx: &mut Context<Self>) {
        let Some(current) = self.task(id).map(|t| t.priority_high) else {
            return;
        };
        let high = !current;
        if let Some(t) = self.tasks.iter_mut().find(|t| t.id == id) {
            t.priority_high = high;
        }
        if let Some(Load::Ready(todos)) = &mut self.todos
            && let Some(t) = todos.iter_mut().find(|t| t.id == id)
        {
            t.priority_high = high;
        }
        self.write(id, "Couldn’t update priority", cx, move |o| {
            o.set_priority(id, high)
        });
    }

    pub fn employees(&self) -> &Load<Vec<Employee>> {
        self.employees.as_ref().unwrap_or(&Load::Loading)
    }

    pub fn ensure_employees(&mut self, cx: &mut Context<Self>) {
        if self.employees.is_none() {
            self.reload_employees(cx);
        }
    }

    pub fn reload_employees(&mut self, cx: &mut Context<Self>) {
        let Some(o) = self.odoo.clone() else { return };
        if self.employees.is_none() {
            self.employees = Some(Load::Loading);
            cx.notify();
        }
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move { o.employees() })
                .await;
            this.update(cx, |this, cx| {
                this.employees = Some(match r {
                    Ok(list) => Load::Ready(list),
                    Err(e) => Load::Failed(e.to_string().into()),
                });
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn reload(&mut self, cx: &mut Context<Self>) {
        let Some(o) = self.odoo.clone() else { return };
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move {
                    let version = o.version().unwrap_or_default();
                    anyhow::Ok((version, o.tasks(serde_json::json!([]))?))
                })
                .await;
            this.update(cx, |this, cx| {
                match r {
                    Ok((version, tasks)) => {
                        this.version = version.into();
                        this.tasks = tasks;
                        this.today = date::today();
                        this.status = "Synced just now".into();
                        if this.timesheets.is_some() {
                            this.reload_timesheets(cx);
                        }
                        if this.todos.is_some() {
                            this.reload_todos(cx);
                        }
                    }
                    Err(e) => this.status = e.to_string().into(),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn ensure_detail(&mut self, id: i64, cx: &mut Context<Self>) {
        if self.details.contains_key(&id) {
            return;
        }
        self.details.insert(id, Load::Loading);
        self.refresh_detail(id, cx);
    }

    pub fn refresh_detail(&mut self, id: i64, cx: &mut Context<Self>) {
        let Some(o) = self.odoo.clone() else { return };
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move {
                    let raw = o.description(id)?;
                    let mut description = html::parse(&raw, o.base_url());
                    for a in o.attachments(id).unwrap_or_default() {
                        let Block::Attachment { url, .. } = &a else { continue };
                        let known = |b: &Block| matches!(b, Block::Attachment { url: u, .. } if u.split('?').next() == url.split('?').next());
                        if !description.iter().any(known) {
                            description.push(a);
                        }
                    }
                    anyhow::Ok(Detail {
                        description,
                        timesheets: o.timesheets(id).unwrap_or_default(),
                        messages: o.messages(id).map_err(|e| e.to_string()),
                    })
                })
                .await;
            this.update(cx, |this, cx| {
                let load = match r {
                    Ok(d) => Load::Ready(d),
                    Err(e) => Load::Failed(e.to_string().into()),
                };
                this.details.insert(id, load);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn refresh_messages(&mut self, id: i64, cx: &mut Context<Self>) {
        let Some(o) = self.odoo.clone() else { return };
        if !self.details.contains_key(&id) {
            return;
        }
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move { o.messages(id).map_err(|e| e.to_string()) })
                .await;
            this.update(cx, |this, cx| {
                if let Some(Load::Ready(d)) = this.details.get_mut(&id) {
                    d.messages = r;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    pub fn ensure_stages(&mut self, project: Option<i64>, cx: &mut Context<Self>) {
        let (Some(o), Some(project)) = (self.odoo.clone(), project) else {
            return;
        };
        if self.stages.contains_key(&project) {
            return;
        }
        self.stages.insert(project, Vec::new());
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

    fn write(
        &mut self,
        id: i64,
        what: &'static str,
        cx: &mut Context<Self>,
        job: impl FnOnce(Arc<Odoo>) -> anyhow::Result<()> + Send + 'static,
    ) {
        let Some(o) = self.odoo.clone() else { return };
        let parent = self.task(id).and_then(|t| t.parent.as_ref().map(|p| p.0));
        cx.notify();
        cx.spawn(async move |this, cx| {
            let r = cx.background_executor().spawn(async move { job(o) }).await;
            this.update(cx, |this, cx| match r {
                Ok(()) => {
                    this.refresh_messages(id, cx);
                    if let Some(pid) = parent {
                        this.refresh_messages(pid, cx);
                    }
                }
                Err(e) => {
                    cx.emit(StoreEvent::Failed(format!("{what}: {e}").into()));
                    this.reload(cx);
                }
            })
            .ok();
        })
        .detach();
    }

    pub fn set_stage(&mut self, id: i64, stage: Stage, cx: &mut Context<Self>) {
        if let Some(t) = self.tasks.iter_mut().find(|t| t.id == id) {
            t.stage = stage.name.clone();
            t.stage_id = Some(stage.id);
            t.done = stage.fold || closed(&t.state);
        }
        self.write(id, "Couldn’t change the status", cx, move |o| {
            o.set_stage(id, stage.id)
        });
    }

    pub fn edit(
        &mut self,
        id: i64,
        what: &'static str,
        vals: serde_json::Value,
        reload: bool,
        patch: impl FnOnce(&mut Task),
        cx: &mut Context<Self>,
    ) {
        let Some(o) = self.odoo.clone() else { return };
        if let Some(t) = self.tasks.iter_mut().find(|t| t.id == id) {
            patch(t);
            let updated = t.clone();
            if let Some(Load::Ready(todos)) = &mut self.todos
                && let Some(todo) = todos.iter_mut().find(|t| t.id == id)
            {
                *todo = updated;
            }
        } else if let Some(Load::Ready(todos)) = &mut self.todos
            && let Some(todo) = todos.iter_mut().find(|t| t.id == id)
        {
            patch(todo);
        }
        cx.notify();
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move { o.set_fields(id, vals) })
                .await;
            this.update(cx, |this, cx| {
                match &r {
                    Ok(()) => this.refresh_messages(id, cx),
                    Err(e) => cx.emit(StoreEvent::Failed(format!("{what}: {e}").into())),
                }
                if r.is_err() || reload {
                    this.reload(cx);
                }
            })
            .ok();
        })
        .detach();
    }

    pub fn search_options(
        &self,
        model: &'static str,
        query: String,
        cx: &mut Context<Self>,
    ) -> gpui::Task<Result<Vec<(i64, String)>, SharedString>> {
        let Some(o) = self.odoo.clone() else {
            return gpui::Task::ready(Err("Not connected".into()));
        };
        cx.background_executor().spawn(async move {
            o.options(model, &query)
                .map_err(|e| SharedString::from(e.to_string()))
        })
    }

    pub fn toggle_done(&mut self, id: i64, cx: &mut Context<Self>) {
        let Some(current) = self.task(id).map(|t| t.done) else {
            return;
        };
        let done = !current;
        let state = if done { "1_done" } else { "01_in_progress" };
        if let Some(t) = self.tasks.iter_mut().find(|t| t.id == id) {
            t.done = done;
            t.state = state.into();
        }
        if let Some(Load::Ready(todos)) = &mut self.todos
            && let Some(t) = todos.iter_mut().find(|t| t.id == id)
        {
            t.done = done;
            t.state = state.into();
        }
        self.write(id, "Couldn’t update the task", cx, move |o| {
            o.set_done(id, done)
        });
    }

    pub fn reorder(&mut self, parent: i64, dragged: i64, onto: i64, cx: &mut Context<Self>) {
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
        self.write(parent, "Couldn’t save the new order", cx, move |o| {
            o.resequence(&ids)
        });
    }

    pub fn add_work(
        &mut self,
        parent: Task,
        items: Vec<WorkItem>,
        cx: &mut Context<Self>,
    ) -> gpui::Task<Result<(), SharedString>> {
        self.record(parent, cx, move |o, parent| o.add_work(parent, &items))
    }

    pub fn add_timesheet(
        &mut self,
        task: Task,
        item: WorkItem,
        cx: &mut Context<Self>,
    ) -> gpui::Task<Result<(), SharedString>> {
        self.record(task, cx, move |o, task| {
            o.add_timesheet(task, &item).map(|_| None)
        })
    }

    pub fn post_message(
        &mut self,
        task: Task,
        text: String,
        as_note: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Task<Result<(), SharedString>> {
        self.record(task, cx, move |o, task| {
            o.post_message(task.id, &text, as_note).map(|_| None)
        })
    }

    fn record(
        &mut self,
        task: Task,
        cx: &mut Context<Self>,
        job: impl FnOnce(&Odoo, &Task) -> anyhow::Result<Option<String>> + Send + 'static,
    ) -> gpui::Task<Result<(), SharedString>> {
        let Some(o) = self.odoo.clone() else {
            return gpui::Task::ready(Err("Not connected".into()));
        };
        let id = task.id;
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move { job(&o, &task) })
                .await
                .map_err(|e| SharedString::from(e.to_string()))
                .map(|w| w.map(SharedString::from));
            this.update(cx, |this, cx| {
                if let Ok(warning) = &r {
                    this.refresh_detail(id, cx);
                    this.reload(cx);
                    if this.timesheets.is_some() {
                        this.reload_timesheets(cx);
                    }
                    if let Some(w) = warning {
                        cx.emit(StoreEvent::Failed(w.clone()));
                    }
                }
            })
            .ok();
            r.map(|_| ())
        })
    }
}
