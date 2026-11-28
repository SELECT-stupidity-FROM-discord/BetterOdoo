use crate::components::{Avatar, Button, Chip, DatePicker, Field, Variant, icon, kbd, tag};
use crate::format::{person_tone, stage_dot};
use crate::input::{InputEvent, TextInput};
use crate::odoo::NewTaskInput;
use crate::scale::px;
use crate::store::Store;
use crate::theme::*;
use gpui::{
    BoxShadow, ClickEvent, Context, Div, Entity, FocusHandle, Focusable, FontWeight, IntoElement,
    MouseButton, MouseDownEvent, Render, SharedString, Window, black, div, point, prelude::*, rgb,
    white,
};

const FAB_BG: u32 = 0x262522;
const FAB_HOVER: u32 = 0x1b1a18;
const FAB_BORDER: u32 = 0x1d1c19;

pub struct NewTask {
    store: Entity<Store>,
    root: FocusHandle,
    title: Entity<TextInput>,
    description: Entity<TextInput>,
    due: Entity<DatePicker>,
    assignee_query: Entity<TextInput>,
    customer_query: Entity<TextInput>,
    projects: Vec<(i64, String)>,
    users: Vec<(i64, String)>,
    tags: Vec<(i64, String)>,
    customers: Vec<(i64, String)>,
    project: Option<i64>,
    stage: Option<i64>,
    priority_high: bool,
    assignees: Vec<(i64, String)>,
    selected_tags: Vec<i64>,
    customer: Option<(i64, String)>,
    assignee_token: u64,
    customer_token: u64,
    open: bool,
    busy: bool,
    notice: Option<SharedString>,
}

impl NewTask {
    pub fn new(
        store: Entity<Store>,
        root: FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        let today = store.read(cx).today;
        let title = cx.new(|cx| TextInput::new("e.g. Finalize Q4 release checklist…", cx));
        let description = cx.new(|cx| {
            TextInput::new(
                "Add details, acceptance criteria, links, or notes…",
                cx,
            )
            .multiline()
        });
        let due = cx.new(|cx| {
            DatePicker::new(
                "new-task-due",
                "Due date",
                "No due date",
                today,
                root.clone(),
                window,
                cx,
            )
            .full_width()
            .height(36.)
        });
        let assignee_query = cx.new(|cx| TextInput::new("Search people to assign…", cx));
        let customer_query = cx.new(|cx| TextInput::new("Search customer or contact…", cx));

        cx.subscribe_in(&title, window, Self::on_title).detach();
        cx.subscribe_in(&description, window, Self::on_description)
            .detach();
        cx.subscribe_in(&assignee_query, window, Self::on_assignee_query)
            .detach();
        cx.subscribe_in(&customer_query, window, Self::on_customer_query)
            .detach();

        Self {
            store,
            root,
            title,
            description,
            due,
            assignee_query,
            customer_query,
            projects: Vec::new(),
            users: Vec::new(),
            tags: Vec::new(),
            customers: Vec::new(),
            project: None,
            stage: None,
            priority_high: false,
            assignees: Vec::new(),
            selected_tags: Vec::new(),
            customer: None,
            assignee_token: 0,
            customer_token: 0,
            open: false,
            busy: false,
            notice: None,
        }
    }

