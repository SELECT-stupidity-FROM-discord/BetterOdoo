use crate::components::{Floating, IconButton, NavItem, kbd};
use crate::store::Store;
use crate::theme::*;
use crate::views::task_list::{TOP_PEOPLE, TaskList};
use gpui::{
    ClickEvent, Context, Div, Entity, EventEmitter, FontWeight, MouseDownEvent, Window, div,
    prelude::*, px, rgb, white,
};

pub enum SidebarEvent {
    Tasks,
    Project(String),
    Settings,
    Search,
}

pub struct Sidebar {
    store: Entity<Store>,
    list: Entity<TaskList>,
    shortcuts: bool,
    settings_open: bool,
}

impl EventEmitter<SidebarEvent> for Sidebar {}

impl Sidebar {
    pub fn new(store: Entity<Store>, list: Entity<TaskList>, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        cx.observe(&list, |_, _, cx| cx.notify()).detach();
        Self {
            store,
            list,
            shortcuts: false,
            settings_open: false,
        }
    }

    pub fn toggle_shortcuts(&mut self, cx: &mut Context<Self>) {
        self.shortcuts = !self.shortcuts;
        cx.notify();
    }

    pub fn dismiss_shortcuts(&mut self, cx: &mut Context<Self>) -> bool {
        let open = std::mem::take(&mut self.shortcuts);
        cx.notify();
        open
    }

    pub fn set_settings_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.settings_open = open;
        cx.notify();
    }

    fn workspace_header(&self, cx: &Context<Self>) -> Div {
        let store = self.store.read(cx);
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
                        store
                            .workspace
                            .chars()
                            .next()
                            .map_or("·".into(), |c| c.to_uppercase().to_string()),
                    ),
            )
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .truncate()
                    .child(store.workspace.clone()),
            )
            .child(
                div()
                    .ml_auto()
                    .text_size(px(12.))
                    .text_color(rgb(MUTED))
                    .child(store.version.clone()),
            )
    }

    fn shortcuts_card(&self, people: usize, cx: &mut Context<Self>) -> Floating {
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
        Floating::new("shortcuts")
            .above(38.)
            .width(300.)
            .padded(14., 10.)
            .on_dismiss(cx.listener(|this, _: &MouseDownEvent, _, cx| {
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
            }))
    }

    fn footer(&self, cx: &mut Context<Self>) -> Div {
        let store = self.store.read(cx);
        let open = self.settings_open;
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
                    .bg(rgb(if store.synced() { SUCCESS } else { MUTED })),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(store.status.clone()),
            )
            .child(
                IconButton::new("cog", "cog")
                    .active(open)
                    .on_press(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                        this.shortcuts = false;
                        cx.emit(SidebarEvent::Settings);
                        cx.notify()
                    })),
            )
    }
}

impl Render for Sidebar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (count, projects, people) = {
            let store = self.store.read(cx);
            (store.tasks.len(), store.projects(), store.people().len())
        };
        let current = self.list.read(cx).project().cloned();
        let shortcuts = self.shortcuts;
        let key = if cfg!(target_os = "macos") {
            "⌘K"
        } else {
            "Ctrl K"
        };
        let muted = |text: String| {
            div()
                .text_size(px(12.))
                .font_weight(FontWeight::NORMAL)
                .text_color(rgb(MUTED))
                .child(text)
        };

        div()
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
            .child(self.workspace_header(cx))
            .child(
                NavItem::new("nav-search", "Search")
                    .icon("search")
                    .child(muted(key.into()))
                    .on_click(
                        cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(SidebarEvent::Search)),
                    ),
            )
            .child(
                NavItem::new("nav-tasks", "My Tasks")
                    .icon("tasks")
                    .active(current.is_none() && !self.settings_open)
                    .child(muted(count.to_string()))
                    .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(SidebarEvent::Tasks))),
            )
            .child(NavItem::new("nav-timesheets", "Timesheets").icon("clock"))
            .child(
                div()
                    .px(px(10.))
                    .pt(px(18.))
                    .pb(px(6.))
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(MUTED))
                    .child("Projects"),
            )
            .child(
                div()
                    .id("projects")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .gap(px(1.))
                    .overflow_y_scroll()
                    .children(projects.into_iter().enumerate().map(|(i, p)| {
                        let name = p.clone();
                        NavItem::new(("project", i), p.clone())
                            .dot(PEOPLE[i % PEOPLE.len()].fg)
                            .active(current.as_ref() == Some(&p) && !self.settings_open)
                            .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                                cx.emit(SidebarEvent::Project(name.clone()))
                            }))
                    })),
            )
            .child(
                div()
                    .child(
                        NavItem::new("nav-shortcuts", "Shortcuts")
                            .icon("keyboard")
                            .active(shortcuts)
                            .child(kbd("?"))
                            .on_press(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                                this.shortcuts = !shortcuts;
                                cx.notify()
                            })),
                    )
                    .when(shortcuts, |d| d.child(self.shortcuts_card(people, cx))),
            )
            .child(self.footer(cx))
    }
}
