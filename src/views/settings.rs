use crate::components::{Button, Card, Field, Panel, Radio, Section, Tint, Variant, icon, tag};
use crate::input::{InputEvent, TextInput};
use crate::ollama::{self, Model};
use crate::settings;
use crate::theme::*;
use gpui::{
    ClickEvent, Context, Entity, EventEmitter, FontWeight, SharedString, Window, div, prelude::*,
    px, rgb,
};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

const SUGGESTED: &[(&str, &str)] = &[
    ("qwen2.5:3b", "1.9 GB · quick, fine for short logs"),
    (
        "qwen2.5:7b",
        "4.7 GB · recommended, accurate and still fast",
    ),
    ("qwen2.5:14b", "9.0 GB · most accurate, wants 16 GB of RAM"),
    ("llama3.2:3b", "2.0 GB · small and light"),
    ("gemma3:4b", "3.3 GB · compact, good at following format"),
];

pub enum SettingsEvent {
    Close,
    SignOut,
}

enum Library {
    Loading,
    Ready(Vec<Model>),
    Unreachable(SharedString),
}

#[derive(Default)]
struct Pull {
    name: String,
    status: String,
    fraction: f32,
    result: Option<Result<(), String>>,
}

pub struct SettingsView {
    workspace: SharedString,
    server: SharedString,
    library: Library,
    active: String,
    custom: Entity<TextInput>,
    pulling: Option<(String, String, f32)>,
    notice: Option<(SharedString, bool)>,
}

impl EventEmitter<SettingsEvent> for SettingsView {}

impl SettingsView {
    pub fn new(
        workspace: SharedString,
        server: SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let custom = cx.new(|cx| TextInput::new("Any Ollama model, e.g. mistral:7b", cx));
        cx.subscribe_in(&custom, window, Self::on_custom).detach();
        let mut view = Self {
            workspace,
            server,
            library: Library::Loading,
            active: crate::ai::model(),
            custom,
            pulling: None,
            notice: None,
        };
        view.refresh(cx);
        view
    }

