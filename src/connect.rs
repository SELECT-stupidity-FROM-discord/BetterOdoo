use crate::components::{icon, tag};
use crate::input::{InputEvent, TextInput};
use crate::odoo::{Auth, Login, Odoo, Probe};
use crate::theme::*;
use gpui::{
    BoxShadow, ClickEvent, Context, Div, Entity, EventEmitter, Focusable, FontWeight,
    HighlightStyle, SharedString, Stateful, StyledText, Window, div, point, prelude::*, px, rgb,
};
use std::{sync::Arc, time::Duration};

const FIELD_BORDER: u32 = 0xdcdcd9;
const RADIO_OFF: u32 = 0x9b9a97;

#[derive(Clone, Copy, PartialEq)]
enum Method {
    Browser,
    Password,
    ApiKey,
    Cookie,
}

const METHODS: [(Method, &str, &str); 4] = [
    (
        Method::Browser,
        "Continue in browser window",
        "Google, Microsoft, SSO, 2FA, or password. Works for everyone.",
    ),
    (
        Method::Password,
        "Email and Odoo password",
        "Only if you type a password into Odoo and have no 2FA.",
    ),
    (
        Method::ApiKey,
        "API key",
        "Odoo 19+. Doesn’t expire, good for always-on sync.",
    ),
    (
        Method::Cookie,
        "Paste a session cookie",
        "Works with any sign-in, including Google, Microsoft and 2FA.",
    ),
];

enum Check {
    Idle,
    Checking,
    Found(Probe),
    Failed(SharedString),
}

pub struct Connected {
    pub odoo: Arc<Odoo>,
    pub login: Login,
}

pub struct ConnectView {
    address: Entity<TextInput>,
    email: Entity<TextInput>,
    password: Entity<TextInput>,
    api_key: Entity<TextInput>,
    cookie: Entity<TextInput>,
    method: Method,
    check: Check,
    generation: usize,
    busy: bool,
    error: Option<SharedString>,
}

impl EventEmitter<Connected> for ConnectView {}

impl ConnectView {
    pub fn new(error: Option<String>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let address = cx.new(|cx| TextInput::new("yourco.odoo.com", cx));
        let email = cx.new(|cx| TextInput::new("you@yourco.com", cx));
        let password = cx.new(|cx| TextInput::new("Password", cx).masked());
        let api_key = cx.new(|cx| TextInput::new("Paste your API key", cx).masked());
        let cookie = cx.new(|cx| TextInput::new("Paste the session_id value", cx).masked());
        for input in [&address, &email, &password, &api_key, &cookie] {
            cx.subscribe_in(input, window, Self::on_input).detach();
        }
        window.focus(&address.focus_handle(cx), cx);
        Self {
            address,
            email,
            password,
            api_key,
            cookie,
            method: Method::Password,
            check: Check::Idle,
            generation: 0,
            busy: false,
            error: error.map(Into::into),
        }
    }

    fn on_input(
        &mut self,
        input: &Entity<TextInput>,
        ev: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match ev {
            InputEvent::Changed if input == &self.address => self.schedule_check(cx),
            InputEvent::Submit => self.submit(window, cx),
            _ => {}
        }
    }

