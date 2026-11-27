mod activity;
mod entries;
mod properties;
mod subtasks;
mod timesheets;

use crate::components::{Button, RichText, Segmented, TabBar, Variant, tag};
use crate::date;
use crate::format::stage_tone;
use crate::odoo::Task;
use crate::scale::px;
use crate::store::{Load, Store, StoreEvent};
use crate::theme::*;
use activity::ActivityTab;
use entries::EntryEditor;
use gpui::{
    ClickEvent, Context, Entity, EventEmitter, FocusHandle, FontWeight, KeyDownEvent, SharedString,
    Window, div, prelude::*, rgb,
};
use properties::Properties;
use subtasks::SubtaskList;
use timesheets::TimesheetTab;

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Description,
    Timesheets,
    SubTasks,
    Activity,
}

pub enum ChildEvent {
    Open(i64),
    Notice(SharedString),
}

pub enum PageEvent {
    Close,
}

pub struct TaskPage {
    store: Entity<Store>,
    root: FocusHandle,
    id: i64,
    tab: Tab,
    notice: Option<SharedString>,
    entries: Entity<EntryEditor>,
    subtasks: Entity<SubtaskList>,
    timesheets: Entity<TimesheetTab>,
    activity: Entity<ActivityTab>,
    properties: Entity<Properties>,
    project_label: SharedString,
}

impl EventEmitter<PageEvent> for TaskPage {}

impl TaskPage {
    pub fn new(
        store: Entity<Store>,
        root: FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        cx.subscribe(&store, |this, _, ev: &StoreEvent, cx| {
            let StoreEvent::Failed(msg) = ev;
            this.notice = Some(msg.clone());
            cx.notify();
        })
        .detach();
        let entries = cx.new(|_| EntryEditor::new(store.clone(), root.clone()));
        let subtasks =
            cx.new(|cx| SubtaskList::new(store.clone(), root.clone(), entries.clone(), window, cx));
        let timesheets = cx.new(|cx| TimesheetTab::new(store.clone(), root.clone(), window, cx));
        let activity = cx.new(|cx| ActivityTab::new(store.clone(), root.clone(), window, cx));
        let properties = cx.new(|cx| Properties::new(store.clone(), root.clone(), window, cx));
        cx.subscribe_in(&properties, window, Self::on_child)
            .detach();
        cx.subscribe_in(&activity, window, Self::on_child).detach();
        cx.subscribe_in(&subtasks, window, Self::on_child).detach();
        cx.subscribe_in(&timesheets, window, Self::on_child)
            .detach();
        Self {
            store,
            root,
            id: 0,
            tab: Tab::SubTasks,
            notice: None,
            entries,
            subtasks,
            timesheets,
            activity,
            properties,
            project_label: "My Tasks".into(),
        }
    }

    pub fn open(
        &mut self,
        id: i64,
        project_label: SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let has_subs = !self.store.read(cx).children_of(id).is_empty();
        let project = self.store.read(cx).task(id).map(|t| t.project_id);
        self.id = id;
        self.project_label = project_label;
        self.tab = if has_subs {
            Tab::SubTasks
        } else {
            Tab::Description
        };
        self.notice = None;
        self.entries.update(cx, |e, cx| e.set_task(id, cx));
        self.subtasks.update(cx, |s, cx| s.set_task(id, cx));
        self.timesheets.update(cx, |t, cx| t.set_task(id, cx));
        self.activity.update(cx, |a, cx| a.set_task(id, cx));
        self.properties.update(cx, |p, cx| p.set_task(id, cx));
        self.store.update(cx, |s, cx| {
            s.ensure_detail(id, cx);
            s.ensure_stages(project.flatten(), cx);
        });
        window.focus(&self.root, cx);
        cx.notify();
    }

