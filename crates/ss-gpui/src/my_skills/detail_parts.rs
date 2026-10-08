//! Row and button widgets for the installed-skill detail column.
//!
//! The column module owns what appears and in which order. These functions
//! only paint one fact, source line, upstream note, or action.

use gpui_kit::assets::IconName;
use gpui_kit::base::StyledExt;
use gpui_kit::component::link::Link;
use gpui_kit::component::{Icon, Sizable};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::detail_facts::{self, SourceValue, UpstreamNote};
use crate::chrome::{InteractionSpring, MotionDiv, MotionPaint};
use crate::theme::palette;

pub(super) fn badge(bg: u32, fg: u32, label: impl Into<SharedString>) -> Div {
    div()
        .px_1p5()
        .py(px(1.0))
        .rounded_sm()
        .bg(rgb(bg))
        .text_color(rgb(fg))
        .font_semibold()
        .child(label.into())
}

pub(super) fn field_label(label: String) -> Div {
    div()
        .text_xs()
        .font_semibold()
        .text_color(rgb(palette().fg_muted))
        .child(label)
}

pub(super) fn section(label: String, body: impl IntoElement, action: Option<MotionDiv>) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(field_label(label))
                .when_some(action, |row, action| row.child(action)),
        )
        .child(body)
}

pub(super) fn fact_row(label: String, value: String) -> Div {
    div()
        .flex()
        .items_start()
        .justify_between()
        .gap_3()
        .min_w_0()
        .child(field_label(label))
        .child(
            div()
                .min_w_0()
                .text_xs()
                .text_color(rgb(palette().fg))
                .whitespace_normal()
                .text_right()
                .child(value),
        )
}

pub(super) fn source_line(index: usize, line: SourceValue<'_>) -> AnyElement {
    match line {
        SourceValue::Link(url) => {
            let shown = detail_facts::wrap_long_token(url);
            Link::new(ElementId::Name(
                format!("skill-detail-source-{index}").into(),
            ))
            .href(url.to_string())
            .text_xs()
            .whitespace_normal()
            .child(shown)
            .into_any_element()
        }
        SourceValue::Text(text) => div()
            .text_xs()
            .text_color(rgb(palette().fg))
            .whitespace_normal()
            .child(detail_facts::wrap_long_token(text))
            .into_any_element(),
    }
}

pub(super) fn upstream_block(note: UpstreamNote) -> Div {
    let (title, body, tone) = match note {
        UpstreamNote::Removed { source, folder } => {
            let body = if source.is_empty() {
                crate::i18n::t("skillCard.upstreamRemovedHint").to_string()
            } else {
                crate::i18n::tf(
                    "detailPanel.upstreamRemovedDesc",
                    &[("source", &source), ("folder", &folder)],
                )
                .to_string()
            };
            (
                crate::i18n::t("detailPanel.upstreamRemovedTitle").to_string(),
                body,
                palette().danger,
            )
        }
        UpstreamNote::Renamed { from, to } => (
            crate::i18n::t("skillCard.upstreamRenamed").to_string(),
            crate::i18n::tf(
                "mySkills.updateIdentityChanged",
                &[("name", &from), ("upstream", &to)],
            )
            .to_string(),
            palette().danger,
        ),
        UpstreamNote::LocalEdits { baseline_missing } => {
            let body = if baseline_missing {
                crate::i18n::t("detailPanel.localChangesUnknown")
            } else {
                crate::i18n::t("detailPanel.localChanges")
            };
            (
                crate::i18n::t("detailPanel.localChangesTitle").to_string(),
                body.to_string(),
                palette().warn,
            )
        }
    };
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_xs()
                .font_semibold()
                .text_color(rgb(tone))
                .whitespace_normal()
                .child(title),
        )
        .child(
            div()
                .text_xs()
                .text_color(rgb(palette().fg))
                .whitespace_normal()
                .child(body),
        )
}

pub(super) fn action_button(
    id: &'static str,
    icon: IconName,
    label: String,
    bg: u32,
    fg: u32,
    border: u32,
    emphasize: bool,
) -> crate::chrome::MotionDiv {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .gap_2()
        .w_full()
        .py_2()
        .px_2()
        .rounded_lg()
        .border_1()
        .border_color(rgb(border))
        .bg(rgb(bg))
        .text_color(rgb(fg))
        .text_xs()
        .font_weight(if emphasize {
            FontWeight::SEMIBOLD
        } else {
            FontWeight::MEDIUM
        })
        .cursor_pointer()
        .child(Icon::new(icon).with_size(px(14.0)))
        .child(div().whitespace_normal().text_center().child(label))
        .interaction_spring(
            id,
            true,
            if emphasize {
                MotionPaint::new().opacity(1.0)
            } else {
                MotionPaint::new().bg(rgb(bg))
            },
            if emphasize {
                MotionPaint::new().opacity(0.92)
            } else {
                MotionPaint::new().bg(rgb(palette().card_hover))
            },
        )
}
