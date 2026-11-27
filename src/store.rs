use crate::date;
use crate::html::{self, Block};
use crate::odoo::{Message, Odoo, Stage, Task, Timesheet, WorkItem};
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
        cx.notify();
    }

    pub fn synced(&self) -> bool {
        self.odoo.is_some() && self.status.as_ref() == "Synced just now"
    }

    pub fn task(&self, id: i64) -> Option<&Task> {
        self.tasks.iter().find(|t| t.id == id)
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

    pub fn projects(&self) -> Vec<String> {
        let mut p: Vec<String> = self.tasks.iter().map(|t| t.project.clone()).collect();
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
        let Some(o) = self.odoo.clone() else { return };
        if self.details.contains_key(&id) {
            return;
        }
        self.details.insert(id, Load::Loading);
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
        what: &'static str,
        cx: &mut Context<Self>,
        job: impl FnOnce(Arc<Odoo>) -> anyhow::Result<()> + Send + 'static,
    ) {
        let Some(o) = self.odoo.clone() else { return };
        cx.notify();
        cx.spawn(async move |this, cx| {
            let r = cx.background_executor().spawn(async move { job(o) }).await;
            this.update(cx, |this, cx| {
                if let Err(e) = r {
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
        self.write("Couldn’t change the status", cx, move |o| {
            o.set_stage(id, stage.id)
        });
    }

    pub fn toggle_done(&mut self, id: i64, cx: &mut Context<Self>) {
        let Some(t) = self.tasks.iter_mut().find(|t| t.id == id) else {
            return;
        };
        let done = !t.done;
        t.done = done;
        t.state = if done { "1_done" } else { "01_in_progress" }.into();
        self.write("Couldn’t update the sub-task", cx, move |o| {
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
        self.write("Couldn’t save the new order", cx, move |o| {
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
                    this.details.remove(&id);
                    this.ensure_detail(id, cx);
                    this.reload(cx);
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
