//! The reset card and its confirmation dialog: the ink stack, the sheets
//! behind it, the face, the tip list, and the acknowledgement box.

use std::time::Duration;

use gpui_kit::component::button::Button;
use gpui_kit::component::{Disableable, Sizable};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use ss_usage::subscription::ResetWindow;

use super::AccountsPage;
use super::schedule::{expiry_line, format_stamp};
use crate::accounts::theme::palette;

/// Above menus (`POPUP_PRIORITY` is 100) and the kit tooltip overlay (200).
/// Native `.tooltip()` still paints after every deferred draw.
const RESET_TIP_PRIORITY: usize = 1_000;

pub(super) fn reset_meta(label: &str, subtitle: &str, urgent: bool) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(3.0))
        .flex_1()
        .min_w_0()
        .child(
            div()
                .text_size(px(13.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(palette().fg))
                .child(label.to_string()),
        )
        .when(!subtitle.is_empty(), |col| {
            col.child(
                div()
                    .text_size(px(11.5))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .text_color(rgb(if urgent {
                        palette().os_bad
                    } else {
                        palette().os_muted
                    }))
                    .when(urgent, |line| line.font_weight(FontWeight::SEMIBOLD))
                    .child(subtitle.to_string()),
            )
        })
}

pub(super) fn reset_button(
    id: &str,
    target: ResetWindow,
    label: &str,
    disabled: bool,
    view: WeakEntity<AccountsPage>,
) -> impl IntoElement {
    let id = id.to_string();
    Button::new(ElementId::Name(
        format!("reset-{id}-{}", target.key()).into(),
    ))
    .outline()
    .small()
    .h(px(28.0))
    .px(px(10.0))
    .text_size(px(11.0))
    .bg(rgb(palette().panel))
    .border_color(rgb(palette().os_edge))
    .text_color(rgb(palette().fg))
    .label(label.to_string())
    .disabled(disabled)
    .on_click(move |_, window, cx| {
        let _ = view.update(cx, |this, cx| {
            this.open_reset_dialog(id.clone(), target, window, cx)
        });
    })
}

pub(super) fn reset_stack(
    id: &str,
    view: WeakEntity<AccountsPage>,
    count: i64,
    expiries: &[i64],
    fill: u32,
    empty: bool,
    busy: bool,
    consumed: Option<std::time::Instant>,
    blink: Option<u64>,
    digits: &str,
    unit: &str,
    countdown: bool,
    tip_open: bool,
) -> AnyElement {
    let depth = (count.max(0) as usize).min(3);
    let account_id = id.to_string();
    let has_tip = !expiries.is_empty();
    let mut stack = div()
        .id(ElementId::Name(format!("reset-stack-{account_id}").into()))
        .relative()
        .flex_shrink_0()
        .w(px(CARD_W))
        .h(px(CARD_H))
        .mt(px(SHEET_PEEK * 2.0))
        .on_hover({
            let account_id = account_id.clone();
            let view = view.clone();
            move |hovered, _, cx| {
                let account_id = account_id.clone();
                let _ = view.update(cx, |this, cx| {
                    let open = this.reset_tip_id.as_deref() == Some(account_id.as_str());
                    if *hovered && has_tip && !open {
                        this.reset_tip_id = Some(account_id);
                        this.revise(cx);
                    } else if !*hovered && open {
                        this.reset_tip_id = None;
                        this.revise(cx);
                    }
                });
            }
        });
    // Farther sheets first. Each one is a full card shifted up and right so
    // only its top-right corner clears the card in front.
    if depth >= 3 {
        stack = stack.child(reset_sheet(SHEET_PEEK * 2.0, fill));
    }
    if depth >= 2 {
        stack = stack.child(reset_sheet(SHEET_PEEK, fill));
    }
    stack = stack.child(reset_face(
        &account_id,
        digits,
        unit,
        fill,
        empty,
        blink,
        countdown,
    ));
    if tip_open && !expiries.is_empty() {
        // In-tree absolute children paint before later siblings, so the label,
        // reset button, next row, and legend cover this list. Defer it so it
        // paints after the tree and is not clipped by the accounts scroller.
        stack = stack.child(deferred(reset_tip(expiries)).with_priority(RESET_TIP_PRIORITY));
    }
    if let Some(started) = consumed {
        stack = stack.child(
            div()
                .absolute()
                .inset_0()
                .child(reset_face(
                    &format!("{id}-consumed"),
                    "1",
                    &crate::i18n::t("usage.resetCardUnit"),
                    fill,
                    false,
                    None,
                    false,
                ))
                .with_spring(
                    ElementId::Name(format!("reset-consumed-{id}-{started:?}").into()),
                    crate::chrome::motion_spring(1.0).from(0.0),
                    |ghost, progress| {
                        let (offset, opacity) = consumed_frame(progress);
                        ghost.top(px(offset)).bottom(px(-offset)).opacity(opacity)
                    },
                ),
        );
    }
    stack
        .with_spring(
            ElementId::Name(format!("reset-lift-{id}").into()),
            crate::chrome::motion_spring(if busy { 1.0 } else { 0.0 }),
            |stack, progress| stack.relative().top(px(-3.0 * progress.clamp(0.0, 1.0))),
        )
        .into_any_element()
}

