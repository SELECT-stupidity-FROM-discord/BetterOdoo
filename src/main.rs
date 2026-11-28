#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod ai;
mod app_icon;
mod assets;
mod browser_login;
mod components;
mod date;
mod format;
mod html;
mod input;
mod logging;
mod odoo;
mod ollama;
mod scale;
mod settings;
mod store;
mod theme;
mod tree;
mod update;
mod views;

use gpui::{
    App, Bounds, Context, Entity, FocusHandle, KeyBinding, KeyDownEvent, TitlebarOptions, Window,
    WindowBounds, WindowOptions, actions, div, prelude::*, rgb, size,
};
use odoo::{Login, Odoo};
use scale::px;
use std::sync::Arc;
use store::Store;
use theme::*;
use views::{
    connect::{ConnectView, Connected},
    employees::{EmployeesEvent, EmployeesView},
    new_task::NewTask,
    settings::{SettingsEvent, SettingsView},
    sidebar::{Section, Sidebar, SidebarEvent},
    task_list::{ListEvent, TaskList},
    task_page::{PageEvent, TaskPage},
    timesheets::{TimesheetsEvent, TimesheetsView},
    titlebar::wrap_window_chrome,
    todos::{TodosEvent, TodosView},
};

actions!(
    better_odoo,
    [
        ZoomIn,
        ZoomOut,
        ZoomReset,
        HideWindow,
        MinimizeWindow,
        ToggleFullscreen,
        QuitApp
    ]
);

fn zoom(next: impl Fn(f32) -> f32, cx: &mut App) {
    let v = scale::set(next(scale::get()));
    settings::set_zoom(v);
    for window in cx.windows() {
        window
            .update(cx, |_, window, _| {
                window.set_rem_size(gpui::px(16. * v));
                window.refresh();
            })
            .ok();
    }
}

macro_rules! key {
    ($k:literal) => {
        if cfg!(target_os = "macos") {
            concat!("cmd-", $k)
        } else {
            concat!("ctrl-", $k)
        }
    };
}

fn bind_zoom(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new(key!("="), ZoomIn, None),
        KeyBinding::new(key!("+"), ZoomIn, None),
        KeyBinding::new(key!("shift-="), ZoomIn, None),
        KeyBinding::new(key!("-"), ZoomOut, None),
        KeyBinding::new(key!("0"), ZoomReset, None),
    ]);
    cx.on_action(|_: &ZoomIn, cx| zoom(|v| v + 0.1, cx));
    cx.on_action(|_: &ZoomOut, cx| zoom(|v| v - 0.1, cx));
    cx.on_action(|_: &ZoomReset, cx| zoom(|_| 1.0, cx));
}

fn bind_window_actions(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-h", HideWindow, None),
        KeyBinding::new("ctrl-m", MinimizeWindow, None),
        KeyBinding::new("f11", ToggleFullscreen, None),
        KeyBinding::new("ctrl-q", QuitApp, None),
    ]);
    cx.on_action(|_: &HideWindow, cx| {
        if let Some(win) = cx.active_window() {
            win.update(cx, |_, window, _| window.minimize_window()).ok();
        } else {
            for win in cx.windows() {
                win.update(cx, |_, window, _| window.minimize_window()).ok();
            }
        }
        cx.hide();
    });
    cx.on_action(|_: &MinimizeWindow, cx| {
        if let Some(win) = cx.active_window() {
            win.update(cx, |_, window, _| window.minimize_window()).ok();
        }
    });
    cx.on_action(|_: &ToggleFullscreen, cx| {
        if let Some(win) = cx.active_window() {
            win.update(cx, |_, window, _| window.toggle_fullscreen())
                .ok();
        }
    });
    cx.on_action(|_: &QuitApp, cx| cx.quit());
}

const KEYCHAIN: &str = "BetterOdoo";

struct BetterOdoo {
    focus: FocusHandle,
    store: Entity<Store>,
    list: Entity<TaskList>,
    sidebar: Entity<Sidebar>,
    new_task: Entity<NewTask>,
    section: Section,
    todos: Option<Entity<TodosView>>,
    timesheets: Option<Entity<TimesheetsView>>,
    employees: Option<Entity<EmployeesView>>,
    page: Option<Entity<TaskPage>>,
    settings: Option<Entity<SettingsView>>,
    connect: Option<Entity<ConnectView>>,
}

