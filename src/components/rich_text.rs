use super::icon;
use crate::html::{Block, Run};
use crate::scale::px;
use crate::theme::*;
use gpui::{
    App, FontWeight, HighlightStyle, IntoElement, RenderOnce, StyledText, Window, div, prelude::*,
    rgb,
};

#[derive(IntoElement)]
pub struct RichText {
    blocks: Vec<Block>,
}

impl RichText {
    pub fn new(blocks: Vec<Block>) -> Self {
        Self { blocks }
    }
}

fn styled(runs: &[Run]) -> StyledText {
    let mut text = String::new();
    let mut bold = Vec::new();
    for run in runs {
        let start = text.len();
        text.push_str(&run.text);
        if run.bold {
            bold.push((
                start..text.len(),
                HighlightStyle {
                    font_weight: Some(FontWeight::SEMIBOLD),
                    color: Some(rgb(TEXT).into()),
                    ..Default::default()
                },
            ));
        }
    }
    StyledText::new(text).with_highlights(bold)
}

impl RenderOnce for RichText {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .line_height(px(22.))
            .text_color(rgb(TEXT_SOFT))
            .children(self.blocks.into_iter().enumerate().map(|(i, block)| {
                match block {
                    Block::Text(runs) => div().child(styled(&runs)).into_any_element(),
                    Block::Item(marker, runs) => div()
                        .flex()
                        .gap(px(8.))
                        .child(
                            div()
                                .flex_none()
                                .w(px(22.))
                                .text_color(rgb(MUTED))
                                .child(marker),
                        )
                        .child(div().flex_1().min_w_0().child(styled(&runs)))
                        .into_any_element(),
                    Block::Attachment { label, url } => div()
                        .id(("attachment", i))
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .cursor_pointer()
                        .text_color(rgb(ACCENT))
                        .hover(|s| s.text_color(rgb(TEXT)))
                        .child(icon("attach", 14., ACCENT))
                        .child(label)
                        .on_click(move |_, _, cx| cx.open_url(&url))
                        .into_any_element(),
                }
            }))
    }
}
