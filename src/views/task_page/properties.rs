use super::ChildEvent;
use crate::components::{
    Avatar, DatePicker, DatePickerEvent, Floating, MenuHeader, Property, icon, tag,
};
use crate::date;
use crate::format::{hours, person_tone};
use crate::input::{InputEvent, TextInput};
use crate::odoo::Task;
use crate::scale::px;
use crate::store::Store;
use crate::theme::*;
use gpui::{
    AnyElement, ClickEvent, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    MouseDownEvent, Render, Window, div, prelude::*, rgb,
};
use serde_json::{Value, json};

#[derive(Clone, Copy, PartialEq)]
enum Field {
    Project,
    Assignees,
    Tags,
    Customer,
    Deadline,
}

impl Field {
    fn model(self) -> &'static str {
        match self {
            Field::Project => "project.project",
            Field::Assignees => "res.users",
            Field::Tags => "project.tags",
            Field::Customer | Field::Deadline => "res.partner",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Field::Project => "Move to project",
            Field::Assignees => "Assign people",
            Field::Tags => "Tags",
            Field::Customer => "Customer",
            Field::Deadline => "Deadline",
        }
    }
}

pub struct Properties {
    store: Entity<Store>,
    root: FocusHandle,
    task: i64,
    open: Option<Field>,
    query: Entity<TextInput>,
    day: Entity<DatePicker>,
    options: Vec<(i64, String)>,
    loading: bool,
    token: u64,
}

impl EventEmitter<ChildEvent> for Properties {}

impl Properties {
    pub fn new(
        store: Entity<Store>,
        root: FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        let today = store.read(cx).today;
        let query = cx.new(|cx| TextInput::new("Search…", cx));
        let root_for_cal = root.clone();
        let day = cx.new(|cx| {
            DatePicker::new(
                "prop-deadline",
                "Deadline",
                "Empty",
                today,
                root_for_cal,
                window,
                cx,
            )
            .headless()
        });
        cx.subscribe_in(&query, window, Self::on_query).detach();
        cx.subscribe_in(&day, window, Self::on_day_picked).detach();
        Self {
            store,
            root,
            task: 0,
            open: None,
            query,
            day,
            options: Vec::new(),
            loading: false,
            token: 0,
        }
    }

    pub fn set_task(&mut self, id: i64, cx: &mut Context<Self>) {
        self.task = id;
        self.open = None;
        self.day.update(cx, |d, cx| {
            d.dismiss(cx);
        });
        cx.notify();
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.day.update(cx, |d, cx| {
            d.dismiss(cx);
        });
        if self.open.take().is_some() {
            window.focus(&self.root, cx);
            cx.notify();
        }
    }

    fn show(&mut self, field: Field, window: &mut Window, cx: &mut Context<Self>) {
        if self.open == Some(field) {
            return self.close(window, cx);
        }
        self.day.update(cx, |d, cx| {
            d.dismiss(cx);
        });
        self.open = Some(field);
        self.options.clear();
        self.query.update(cx, |q, cx| q.set_text("", cx));
        if field == Field::Deadline {
            let today = self.store.read(cx).today;
            let current = self
                .task(cx)
                .and_then(|t| t.deadline)
                .and_then(|d| d.get(..10).map(String::from));
            self.day
                .update(cx, |d, cx| d.open_popover(current, today, window, cx));
        } else {
            window.focus(&self.query.focus_handle(cx), cx);
            self.search(cx);
        }
        cx.notify();
    }