impl BetterOdoo {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        let store = cx.new(|_| Store::new());
        let list = cx.new(|cx| TaskList::new(store.clone(), focus.clone(), window, cx));
        let sidebar = cx.new(|cx| Sidebar::new(store.clone(), list.clone(), cx));
        let new_task = cx.new(|cx| NewTask::new(store.clone(), focus.clone(), window, cx));
        cx.subscribe_in(&list, window, Self::on_list).detach();
        cx.subscribe_in(&sidebar, window, Self::on_sidebar).detach();
        if !cfg!(target_os = "macos") {
            cx.observe_window_activation(window, |_, _, cx| cx.notify())
                .detach();
            cx.observe_window_bounds(window, |_, _, cx| cx.notify())
                .detach();
        }
        let mut me = Self {
            focus,
            store,
            list,
            sidebar,
            new_task,
            section: Section::Tasks,
            todos: None,
            timesheets: None,
            employees: None,
            page: None,
            settings: None,
            connect: None,
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
        self.connect = None;
        self.store.update(cx, |s, cx| s.connect(odoo, cx));
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn sign_out(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        log::info!("signed out");
        cx.delete_credentials(KEYCHAIN).detach();
        self.store.update(cx, |s, cx| s.disconnect(cx));
        self.list.update(cx, |l, _| l.reset());
        self.todos = None;
        self.timesheets = None;
        self.employees = None;
        self.section = Section::Tasks;
        self.sidebar
            .update(cx, |s, cx| s.set_section(Section::Tasks, cx));
        self.page = None;
        self.close_settings(window, cx);
        self.show_connect(None, window, cx);
    }

    fn project_label(&self, cx: &App) -> gpui::SharedString {
        match self.section {
            Section::Todos => return "To-Dos".into(),
            Section::Timesheets => return "Timesheets".into(),
            Section::Employees => return "Employees".into(),
            _ => {}
        }
        self.list
            .read(cx)
            .project()
            .cloned()
            .unwrap_or("My Tasks".into())
            .into()
    }

    fn open_page(&mut self, id: i64, window: &mut Window, cx: &mut Context<Self>) {
        self.close_settings(window, cx);
        let page = match &self.page {
            Some(p) => p.clone(),
            None => {
                let (store, focus) = (self.store.clone(), self.focus.clone());
                let p = cx.new(|cx| TaskPage::new(store, focus, window, cx));
                cx.subscribe_in(&p, window, Self::on_page).detach();
                self.page = Some(p.clone());
                p
            }
        };
        let label = self.project_label(cx);
        page.update(cx, |p, cx| p.open(id, label, window, cx));
        self.list.update(cx, |l, cx| l.select(id, cx));
        cx.notify();
    }

    fn close_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.page = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let store = self.store.read(cx);
        let (workspace, version) = (store.workspace.clone(), store.version.clone());
        let view = cx.new(|cx| SettingsView::new(workspace, version, window, cx));
        cx.subscribe_in(&view, window, Self::on_settings).detach();
        self.settings = Some(view);
        self.sidebar
            .update(cx, |s, cx| s.set_section(Section::Settings, cx));
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.take().is_some() {
            let section = self.section;
            self.sidebar.update(cx, |s, cx| s.set_section(section, cx));
            window.focus(&self.focus, cx);
            cx.notify();
        }
    }

