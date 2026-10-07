//! Accounts — remaining-quota cards.
//!
//! The provider list lives in the shell's left menu. This page is the
//! scrolling legend cards: fixed-width cards, two per row within one
//! provider, a new row for the next provider. Each account switches, refreshes, or signs out.
//! Adding an account opens a centered dialog.

mod card;
mod dialog;
mod dialog_chrome;
mod frame;
mod meters;
mod pane;
mod rail;
mod reset_bank;
mod theme;
mod types;

pub use types::AccountAction;

use gpui_kit::assets::IconName;
use gpui_kit::component::input::InputState;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::*;
use ss_usage::accounts::{
    OAuthStartDto, SubscriptionDto, delete_subscription, list_subscriptions,
    refresh_subscription_usage, reset_subscription_quota, set_active_subscription,
};

use crate::accounts::theme::palette;
use crate::chrome::{bar_refresh_button, icon, window_drag};
use crate::spawn_domain;
use dialog::WarnKind;

/// Subscription cockpit. `selected_filter` `None` is 全部.
pub struct AccountsPage {
    subscriptions: Vec<SubscriptionDto>,
    selected_filter: Option<String>,
    error: Option<String>,
    status: Option<String>,
    busy_id: Option<String>,
    pub(crate) confirm_delete_id: Option<String>,
    pub(crate) confirm_reset_id: Option<String>,
    /// Checkbox on the reset-card confirm. Cleared whenever that dialog opens.
    reset_acked: bool,
    reset_window: ss_usage::subscription::ResetWindow,
    /// Account whose reset-card tooltip is open.
    reset_tip_id: Option<String>,
    /// A countdown loop is already scheduled.
    reset_clock: bool,
    /// This refresh is spending a reset card, not switching or reloading.
    resetting: bool,
    reset_consumed: Option<(
        String,
        ss_usage::subscription::ResetWindow,
        std::time::Instant,
    )>,
    /// Header refresh is walking every subscription.
    refreshing_all: bool,
    add_catalog: Option<String>,
    add_secret: Option<Entity<InputState>>,
    /// Callback paste while a browser login is waiting. Not the API key field.
    add_callback: Option<Entity<InputState>>,
    /// GLM upstream (`zai` / `bigmodel`). `None` for everyone else.
    add_region: Option<String>,
    add_pending: Option<OAuthStartDto>,
    add_error: Option<String>,
    add_busy: Option<&'static str>,
    /// Quota data. Header spin frames notify this page without bumping it.
    pane_epoch: u64,
    pane: Option<Entity<pane::AccountsPane>>,
}