    fn on_custom(
        &mut self,
        _: &Entity<TextInput>,
        ev: &InputEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match ev {
            InputEvent::Submit => {
                let name = self.custom.read(cx).text().trim().to_string();
                self.download(name, cx);
            }
            InputEvent::Cancel => cx.emit(SettingsEvent::Close),
            InputEvent::Changed => {}
        }
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async { ollama::installed() })
                .await;
            this.update(cx, |this, cx| {
                this.library = match r {
                    Ok(m) => Library::Ready(m),
                    Err(_) => Library::Unreachable(
                        format!("Can’t reach Ollama at {}", ollama::root()).into(),
                    ),
                };
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn choose(&mut self, name: &str, cx: &mut Context<Self>) {
        settings::set_model(name);
        self.active = name.to_string();
        cx.notify();
    }

    fn download(&mut self, name: String, cx: &mut Context<Self>) {
        if name.is_empty() || self.pulling.is_some() {
            return;
        }
        self.notice = None;
        self.pulling = Some((name.clone(), "Starting".into(), 0.));
        let shared = Arc::new(Mutex::new(Pull {
            name: name.clone(),
            ..Pull::default()
        }));
        let worker = shared.clone();
        std::thread::spawn(move || {
            let name = worker.lock().unwrap().name.clone();
            let r = ollama::pull(&name, |status, fraction| {
                let mut p = worker.lock().unwrap();
                p.status = status.to_string();
                p.fraction = fraction;
            });
            worker.lock().unwrap().result = Some(r.map_err(|e| e.to_string()));
        });
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(200))
                    .await;
                let (name, status, fraction, result) = {
                    let p = shared.lock().unwrap();
                    (
                        p.name.clone(),
                        p.status.clone(),
                        p.fraction,
                        p.result.clone(),
                    )
                };
                let finished = result.is_some();
                let alive = this
                    .update(cx, |this, cx| {
                        this.pulling = Some((name.clone(), status, fraction));
                        if let Some(r) = result {
                            this.pulling = None;
                            match r {
                                Ok(()) => {
                                    this.choose(&name, cx);
                                    this.notice = Some((
                                        format!("{name} is ready and now active").into(),
                                        false,
                                    ));
                                    this.refresh(cx);
                                }
                                Err(e) => this.notice = Some((e.into(), true)),
                            }
                        }
                        cx.notify();
                    })
                    .is_ok();
                if finished || !alive {
                    break;
                }
            }
        })
        .detach();
    }

    fn account(&self, cx: &mut Context<Self>) -> Panel {
        let row = |label: &'static str, value: SharedString| {
            div()
                .flex()
                .items_center()
                .justify_between()
                .px(px(14.))
                .py(px(10.))
                .border_b_1()
                .border_color(rgb(ROW_RULE))
                .child(div().text_color(rgb(MUTED)).child(label))
                .child(value)
        };
        Panel::new()
            .child(row("Workspace", self.workspace.clone()))
            .child(row("Server", self.server.clone()))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .px(px(14.))
                    .py(px(10.))
                    .child(
                        div()
                            .text_color(rgb(MUTED))
                            .child("Forget this login and return to the Connect screen"),
                    )
                    .child(
                        Button::new("sign-out", "Sign out")
                            .variant(Variant::Danger)
                            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                                cx.emit(SettingsEvent::SignOut)
                            })),
                    ),
            )
    }

    fn installed(&self, cx: &mut Context<Self>) -> Panel {
        let note = |text: &'static str| div().p(px(14.)).text_color(rgb(MUTED)).child(text);
        match &self.library {
            Library::Loading => Panel::new().child(note("Looking for models…")),
            Library::Unreachable(msg) => Panel::new().child(
                div()
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap(px(8.))
                    .p(px(14.))
                    .child(div().text_color(rgb(DANGER)).child(msg.clone()))
                    .child(
                        div()
                            .text_color(rgb(MUTED))
                            .child("Install Ollama from ollama.com and start it, then try again."),
                    )
                    .child(Button::new("retry", "Try again").on_click(cx.listener(
                        |this, _: &ClickEvent, _, cx| {
                            this.library = Library::Loading;
                            this.refresh(cx);
                            cx.notify()
                        },
                    ))),
            ),
            Library::Ready(models) if models.is_empty() => {
                Panel::new().child(note("No models downloaded yet. Pick one below."))
            }
            Library::Ready(models) => {
                let rows = models.iter().enumerate().map(|(i, m)| {
                    let on = m.name == self.active;
                    let name = m.name.clone();
                    div()
                        .id(("model", i))
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .px(px(14.))
                        .py(px(10.))
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(HOVER)))
                        .when(i + 1 < models.len(), |d| {
                            d.border_b_1().border_color(rgb(ROW_RULE))
                        })
                        .child(Radio::new(on))
                        .child(div().flex_1().min_w_0().truncate().child(m.name.clone()))
                        .when(on, |d| d.child(tag("Active", PURPLE, 11.)))
                        .child(
                            div()
                                .flex_none()
                                .text_size(px(12.))
                                .text_color(rgb(MUTED))
                                .child(ollama::size_label(m.size)),
                        )
                        .on_click(
                            cx.listener(move |this, _: &ClickEvent, _, cx| this.choose(&name, cx)),
                        )
                });
                Panel::new().children(rows.collect::<Vec<_>>())
            }
        }
    }

    fn catalogue(&self, cx: &mut Context<Self>) -> Panel {
        let have: Vec<&str> = match &self.library {
            Library::Ready(m) => m.iter().map(|m| m.name.as_str()).collect(),
            _ => Vec::new(),
        };
        let busy = self.pulling.is_some();
        let rows = SUGGESTED.iter().enumerate().map(|(i, (name, note))| {
            let owned = have
                .iter()
                .any(|h| h == name || h.strip_suffix(":latest") == Some(name));
            let progress = self.pulling.as_ref().filter(|(n, _, _)| n == name);
            let target = name.to_string();
            let action = match (owned, progress) {
                (_, Some((_, _, f))) => div()
                    .text_size(px(12.))
                    .text_color(rgb(PURPLE.fg))
                    .child(format!("{}%", (f * 100.) as u32)),
                (true, _) => div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .text_size(px(12.))
                    .text_color(rgb(SUCCESS))
                    .child(icon("check", 12., SUCCESS))
                    .child("Installed"),
                (false, _) => {
                    div().child(Button::new(("get", i), "Download").disabled(busy).on_click(
                        cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.download(target.clone(), cx)
                        }),
                    ))
                }
            };
            div()
                .flex()
                .items_center()
                .gap(px(12.))
                .px(px(14.))
                .py(px(10.))
                .border_b_1()
                .border_color(rgb(ROW_RULE))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(div().child(name.to_string()))
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(rgb(MUTED))
                                .child(note.to_string()),
                        ),
                )
                .child(action)
        });
        Panel::new()
            .children(rows.collect::<Vec<_>>())
            .child(self.custom_row(busy, cx))
    }

    fn custom_row(&self, busy: bool, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap(px(10.))
            .px(px(14.))
            .py(px(10.))
            .child(div().flex_1().child(Field::new(&self.custom)))
            .child(
                Button::new("get-custom", "Download")
                    .variant(Variant::Primary)
                    .disabled(busy)
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        let name = this.custom.read(cx).text().trim().to_string();
                        this.download(name, cx)
                    })),
            )
    }

    fn progress(&self) -> Option<Card> {
        let (name, status, fraction) = self.pulling.as_ref()?;
        let percent = format!("{}%", (fraction * 100.) as u32);
        Some(
            Card::new(format!("Downloading {name} · {percent}"))
                .tint(Tint::Ai)
                .child(
                    div().h(px(6.)).rounded_full().bg(rgb(AI_BORDER)).child(
                        div()
                            .h_full()
                            .rounded_full()
                            .bg(rgb(PURPLE.fg))
                            .w(gpui::relative(fraction.clamp(0.02, 1.))),
                    ),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(rgb(MUTED))
                        .child(status.clone()),
                ),
        )
    }
}

