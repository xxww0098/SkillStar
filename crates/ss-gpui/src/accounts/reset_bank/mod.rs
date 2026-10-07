//! Provider reset-card banks, grouped by the window each card clears.
//!
//! A dashed frame holds weekly / five-hour rows: an ink count card (sheets
//! behind it peek toward the top-right to show depth), the window name, the
//! earliest expiry, and 重置. Spending opens a confirm
//! that stays disabled until the checkbox is checked. Cards past `now` drop
//! without waiting for the next fetch. A successful redemption lifts and fades one card.
//!
//! `schedule.rs` owns the window arithmetic, `view.rs` the card geometry.

mod schedule;
mod view;

use std::time::Duration;

use gpui_kit::component::button::Button;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::{Disableable, WindowExt};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use ss_usage::accounts::SubscriptionDto;
use ss_usage::subscription::ResetWindow;

use super::AccountsPage;
use super::meters::current_epoch_seconds;
use crate::accounts::theme::palette;

use self::schedule::{
    blink_ms, clock_delay_for, format_countdown, format_stamp, read_reset_bank, reset_subtitle,
    reset_view,
};
use self::view::{reset_button, reset_meta, reset_stack};

fn window_label(window: ResetWindow) -> String {
    match window {
        ResetWindow::FiveHour => crate::i18n::t("usage.window5h").to_string(),
        ResetWindow::Weekly => crate::i18n::t("usage.windowWeekly").to_string(),
    }
}

