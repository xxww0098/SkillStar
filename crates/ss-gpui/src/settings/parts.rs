//! Presentation parts shared by the Settings sections: the section header
//! block, the card surface, the meta chip, the field label, the collapse
//! chevron, and the segmented choice pills.
//!
//! Every section file (`general`, `storage`, `about`, …) builds on these, so
//! they sit beside the page instead of inside it. They read the palette and
//! carry no state of their own.

use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::{SettingsPage, SettingsSection};
use crate::chrome::{InteractionSpring, MotionPaint};
use crate::theme::palette;

pub(crate) fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".into();
    }
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let exp = ((bytes as f64).ln() / 1024_f64.ln()).floor() as usize;
    let exp = exp.min(UNITS.len() - 1);
    let value = bytes as f64 / 1024_f64.powi(exp as i32);
    let mut text = format!("{value:.2}");
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    format!("{text} {}", UNITS[exp])
}

/// primary @ 10% / 20% over `palette().card` — `SettingsSectionHeader` icon well.
/// `bg-primary/15` over the card, used by the active rail item.
/// `bg-muted/50` track behind segmented controls.

/// Section block — GPUI twin of `SettingsSectionHeader` plus its card.
pub(crate) fn section_shell(
    section: SettingsSection,
    meta: Option<AnyElement>,
    action: Option<AnyElement>,
    body: impl IntoElement,
) -> Div {
    let mut title_row = div()
        .flex()
        .items_center()
        .gap_2()
        .min_w_0()
        .flex_1()
        .child(
            div()
                .size(px(28.0))
                .rounded_lg()
                .flex_shrink_0()
                .border_1()
                .border_color(rgb(palette().accent_soft_edge))
                .bg(rgb(palette().accent_soft))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    Icon::new(section.icon())
                        .size(px(16.0))
                        .text_color(rgb(palette().accent)),
                ),
        )
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(palette().fg))
                .child(section.label()),
        );
    if let Some(meta) = meta {
        title_row = title_row.child(meta);
    }
    let mut header = div()
        .mb(px(12.0))
        .px_1()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap_3()
        .w_full()
        .child(title_row);
    if let Some(action) = action {
        header = header.child(div().flex_shrink_0().child(action));
    }
    div()
        .w_full()
        .flex_none()
        .flex()
        .flex_col()
        .child(header)
        .child(body)
}

/// `rounded-xl border bg-card` surface used by every section body.
pub(crate) fn card() -> Div {
    div()
        .w_full()
        .rounded_xl()
        .border_1()
        .border_color(rgb(palette().border))
        .bg(rgb(palette().card))
        .overflow_hidden()
}

/// Truncated host chip in a section header (`max-w-[260px]`).
pub(crate) fn meta_chip(text: impl Into<SharedString>) -> Div {
    div()
        .max_w(px(260.0))
        .px_2()
        .py(px(2.0))
        .rounded_md()
        .border_1()
        .border_color(rgb(palette().border))
        .bg(rgb(palette().well))
        .text_xs()
        .text_color(rgb(palette().fg_muted))
        .truncate()
        .child(text.into())
}

pub(crate) fn field_label(title: impl Into<SharedString>, control: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .min_w_0()
        .child(
            div()
                .text_xs()
                .text_color(rgb(palette().fg_muted))
                .child(title.into()),
        )
        .child(control)
}

pub(crate) fn collapse_chevron(open: bool) -> Icon {
    Icon::new(if open {
        IconName::ChevronDown
    } else {
        IconName::ChevronRight
    })
    .size(px(16.0))
    .text_color(rgb(palette().fg_muted))
}
/// Segmented choices — Language, Appearance. Selected chip is the page
/// background on a muted track, matching the React radiogroup.
pub(crate) fn choice_pills(
    id_prefix: &'static str,
    options: &[(&'static str, SharedString)],
    current: &str,
    view: WeakEntity<SettingsPage>,
    apply: impl Fn(&mut SettingsPage, &'static str, &mut Context<SettingsPage>) + Clone + 'static,
) -> Div {
    let mut options_row = div()
        .flex()
        .flex_row()
        .flex_wrap()
        .gap(px(6.0))
        .p_1()
        .rounded_lg()
        .bg(rgb(palette().well));
    for (id, label) in options {
        let id = *id;
        let selected = current == id || current.starts_with(id);
        let v = view.clone();
        let apply = apply.clone();
        options_row = options_row.child(
            div()
                .id(ElementId::Name(format!("{id_prefix}-{id}").into()))
                .h(px(32.0))
                .px_4()
                .flex()
                .items_center()
                .rounded_md()
                .cursor_pointer()
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(if selected {
                    palette().fg
                } else {
                    palette().fg_muted
                }))
                .when(selected, |d| d.bg(rgb(palette().bg)))
                .child(label.clone())
                .on_click(move |_, _, cx| {
                    let _ = v.update(cx, |this, cx| {
                        apply(this, id, cx);
                        cx.notify();
                    });
                })
                .interaction_spring(
                    format!("{id_prefix}-{id}"),
                    true,
                    if selected {
                        MotionPaint::new()
                            .bg(rgb(palette().bg))
                            .fg(rgb(palette().fg))
                    } else {
                        MotionPaint::new().fg(rgb(palette().fg_muted))
                    },
                    if selected {
                        MotionPaint::new()
                            .bg(rgb(palette().bg))
                            .fg(rgb(palette().fg))
                    } else {
                        MotionPaint::new()
                            .bg(rgb(palette().card_hover))
                            .fg(rgb(palette().fg))
                    },
                ),
        );
    }
    options_row
}