    fn open_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = true;
        self.notice = None;
        window.focus(&self.title.focus_handle(cx), cx);
        self.load_initial_options(cx);
        cx.notify();
    }

    fn load_initial_options(&mut self, cx: &mut Context<Self>) {
        let proj_job = self
            .store
            .update(cx, |s, cx| s.search_options("project.project", String::new(), cx));
        let user_job = self
            .store
            .update(cx, |s, cx| s.search_options("res.users", String::new(), cx));
        let tag_job = self
            .store
            .update(cx, |s, cx| s.search_options("project.tags", String::new(), cx));
        let cust_job = self
            .store
            .update(cx, |s, cx| s.search_options("res.partner", String::new(), cx));

        cx.spawn(async move |this, cx| {
            if let Ok(list) = proj_job.await {
                this.update(cx, |this, cx| {
                    this.projects = list;
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
        cx.spawn(async move |this, cx| {
            if let Ok(list) = user_job.await {
                this.update(cx, |this, cx| {
                    this.users = list;
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
        cx.spawn(async move |this, cx| {
            if let Ok(list) = tag_job.await {
                this.update(cx, |this, cx| {
                    this.tags = list;
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
        cx.spawn(async move |this, cx| {
            if let Ok(list) = cust_job.await {
                this.update(cx, |this, cx| {
                    this.customers = list;
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    pub fn dismiss(&mut self, cx: &mut Context<Self>) -> bool {
        let was = std::mem::take(&mut self.open);
        if was {
            self.due.update(cx, |d, cx| d.dismiss(cx));
            self.notice = None;
            cx.notify();
        }
        was
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.dismiss(cx) {
            window.focus(&self.root, cx);
        }
    }

    fn reset_fields(&mut self, cx: &mut Context<Self>) {
        let today = self.store.read(cx).today;
        self.title.update(cx, |i, cx| i.set_text("", cx));
        self.description.update(cx, |i, cx| i.set_text("", cx));
        self.assignee_query.update(cx, |i, cx| i.set_text("", cx));
        self.customer_query.update(cx, |i, cx| i.set_text("", cx));
        self.due.update(cx, |d, cx| d.set_value(None, today, cx));
        self.project = None;
        self.stage = None;
        self.priority_high = false;
        self.assignees.clear();
        self.selected_tags.clear();
        self.customer = None;
        self.notice = None;
    }

    fn on_title(
        &mut self,
        _: &Entity<TextInput>,
        ev: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match ev {
            InputEvent::Submit => self.create(window, cx),
            InputEvent::Cancel => self.close(window, cx),
            InputEvent::Changed => {
                if self.notice.is_some() {
                    self.notice = None;
                    cx.notify();
                }
            }
        }
    }

    fn on_description(
        &mut self,
        _: &Entity<TextInput>,
        ev: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match ev {
            InputEvent::Submit => self.create(window, cx),
            InputEvent::Cancel => self.close(window, cx),
            InputEvent::Changed => {}
        }
    }

    fn on_assignee_query(
        &mut self,
        _: &Entity<TextInput>,
        ev: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match ev {
            InputEvent::Cancel => self.close(window, cx),
            InputEvent::Submit => {
                if let Some((id, name)) = self
                    .users
                    .iter()
                    .find(|(id, _)| !self.assignees.iter().any(|(a, _)| a == id))
                    .cloned()
                {
                    self.assignees.push((id, name));
                    self.assignee_query.update(cx, |i, cx| i.set_text("", cx));
                    self.search_users(String::new(), cx);
                    cx.notify();
                }
            }
            InputEvent::Changed => {
                let q = self.assignee_query.read(cx).text().trim().to_string();
                self.search_users(q, cx);
            }
        }
    }

    fn search_users(&mut self, q: String, cx: &mut Context<Self>) {
        self.assignee_token += 1;
        let token = self.assignee_token;
        let job = self
            .store
            .update(cx, |s, cx| s.search_options("res.users", q, cx));
        cx.spawn(async move |this, cx| {
            if let Ok(list) = job.await {
                this.update(cx, |this, cx| {
                    if this.assignee_token == token {
                        this.users = list;
                        cx.notify();
                    }
                })
                .ok();
            }
        })
        .detach();
    }

    fn on_customer_query(
        &mut self,
        _: &Entity<TextInput>,
        ev: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match ev {
            InputEvent::Cancel => self.close(window, cx),
            InputEvent::Submit => {
                if let Some(first) = self.customers.first().cloned() {
                    self.customer = Some(first);
                    self.customer_query.update(cx, |i, cx| i.set_text("", cx));
                    cx.notify();
                }
            }
            InputEvent::Changed => {
                let q = self.customer_query.read(cx).text().trim().to_string();
                self.customer_token += 1;
                let token = self.customer_token;
                let job = self
                    .store
                    .update(cx, |s, cx| s.search_options("res.partner", q, cx));
                cx.spawn(async move |this, cx| {
                    if let Ok(list) = job.await {
                        this.update(cx, |this, cx| {
                            if this.customer_token == token {
                                this.customers = list;
                                cx.notify();
                            }
                        })
                        .ok();
                    }
                })
                .detach();
            }
        }
    }

    fn select_project(&mut self, pid: Option<i64>, cx: &mut Context<Self>) {
        if self.project != pid {
            self.project = pid;
            self.stage = None;
            if let Some(id) = pid {
                self.store.update(cx, |s, cx| s.ensure_stages(Some(id), cx));
            }
            cx.notify();
        }
    }

    fn create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.title.read(cx).text().trim().to_string();
        if self.busy || name.is_empty() {
            if name.is_empty() {
                self.notice = Some("Please enter a task title.".into());
                window.focus(&self.title.focus_handle(cx), cx);
                cx.notify();
            }
            return;
        }
        let desc = self.description.read(cx).text().trim().to_string();
        let input = NewTaskInput {
            name,
            description: (!desc.is_empty()).then_some(desc),
            project_id: self.project,
            stage_id: self.stage,
            deadline: self.due.read(cx).value().map(String::from),
            priority_high: self.priority_high,
            user_ids: self.assignees.iter().map(|(id, _)| *id).collect(),
            tag_ids: self.selected_tags.clone(),
            partner_id: self.customer.as_ref().map(|(id, _)| *id),
        };
        self.busy = true;
        self.notice = None;
        cx.notify();
        let job = self.store.update(cx, |s, cx| s.create_task(input, cx));
        cx.spawn_in(window, async move |this, cx| {
            let r = job.await;
            this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match r {
                    Ok(()) => {
                        this.reset_fields(cx);
                        this.close(window, cx);
                    }
                    Err(e) => this.notice = Some(format!("Couldn’t create task: {e}").into()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn field_label(title: &'static str, hint: Option<&'static str>, required: bool) -> Div {
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .text_size(px(12.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(TEXT))
                    .child(title)
                    .when(required, |d| {
                        d.child(div().text_color(rgb(DANGER)).child("*"))
                    }),
            )
            .when_some(hint, |d, h| {
                d.child(div().text_size(px(11.5)).text_color(rgb(MUTED)).child(h))
            })
    }

    fn modal(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let store = self.store.read(cx);
        let all_people = store.people();
        let desc_focused = self.description.focus_handle(cx).is_focused(window);
        let selected_project_name = self
            .project
            .and_then(|pid| self.projects.iter().find(|(id, _)| *id == pid))
            .map(|(_, name)| name.clone());
        let stages = self
            .project
            .map(|pid| store.stages_of(Some(pid)).to_vec())
            .unwrap_or_default();

        let shadow = |alpha: f32, y: f32, blur: f32, spread: f32| BoxShadow {
            color: black().opacity(alpha),
            offset: point(px(0.), px(y)),
            blur_radius: px(blur),
            spread_radius: px(spread),
            inset: false,
        };
        let submit_key = if cfg!(target_os = "macos") {
            "⌘"
        } else {
            "Ctrl"
        };

        let project_chips = self.projects.iter().map(|&(id, ref name)| {
            Chip::new(("nt-proj", id as usize), name.clone())
                .selected(self.project == Some(id))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    let next = (this.project != Some(id)).then_some(id);
                    this.select_project(next, cx);
                }))
        });

        let stage_chips = stages.iter().map(|s| {
            let sid = s.id;
            let selected = self.stage == Some(sid);
            div()
                .id(("nt-stage", sid as usize))
                .flex()
                .items_center()
                .gap(px(6.))
                .px(px(11.))
                .py(px(5.))
                .rounded_full()
                .border_1()
                .border_color(rgb(if selected { TEXT } else { BORDER }))
                .bg(rgb(if selected { TEXT } else { PAGE }))
                .text_color(rgb(if selected { PAGE } else { TEXT_SOFT }))
                .text_size(px(12.5))
                .font_weight(FontWeight::MEDIUM)
                .cursor_pointer()
                .when(!selected, |d| d.hover(|s| s.bg(rgb(SIDEBAR))))
                .child(
                    div()
                        .size(px(7.))
                        .rounded_full()
                        .bg(rgb(if selected { PAGE } else { stage_dot(&s.name) })),
                )
                .child(s.name.clone())
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.stage = (this.stage != Some(sid)).then_some(sid);
                    cx.notify();
                }))
        });

        let priority_btn = |id: &'static str, label: &'static str, high: bool| {
            let active = self.priority_high == high;
            div()
                .id(id)
                .flex_1()
                .h(px(34.))
                .rounded(px(7.))
                .flex()
                .items_center()
                .justify_center()
                .gap(px(6.))
                .cursor_pointer()
                .text_size(px(13.))
                .font_weight(FontWeight::MEDIUM)
                .when(active && high, |d| {
                    d.bg(rgb(ORANGE.bg))
                        .border_1()
                        .border_color(rgb(ORANGE.fg))
                        .text_color(rgb(ORANGE.fg))
                })
                .when(active && !high, |d| {
                    d.bg(rgb(PAGE))
                        .border_1()
                        .border_color(rgb(BORDER_STRONG))
                        .text_color(rgb(TEXT))
                })
                .when(!active, |d| {
                    d.text_color(rgb(MUTED))
                        .hover(|s| s.text_color(rgb(TEXT)))
                })
                .when(high, |d| {
                    d.child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(if active { ORANGE.fg } else { MUTED }))
                            .child("★"),
                    )
                })
                .child(label)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.priority_high = high;
                    cx.notify();
                }))
        };

        let selected_assignee_pills = self.assignees.iter().map(|&(uid, ref name)| {
            let tone = person_tone(&all_people, name);
            div()
                .id(("nt-assignee-sel", uid as usize))
                .flex()
                .items_center()
                .gap(px(6.))
                .pl(px(4.))
                .pr(px(8.))
                .py(px(3.))
                .rounded_full()
                .bg(rgb(ACTIVE_STEP))
                .border_1()
                .border_color(rgb(BLUE.bg))
                .text_size(px(12.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(ACTIVE_STEP_TEXT))
                .cursor_pointer()
                .hover(|s| s.opacity(0.85))
                .child(Avatar::new(name, tone).size(20.))
                .child(name.clone())
                .child(div().text_size(px(13.)).text_color(rgb(MUTED)).child("×"))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.assignees.retain(|(id, _)| *id != uid);
                    cx.notify();
                }))
        });

        let available_user_chips = self
            .users
            .iter()
            .filter(|(uid, _)| !self.assignees.iter().any(|(a, _)| a == uid))
            .take(10)
            .map(|&(uid, ref name)| {
                let uname = name.clone();
                let tone = person_tone(&all_people, name);
                div()
                    .id(("nt-assignee-opt", uid as usize))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .pl(px(4.))
                    .pr(px(10.))
                    .py(px(3.))
                    .rounded_full()
                    .bg(rgb(PAGE))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .text_size(px(12.5))
                    .text_color(rgb(TEXT_SOFT))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(SIDEBAR)).text_color(rgb(TEXT)))
                    .child(Avatar::new(name, tone).size(20.))
                    .child(name.clone())
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        if !this.assignees.iter().any(|(a, _)| *a == uid) {
                            this.assignees.push((uid, uname.clone()));
                            this.assignee_query.update(cx, |i, cx| i.set_text("", cx));
                            cx.notify();
                        }
                    }))
            });

        let tag_chips = self.tags.iter().map(|&(tid, ref name)| {
            let selected = self.selected_tags.contains(&tid);
            div()
                .id(("nt-tag", tid as usize))
                .flex()
                .items_center()
                .gap(px(5.))
                .px(px(10.))
                .py(px(4.))
                .rounded_full()
                .border_1()
                .border_color(rgb(if selected { PURPLE.fg } else { BORDER }))
                .bg(rgb(if selected { PURPLE.bg } else { PAGE }))
                .text_color(rgb(if selected { PURPLE.fg } else { TEXT_SOFT }))
                .text_size(px(12.))
                .font_weight(FontWeight::MEDIUM)
                .cursor_pointer()
                .when(!selected, |d| d.hover(|s| s.bg(rgb(SIDEBAR))))
                .child(icon("tag", 11., if selected { PURPLE.fg } else { MUTED }))
                .child(name.clone())
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    if let Some(pos) = this.selected_tags.iter().position(|&id| id == tid) {
                        this.selected_tags.remove(pos);
                    } else {
                        this.selected_tags.push(tid);
                    }
                    cx.notify();
                }))
        });

        let customer_chips = self.customers.iter().take(8).map(|&(cid, ref name)| {
            let cname = name.clone();
            Chip::new(("nt-cust", cid as usize), name.clone()).on_click(cx.listener(
                move |this, _: &ClickEvent, _, cx| {
                    this.customer = Some((cid, cname.clone()));
                    this.customer_query.update(cx, |i, cx| i.set_text("", cx));
                    cx.notify();
                },
            ))
        });

        div()
            .id("new-task-backdrop")
            .absolute()
            .inset_0()
            .occlude()
            .bg(black().opacity(0.42))
            .flex()
            .items_start()
            .justify_center()
            .pt(px(52.))
            .pb(px(32.))
            .px(px(24.))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, window, cx| this.close(window, cx)),
            )
            .child(
                div()
                    .id("new-task-modal")
                    .w(px(640.))
                    .max_w_full()
                    .max_h_full()
                    .rounded(px(16.))
                    .bg(rgb(PAGE))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .flex()
                    .flex_col()
                    .shadow(vec![
                        shadow(0.10, 2., 6., 0.),
                        shadow(0.16, 16., 40., -6.),
                        shadow(0.14, 28., 64., -12.),
                    ])
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    // Header
                    .child(
                        div()
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(12.))
                            .px(px(22.))
                            .pt(px(18.))
                            .pb(px(14.))
                            .border_b_1()
                            .border_color(rgb(ROW_RULE))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(3.))
                                    .min_w_0()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(8.))
                                            .child(
                                                div()
                                                    .text_size(px(16.))
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .text_color(rgb(TEXT))
                                                    .child("Create new task"),
                                            )
                                            .child(tag(
                                                selected_project_name
                                                    .unwrap_or_else(|| "Personal To-Do".into()),
                                                if self.project.is_some() { BLUE } else { GRAY },
                                                11.,
                                            )),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .text_color(rgb(MUTED))
                                            .child("Fill in the task details below to create it directly in Odoo."),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .child(kbd("Esc"))
                                    .child(
                                        div()
                                            .id("new-task-modal-close")
                                            .size(px(28.))
                                            .rounded_full()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .cursor_pointer()
                                            .text_size(px(16.))
                                            .text_color(rgb(MUTED))
                                            .hover(|s| s.bg(rgb(HOVER)).text_color(rgb(TEXT)))
                                            .child("×")
                                            .on_click(cx.listener(
                                                |this, _: &ClickEvent, window, cx| {
                                                    this.close(window, cx)
                                                },
                                            )),
                                    ),
                            ),
                    )
                    // Scrollable body
                    .child(
                        div()
                            .id("new-task-body")
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
                                    .gap(px(18.))
                                    .px(px(22.))
                                    .py(px(18.))
                                    // Error banner
                                    .when_some(self.notice.clone(), |d, msg| {
                                        d.child(
                                            div()
                                                .px(px(12.))
                                                .py(px(9.))
                                                .rounded(px(8.))
                                                .bg(rgb(STAGE_RED.bg))
                                                .text_size(px(12.5))
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(rgb(DANGER))
                                                .child(msg),
                                        )
                                    })
                                    // Title
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap(px(6.))
                                            .child(Self::field_label("Task title", None, true))
                                            .child(Field::new(&self.title).height(40.)),
                                    )
                                    // Description
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap(px(6.))
                                            .child(Self::field_label(
                                                "Description",
                                                Some("Optional"),
                                                false,
                                            ))
                                            .child(
                                                div()
                                                    .min_h(px(84.))
                                                    .px(px(12.))
                                                    .py(px(9.))
                                                    .rounded(px(8.))
                                                    .border_1()
                                                    .border_color(rgb(if desc_focused {
                                                        ACCENT
                                                    } else {
                                                        BORDER
                                                    }))
                                                    .bg(rgb(if desc_focused {
                                                        PAGE
                                                    } else {
                                                        SIDEBAR
                                                    }))
                                                    .when(!desc_focused, |d| {
                                                        d.hover(|s| {
                                                            s.border_color(rgb(BORDER_STRONG))
                                                        })
                                                    })
                                                    .text_size(px(13.))
                                                    .line_height(px(20.))
                                                    .child(self.description.clone()),
                                            ),
                                    )
                                    // Project
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap(px(7.))
                                            .child(Self::field_label(
                                                "Project",
                                                Some("Leave unselected for a personal To-Do"),
                                                false,
                                            ))
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_wrap()
                                                    .gap(px(6.))
                                                    .child(
                                                        Chip::new("nt-proj-none", "Personal To-Do")
                                                            .selected(self.project.is_none())
                                                            .on_click(cx.listener(
                                                                |this, _: &ClickEvent, _, cx| {
                                                                    this.select_project(None, cx)
                                                                },
                                                            )),
                                                    )
                                                    .children(project_chips),
                                            ),
                                    )
                                    // Stage (when project has stages)
                                    .when(!stages.is_empty(), |d| {
                                        d.child(
                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap(px(7.))
                                                .child(Self::field_label(
                                                    "Stage",
                                                    Some("Defaults to the project’s first stage"),
                                                    false,
                                                ))
                                                .child(
                                                    div()
                                                        .flex()
                                                        .flex_wrap()
                                                        .gap(px(6.))
                                                        .children(stage_chips),
                                                ),
                                        )
                                    })
                                    // Due date + Priority (2-column)
                                    .child(
                                        div()
                                            .flex()
                                            .gap(px(14.))
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .flex()
                                                    .flex_col()
                                                    .gap(px(6.))
                                                    .child(Self::field_label(
                                                        "Due date",
                                                        Some("Optional"),
                                                        false,
                                                    ))
                                                    .child(self.due.clone()),
                                            )
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .flex()
                                                    .flex_col()
                                                    .gap(px(6.))
                                                    .child(Self::field_label(
                                                        "Priority",
                                                        None,
                                                        false,
                                                    ))
                                                    .child(
                                                        div()
                                                            .flex()
                                                            .items_center()
                                                            .gap(px(4.))
                                                            .p(px(2.))
                                                            .rounded(px(9.))
                                                            .bg(rgb(SIDEBAR))
                                                            .border_1()
                                                            .border_color(rgb(BORDER))
                                                            .child(priority_btn(
                                                                "nt-prio-normal",
                                                                "Normal",
                                                                false,
                                                            ))
                                                            .child(priority_btn(
                                                                "nt-prio-high",
                                                                "High priority",
                                                                true,
                                                            )),
                                                    ),
                                            ),
                                    )
                                    // Assignees
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap(px(8.))
                                            .child(Self::field_label(
                                                "Assignees",
                                                Some("Defaults to you if none selected"),
                                                false,
                                            ))
                                            .when(!self.assignees.is_empty(), |d| {
                                                d.child(
                                                    div()
                                                        .flex()
                                                        .flex_wrap()
                                                        .gap(px(6.))
                                                        .children(selected_assignee_pills),
                                                )
                                            })
                                            .child(Field::new(&self.assignee_query).height(34.))
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_wrap()
                                                    .gap(px(6.))
                                                    .children(available_user_chips),
                                            ),
                                    )
                                    // Tags
                                    .when(!self.tags.is_empty(), |d| {
                                        d.child(
                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap(px(7.))
                                                .child(Self::field_label(
                                                    "Tags",
                                                    Some("Optional"),
                                                    false,
                                                ))
                                                .child(
                                                    div()
                                                        .flex()
                                                        .flex_wrap()
                                                        .gap(px(6.))
                                                        .children(tag_chips),
                                                ),
                                        )
                                    })
                                    // Customer
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap(px(8.))
                                            .child(Self::field_label(
                                                "Customer",
                                                Some("Optional"),
                                                false,
                                            ))
                                            .when_some(self.customer.clone(), |d, (_, cname)| {
                                                d.child(
                                                    div().flex().child(
                                                        div()
                                                            .id("nt-cust-selected")
                                                            .flex()
                                                            .items_center()
                                                            .gap(px(6.))
                                                            .px(px(10.))
                                                            .py(px(5.))
                                                            .rounded_full()
                                                            .bg(rgb(SIDEBAR))
                                                            .border_1()
                                                            .border_color(rgb(BORDER_STRONG))
                                                            .text_size(px(12.5))
                                                            .font_weight(FontWeight::MEDIUM)
                                                            .text_color(rgb(TEXT))
                                                            .cursor_pointer()
                                                            .hover(|s| s.bg(rgb(HOVER)))
                                                            .child(icon("user", 13., MUTED))
                                                            .child(cname)
                                                            .child(
                                                                div()
                                                                    .text_color(rgb(MUTED))
                                                                    .child("×"),
                                                            )
                                                            .on_click(cx.listener(
                                                                |this, _: &ClickEvent, _, cx| {
                                                                    this.customer = None;
                                                                    cx.notify();
                                                                },
                                                            )),
                                                    ),
                                                )
                                            })
                                            .when(self.customer.is_none(), |d| {
                                                d.child(
                                                    Field::new(&self.customer_query).height(34.),
                                                )
                                                .when(!self.customers.is_empty(), |el| {
                                                    el.child(
                                                        div()
                                                            .flex()
                                                            .flex_wrap()
                                                            .gap(px(6.))
                                                            .children(customer_chips),
                                                    )
                                                })
                                            }),
                                    ),
                            ),
                    )
                    // Footer
                    .child(
                        div()
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(12.))
                            .px(px(22.))
                            .py(px(14.))
                            .bg(rgb(SIDEBAR))
                            .border_t_1()
                            .border_color(rgb(ROW_RULE))
                            .rounded_b(px(16.))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(5.))
                                    .text_size(px(12.))
                                    .text_color(rgb(MUTED))
                                    .child(kbd(submit_key))
                                    .child(kbd("↵"))
                                    .child("to create"),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .child(Button::new("new-task-cancel", "Cancel").on_click(
                                        cx.listener(|this, _: &ClickEvent, window, cx| {
                                            this.close(window, cx)
                                        }),
                                    ))
                                    .child(
                                        Button::new(
                                            "new-task-create",
                                            if self.busy {
                                                "Creating…"
                                            } else {
                                                "Create task"
                                            },
                                        )
                                        .icon("plus")
                                        .variant(Variant::Primary)
                                        .disabled(self.busy)
                                        .on_click(cx.listener(
                                            |this, _: &ClickEvent, window, cx| {
                                                this.create(window, cx)
                                            },
                                        )),
                                    ),
                            ),
                    ),
            )
    }
}