impl Render for SettingsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let notice = self.notice.clone().map(|(msg, bad)| {
            div()
                .text_size(px(13.))
                .text_color(rgb(if bad { DANGER } else { SUCCESS }))
                .child(msg)
        });
        div()
            .id("settings")
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .items_center()
            .px(px(48.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .max_w(px(760.))
                    .pt(px(40.))
                    .pb(px(64.))
                    .gap(px(24.))
                    .child(
                        div()
                            .flex()
                            .gap(px(6.))
                            .text_size(px(12.))
                            .text_color(rgb(MUTED))
                            .child(
                                div()
                                    .id("back")
                                    .cursor_pointer()
                                    .hover(|s| s.text_color(rgb(TEXT)))
                                    .child("Tasks")
                                    .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                                        cx.emit(SettingsEvent::Close)
                                    })),
                            )
                            .child("/ Settings"),
                    )
                    .child(
                        div()
                            .text_size(px(32.))
                            .font_weight(FontWeight::BOLD)
                            .line_height(px(40.))
                            .child("Settings"),
                    )
                    .child(
                        Section::new("Account", "The Odoo workspace you’re signed in to.")
                            .child(self.account(cx)),
                    )
                    .child(
                        Section::new(
                            "AI model",
                            "Autofill with AI runs on your computer through Ollama. Choose which model it uses.",
                        )
                        .child(self.installed(cx)),
                    )
                    .child(
                        Section::new(
                            "Download a model",
                            "Bigger models read messy notes better but need more memory and time.",
                        )
                        .children(self.progress())
                        .children(notice)
                        .child(self.catalogue(cx)),
                    ),
            )
    }
}