    fn schedule_check(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        let generation = self.generation;
        let text = self.address.read(cx).text().trim().to_string();
        self.check = if text.is_empty() {
            Check::Idle
        } else {
            Check::Checking
        };
        cx.notify();
        if text.is_empty() {
            return;
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(500))
                .await;
            if this
                .read_with(cx, |this, _| this.generation != generation)
                .unwrap_or(true)
            {
                return;
            }
            let r = cx
                .background_executor()
                .spawn(async move { Odoo::probe(&text) })
                .await;
            this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.check = match r {
                    Ok(p) => {
                        if p.sso.is_some() && this.method == Method::Password {
                            this.method = Method::Cookie;
                        }
                        Check::Found(p)
                    }
                    Err(e) => Check::Failed(e.to_string().into()),
                };
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Check::Found(probe) = &self.check else {
            self.fail("Enter your Odoo address first.", cx);
            window.focus(&self.address.focus_handle(cx), cx);
            return;
        };
        let value = |input: &Entity<TextInput>| input.read(cx).text().trim().to_string();
        let auth = match self.method {
            Method::Browser => return,
            Method::Password => {
                let (login, password) = (value(&self.email), value(&self.password));
                if login.is_empty() || password.is_empty() {
                    return self.fail("Enter your email and password.", cx);
                }
                Auth::Password { login, password }
            }
            Method::ApiKey => match value(&self.api_key) {
                k if k.is_empty() => return self.fail("Paste your API key.", cx),
                k => Auth::ApiKey(k),
            },
            Method::Cookie => match value(&self.cookie) {
                c if c.is_empty() => return self.fail("Paste the session_id value.", cx),
                c => Auth::Session(c),
            },
        };
        let login = Login {
            url: probe.url.clone(),
            db: probe.db.clone(),
            auth,
        };
        self.busy = true;
        self.error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let attempt = login.clone();
            let r = cx
                .background_executor()
                .spawn(async move { Odoo::connect(&attempt) })
                .await;
            this.update(cx, |this, cx| {
                this.busy = false;
                match r {
                    Ok(odoo) => cx.emit(Connected {
                        odoo: Arc::new(odoo),
                        login,
                    }),
                    Err(e) => this.fail(&e.to_string(), cx),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn fail(&mut self, message: &str, cx: &mut Context<Self>) {
        self.error = Some(message.to_string().into());
        cx.notify();
    }

    fn field(
        &self,
        input: &Entity<TextInput>,
        height: f32,
        window: &Window,
        cx: &Context<Self>,
    ) -> Div {
        let focused = input.focus_handle(cx).is_focused(window);
        div()
            .flex()
            .items_center()
            .px(px(12.))
            .min_h(px(height))
            .rounded(px(6.))
            .border_1()
            .border_color(rgb(if focused { MUTED } else { FIELD_BORDER }))
            .bg(rgb(PAGE))
            .child(input.clone())
    }

    fn method_card(&self, i: usize, cx: &mut Context<Self>) -> Stateful<Div> {
        let (method, title, sub) = METHODS[i];
        let on = self.method == method;
        let disabled = method == Method::Browser;
        let radio = if on {
            div()
                .size(px(14.))
                .rounded_full()
                .bg(rgb(TEXT))
                .flex()
                .items_center()
                .justify_center()
                .child(div().size(px(5.)).rounded_full().bg(rgb(PAGE)))
        } else {
            div()
                .size(px(14.))
                .rounded_full()
                .border_1()
                .border_color(rgb(RADIO_OFF))
        };
        div()
            .id(("method", i))
            .flex()
            .items_start()
            .gap(px(12.))
            .px(px(16.))
            .py(px(14.))
            .rounded(px(8.))
            .bg(rgb(PAGE))
            .border_1()
            .border_color(rgb(if on { TEXT } else { BORDER }))
            .when(on, |d| {
                d.shadow(vec![BoxShadow {
                    color: rgb(TEXT).into(),
                    offset: point(px(0.), px(0.)),
                    blur_radius: px(0.),
                    spread_radius: px(1.),
                    inset: false,
                }])
            })
            .when(disabled, |d| d.opacity(0.55))
            .when(!disabled, |d| {
                d.cursor_pointer()
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.method = method;
                        this.error = None;
                        cx.notify()
                    }))
            })
            .child(div().flex_none().mt(px(3.)).child(radio))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title)
                            .when(disabled, |d| d.child(tag("Coming soon", GRAY, 11.))),
                    )
                    .child(div().text_size(px(13.)).text_color(rgb(MUTED)).child(sub)),
            )
    }

    fn form(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let label = |text: &'static str| {
            div()
                .text_size(px(13.))
                .font_weight(FontWeight::MEDIUM)
                .child(text)
        };
        let help = |text: &'static str| div().text_size(px(12.)).text_color(rgb(MUTED)).child(text);
        let mut form = div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .p(px(18.))
            .rounded(px(10.))
            .bg(rgb(PAGE))
            .border_1()
            .border_color(rgb(BORDER));
        let action = match self.method {
            Method::Browser => return form,
            Method::Password => {
                form = form
                    .child(label("Email"))
                    .child(self.field(&self.email, 40., window, cx))
                    .child(label("Odoo password"))
                    .child(self.field(&self.password, 40., window, cx))
                    .child(help("This doesn’t work if you sign in with Google or Microsoft, or if 2FA codes are on. Use a session cookie instead."));
                "Sign in"
            }
            Method::ApiKey => {
                form = form
                    .child(label("API key"))
                    .child(self.field(&self.api_key, 40., window, cx))
                    .child(help("Odoo 19 or newer. In Odoo: your avatar → Preferences → Account Security → New API key. Creating a key asks for your password, so SSO-only accounts may not be able to make one."));
                "Connect"
            }
            Method::Cookie => {
                let step3 = "Copy the value of session_id.";
                let at = step3.find("session_id").unwrap_or(0);
                form = form
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .text_size(px(13.))
                            .text_color(rgb(TEXT_SOFT))
                            .child("1. Sign in to Odoo in your browser.")
                            .child("2. Open developer tools → Application → Cookies.")
                            .child(div().flex().child("3. ").child(
                                StyledText::new(step3).with_highlights([(
                                    at..at + "session_id".len(),
                                    HighlightStyle {
                                        color: Some(rgb(TEXT).into()),
                                        font_weight: Some(FontWeight::SEMIBOLD),
                                        ..Default::default()
                                    },
                                )]),
                            )),
                    )
                    .child(label("session_id"))
                    .child(self.field(&self.cookie, 40., window, cx))
                    .child(help("Works with any sign-in. Lasts until you log out or it expires, then you paste a new one."));
                "Connect"
            }
        };
        form.when_some(self.error.clone(), |d, e| {
            d.child(div().text_size(px(13.)).text_color(rgb(DANGER)).child(e))
        })
        .child(
            div()
                .id("submit")
                .flex()
                .items_center()
                .justify_center()
                .px(px(16.))
                .py(px(10.))
                .min_h(px(42.))
                .rounded(px(6.))
                .bg(rgb(ACCENT))
                .text_color(rgb(PAGE))
                .font_weight(FontWeight::MEDIUM)
                .cursor_pointer()
                .when(self.busy, |d| d.opacity(0.7))
                .child(if self.busy { "Connecting…" } else { action })
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.submit(window, cx))),
        )
    }
}