impl Render for NewTask {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.open {
            return self.modal(window, cx).into_any_element();
        }
        let shadow = |alpha: f32, y: f32, blur: f32, spread: f32| BoxShadow {
            color: black().opacity(alpha),
            offset: point(px(0.), px(y)),
            blur_radius: px(blur),
            spread_radius: px(spread),
            inset: false,
        };
        let specular = BoxShadow {
            color: white().opacity(0.14),
            offset: point(px(0.), px(1.)),
            blur_radius: px(0.),
            spread_radius: px(0.),
            inset: true,
        };
        div()
            .absolute()
            .right(px(28.))
            .bottom(px(24.))
            .child(
                div()
                    .id("new-task-fab")
                    .flex()
                    .items_center()
                    .gap(px(9.))
                    .h(px(40.))
                    .pl(px(7.))
                    .pr(px(16.))
                    .rounded_full()
                    .bg(rgb(FAB_BG))
                    .border_1()
                    .border_color(rgb(FAB_BORDER))
                    .text_size(px(13.5))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(white())
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(FAB_HOVER)))
                    .shadow(vec![
                        specular,
                        shadow(0.14, 1., 2., 0.),
                        shadow(0.14, 6., 16., -3.),
                        shadow(0.10, 14., 28., -6.),
                    ])
                    .child(
                        div()
                            .size(px(26.))
                            .rounded_full()
                            .bg(white().opacity(0.13))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(icon("plus", 13., 0xffffff)),
                    )
                    .child("New task")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            this.open_modal(window, cx);
                        }),
                    ),
            )
            .into_any_element()
    }
}
