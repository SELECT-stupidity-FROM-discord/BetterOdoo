mod ai;
mod assets;
mod components;
mod date;
mod format;
mod html;
mod input;
mod odoo;
mod ollama;
mod settings;
mod store;
mod theme;
mod tree;
mod views;

use gpui::{
    App, Bounds, Context, Entity, FocusHandle, KeyDownEvent, TitlebarOptions, Window, WindowBounds,
    WindowOptions, div, prelude::*, px, rgb, size,
};
use odoo::{Login, Odoo};
use std::sync::Arc;
use store::Store;
use theme::*;
use views::{
    connect::{ConnectView, Connected},
    settings::{SettingsEvent, SettingsView},
    sidebar::{Sidebar, SidebarEvent},
    task_list::{ListEvent, TaskList},
    task_page::{PageEvent, TaskPage},
};

const KEYCHAIN: &str = "BetterOdoo";

struct BetterOdoo {
    focus: FocusHandle,
    store: Entity<Store>,
    list: Entity<TaskList>,
    sidebar: Entity<Sidebar>,
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
        cx.subscribe_in(&list, window, Self::on_list).detach();
        cx.subscribe_in(&sidebar, window, Self::on_sidebar).detach();
        let mut me = Self {
            focus,
            store,
            list,
            sidebar,
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
        cx.delete_credentials(KEYCHAIN).detach();
        self.store.update(cx, |s, cx| s.disconnect(cx));
        self.list.update(cx, |l, _| l.reset());
        self.page = None;
        self.close_settings(window, cx);
        self.show_connect(None, window, cx);
    }

    fn project_label(&self, cx: &App) -> gpui::SharedString {
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
            .update(cx, |s, cx| s.set_settings_open(true, cx));
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.take().is_some() {
            self.sidebar
                .update(cx, |s, cx| s.set_settings_open(false, cx));
            window.focus(&self.focus, cx);
            cx.notify();
        }
    }

    fn show_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.page = None;
        self.close_settings(window, cx);
        cx.notify();
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
            SidebarEvent::Search => {
                self.show_list(window, cx);
                self.list.update(cx, |l, cx| l.focus_filter(window, cx));
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
            && (self.sidebar.update(cx, |s, cx| s.dismiss_shortcuts(cx))
                || self.list.update(cx, |l, cx| l.dismiss_menu(cx)))
        {
            return;
        }
        let Some(page) = self.page.clone() else {
            if key == "a" {
                if let Some(id) = self.list.read(cx).selected_id(cx) {
                    self.open_page(id, window, cx);
                    if let Some(page) = self.page.clone() {
                        page.update(cx, |p, cx| p.handle_key(ev, window, cx));
                    }
                }
            } else {
                self.list.update(cx, |l, cx| l.handle_key(ev, window, cx));
            }
            return;
        };
        if page.update(cx, |p, cx| p.handle_key(ev, window, cx)) {
            return;
        }
        if matches!(key, "j" | "k" | "up" | "down") {
            self.list.update(cx, |l, cx| l.handle_key(ev, window, cx));
            if let Some(id) = self.list.read(cx).selected_id(cx) {
                self.open_page(id, window, cx);
            }
        }
    }
}

impl Render for BetterOdoo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let base = div()
            .size_full()
            .text_color(rgb(TEXT))
            .font_family(FONT)
            .text_size(px(14.));
        if let Some(connect) = &self.connect {
            return base.child(connect.clone());
        }
        let content = match (&self.settings, &self.page) {
            (Some(s), _) => s.clone().into_any_element(),
            (None, Some(p)) => p.clone().into_any_element(),
            (None, None) => self.list.clone().into_any_element(),
        };
        base.track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .flex()
            .bg(rgb(PAGE))
            .child(self.sidebar.clone())
            .child(content)
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