    fn show_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.page = None;
        self.section = Section::Tasks;
        self.sidebar
            .update(cx, |s, cx| s.set_section(Section::Tasks, cx));
        self.close_settings(window, cx);
        cx.notify();
    }

    fn open_todos(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.page = None;
        self.section = Section::Todos;
        self.close_settings(window, cx);
        if self.todos.is_none() {
            let (store, focus) = (self.store.clone(), self.focus.clone());
            let view = cx.new(|cx| TodosView::new(store, focus, window, cx));
            cx.subscribe_in(&view, window, Self::on_todos).detach();
            self.todos = Some(view);
        }
        self.sidebar
            .update(cx, |s, cx| s.set_section(Section::Todos, cx));
        self.store.update(cx, |s, cx| s.ensure_todos(cx));
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn open_timesheets(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.page = None;
        self.section = Section::Timesheets;
        self.close_settings(window, cx);
        if self.timesheets.is_none() {
            let (store, focus) = (self.store.clone(), self.focus.clone());
            let view = cx.new(|cx| TimesheetsView::new(store, focus, window, cx));
            cx.subscribe_in(&view, window, Self::on_timesheets).detach();
            self.timesheets = Some(view);
        }
        self.sidebar
            .update(cx, |s, cx| s.set_section(Section::Timesheets, cx));
        self.store.update(cx, |s, cx| s.ensure_timesheets(cx));
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn open_employees(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.page = None;
        self.section = Section::Employees;
        self.close_settings(window, cx);
        if self.employees.is_none() {
            let (store, focus) = (self.store.clone(), self.focus.clone());
            let view = cx.new(|cx| EmployeesView::new(store, focus, window, cx));
            cx.subscribe_in(&view, window, Self::on_employees).detach();
            self.employees = Some(view);
        }
        self.sidebar
            .update(cx, |s, cx| s.set_section(Section::Employees, cx));
        self.store.update(cx, |s, cx| s.ensure_employees(cx));
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn on_todos(
        &mut self,
        _: &Entity<TodosView>,
        ev: &TodosEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let TodosEvent::OpenTask(id) = ev;
        self.open_page(*id, window, cx);
    }

    fn on_timesheets(
        &mut self,
        _: &Entity<TimesheetsView>,
        ev: &TimesheetsEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let TimesheetsEvent::OpenTask(id) = ev;
        self.open_page(*id, window, cx);
    }

    fn on_employees(
        &mut self,
        _: &Entity<EmployeesView>,
        ev: &EmployeesEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let EmployeesEvent::FilterTasks(who) = ev;
        self.list
            .update(cx, |l, cx| l.set_assignee_filter(Some(who.clone()), cx));
        self.show_list(window, cx);
    }

    fn on_list(
        &mut self,
        _: &Entity<TaskList>,
        ev: &ListEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ListEvent::Open(id) = ev;
        self.open_page(*id, window, cx);
    }

    fn on_page(
        &mut self,
        _: &Entity<TaskPage>,
        ev: &PageEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let PageEvent::Close = ev;
        self.close_page(window, cx);
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

    fn on_sidebar(
        &mut self,
        _: &Entity<Sidebar>,
        ev: &SidebarEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match ev {
            SidebarEvent::Tasks => {
                self.list.update(cx, |l, cx| l.set_project(None, cx));
                self.show_list(window, cx);
            }
            SidebarEvent::Project(name) => {
                self.list
                    .update(cx, |l, cx| l.set_project(Some(name.clone()), cx));
                self.show_list(window, cx);
            }
            SidebarEvent::Todos => self.open_todos(window, cx),
            SidebarEvent::Timesheets => self.open_timesheets(window, cx),
            SidebarEvent::Employees => self.open_employees(window, cx),
            SidebarEvent::Search => {
                if self.page.is_none() {
                    self.close_settings(window, cx);
                    match self.section {
                        Section::Todos => {
                            if let Some(v) = &self.todos {
                                v.update(cx, |v, cx| v.focus_filter(window, cx));
                            }
                        }
                        Section::Timesheets => {
                            if let Some(v) = &self.timesheets {
                                v.update(cx, |v, cx| v.focus_filter(window, cx));
                            }
                        }
                        Section::Employees => {
                            if let Some(v) = &self.employees {
                                v.update(cx, |v, cx| v.focus_filter(window, cx));
                            }
                        }
                        _ => {
                            self.show_list(window, cx);
                            self.list.update(cx, |l, cx| l.focus_filter(window, cx));
                        }
                    }
                } else {
                    self.show_list(window, cx);
                    self.list.update(cx, |l, cx| l.focus_filter(window, cx));
                }
            }
            SidebarEvent::Settings if self.settings.is_some() => self.close_settings(window, cx),
            SidebarEvent::Settings => self.open_settings(window, cx),
        }
    }

    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.focus.is_focused(window) {
            return;
        }
        let key = ev.keystroke.key.as_str();
        if self.settings.is_some() {
            if key == "escape" {
                self.close_settings(window, cx);
            }
            return;
        }
        if ev.keystroke.key_char.as_deref() == Some("?") {
            self.sidebar.update(cx, |s, cx| s.toggle_shortcuts(cx));
            return;
        }
        if key == "escape"
            && (self.new_task.update(cx, |n, cx| n.dismiss(cx))
                || self.sidebar.update(cx, |s, cx| s.dismiss_shortcuts(cx))
                || self
                    .todos
                    .as_ref()
                    .is_some_and(|t| t.update(cx, |t, cx| t.dismiss_menu(cx)))
                || self
                    .timesheets
                    .as_ref()
                    .is_some_and(|t| t.update(cx, |t, cx| t.dismiss_menu(cx)))
                || self
                    .employees
                    .as_ref()
                    .is_some_and(|e| e.update(cx, |e, cx| e.dismiss_menu(cx)))
                || self.list.update(cx, |l, cx| l.dismiss_menu(cx)))
        {
            return;
        }
        let Some(page) = self.page.clone() else {
            match self.section {
                Section::Todos => {
                    if let Some(v) = &self.todos {
                        v.update(cx, |v, cx| v.handle_key(ev, window, cx));
                    }
                }
                Section::Timesheets => {
                    if let Some(v) = &self.timesheets {
                        v.update(cx, |v, cx| v.handle_key(ev, window, cx));
                    }
                }
                Section::Employees => {
                    if let Some(v) = &self.employees {
                        v.update(cx, |v, cx| v.handle_key(ev, window, cx));
                    }
                }
                _ if key == "a" => {
                    if let Some(id) = self.list.read(cx).selected_id(cx) {
                        self.open_page(id, window, cx);
                        if let Some(page) = self.page.clone() {
                            page.update(cx, |p, cx| p.handle_key(ev, window, cx));
                        }
                    }
                }
                _ => {
                    self.list.update(cx, |l, cx| l.handle_key(ev, window, cx));
                }
            }
            return;
        };
        if page.update(cx, |p, cx| p.handle_key(ev, window, cx)) {
            return;
        }
        if self.section == Section::Tasks && matches!(key, "j" | "k" | "up" | "down") {
            self.list.update(cx, |l, cx| l.handle_key(ev, window, cx));
            if let Some(id) = self.list.read(cx).selected_id(cx) {
                self.open_page(id, window, cx);
            }
        }
    }
}

impl Render for BetterOdoo {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let base = div()
            .size_full()
            .text_color(rgb(TEXT))
            .font_family(FONT)
            .text_size(px(14.));
        if let Some(connect) = &self.connect {
            let body = base.child(connect.clone()).into_any_element();
            return wrap_window_chrome(None, body, window);
        }
        let workspace = self.store.read(cx).workspace.clone();
        let content = match (&self.settings, &self.page) {
            (Some(s), _) => s.clone().into_any_element(),
            (None, Some(p)) => p.clone().into_any_element(),
            (None, None) => match self.section {
                Section::Todos => self.todos.clone().map_or_else(
                    || self.list.clone().into_any_element(),
                    IntoElement::into_any_element,
                ),
                Section::Timesheets => self.timesheets.clone().map_or_else(
                    || self.list.clone().into_any_element(),
                    IntoElement::into_any_element,
                ),
                Section::Employees => self.employees.clone().map_or_else(
                    || self.list.clone().into_any_element(),
                    IntoElement::into_any_element,
                ),
                _ => self.list.clone().into_any_element(),
            },
        };
        let body = base
            .relative()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .flex()
            .bg(rgb(PAGE))
            .child(self.sidebar.clone())
            .child(content)
            .when(self.settings.is_none(), |d| d.child(self.new_task.clone()))
            .into_any_element();
        wrap_window_chrome(Some(workspace), body, window)
    }
}

fn main() {
    logging::init();
    gpui_platform::application()
        .with_assets(assets::Assets)
        .run(|cx: &mut App| {
            assets::load_fonts(cx).expect("bundled fonts load");
            app_icon::set();
            input::bind_keys(cx);
            bind_zoom(cx);
            if !cfg!(target_os = "macos") {
                bind_window_actions(cx);
                cx.on_window_closed(|cx, _| {
                    if cx.windows().is_empty() {
                        cx.quit();
                    }
                })
                .detach();
            }
            scale::set(settings::zoom().unwrap_or(1.0));
            let bounds = Bounds::centered(None, size(px(1440.), px(900.)), cx);
            let custom_chrome = !cfg!(target_os = "macos");
            let window_options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("BetterOdoo".into()),
                    appears_transparent: custom_chrome,
                    ..Default::default()
                }),
                window_decorations: custom_chrome.then_some(gpui::WindowDecorations::Client),
                app_id: Some("better-odoo".into()),
                icon: app_icon::window(),
                ..Default::default()
            };
            cx.open_window(window_options, |window, cx| {
                let view = cx.new(|cx| BetterOdoo::new(window, cx));
                let focus = view.read(cx).focus.clone();
                window.focus(&focus, cx);
                view
            })
            .unwrap();
            cx.activate(true);
        });
}
