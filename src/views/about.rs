use crate::components::{Button, Panel, Variant, icon};
use crate::scale::px;
use crate::theme::*;
use crate::update::{self, Check};
use gpui::{
    ClickEvent, Context, FontWeight, IntoElement, ObjectFit, Render, SharedString, Window, div,
    img, prelude::*, rgb,
};

enum Status {
    Idle,
    Checking,
    Done(Check),
    Failed(SharedString),
}

pub struct About {
    status: Status,
}

impl About {
    pub fn new() -> Self {
        Self {
            status: Status::Idle,
        }
    }

    fn check(&mut self, cx: &mut Context<Self>) {
        if matches!(self.status, Status::Checking) {
            return;
        }
        self.status = Status::Checking;
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async { update::latest() })
                .await;
            this.update(cx, |this, cx| {
                this.status = match r {
                    Ok(c) => Status::Done(c),
                    Err(_) => {
                        Status::Failed("Couldn’t reach GitHub. Check your connection.".into())
                    }
                };
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn message(&self) -> (SharedString, u32) {
        match &self.status {
            Status::Idle => ("Check whether a newer release is available.".into(), MUTED),
            Status::Checking => ("Checking for updates…".into(), MUTED),
            Status::Failed(msg) => (msg.clone(), DANGER),
            Status::Done(Check::UpToDate) => ("You’re on the latest version.".into(), SUCCESS),
            Status::Done(Check::NoReleases) => {
                ("No releases have been published yet.".into(), MUTED)
            }
            Status::Done(Check::Available { tag, .. }) => {
                (format!("{tag} is available.").into(), ACCENT)
            }
        }
    }

    fn header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap(px(16.))
            .px(px(18.))
            .py(px(16.))
            .child(
                img("logo.png")
                    .size(px(56.))
                    .flex_none()
                    .rounded(px(13.))
                    .object_fit(ObjectFit::Cover),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(TEXT))
                            .child(update::NAME),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(MUTED))
                            .child(format!("Version {}", update::VERSION)),
                    ),
            )
            .child(
                div()
                    .id("about-github")
                    .flex_none()
                    .p(px(6.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .text_color(rgb(MUTED))
                    .hover(|s| s.bg(rgb(HOVER)).text_color(rgb(TEXT)))
                    .child(icon("github", 20., MUTED))
                    .on_click(
                        cx.listener(|_, _: &ClickEvent, _, cx| cx.open_url(update::REPO_URL)),
                    ),
            )
    }

    fn updates(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (text, color) = self.message();
        let release = match &self.status {
            Status::Done(Check::Available { url, .. }) => Some(url.clone()),
            _ => None,
        };
        div()
            .flex()
            .items_center()
            .gap(px(12.))
            .px(px(18.))
            .py(px(12.))
            .border_t_1()
            .border_color(rgb(ROW_RULE))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(13.))
                    .text_color(rgb(color))
                    .child(text),
            )
            .children(release.map(|url| {
                Button::new("about-release", "View release")
                    .variant(Variant::Primary)
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.open_url(&url)))
            }))
            .child(
                Button::new("about-check", "Check for updates")
                    .disabled(matches!(self.status, Status::Checking))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.check(cx))),
            )
    }
}

impl Render for About {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        Panel::new().child(self.header(cx)).child(self.updates(cx))
    }
}