impl AccountsPage {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            subscriptions: Vec::new(),
            selected_filter: None,
            error: None,
            status: None,
            busy_id: None,
            confirm_delete_id: None,
            confirm_reset_id: None,
            reset_acked: false,
            reset_window: Default::default(),
            reset_tip_id: None,
            reset_clock: false,
            resetting: false,
            reset_consumed: None,
            refreshing_all: false,
            add_catalog: None,
            add_secret: None,
            add_callback: None,
            add_region: None,
            add_pending: None,
            add_error: None,
            add_busy: None,
            pane_epoch: 0,
            pane: None,
        };
        this.load();
        this
    }

    pub(super) fn pane_epoch(&self) -> u64 {
        self.pane_epoch
    }

    /// Quota data changed. The header refresh spin notifies this page and
    /// must not come through here.
    pub(crate) fn revise(&mut self, cx: &mut Context<Self>) {
        self.pane_epoch = self.pane_epoch.wrapping_add(1);
        cx.notify();
    }

    /// Loads subscriptions: active accounts first, then sort index.
    pub fn load(&mut self) {
        match list_subscriptions() {
            Ok(mut subs) => {
                subs.sort_by(|a, b| {
                    b.is_active
                        .cmp(&a.is_active)
                        .then_with(|| a.sort_index.cmp(&b.sort_index))
                        .then_with(|| a.catalog_id.cmp(&b.catalog_id))
                });
                self.subscriptions = subs;
                self.error = None;
            }
            Err(err) => self.error = Some(err.to_string()),
        }
    }

    pub(crate) fn run(&mut self, action: AccountAction, cx: &mut Context<Self>) {
        if self.busy_id.is_some() || self.refreshing_all {
            return;
        }
        let entity = cx.entity();
        macro_rules! dispatch {
            ($fut:expr, $label:expr) => {{
                let label = $label;
                let ok_label = label.clone();
                spawn_domain(&entity, cx, $fut, move |this, cx, res| {
                    if this.resetting && res.is_ok() {
                        this.animate_reset_consumed(cx);
                    }
                    this.busy_id = None;
                    this.resetting = false;
                    match res {
                        Ok(_) => {
                            this.status =
                                Some(format!("{ok_label} {}", crate::i18n::t("common.done")));
                            this.load();
                        }
                        Err(err) => this.status = Some(format!("{label}: {err}")),
                    }
                    this.revise(cx);
                })
            }};
        }

        self.busy_id = match &action {
            AccountAction::Activate(id)
            | AccountAction::Refresh(id)
            | AccountAction::ResetQuota(id, _)
            | AccountAction::Delete(id) => Some(id.clone()),
        };
        self.resetting = matches!(action, AccountAction::ResetQuota(_, _));

        match action {
            AccountAction::Activate(id) => {
                dispatch!(
                    set_active_subscription(id),
                    crate::i18n::t("usage.setActive").to_string()
                )
            }
            AccountAction::Refresh(id) => {
                dispatch!(
                    refresh_subscription_usage(id),
                    crate::i18n::t("common.refresh").to_string()
                )
            }
            AccountAction::ResetQuota(id, window) => dispatch!(
                reset_subscription_quota(id, window),
                crate::i18n::t("usage.resetQuota").to_string()
            ),
            AccountAction::Delete(id) => dispatch!(
                delete_subscription(id),
                crate::i18n::t("settings.githubAuthLogout").to_string()
            ),
        }
        self.revise(cx);
    }

    /// Header refresh walks every subscription. Cards keep their own
    /// `busy_id`, so this does not stamp a shared id onto a row.
    fn refresh_all(&mut self, cx: &mut Context<Self>) {
        if self.refreshing_all {
            return;
        }
        let ids: Vec<String> = self.subscriptions.iter().map(|s| s.id.clone()).collect();
        if ids.is_empty() {
            return;
        }
        self.refreshing_all = true;
        self.revise(cx);
        let entity = cx.entity();
        spawn_domain(
            &entity,
            cx,
            async move {
                for id in ids {
                    let _ = refresh_subscription_usage(id).await;
                }
            },
            |this, cx, _| {
                this.refreshing_all = false;
                this.load();
                this.status = Some(crate::i18n::t("common.refreshed").to_string());
                this.revise(cx);
            },
        );
    }

    fn render_header(&self, view: WeakEntity<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .h(px(48.0))
            .px_4()
            .gap_3()
            .flex_shrink_0()
            .border_b_1()
            .border_color(rgb(palette().border))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .flex_shrink_0()
                    .child(
                        div()
                            .size(px(32.0))
                            .rounded_lg()
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(rgb(palette().accent_soft))
                            .border_1()
                            .border_color(rgb(palette().accent))
                            .child(icon(IconName::Users, 16.0, palette().accent)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(palette().fg))
                                    .child(crate::i18n::t("sidebar.accounts")),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgb(palette().fg_muted))
                                    .child(crate::i18n::t("accounts.panelSubtitle")),
                            ),
                    ),
            )
            .child(window_drag(
                "accounts-header-drag",
                div().flex_1().min_w(px(48.0)).h_full(),
            ))
            .child(
                bar_refresh_button("accounts-refresh-all", self.refreshing_all).on_click(
                    move |_, _, cx| {
                        let _ = view.update(cx, |this, cx| this.refresh_all(cx));
                    },
                ),
            )
    }

    fn render_pane(&self, columns: usize, view: WeakEntity<Self>) -> impl IntoElement {
        let mut scroll = div()
            .size_full()
            .bg(rgb(palette().panel))
            .flex()
            .flex_col()
            .items_stretch()
            .justify_start()
            .gap_4()
            .pt(px(12.0))
            .pr_1()
            .overflow_y_scrollbar();

        if let Some(err) = &self.error {
            scroll = scroll.child(notice(err, true));
        }
        if let Some(status) = &self.status {
            scroll = scroll.child(notice(status, false));
        }

        for entry in self.visible_families() {
            scroll = scroll.child(self.render_provider_card(
                entry.display_name,
                entry.id,
                columns,
                view.clone(),
            ));
        }

        div().flex_1().min_w_0().min_h_0().h_full().child(scroll)
    }
}

fn notice(text: &str, bad: bool) -> Div {
    div()
        .text_size(px(12.0))
        .text_color(rgb(if bad {
            palette().os_bad
        } else {
            palette().os_muted
        }))
        .child(text.to_string())
}

impl Render for AccountsPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.arm_reset_clock(cx);
        let view = cx.entity().downgrade();
        let pane = self.ensure_pane(cx);
        let mut page = div()
            .relative()
            .size_full()
            .bg(rgb(palette().panel))
            .flex()
            .flex_col()
            .text_color(rgb(palette().fg))
            .child(self.render_header(view.clone()))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(crate::chrome::replay_view(pane.into())),
            );

        if self.add_catalog.is_some() {
            page = page.child(self.render_add_dialog(view.clone()));
        } else if self.confirm_delete_id.is_some() {
            page = page.child(self.render_warn(WarnKind::Logout, view.clone()));
        }

        page
    }
}
