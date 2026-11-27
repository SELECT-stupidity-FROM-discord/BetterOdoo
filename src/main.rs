mod ai;
mod odoo;

use gpui::{
    App, Bounds, Context, FocusHandle, KeyDownEvent, SharedString, Window, WindowBounds,
    WindowOptions, div, prelude::*, px, rgb, size,
};
use odoo::{Odoo, Task};
use serde_json::json;
use std::sync::Arc;

const BG: u32 = 0xffffff;
const PANEL: u32 = 0xf7f7f5;
const LINE: u32 = 0xe9e9e7;
const SEL: u32 = 0xefefed;
const TEXT: u32 = 0x37352f;
const MUTED: u32 = 0x6f6e69;
const ACCENT: u32 = 0x1f74c8;
const HOT: u32 = 0xa8550b;

struct BetterOdoo {
    focus: FocusHandle,
    odoo: Option<Arc<Odoo>>,
    tasks: Vec<Task>,
    assignee: Option<String>,
    selected: usize,
    status: SharedString,
    proposed: Vec<String>,
}

impl BetterOdoo {
    fn new(cx: &mut Context<Self>) -> Self {
        let me = Self {
            focus: cx.focus_handle(),
            odoo: None,
            tasks: vec![],
            assignee: None,
            selected: 0,
            status: "Connecting…".into(),
            proposed: vec![],
        };
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async {
                    let o = Odoo::from_env()?;
                    let tasks = o.tasks(json!([["stage_id.fold", "=", false]]))?;
                    anyhow::Ok((Arc::new(o), tasks))
                })
                .await;
            this.update(cx, |this, cx| {
                match r {
                    Ok((o, t)) => {
                        this.status = format!("{} open tasks", t.len()).into();
                        this.odoo = Some(o);
                        this.tasks = t;
                    }
                    Err(e) => this.status = e.to_string().into(),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        me
    }

    fn visible(&self) -> Vec<&Task> {
        self.tasks
            .iter()
            .filter(|t| {
                self.assignee
                    .as_ref()
                    .is_none_or(|a| t.assignees.contains(a))
            })
            .collect()
    }

    fn people(&self) -> Vec<String> {
        let mut p: Vec<String> = self
            .tasks
            .iter()
            .flat_map(|t| t.assignees.clone())
            .collect();
        p.sort();
        p.dedup();
        p
    }

    fn suggest_subtasks(&mut self, cx: &mut Context<Self>) {
        let Some(t) = self.visible().get(self.selected).map(|t| t.name.clone()) else {
            return;
        };
        self.status = "✦ Thinking…".into();
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move { ai::subtasks(&t) })
                .await;
            this.update(cx, |this, cx| {
                match r {
                    Ok(s) => {
                        this.status = "✦ Enter to create, Esc to discard".into();
                        this.proposed = s;
                    }
                    Err(e) => this.status = format!("AI: {e}").into(),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn commit_subtasks(&mut self, cx: &mut Context<Self>) {
        let (Some(o), Some(t)) = (
            self.odoo.clone(),
            self.visible().get(self.selected).map(|t| t.id),
        ) else {
            return;
        };
        let names = std::mem::take(&mut self.proposed);
        cx.spawn(async move |this, cx| {
            let r = cx
                .background_executor()
                .spawn(async move { o.create_subtasks(t, &names) })
                .await;
            this.update(cx, |this, cx| {
                this.status = r
                    .map_or_else(|e| e.to_string(), |_| "Sub-tasks created".into())
                    .into();
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn on_key(&mut self, ev: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let n = self.visible().len();
        match ev.keystroke.key.as_str() {
            "j" | "down" if self.selected + 1 < n => self.selected += 1,
            "k" | "up" => self.selected = self.selected.saturating_sub(1),
            "s" => self.suggest_subtasks(cx),
            "enter" if !self.proposed.is_empty() => self.commit_subtasks(cx),
            "escape" => self.proposed.clear(),
            "0" => self.set_filter(None),
            d if d.len() == 1 && d.as_bytes()[0].is_ascii_digit() => {
                let p = self.people().get(d.parse::<usize>().unwrap() - 1).cloned();
                if p.is_some() {
                    self.set_filter(p)
                }
            }
            _ => return,
        }
        cx.notify();
    }

    fn set_filter(&mut self, who: Option<String>) {
        self.assignee = who;
        self.selected = 0;
        self.proposed.clear();
    }
}

fn chip(label: String, on: bool) -> gpui::Div {
    div()
        .px_2()
        .py_0p5()
        .rounded_md()
        .text_xs()
        .bg(rgb(if on { ACCENT } else { LINE }))
        .text_color(rgb(if on { BG } else { TEXT }))
        .child(label)
}

impl Render for BetterOdoo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let visible = self.visible();
        let sel = visible.get(self.selected).cloned().cloned();

        let sidebar = div()
            .w(px(200.))
            .p_3()
            .gap_1()
            .flex()
            .flex_col()
            .bg(rgb(PANEL))
            .border_r_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .text_sm()
                    .font_weight(gpui::FontWeight::BOLD)
                    .mb_3()
                    .child("BetterOdoo"),
            )
            .child(
                div()
                    .text_sm()
                    .child(format!("My Tasks  {}", self.tasks.len())),
            )
            .child(div().text_sm().text_color(rgb(MUTED)).child("Timesheets"))
            .child(div().flex_1())
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child(self.status.clone()),
            );

        let mut chips = div()
            .flex()
            .gap_1()
            .p_3()
            .border_b_1()
            .border_color(rgb(LINE));
        chips = chips.child(
            chip("All".into(), self.assignee.is_none())
                .id("chip-all")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.set_filter(None);
                    cx.notify()
                })),
        );
        for (i, p) in self.people().into_iter().enumerate() {
            let on = self.assignee.as_ref() == Some(&p);
            let who = p.clone();
            chips = chips.child(chip(format!("{} {p}", i + 1), on).id(("chip", i)).on_click(
                cx.listener(move |this, _, _, cx| {
                    this.set_filter(Some(who.clone()));
                    cx.notify()
                }),
            ));
        }

        let rows = visible.iter().enumerate().map(|(i, t)| {
            div()
                .id(("row", i))
                .flex()
                .gap_2()
                .px_3()
                .py_1p5()
                .text_sm()
                .when(i == self.selected, |d| d.bg(rgb(SEL)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.selected = i;
                    this.proposed.clear();
                    cx.notify()
                }))
                .child(div().flex_1().truncate().child(t.name.clone()))
                .child(
                    div()
                        .text_color(rgb(MUTED))
                        .text_xs()
                        .child(t.assignees.join(", ")),
                )
                .when(t.priority_high, |d| {
                    d.child(div().text_color(rgb(HOT)).child("⚑"))
                })
        });
        let list = div()
            .flex_1()
            .flex()
            .flex_col()
            .child(chips)
            .child(div().id("rows").flex_1().overflow_y_scroll().children(rows))
            .child(
                div()
                    .px_3()
                    .py_1()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .child("j/k move · 1-9 filter · 0 all · s ✦ sub-tasks"),
            );

        let field = |k: &str, v: String| {
            div()
                .flex()
                .text_sm()
                .child(div().w(px(90.)).text_color(rgb(MUTED)).child(k.to_string()))
                .child(v)
        };
        let detail = div()
            .w(px(320.))
            .p_4()
            .gap_2()
            .flex()
            .flex_col()
            .bg(rgb(PANEL))
            .border_l_1()
            .border_color(rgb(LINE))
            .children(sel.map(|t| {
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(MUTED))
                            .child(format!("#{}", t.id)),
                    )
                    .child(div().text_lg().child(t.name))
                    .child(field("Project", t.project))
                    .child(field("Stage", t.stage))
                    .child(field("Assignees", t.assignees.join(", ")))
                    .child(field("Deadline", t.deadline.unwrap_or("—".into())))
            }))
            .when(!self.proposed.is_empty(), |d| {
                d.child(
                    div()
                        .mt_3()
                        .text_sm()
                        .text_color(rgb(ACCENT))
                        .child("✦ Proposed sub-tasks"),
                )
                .children(
                    self.proposed
                        .iter()
                        .map(|s| div().text_sm().child(format!("☐ {s}"))),
                )
            });

        div()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .size_full()
            .flex()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .child(sidebar)
            .child(list)
            .child(detail)
    }
}

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1200.), px(760.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| {
                let view = cx.new(BetterOdoo::new);
                let focus = view.read(cx).focus.clone();
                window.focus(&focus, cx);
                view
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