    fn search(&mut self, cx: &mut Context<Self>) {
        let Some(field) = self.open else { return };
        self.token += 1;
        let token = self.token;
        self.loading = true;
        let text = self.query.read(cx).text().trim().to_string();
        let job = self
            .store
            .update(cx, |s, cx| s.search_options(field.model(), text, cx));
        cx.spawn(async move |this, cx| {
            let r = job.await;
            this.update(cx, |this, cx| {
                if this.token != token {
                    return;
                }
                this.loading = false;
                match r {
                    Ok(options) => this.options = options,
                    Err(e) => {
                        this.options.clear();
                        cx.emit(ChildEvent::Notice(format!("Couldn’t search: {e}").into()));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn on_query(
        &mut self,
        _: &Entity<TextInput>,
        ev: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match ev {
            InputEvent::Changed => self.search(cx),
            InputEvent::Cancel => self.close(window, cx),
            InputEvent::Submit => {
                if let (Some(field), Some((id, name))) = (self.open, self.options.first().cloned())
                {
                    self.pick(field, id, name, window, cx);
                }
            }
        }
    }

    fn on_day_picked(
        &mut self,
        _: &Entity<DatePicker>,
        ev: &DatePickerEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match ev {
            DatePickerEvent::Changed(day) => self.set_deadline(day.clone(), window, cx),
            DatePickerEvent::Closed => self.close(window, cx),
        }
    }

    fn task(&self, cx: &Context<Self>) -> Option<Task> {
        self.store.read(cx).task(self.task).cloned()
    }

    fn set_deadline(&mut self, day: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(t) = self.task(cx) else { return };
        let value = day.map(|d| match t.deadline.as_deref().and_then(|o| o.get(10..)) {
            Some(time) => format!("{d}{time}"),
            None => d,
        });
        let vals = json!({"date_deadline": value.clone().map_or(Value::Bool(false), Value::from)});
        cx.emit(ChildEvent::Notice("".into()));
        self.store.update(cx, |s, cx| {
            s.edit(
                t.id,
                "Couldn’t change the deadline",
                vals,
                false,
                |t| t.deadline = value,
                cx,
            )
        });
        self.close(window, cx);
    }

    fn pick(
        &mut self,
        field: Field,
        id: i64,
        name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(t) = self.task(cx) else { return };
        cx.emit(ChildEvent::Notice("".into()));
        match field {
            Field::Project => {
                let vals = json!({"project_id": id});
                self.store.update(cx, |s, cx| {
                    s.edit(
                        t.id,
                        "Couldn’t move the task",
                        vals,
                        true,
                        |t| {
                            t.project = name;
                            t.project_id = Some(id);
                        },
                        cx,
                    );
                    s.ensure_stages(Some(id), cx);
                });
                self.close(window, cx);
            }
            Field::Customer => {
                let vals = json!({"partner_id": id});
                self.store.update(cx, |s, cx| {
                    s.edit(
                        t.id,
                        "Couldn’t set the customer",
                        vals,
                        false,
                        |t| t.partner = Some((id, name)),
                        cx,
                    )
                });
                self.close(window, cx);
            }
            Field::Assignees => {
                let mut ids = t.user_ids.clone();
                let at = ids.iter().position(|i| *i == id);
                match at {
                    Some(n) => {
                        ids.remove(n);
                    }
                    None => ids.push(id),
                }
                let vals = json!({"user_ids": [[6, 0, ids.clone()]]});
                self.store.update(cx, |s, cx| {
                    s.edit(
                        t.id,
                        "Couldn’t change the assignees",
                        vals,
                        false,
                        |t| {
                            match at {
                                Some(n) if n < t.assignees.len() => {
                                    t.assignees.remove(n);
                                }
                                Some(_) => {}
                                None => t.assignees.push(name),
                            }
                            t.user_ids = ids;
                        },
                        cx,
                    )
                });
            }
            Field::Tags => {
                let mut tags = t.tags.clone();
                match tags.iter().position(|(i, _)| *i == id) {
                    Some(n) => {
                        tags.remove(n);
                    }
                    None => tags.push((id, name)),
                }
                let ids: Vec<i64> = tags.iter().map(|(i, _)| *i).collect();
                let vals = json!({"tag_ids": [[6, 0, ids]]});
                self.store.update(cx, |s, cx| {
                    s.edit(
                        t.id,
                        "Couldn’t change the tags",
                        vals,
                        false,
                        |t| t.tags = tags,
                        cx,
                    )
                });
            }
            Field::Deadline => {}
        }
        cx.notify();
    }

    fn clear(&mut self, field: Field, window: &mut Window, cx: &mut Context<Self>) {
        let Some(t) = self.task(cx) else { return };
        let id = t.id;
        match field {
            Field::Customer => {
                self.store.update(cx, |s, cx| {
                    s.edit(
                        id,
                        "Couldn’t clear the customer",
                        json!({"partner_id": false}),
                        false,
                        |t| t.partner = None,
                        cx,
                    )
                });
            }
            Field::Assignees => {
                self.store.update(cx, |s, cx| {
                    s.edit(
                        id,
                        "Couldn’t clear the assignees",
                        json!({"user_ids": [[6, 0, []]]}),
                        false,
                        |t| {
                            t.assignees.clear();
                            t.user_ids.clear();
                        },
                        cx,
                    )
                });
            }
            Field::Tags => {
                self.store.update(cx, |s, cx| {
                    s.edit(
                        id,
                        "Couldn’t clear the tags",
                        json!({"tag_ids": [[6, 0, []]]}),
                        false,
                        |t| t.tags.clear(),
                        cx,
                    )
                });
            }
            Field::Deadline => return self.set_deadline(None, window, cx),
            Field::Project => {}
        }
        cx.notify();
    }

    fn selected(&self, field: Field, id: i64, t: &Task) -> bool {
        match field {
            Field::Project => t.project_id == Some(id),
            Field::Customer => t.partner.as_ref().is_some_and(|(p, _)| *p == id),
            Field::Assignees => t.user_ids.contains(&id),
            Field::Tags => t.tags.iter().any(|(i, _)| *i == id),
            Field::Deadline => false,
        }
    }

    fn value(
        &self,
        field: Field,
        id: &'static str,
        content: impl IntoElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id(id)
            .ml(px(-6.))
            .px(px(6.))
            .py(px(2.))
            .rounded(px(5.))
            .cursor_pointer()
            .hover(|s| s.bg(rgb(HOVER)))
            .when(self.open == Some(field), |d| d.bg(rgb(HOVER)))
            .child(content)
            .on_click(
                cx.listener(move |this, _: &ClickEvent, window, cx| this.show(field, window, cx)),
            )
            .into_any_element()
    }

    fn empty() -> AnyElement {
        div()
            .text_color(rgb(MUTED))
            .child("Empty")
            .into_any_element()
    }

    fn picker(&self, field: Field, t: &Task, cx: &mut Context<Self>) -> Floating {
        let has_value = match field {
            Field::Customer => t.partner.is_some(),
            Field::Assignees => !t.user_ids.is_empty(),
            Field::Tags => !t.tags.is_empty(),
            _ => false,
        };
        let mut menu = Floating::new("property-picker")
            .width(300.)
            .max_height(380.)
            .on_dismiss(cx.listener(|this, _: &MouseDownEvent, window, cx| this.close(window, cx)))
            .child(MenuHeader::new(field.title()).on_clear(
                has_value,
                cx.listener(move |this, _: &ClickEvent, window, cx| this.clear(field, window, cx)),
            ))
            .child(
                div()
                    .mx(px(6.))
                    .mb(px(4.))
                    .px(px(10.))
                    .py(px(5.))
                    .rounded(px(6.))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .child(self.query.clone()),
            );
        if self.loading && self.options.is_empty() {
            menu = menu.child(
                div()
                    .px(px(10.))
                    .py(px(6.))
                    .text_color(rgb(MUTED))
                    .child("Searching…"),
            );
        } else if self.options.is_empty() {
            menu = menu.child(
                div()
                    .px(px(10.))
                    .py(px(6.))
                    .text_color(rgb(MUTED))
                    .child("No matches"),
            );
        }
        menu.children(
            self.options
                .iter()
                .enumerate()
                .map(|(i, (id, name))| {
                    let (id, label) = (*id, name.clone());
                    let on = self.selected(field, id, t);
                    div()
                        .id(("option", i))
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .min_h(px(36.))
                        .py(px(6.))
                        .px(px(10.))
                        .rounded(px(6.))
                        .cursor_pointer()
                        .text_size(px(13.))
                        .hover(|s| s.bg(rgb(HOVER)))
                        .child(div().flex_1().min_w_0().truncate().child(name.clone()))
                        .child(
                            div()
                                .flex_none()
                                .size(px(14.))
                                .when(on, |d| d.child(icon("check", 14., ACCENT))),
                        )
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            this.pick(field, id, label.clone(), window, cx)
                        }))
                })
                .collect::<Vec<_>>(),
        )
    }

    fn row(
        &self,
        field: Field,
        icon_name: &'static str,
        label: &'static str,
        content: AnyElement,
        t: &Task,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let popover = (self.open == Some(field)).then(|| {
            if field == Field::Deadline {
                self.day.clone().into_any_element()
            } else {
                self.picker(field, t, cx).into_any_element()
            }
        });
        div()
            .flex()
            .flex_col()
            .child(Property::new(icon_name, label).child(self.value(field, label, content, cx)))
            .children(popover)
    }
}

impl Render for Properties {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(t) = self.task(cx) else {
            return div().into_any_element();
        };
        let today = self.store.read(cx).today;
        let mut due = date::due(t.deadline.as_deref(), today);
        due.overdue &= !t.done;
        let people = if t.assignees.is_empty() {
            Self::empty()
        } else {
            div()
                .flex()
                .items_center()
                .gap(px(12.))
                .children(t.assignees.iter().map(|n| {
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(Avatar::new(n, person_tone(&[], n)).size(20.))
                        .child(n.clone())
                }))
                .into_any_element()
        };
        let tags = if t.tags.is_empty() {
            Self::empty()
        } else {
            div()
                .flex()
                .gap(px(6.))
                .children(t.tags.iter().map(|(_, n)| tag(n.clone(), BLUE, 12.)))
                .into_any_element()
        };
        let customer = t.partner.as_ref().map_or_else(Self::empty, |(_, n)| {
            div().child(n.clone()).into_any_element()
        });
        let deadline = if t.deadline.is_some() {
            div()
                .when(due.overdue, |d| d.text_color(rgb(DANGER)))
                .child(due.label)
                .into_any_element()
        } else {
            Self::empty()
        };
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(self.row(
                Field::Project,
                "status",
                "Project",
                div().child(t.project.clone()).into_any_element(),
                &t,
                cx,
            ))
            .child(self.row(Field::Assignees, "user", "Assignees", people, &t, cx))
            .child(self.row(Field::Deadline, "calendar", "Due", deadline, &t, cx))
            .child(self.row(Field::Tags, "tag", "Tags", tags, &t, cx))
            .child(self.row(Field::Customer, "user", "Customer", customer, &t, cx))
            .child(Property::new("clock", "Logged").child(div().child(hours(t.logged_hours))))
            .into_any_element()
    }
}
