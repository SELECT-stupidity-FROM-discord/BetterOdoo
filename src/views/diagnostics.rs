use crate::components::{Button, Panel};
use crate::logging;
use crate::scale::px;
use crate::theme::*;
use gpui::{
    ClickEvent, ClipboardItem, Context, FontWeight, IntoElement, Render, SharedString, Window, div,
    prelude::*, rgb,
};
use std::path::PathBuf;

pub struct Diagnostics {
    notice: Option<SharedString>,
}

impl Diagnostics {
    pub fn new() -> Self {
        Self { notice: None }
    }

    fn copy(&mut self, text: String, cx: &mut Context<Self>) {
        let lines = text.lines().count();
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.notice =
            Some(format!("Copied {lines} lines. Paste them into your bug report.").into());
        cx.notify();
    }

    fn row(
        &self,
        id: &'static str,
        title: &'static str,
        note: impl Into<SharedString>,
        path: Option<PathBuf>,
        text: impl Fn() -> String + 'static,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let shown = path.clone();
        div()
            .flex()
            .items_center()
            .gap(px(12.))
            .px(px(18.))
            .py(px(14.))
            .border_b_1()
            .border_color(rgb(ROW_RULE))
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
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(MUTED))
                            .child(note.into()),
                    ),
            )
            .child(
                Button::new((id, 0usize), "Show file")
                    .disabled(path.is_none())
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                        if let Some(p) = &shown {
                            cx.reveal_path(p)
                        }
                    })),
            )
            .child(
                Button::new((id, 1usize), "Copy log").on_click(
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.copy(text(), cx)),
                ),
            )
    }
}

impl Render for Diagnostics {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let crash = logging::crash_files().into_iter().next();
        let crash_note = crash.as_ref().map(|p| {
            let name = p.file_stem().and_then(|n| n.to_str()).unwrap_or("");
            format!(
                "The app closed unexpectedly ({}). Share this report along with the session log.",
                name.trim_start_matches("crash-")
            )
        });
        let crash_path = crash.clone();
        Panel::new()
            .child(self.row(
                "diag-session",
                "Session log",
                "What happened in this session. Server address, emails, keys and tokens are removed automatically; skim it before sharing.",
                logging::session_path(),
                logging::session_text,
                cx,
            ))
            .children(crash_note.map(|note| {
                self.row(
                    "diag-crash",
                    "Crash report",
                    note,
                    crash,
                    move || {
                        crash_path
                            .as_deref()
                            .map(logging::read_crash)
                            .unwrap_or_default()
                    },
                    cx,
                )
            }))
            .children(self.notice.clone().map(|msg| {
                div()
                    .px(px(18.))
                    .py(px(10.))
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(SUCCESS))
                    .child(msg)
            }))
    }
}
