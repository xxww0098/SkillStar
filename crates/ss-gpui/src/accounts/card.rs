//! Account bodies. The row and the card width come from `skill_card::tracks`.
//! The face is [`super::frame::legend_frame`].
//!
//! Every cell of one provider, including add, is that face. A different
//! provider starts the next block. Inside an account cell: identity, plan,
//! A check marks the account the CLI is using, then icon buttons for
//! 切换 / 刷新 / 退出. Quota is remaining-only.

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use ss_usage::accounts::SubscriptionDto;
use ss_usage::catalog::AuthMode;
use ss_usage::subscription::{
    CODEX_CREDITS, CODEX_CREDITS_UNLIMITED, CreditInfo, ResetWindow, UsageUnit, UsageWindow,
};

use super::AccountsPage;
use super::meters::render_remaining_meter;

use super::frame::legend_frame;
use super::types::AccountAction;
use crate::accounts::theme::palette;
use crate::chrome::{InteractionSpring, MotionPaint, icon, icon_spin};
use crate::layout::{CARD_GAP, QUOTA_CARD_W};
use crate::skill_card::tracks;

impl AccountsPage {
    pub(crate) fn subs_for<'a>(&'a self, catalog_id: &str) -> Vec<&'a SubscriptionDto> {
        self.subscriptions
            .iter()
            .filter(|sub| sub.catalog_id == catalog_id)
            .collect()
    }

    /// One provider group. Every cell, including add, is a legend on a
    /// shared track. The next provider is a later sibling in the page column.
    pub(crate) fn render_provider_card(
        &self,
        title: &str,
        catalog_id: &str,
        columns: usize,
        view: WeakEntity<Self>,
    ) -> impl IntoElement {
        let roster = self.subs_for(catalog_id);
        let mut cells = Vec::with_capacity(roster.len() + 1);
        for sub in &roster {
            cells.push(
                legend_frame(title, self.render_account(sub, view.clone())).into_any_element(),
            );
        }
        cells.push(legend_frame(title, self.add_row(catalog_id, view)).into_any_element());
        tracks(QUOTA_CARD_W, columns, CARD_GAP, cells)
    }

    fn add_row(&self, catalog_id: &str, view: WeakEntity<Self>) -> impl IntoElement {
        let catalog_id = catalog_id.to_string();
        div()
            .id(ElementId::Name(format!("add-{catalog_id}").into()))
            .w_full()
            .min_h(px(34.0))
            .px(px(14.0))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(7.0))
            .rounded(px(12.0))
            .border_1()
            .border_dashed()
            .border_color(rgb(palette().os_edge))
            .text_size(px(12.0))
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgb(palette().os_muted))
            .cursor_pointer()
            .interaction_spring(
                format!("add-{catalog_id}"),
                true,
                MotionPaint::new()
                    .fg(rgb(palette().os_muted))
                    .border(rgb(palette().os_edge)),
                MotionPaint::new()
                    .bg(rgb(palette().os_fill))
                    .fg(rgb(palette().fg))
                    .border(rgb(palette().os_active_edge)),
            )
            .child(icon(
                gpui_kit::assets::IconName::Plus,
                10.0,
                palette().os_muted,
            ))
            .child(crate::i18n::t("common.add"))
            .on_click(move |_, window, cx| {
                let catalog_id = catalog_id.clone();
                let _ = view.update(cx, |this, cx| {
                    this.open_add(catalog_id, window, cx);
                });
            })
    }

    fn render_account(&self, sub: &SubscriptionDto, view: WeakEntity<Self>) -> impl IntoElement {
        let active = sub.is_active;
        let refreshing = self.busy_id.as_deref() == Some(sub.id.as_str());

        // Switching is the 切换 button only. The card body used to activate
        // on any click, so a miss on 刷新 / 额度 / 添加 swapped the CLI account.
        div()
            .id(ElementId::Name(format!("acct-{}", sub.id).into()))
            .w_full()
            .flex()
            .flex_col()
            .gap_3()
            .px(px(16.0))
            .pt(px(14.0))
            .pb(px(16.0))
            .rounded(px(12.0))
            .border_1()
            .border_color(rgb(if active {
                palette().os_active_edge
            } else {
                palette().os_line
            }))
            .child(self.account_head(sub, refreshing, view.clone()))
            .child(self.account_body(sub, view))
    }

    fn account_head(&self, sub: &SubscriptionDto, refreshing: bool, view: WeakEntity<Self>) -> Div {
        let plan = sub
            .usage
            .as_ref()
            .and_then(|usage| usage.plan_name.clone())
            .or_else(|| sub.plan_tier.clone());

        let mut identity = div()
            .flex_1()
            .min_w(px(180.0))
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2();
        identity = identity.child(
            div()
                .font_family("JetBrains Mono")
                .text_size(px(12.5))
                .text_color(rgb(palette().fg))
                .child(sub.display_name.clone()),
        );
        if sub.is_active {
            identity = identity.child(active_mark(&sub.id));
        }
        if let Some(plan) = plan {
            if !plan.trim().is_empty() {
                identity = identity.child(tag(&plan.to_uppercase()));
            }
        }
        if let Some(region) = sub.oauth_region.as_deref().filter(|r| !r.is_empty()) {
            identity = identity.child(tag(region));
        }
        if let Some(label) = auth_tag(sub.auth_mode) {
            identity = identity.child(tag(label.as_ref()));
        }
        if sub.requires_reauth {
            identity = identity.child(warn_tag(crate::i18n::t("usage.requiresReauth").as_ref()));
        }

        div()
            .flex()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap_3()
            .w_full()
            .child(identity)
            .child(self.account_actions(sub, refreshing, view))
    }

    fn account_actions(
        &self,
        sub: &SubscriptionDto,
        refreshing: bool,
        view: WeakEntity<Self>,
    ) -> Div {
        let mut actions = div().flex().flex_wrap().items_center().gap_2();
        if !sub.is_active {
            let id = sub.id.clone();
            let view = view.clone();
            actions = actions.child(card_icon_button(
                format!("switch-{id}"),
                gpui_kit::assets::IconName::ArrowLeftRight,
                crate::i18n::t("usage.setActive"),
                false,
                false,
                move |_, _, cx| {
                    let id = id.clone();
                    let _ = view.update(cx, |this, cx| {
                        this.run(AccountAction::Activate(id), cx);
                    });
                },
            ));
        }
        actions = actions.child(refresh_button(&sub.id, refreshing, view.clone()));
        actions = actions.child(self.logout_button(sub, view));
        actions
    }

    fn logout_button(&self, sub: &SubscriptionDto, view: WeakEntity<Self>) -> impl IntoElement {
        let id = sub.id.clone();
        card_icon_button(
            format!("logout-{id}"),
            gpui_kit::assets::IconName::LogOut,
            crate::i18n::t("settings.githubAuthLogout"),
            false,
            false,
            move |_, _, cx| {
                let id = id.clone();
                let _ = view.update(cx, |this, cx| {
                    this.confirm_reset_id = None;
                    this.confirm_delete_id = Some(id);
                    this.revise(cx);
                });
            },
        )
    }

    fn account_body(&self, sub: &SubscriptionDto, view: WeakEntity<Self>) -> Div {
        let mut body = div()
            .flex()
            .flex_col()
            .gap_3()
            .w_full()
            .pt_3()
            .border_t_1()
            .border_color(rgb(palette().os_hair));

        if sub.requires_reauth {
            body = body.child(
                div()
                    .text_size(px(12.0))
                    .text_color(rgb(palette().os_warn))
                    .child(crate::i18n::t("usage.reauthRequiredHint")),
            );
        }

        if let Some(usage) = &sub.usage {
            let mut meters = div().flex().flex_col().gap_3().w_full();
            let mut any = false;
            for window in [&usage.hourly, &usage.weekly, &usage.monthly]
                .into_iter()
                .flatten()
            {
                any = true;
                meters = meters.child(render_remaining_meter(window));
            }
            if any {
                body = body.child(meters);
            }
            if let Some(balance) = &usage.balance {
                let symbol = match balance.currency.as_str() {
                    "CNY" => "¥",
                    "USD" => "$",
                    other => other,
                };
                body = body.child(prepaid_row(
                    crate::i18n::t("usage.cardBalance").as_ref(),
                    &format!("{symbol}{:.2}", balance.total),
                ));
            }
            for credit in &usage.credits {
                if let Some((label, amount)) = credit_line(credit) {
                    body = body.child(prepaid_row(&label, &amount));
                }
            }
            if let Some(err) = &usage.error {
                body = body.child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgb(palette().os_bad))
                        .child(crate::i18n::tf(
                            "settings.connectionFailed",
                            &[("error", err)],
                        )),
                );
            }
        }

        if let Some(manual) = &sub.manual_quota {
            let label = manual
                .period_label
                .clone()
                .unwrap_or_else(|| crate::i18n::t("usage.manualQuota").to_string());
            let window = UsageWindow {
                label,
                used: manual.used_tokens.unwrap_or(0),
                total: manual.total_tokens,
                percent: None,
                reset_at: None,
                breakdown: Vec::new(),

                unit: UsageUnit::Count,
            };
            body = body.child(render_remaining_meter(&window));
        }

        if !ResetWindow::for_catalog(&sub.catalog_id).is_empty() {
            body = body.child(self.render_reset_bank(sub, view.clone()));
        }

        if let Some(note) = sub.note.as_deref().filter(|n| !n.trim().is_empty()) {
            body = body.child(
                div()
                    .text_size(px(11.5))
                    .text_color(rgb(palette().os_faint))
                    .child(note.to_string()),
            );
        }

        body
    }
}