impl Render for ConnectView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let status = match &self.check {
            Check::Idle => div(),
            Check::Checking => div().text_color(rgb(MUTED)).child("Checking…"),
            Check::Found(p) => div()
                .flex()
                .items_center()
                .gap(px(6.))
                .text_color(rgb(SUCCESS))
                .child(icon("check", 13., SUCCESS))
                .child(format!(
                    "Found {} · database “{}”{}",
                    p.version,
                    p.db,
                    p.sso
                        .map(|s| format!(" · {s} sign-in enabled"))
                        .unwrap_or_default()
                )),
            Check::Failed(e) => div().text_color(rgb(DANGER)).child(e.clone()),
        };
        let cards: Vec<_> = (0..METHODS.len())
            .map(|i| self.method_card(i, cx))
            .collect();
        div()
            .id("connect")
            .size_full()
            .overflow_y_scroll()
            .bg(rgb(SIDEBAR))
            .flex()
            .flex_col()
            .items_center()
            .px(px(24.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(22.))
                    .w_full()
                    .max_w(px(480.))
                    .py(px(64.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .child(icon("logo", 32., TEXT))
                            .child(
                                div()
                                    .mt(px(8.))
                                    .text_size(px(28.))
                                    .font_weight(FontWeight::BOLD)
                                    .child("Connect to Odoo"),
                            )
                            .child(div().text_color(rgb(MUTED)).line_height(px(21.)).child(
                                "Sign in the way you normally do. BetterOdoo only sees what your Odoo account can see.",
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child("Odoo address"),
                            )
                            .child(self.field(&self.address, 42., window, cx))
                            .child(status.text_size(px(12.))),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .children(cards),
                    )
                    .child(self.form(window, cx))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .text_size(px(12.))
                            .text_color(rgb(MUTED))
                            .child(icon("lock", 14., MUTED))
                            .child("Stored in your system keychain. Nothing leaves this computer except requests to your Odoo."),
                    ),
            )
    }
}