const CARD_W: f32 = 40.0;
const CARD_H: f32 = 44.0;
/// How far each card behind the face sticks out toward the top-right.
const SHEET_PEEK: f32 = 4.0;

fn reset_sheet(shift: f32, color: u32) -> Div {
    div()
        .absolute()
        .left(px(shift))
        .top(px(-shift))
        .w(px(CARD_W))
        .h(px(CARD_H))
        .rounded(px(7.0))
        .bg(rgb(color))
}

pub(super) fn reset_face(
    id: &str,
    digits: &str,
    unit: &str,
    fill: u32,
    empty: bool,
    blink: Option<u64>,
    countdown: bool,
) -> AnyElement {
    let face = div()
        .absolute()
        .inset_0()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(2.0))
        .rounded(px(7.0))
        .when(empty, |face| {
            face.border_1()
                .border_dashed()
                .border_color(rgb(palette().os_edge))
        })
        .when(!empty, |face| face.bg(rgb(fill)))
        .child(
            div()
                .text_size(px(if countdown { 15.0 } else { 19.0 }))
                .font_weight(FontWeight::SEMIBOLD)
                .whitespace_nowrap()
                .line_height(px(19.0))
                .text_color(rgb(if empty {
                    palette().os_faint
                } else {
                    palette().panel
                }))
                .child(digits.to_string()),
        )
        .child(
            div()
                .text_size(px(10.0))
                .line_height(px(10.0))
                .whitespace_nowrap()
                .text_color(rgb(if empty {
                    palette().os_faint
                } else {
                    palette().panel
                }))
                .when(!empty, |unit| unit.opacity(0.75))
                .child(unit.to_string()),
        );
    if let Some(ms) = blink.filter(|_| !empty) {
        face.with_animation(
            ElementId::Name(format!("reset-blink-{id}").into()),
            Animation::new(Duration::from_millis(ms))
                .repeat()
                .with_max_fps(crate::chrome::PULSE_FPS),
            |face, t| {
                let wave = if t < 0.5 { t * 2.0 } else { (1.0 - t) * 2.0 };
                face.opacity(1.0 - 0.28 * wave)
            },
        )
        .into_any_element()
    } else {
        face.into_any_element()
    }
}

pub(super) fn reset_tip(expiries: &[i64]) -> Div {
    // Deferred paint puts this list over the next row. Occlude so those
    // buttons do not hover while the pointer is on the list.
    let mut tip = div()
        .occlude()
        .absolute()
        .left_0()
        .top(px(52.0))
        .min_w(px(180.0))
        .flex()
        .flex_col()
        .gap(px(4.0))
        .p(px(8.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(rgb(palette().os_line))
        .bg(rgb(palette().panel))
        .shadow_xl();
    for (index, stamp) in expiries.iter().copied().enumerate() {
        let when = expiry_line(&format_stamp(stamp));
        tip = tip.child(
            div()
                .flex()
                .items_baseline()
                .gap(px(8.0))
                .child(
                    div()
                        .min_w(px(16.0))
                        .text_size(px(11.0))
                        .text_color(rgb(palette().os_muted))
                        .child(format!("{}", index + 1)),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .whitespace_nowrap()
                        .text_color(rgb(palette().fg))
                        .child(when),
                ),
        );
    }
    tip
}

fn consumed_frame(progress: f32) -> (f32, f32) {
    let progress = progress.clamp(0.0, 1.0);
    (-18.0 * progress, 1.0 - progress)
}

#[cfg(test)]
mod tests {
    #[test]
    fn consumed_card_lifts_and_fades_without_spring_overshoot() {
        assert_eq!(super::consumed_frame(0.0), (0.0, 1.0));
        assert_eq!(super::consumed_frame(0.5), (-9.0, 0.5));
        assert_eq!(super::consumed_frame(1.2), (-18.0, 0.0));
    }
}