impl AccountsPage {
    pub(super) fn animate_reset_consumed(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.busy_id.clone() else {
            return;
        };
        let started = std::time::Instant::now();
        self.reset_consumed = Some((id, self.reset_window, started));
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(450))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this
                    .reset_consumed
                    .as_ref()
                    .is_some_and(|(_, _, time)| *time == started)
                {
                    this.reset_consumed = None;
                }
                this.revise(cx);
            });
        })
        .detach();
    }

    fn reset_clock_delay(&self) -> Option<Duration> {
        let now = current_epoch_seconds();
        let mut best: Option<i64> = None;
        for sub in &self.subscriptions {
            let credits = sub
                .usage
                .as_ref()
                .map(|usage| usage.credits.as_slice())
                .unwrap_or(&[]);
            for window in ResetWindow::for_catalog(&sub.catalog_id) {
                let Some(earliest) =
                    reset_view(&read_reset_bank(credits, &sub.catalog_id, *window), now).earliest
                else {
                    continue;
                };
                let left = earliest - now;
                if left > 0 {
                    best = Some(best.map_or(left, |current| current.min(left)));
                }
            }
        }
        clock_delay_for(best)
    }

    /// One shared clock while any live card is inside 24h.
    /// The first wait happens before `update`: this is called from `render`,
    /// and a synchronous update would re-enter the entity being drawn.
    pub(crate) fn arm_reset_clock(&mut self, cx: &mut Context<Self>) {
        if self.reset_clock {
            return;
        }
        let Some(delay) = self.reset_clock_delay() else {
            return;
        };
        self.reset_clock = true;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            loop {
                let Ok(next) = this.update(cx, |this, cx| {
                    this.revise(cx);
                    let next = this.reset_clock_delay();
                    if next.is_none() {
                        this.reset_clock = false;
                    }
                    next
                }) else {
                    break;
                };
                let Some(next) = next else { break };
                cx.background_executor().timer(next).await;
            }
        })
        .detach();
    }

    pub(crate) fn render_reset_bank(
        &self,
        sub: &SubscriptionDto,
        view: WeakEntity<Self>,
    ) -> AnyElement {
        let id = sub.id.clone();
        let credits = sub
            .usage
            .as_ref()
            .map(|usage| usage.credits.as_slice())
            .unwrap_or(&[]);
        let mut rows = div()
            .px(px(14.0))
            .pt(px(8.75))
            .pb(px(4.0))
            .flex()
            .flex_col()
            .border_1()
            .border_dashed()
            .border_color(rgb(palette().os_edge))
            .rounded(px(10.0));
        for (index, window) in ResetWindow::for_catalog(&sub.catalog_id)
            .iter()
            .copied()
            .enumerate()
        {
            let key = format!("{id}-{}", window.key());
            let shown = reset_view(
                &read_reset_bank(credits, &sub.catalog_id, window),
                current_epoch_seconds(),
            );
            let busy = self.resetting
                && self.busy_id.as_deref() == Some(id.as_str())
                && self.reset_window == window;
            let disabled =
                self.busy_id.is_some() || self.refreshing_all || !shown.known || shown.count == 0;
            let stamp = shown.earliest.map(format_stamp).unwrap_or_default();
            let subtitle = if shown.known {
                reset_subtitle(&shown, &stamp)
            } else {
                crate::i18n::t("usage.resetCardRefresh").to_string()
            };
            let unit = if shown.countdown {
                crate::i18n::t("usage.resetCardSeconds")
            } else {
                crate::i18n::t("usage.resetCardUnit")
            };
            let action = if busy {
                crate::i18n::t("usage.resetCardBusy")
            } else {
                crate::i18n::t("usage.resetLabelShort")
            };
            let digits = if shown.countdown {
                format_countdown(shown.left_secs)
            } else if shown.known {
                shown.count.to_string()
            } else {
                "–".into()
            };
            let fill = if shown.urgent {
                palette().os_bad
            } else {
                palette().fg
            };
            let blink = if shown.urgent {
                blink_ms(shown.left_secs)
            } else {
                None
            };
            rows = rows.child(
                div()
                    .id(ElementId::Name(format!("reset-row-{key}").into()))
                    .flex()
                    .items_center()
                    .gap(px(14.0))
                    .min_h(px(60.0))
                    .py(px(10.0))
                    .w_full()
                    .when(index > 0, |row| {
                        row.border_t_1()
                            .border_dashed()
                            .border_color(rgb(palette().os_line))
                    })
                    .child(reset_stack(
                        &key,
                        view.clone(),
                        shown.count,
                        &shown.expiries,
                        fill,
                        !shown.known || shown.count == 0,
                        busy,
                        self.reset_consumed
                            .as_ref()
                            .filter(|(account, target, _)| account == &id && *target == window)
                            .map(|(_, _, started)| *started),
                        blink,
                        &digits,
                        unit.as_ref(),
                        shown.countdown,
                        self.reset_tip_id.as_deref() == Some(&key),
                    ))
                    .child(reset_meta(&window_label(window), &subtitle, shown.urgent))
                    .child(reset_button(
                        &id,
                        window,
                        action.as_ref(),
                        disabled,
                        view.clone(),
                    )),
            );
        }

        // The 重置卡 legend must paint after the dashed box it notches: GPUI
        // draws a parent border over its children, so it has to be a later
        // sibling, not a child of the bordered element.
        div()
            .id(ElementId::Name(format!("reset-bank-{id}").into()))
            .relative()
            .w_full()
            .mt(px(2.0))
            .child(rows)
            .child(
                div()
                    .absolute()
                    .top(px(-8.0))
                    .left(px(8.0))
                    .px(px(6.0))
                    .bg(rgb(palette().panel))
                    .text_size(px(13.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(palette().fg))
                    .child(crate::i18n::t("usage.resetCards")),
            )
            .into_any_element()
    }

    pub(crate) fn open_reset_dialog(
        &mut self,
        id: String,
        target: ResetWindow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy_id.is_some() || self.refreshing_all {
            return;
        }
        let Some(sub) = self.subscriptions.iter().find(|sub| sub.id == id) else {
            return;
        };
        let credits = sub
            .usage
            .as_ref()
            .map(|u| u.credits.as_slice())
            .unwrap_or(&[]);
        let shown = reset_view(
            &read_reset_bank(credits, &sub.catalog_id, target),
            current_epoch_seconds(),
        );
        if !shown.known || shown.count == 0 {
            return;
        }
        let when = shown.earliest.map(format_stamp).filter(|s| !s.is_empty());
        let card = when
            .map(|v| crate::i18n::tf("usage.resetCardOneDated", &[("when", &v)]).to_string())
            .unwrap_or_else(|| crate::i18n::t("usage.resetCardOne").to_string());
        let window_name = window_label(target);
        let body = crate::i18n::tf(
            "usage.resetCardConfirmBody",
            &[("window", &window_name), ("card", &card)],
        )
        .to_string();
        self.confirm_reset_id = Some(id);
        self.reset_window = target;
        self.reset_acked = false;
        self.reset_tip_id = None;
        let view = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, cx| {
            let acked = view.upgrade().is_some_and(|v| v.read(cx).reset_acked);
            let toggle = view.clone();
            let confirm = view.clone();
            let close = view.clone();
            let enter = view.clone();
            dialog
                .title(crate::i18n::t("usage.resetCardSpendTitle"))
                .width(px(440.0))
                .child(
                    div()
                        .text_size(px(14.0))
                        .line_height(px(22.0))
                        .child(body.clone()),
                )
                .child(
                    Checkbox::new("accounts-reset-ack")
                        .label(crate::i18n::t("usage.resetCardAck"))
                        .checked(acked)
                        .on_click(move |checked, _, cx| {
                            let _ = toggle.update(cx, |this, cx| {
                                this.reset_acked = *checked;
                                this.revise(cx);
                            });
                        }),
                )
                .footer(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("accounts-reset-cancel")
                                .outline()
                                .label(crate::i18n::t("common.cancel"))
                                .on_click(|_, window, cx| window.close_dialog(cx)),
                        )
                        .child(
                            Button::new("accounts-reset-ok")
                                .label(crate::i18n::t("usage.resetCardConfirm"))
                                .disabled(!acked)
                                .on_click(move |_, window, cx| {
                                    if confirm_reset(&confirm, cx) {
                                        window.close_dialog(cx);
                                    }
                                }),
                        ),
                )
                .on_ok(move |_, _, cx| confirm_reset(&enter, cx))
                .on_close(move |_, _, cx| {
                    let _ = close.update(cx, |this, cx| {
                        this.confirm_reset_id = None;
                        this.reset_acked = false;
                        this.revise(cx);
                    });
                })
        });
        self.revise(cx);
    }
}

fn confirm_reset(view: &WeakEntity<AccountsPage>, cx: &mut App) -> bool {
    view.update(cx, |this, cx| {
        if !this.reset_acked || this.busy_id.is_some() || this.refreshing_all {
            return false;
        }
        let Some(id) = this.confirm_reset_id.clone() else {
            return false;
        };
        let available = this
            .subscriptions
            .iter()
            .find(|sub| sub.id == id)
            .is_some_and(|sub| {
                let credits = sub
                    .usage
                    .as_ref()
                    .map(|usage| usage.credits.as_slice())
                    .unwrap_or(&[]);
                let shown = reset_view(
                    &read_reset_bank(credits, &sub.catalog_id, this.reset_window),
                    current_epoch_seconds(),
                );
                shown.known && shown.count > 0
            });
        if !available {
            return false;
        }
        this.confirm_reset_id = None;
        this.reset_acked = false;
        this.run(super::AccountAction::ResetQuota(id, this.reset_window), cx);
        true
    })
    .unwrap_or(false)
}