/// Check beside the name. The card border already frames the active account;
/// this mark names which identity the CLI is serving. The tip carries that scope.
fn active_mark(id: &str) -> impl IntoElement {
    let tip = crate::i18n::t("usage.cardActiveTitle");
    div()
        .id(ElementId::Name(format!("active-{id}").into()))
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .child(icon(
            gpui_kit::assets::IconName::CircleCheck,
            14.0,
            palette().fg,
        ))
        .tooltip(move |window, cx| crate::chrome::tooltip(tip.clone()).build(window, cx))
}

fn tag(label: &str) -> Div {
    div()
        .px(px(5.0))
        .py(px(2.0))
        .rounded(px(5.0))
        .text_size(px(10.0))
        .font_weight(FontWeight::SEMIBOLD)
        .bg(rgb(palette().os_fill_2))
        .text_color(rgb(palette().os_tag))
        .child(label.to_string())
}

fn warn_tag(label: &str) -> Div {
    div()
        .px(px(5.0))
        .py(px(2.0))
        .rounded(px(5.0))
        .text_size(px(10.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(palette().os_warn))
        .bg(rgb(palette().os_fill))
        .child(label.to_string())
}

/// Label and amount for one non-reset credit. Codex points use
/// `usage.creditPoints`; the unlimited sentinel is not shown raw.
fn credit_line(credit: &CreditInfo) -> Option<(String, String)> {
    if credit.is_reset_card() {
        return None;
    }
    if credit.credit_type == CODEX_CREDITS {
        let amount = match credit.credit_amount.as_deref() {
            Some(CODEX_CREDITS_UNLIMITED) => crate::i18n::t("usage.creditsUnlimited").to_string(),
            Some(amount) if !amount.is_empty() => {
                ss_usage::subscription::format_codex_credit_points(amount)
            }
            _ => return None,
        };
        return Some((crate::i18n::t("usage.creditPoints").to_string(), amount));
    }
    Some((
        credit.credit_type.clone(),
        credit
            .credit_amount
            .clone()
            .unwrap_or_else(|| "—".to_string()),
    ))
}

fn prepaid_row(label: &str, amount: &str) -> Div {
    div()
        .flex()
        .items_baseline()
        .justify_between()
        .gap(px(10.0))
        .w_full()
        .text_size(px(12.0))
        .child(
            div()
                .text_color(rgb(palette().os_muted))
                .child(label.to_string()),
        )
        .child(
            div()
                .font_family("JetBrains Mono")
                .text_size(px(12.5))
                .text_color(rgb(palette().fg))
                .child(amount.to_string()),
        )
}

/// Square outline control. The glyph is the label; the tooltip keeps the
/// localized verb (切换 / 刷新 / 退出).
fn card_icon_button(
    element_id: impl Into<SharedString>,
    glyph: gpui_kit::assets::IconName,
    tip: SharedString,
    dimmed: bool,
    spin: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let element_id = element_id.into();
    let ink = if spin { palette().accent } else { palette().fg };
    div()
        .id(ElementId::Name(element_id.to_string().into()))
        .size(px(28.0))
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .rounded(px(8.0))
        .border_1()
        .border_color(rgb(palette().os_edge))
        .when(dimmed, |btn| btn.opacity(0.45))
        .when(!dimmed, |btn| btn.cursor_pointer())
        .interaction_spring(
            element_id.to_string(),
            !dimmed,
            MotionPaint::new().border(rgb(palette().os_edge)),
            MotionPaint::new()
                .bg(rgb(palette().os_fill))
                .border(rgb(palette().os_active_edge)),
        )
        .child(icon_spin(glyph, 14.0, ink, spin))
        .tooltip(move |window, cx| crate::chrome::tooltip(tip.clone()).build(window, cx))
        .on_click(move |event, window, cx| {
            if dimmed {
                return;
            }
            on_click(event, window, cx);
        })
}

fn refresh_button(id: &str, refreshing: bool, view: WeakEntity<AccountsPage>) -> impl IntoElement {
    let action_id = id.to_string();
    let tip = if refreshing {
        crate::i18n::t("common.loading")
    } else {
        crate::i18n::t("common.refresh")
    };
    card_icon_button(
        format!("refresh-{id}"),
        gpui_kit::assets::IconName::RefreshCw,
        tip,
        refreshing,
        refreshing,
        move |_, _, cx| {
            let id = action_id.clone();
            let _ = view.update(cx, |this, cx| {
                this.run(AccountAction::Refresh(id), cx);
            });
        },
    )
}

#[cfg(test)]
mod tests {
    use super::credit_line;
    use ss_usage::subscription::{CODEX_CREDITS, CODEX_CREDITS_UNLIMITED, CreditInfo};

    fn credit(amount: Option<&str>) -> CreditInfo {
        CreditInfo {
            credit_type: CODEX_CREDITS.into(),
            credit_amount: amount.map(str::to_string),
            minimum_credit_amount_for_usage: None,
        }
    }

    #[test]
    fn codex_points_use_the_points_label() {
        let lang = crate::i18n::set_language_for_test("zh-CN");
        assert_eq!(
            credit_line(&credit(Some("25"))).unwrap(),
            ("额度点数".into(), "25".into())
        );
        assert_eq!(
            credit_line(&credit(Some("58654.7347730000"))).unwrap(),
            ("额度点数".into(), "58654.73".into())
        );
        assert_eq!(
            credit_line(&credit(Some(CODEX_CREDITS_UNLIMITED))).unwrap(),
            ("额度点数".into(), "无限".into())
        );
        assert!(credit_line(&credit(None)).is_none());
        lang.set("en");
        assert_eq!(
            credit_line(&credit(Some(CODEX_CREDITS_UNLIMITED))).unwrap(),
            ("Credits".into(), "Unlimited".into())
        );
    }
}

fn auth_tag(mode: AuthMode) -> Option<SharedString> {
    match mode {
        AuthMode::OAuth => None,
        AuthMode::ApiKey => Some("API KEY".into()),
        AuthMode::Cookie => Some("COOKIE".into()),
        AuthMode::TokenImport => Some("TOKEN".into()),
        AuthMode::Manual => Some(crate::i18n::t("usage.authManual")),
    }
}