    fn on_child<T: 'static>(
        &mut self,
        _: &Entity<T>,
        ev: &ChildEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match ev {
            ChildEvent::Open(id) => {
                let label = self.project_label.clone();
                self.open(*id, label, window, cx)
            }
            ChildEvent::Notice(msg) => {
                self.notice = Some(msg.clone()).filter(|m| !m.is_empty());
                cx.notify();
            }
        }
    }

    pub fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let parent = self
            .store
            .read(cx)
            .task(self.id)
            .and_then(|t| t.parent.as_ref().map(|p| p.0))
            .filter(|p| self.store.read(cx).task(*p).is_some());
        match parent {
            Some(p) => {
                let label = self.project_label.clone();
                self.open(p, label, window, cx)
            }
            None => cx.emit(PageEvent::Close),
        }
    }

    pub fn handle_key(
        &mut self,
        ev: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match ev.keystroke.key.as_str() {
            "a" => {
                self.tab = Tab::SubTasks;
                self.entries.update(cx, |e, cx| e.open_ai(window, cx));
            }
            "enter" if self.entries.read(cx).has_drafts() => {
                self.entries.update(cx, |e, cx| e.commit(cx))
            }
            "escape" => {
                if !self.entries.update(cx, |e, cx| e.dismiss(window, cx)) {
                    self.back(window, cx);
                }
            }
            _ => return false,
        }
        cx.notify();
        true
    }

    fn crumb(&self, t: &Task, cx: &mut Context<Self>) -> impl IntoElement {
        let parent = t
            .parent
            .clone()
            .filter(|p| self.store.read(cx).task(p.0).is_some());
        div()
            .flex()
            .gap(px(6.))
            .text_size(px(12.))
            .text_color(rgb(MUTED))
            .child(
                div()
                    .id("back-to-list")
                    .cursor_pointer()
                    .hover(|s| s.text_color(rgb(TEXT)))
                    .child(self.project_label.clone())
                    .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(PageEvent::Close))),
            )
            .when_some(parent, |d, (pid, pname)| {
                d.child("/").child(
                    div()
                        .id("parent")
                        .cursor_pointer()
                        .hover(|s| s.text_color(rgb(TEXT)))
                        .child(pname)
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            let label = this.project_label.clone();
                            this.open(pid, label, window, cx)
                        })),
                )
            })
            .child(format!("/ #{}", t.id))
    }

    fn status(&self, t: &Task, cx: &mut Context<Self>) -> impl IntoElement {
        let store = self.store.read(cx);
        let (stages, today) = (store.stages_of(t.project_id).to_vec(), store.today);
        if stages.is_empty() {
            return div()
                .flex()
                .child(tag(t.stage.clone(), stage_tone(&t.stage), 12.))
                .into_any_element();
        }
        let id = t.id;
        stages
            .into_iter()
            .fold(Segmented::new(), |bar, s| {
                let on = t.stage_id == Some(s.id);
                let name = s.name.clone();
                let note = on
                    .then_some(t.stage_since.as_deref())
                    .flatten()
                    .and_then(|d| date::days_since(d, today))
                    .map(|n| format!("{n}d"));
                bar.item(
                    name,
                    on,
                    note,
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        let stage = s.clone();
                        this.store.update(cx, |st, cx| st.set_stage(id, stage, cx))
                    }),
                )
            })
            .into_any_element()
    }

    fn tabs(&self, subs: &[Task], cx: &mut Context<Self>) -> impl IntoElement {
        let tab = |which: Tab| {
            cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.tab = which;
                cx.notify()
            })
        };
        let sub_label = if subs.is_empty() {
            "Sub-tasks".to_string()
        } else {
            format!(
                "Sub-tasks {}/{}",
                subs.iter().filter(|s| s.done).count(),
                subs.len()
            )
        };
        let actions = (self.tab == Tab::SubTasks).then(|| {
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(
                    Button::new("add-entries", "Add entries")
                        .icon("plus")
                        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                            this.entries.update(cx, |e, cx| e.open_manual(window, cx))
                        })),
                )
                .child(
                    Button::new("autofill", "Autofill with AI")
                        .icon("sparkle")
                        .variant(Variant::Ai)
                        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                            this.entries.update(cx, |e, cx| e.open_ai(window, cx))
                        })),
                )
        });
        TabBar::new()
            .trailing(actions)
            .tab(
                "Description",
                self.tab == Tab::Description,
                tab(Tab::Description),
            )
            .tab(
                "Timesheets",
                self.tab == Tab::Timesheets,
                tab(Tab::Timesheets),
            )
            .tab(sub_label, self.tab == Tab::SubTasks, tab(Tab::SubTasks))
            .tab("Activity", self.tab == Tab::Activity, tab(Tab::Activity))
    }

    fn description(&self, cx: &Context<Self>) -> gpui::AnyElement {
        let note = |text: String| div().text_color(rgb(MUTED)).child(text).into_any_element();
        match self.store.read(cx).detail(self.id) {
            Load::Loading => note("Loading…".into()),
            Load::Failed(e) => note(format!("Couldn’t load: {e}")),
            Load::Ready(d) if d.description.is_empty() => note("No description.".into()),
            Load::Ready(d) => RichText::new(d.description.clone()).into_any_element(),
        }
    }
}

impl Render for TaskPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let store = self.store.read(cx);
        let Some(t) = store.task(self.id).cloned() else {
            return div().id("task-page").flex_1();
        };
        let today = store.today;
        let subs = store.children_of(self.id);
        let mut due = date::due(t.deadline.as_deref(), today);
        due.overdue &= !t.done;

        let body = match self.tab {
            Tab::Description => self.description(cx),
            Tab::Timesheets => self.timesheets.clone().into_any_element(),
            Tab::SubTasks => self.subtasks.clone().into_any_element(),
            Tab::Activity => self.activity.clone().into_any_element(),
        };

        div()
            .id("task-page")
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .px(px(48.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .pt(px(40.))
                    .pb(px(64.))
                    .gap(px(18.))
                    .child(self.crumb(&t, cx))
                    .child(
                        div()
                            .text_size(px(32.))
                            .font_weight(FontWeight::BOLD)
                            .line_height(px(40.))
                            .child(t.name.clone()),
                    )
                    .child(self.status(&t, cx))
                    .when_some(self.notice.clone(), |d, n| {
                        d.child(div().text_size(px(13.)).text_color(rgb(DANGER)).child(n))
                    })
                    .child(self.properties.clone())
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .child(self.tabs(&subs, cx))
                            .child(body),
                    ),
            )
    }
}
