//! Centered add-account dialog and the destructive confirm.
//!
//! Login methods stay inside the dialog. The card only keeps 添加账号.
//! Escape is not wired here; the mask and the close button dismiss it.

use gpui_kit::component::input::{Input, InputState};
use gpui_kit::*;
use ss_usage::accounts::{
    CreateSubscriptionInput, OAuthStartDto, await_oauth_completion, cancel_oauth_login,
    create_subscription, import_subscription_from_local, import_subscription_token,
    start_oauth_login, submit_oauth_callback,
};
use ss_usage::catalog::AuthMode;
use ss_usage::fetchers::oauth::OAuthFlow;
use ss_usage::local_import::local_import_supported;

use super::AccountsPage;
use super::dialog_chrome::*;
use super::types::{AccountAction, find_catalog_entry};
use crate::accounts::theme::palette;
use crate::chrome::{InteractionSpring, MotionPaint};
use crate::spawn_domain;

const FORM_NEED_TEXT: &str = "请先填写内容";
const FORM_NEED_CALLBACK: &str = "请先粘贴回调链接";

fn form_error(message: &str) -> bool {
    message == FORM_NEED_TEXT || message == FORM_NEED_CALLBACK
}

fn form_error_line(message: &str) -> Div {
    div()
        .text_size(px(12.0))
        .text_color(rgb(palette().os_bad))
        .child(message.to_string())
}

fn needs_callback(flow: &OAuthFlow) -> bool {
    matches!(
        flow,
        OAuthFlow::LocalCallback | OAuthFlow::SchemePaste { .. }
    )
}

impl AccountsPage {
    pub(crate) fn open_add(
        &mut self,
        catalog_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entry = find_catalog_entry(&catalog_id);
        let placeholder = entry.as_ref().map(secret_placeholder).unwrap_or("");
        self.add_secret = Some(cx.new(|cx| InputState::new(window, cx).placeholder(placeholder)));
        let oauth = entry
            .as_ref()
            .is_some_and(|entry| entry.auth_modes.contains(&AuthMode::OAuth));
        self.add_callback = oauth
            .then(|| cx.new(|cx| InputState::new(window, cx).placeholder("回调链接或授权 code")));
        self.add_region = entry
            .as_ref()
            .and_then(|entry| entry.regions.first().copied())
            .map(str::to_string);
        self.add_catalog = Some(catalog_id);
        self.add_pending = None;
        self.add_error = None;
        self.add_busy = None;
        self.confirm_delete_id = None;
        self.confirm_reset_id = None;
        self.revise(cx);
    }

    pub(crate) fn close_add(&mut self, cx: &mut Context<Self>) {
        if let Some(pending) = self.add_pending.take() {
            let _ = cancel_oauth_login(pending.pending_id);
        }
        self.add_catalog = None;
        self.add_callback = None;
        self.add_region = None;
        self.add_error = None;
        self.add_busy = None;
        self.revise(cx);
    }

    fn secret_text(&self, cx: &App) -> String {
        self.add_secret
            .as_ref()
            .map(|input| input.read(cx).value().trim().to_string())
            .unwrap_or_default()
    }

    pub(crate) fn start_login(&mut self, cx: &mut Context<Self>) {
        let Some(catalog_id) = self.add_catalog.clone() else {
            return;
        };
        if self.add_busy.is_some() {
            return;
        }
        self.add_busy = Some("login");
        self.add_error = None;
        let region = self.add_region.clone();
        let entity = cx.entity();
        spawn_domain(
            &entity,
            cx,
            async move { start_oauth_login(catalog_id, region, None).await },
            |this, cx, res| {
                this.add_busy = None;
                match res {
                    Ok(start) => {
                        let url = auth_url_of(&start);
                        if !url.is_empty() {
                            cx.open_url(&url);
                        }
                        let pending_id = start.pending_id.clone();
                        this.add_pending = Some(start);
                        this.await_login(pending_id, cx);
                    }
                    Err(err) => this.add_error = Some(err.to_string()),
                }
                this.revise(cx);
            },
        );
        self.revise(cx);
    }

    fn submit_callback(&mut self, cx: &mut Context<Self>) {
        let Some(pending_id) = self
            .add_pending
            .as_ref()
            .map(|pending| pending.pending_id.clone())
        else {
            return;
        };
        if self.add_busy.is_some() {
            return;
        }
        let text = self
            .add_callback
            .as_ref()
            .map(|input| input.read(cx).value().trim().to_string())
            .unwrap_or_default();
        if text.is_empty() {
            self.add_error = Some(FORM_NEED_CALLBACK.into());
            self.revise(cx);
            return;
        }
        self.add_busy = Some("callback");
        self.add_error = None;
        let entity = cx.entity();
        spawn_domain(
            &entity,
            cx,
            async move { submit_oauth_callback(pending_id, text).await },
            |this, cx, res| {
                this.add_busy = None;
                if let Err(err) = res {
                    this.add_error = Some(err.to_string());
                }
                this.revise(cx);
            },
        );
        self.revise(cx);
    }

    fn await_login(&mut self, pending_id: String, cx: &mut Context<Self>) {
        let entity = cx.entity();
        spawn_domain(
            &entity,
            cx,
            async move { await_oauth_completion(pending_id).await },
            |this, cx, res| {
                if this.add_catalog.is_none() {
                    return;
                }
                match res {
                    Ok(_) => {
                        this.add_pending = None;
                        this.add_catalog = None;
                        this.add_error = None;
                        this.status = Some("已添加账号".into());
                        this.load();
                    }
                    Err(err) => {
                        this.add_pending = None;
                        this.add_error = Some(err.to_string());
                    }
                }
                this.revise(cx);
            },
        );
    }

    pub(crate) fn import_local(&mut self, cx: &mut Context<Self>) {
        let Some(catalog_id) = self.add_catalog.clone() else {
            return;
        };
        if self.add_busy.is_some() {
            return;
        }
        self.add_busy = Some("import");
        self.add_error = None;
        let entity = cx.entity();
        spawn_domain(
            &entity,
            cx,
            async move { import_subscription_from_local(catalog_id).await },
            |this, cx, res| {
                this.add_busy = None;
                match res {
                    Ok(_) => {
                        this.add_catalog = None;
                        this.status = Some("已从本机导入".into());
                        this.load();
                    }
                    Err(err) => this.add_error = Some(err.to_string()),
                }
                this.revise(cx);
            },
        );
        self.revise(cx);
    }

    pub(crate) fn save_secret(&mut self, mode: AuthMode, cx: &mut Context<Self>) {
        let Some(catalog_id) = self.add_catalog.clone() else {
            return;
        };
        let secret = self.secret_text(cx);
        if secret.is_empty() {
            self.add_error = Some(FORM_NEED_TEXT.into());
            self.revise(cx);
            return;
        }
        if mode == AuthMode::TokenImport {
            self.add_busy = Some("token");
            self.add_error = None;
            let entity = cx.entity();
            spawn_domain(
                &entity,
                cx,
                async move { import_subscription_token(catalog_id, secret, None).await },
                |this, cx, res| {
                    this.add_busy = None;
                    match res {
                        Ok(_) => {
                            this.add_catalog = None;
                            this.status = Some("已导入凭证".into());
                            this.load();
                        }
                        Err(err) => this.add_error = Some(err.to_string()),
                    }
                    this.revise(cx);
                },
            );
            self.revise(cx);
            return;
        }

        let mut input = CreateSubscriptionInput {
            catalog_id,
            display_name: None,
            auth_mode: mode,
            plan_tier: None,
            monthly_price: None,
            currency: None,
            billing_cycle: None,
            start_date: None,
            renew_date: None,
            auto_renew: None,
            api_key: None,
            platform_token: None,
            oauth_region: None,
            manual_quota: None,
            note: None,
            cookie_header: None,
        };
        match mode {
            AuthMode::ApiKey => input.api_key = Some(secret),
            AuthMode::Cookie => input.cookie_header = Some(secret),
            AuthMode::Manual => input.display_name = Some(secret),
            AuthMode::OAuth | AuthMode::TokenImport => {}
        }
        match create_subscription(input) {
            Ok(_) => {
                self.add_catalog = None;
                self.add_error = None;
                self.status = Some("已添加账号".into());
                self.load();
            }
            Err(err) => self.add_error = Some(err.to_string()),
        }
        self.revise(cx);
    }

    pub(crate) fn render_add_dialog(&self, view: WeakEntity<Self>) -> impl IntoElement {
        let catalog_id = self.add_catalog.clone().unwrap_or_default();
        let entry = find_catalog_entry(&catalog_id);
        let title = entry
            .as_ref()
            .map(|entry| entry.display_name.to_string())
            .unwrap_or_else(|| catalog_id.clone());
        let close = view.clone();

        let mut card =
            dialog_card(480.0).child(dialog_head(&catalog_id, "添加账号", &title, close));

        let mut stack = div().flex().flex_col().gap(px(10.0)).px(px(24.0)).w_full();

        if let Some(err) = self.add_error.as_deref().filter(|err| !form_error(err)) {
            stack = stack.child(
                div()
                    .p_3()
                    .rounded(px(10.0))
                    .bg(rgb(palette().os_fill))
                    .text_size(px(12.0))
                    .text_color(rgb(palette().os_bad))
                    .child(format!("失败: {err}")),
            );
        }

        if let Some(pending) = &self.add_pending {
            stack = stack.child(self.auth_panel(pending, view.clone()));
        } else if let Some(entry) = entry.as_ref() {
            stack = stack.child(self.login_methods(entry, view.clone()));
        }

        card = card.child(stack);
        overlay(view, card)
    }

    fn auth_panel(&self, pending: &OAuthStartDto, view: WeakEntity<Self>) -> Div {
        let mut panel = div().flex().flex_col().gap_3().w_full();
        panel = panel.child(
            div()
                .text_size(px(13.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(palette().fg))
                .child(crate::i18n::t("usage.oauthWaiting")),
        );
        let hint = if pending
            .user_code
            .as_deref()
            .is_some_and(|code| !code.is_empty())
        {
            "usage.oauthDeviceHint"
        } else {
            "usage.oauthWaitingHint"
        };
        panel = panel.child(
            div()
                .text_size(px(12.0))
                .text_color(rgb(palette().os_muted))
                .child(crate::i18n::t(hint)),
        );
        if let Some(code) = pending.user_code.as_deref().filter(|c| !c.is_empty()) {
            panel = panel.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .px_3()
                    .py_2()
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(rgb(palette().os_edge))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(rgb(palette().os_muted))
                            .child(crate::i18n::t("usage.userCode")),
                    )
                    .child(
                        div()
                            .font_family("JetBrains Mono")
                            .text_size(px(16.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(palette().fg))
                            .child(code.to_string()),
                    ),
            );
        }
        let url = auth_url_of(pending);
        if !url.is_empty() {
            let open_url = url.clone();
            panel = panel.child(
                div()
                    .id("accounts-open-auth")
                    .text_size(px(12.5))
                    .text_color(rgb(palette().fg))
                    .underline()
                    .cursor_pointer()
                    .child(crate::i18n::t("usage.oauthOpenLink"))
                    .on_click(move |_, _, cx| {
                        cx.open_url(&open_url);
                    }),
            );
        }
        if needs_callback(&pending.flow)
            && let Some(input) = &self.add_callback
        {
            let saving = self.add_busy == Some("callback");
            panel = panel.child(
                div()
                    .text_size(px(12.0))
                    .text_color(rgb(palette().os_muted))
                    .child("没自动返回时，粘贴回调链接或授权 code"),
            );
            panel = panel.child(
                Input::new(input)
                    .aria_label("回调")
                    .h(px(40.0))
                    .rounded(px(10.0)),
            );
            if self.add_error.as_deref() == Some(FORM_NEED_CALLBACK) {
                panel = panel.child(form_error_line(FORM_NEED_CALLBACK));
            }
            let submit = view.clone();
            panel = panel.child(commit_button(
                "accounts-submit-callback",
                "提交回调",
                "正在提交",
                true,
                saving,
                self.add_busy.is_some(),
                submit,
                |this, cx| this.submit_callback(cx),
            ));
        }
        let cancel = view.clone();
        panel.child(
            div()
                .id("accounts-cancel-auth")
                .h(px(36.0))
                .px_4()
                .self_start()
                .flex()
                .items_center()
                .rounded(px(8.0))
                .border_1()
                .border_color(rgb(palette().os_edge))
                .text_size(px(13.0))
                .font_weight(FontWeight::MEDIUM)
                .cursor_pointer()
                .interaction_spring(
                    "accounts-cancel-auth",
                    true,
                    MotionPaint::new(),
                    MotionPaint::new().bg(rgb(palette().os_fill)),
                )
                .child(crate::i18n::t("common.cancel"))
                .on_click(move |_, _, cx| {
                    let _ = cancel.update(cx, |this, cx| this.close_add(cx));
                }),
        )
    }

    fn login_methods(
        &self,
        entry: &ss_usage::catalog::CatalogEntry,
        view: WeakEntity<Self>,
    ) -> Div {
        let mut rows = div().flex().flex_col().gap(px(8.0)).w_full();
        let busy = self.add_busy;
        let has_session =
            entry.auth_modes.contains(&AuthMode::OAuth) || local_import_supported(entry.id);

        if !entry.regions.is_empty() {
            rows = rows.child(self.region_picks(entry.regions, view.clone()));
        }
        if entry.auth_modes.contains(&AuthMode::OAuth) {
            rows = rows.child(action_row(
                "accounts-login-oauth",
                "登录",
                "打开浏览器完成登录",
                "正在登录",
                true,
                busy == Some("login"),
                busy.is_some(),
                view.clone(),
                |this, cx| this.start_login(cx),
            ));
        }
        if local_import_supported(entry.id) {
            rows = rows.child(action_row(
                "accounts-login-import",
                "导入本机会话",
                "复制本机已有登录，原来的凭证保持不动",
                "正在核对额度",
                false,
                busy == Some("import"),
                busy.is_some(),
                view.clone(),
                |this, cx| this.import_local(cx),
            ));
        }

        let paste = entry.auth_modes.iter().copied().find(|mode| {
            matches!(
                mode,
                AuthMode::ApiKey | AuthMode::Cookie | AuthMode::TokenImport | AuthMode::Manual
            )
        });
        if let Some(mode) = paste {
            if let Some(input) = &self.add_secret {
                let label = field_copy(mode).0;
                if has_session {
                    rows = rows.child(or_divider());
                }
                let saving = busy == Some("token");
                let mut block = div().flex().flex_col().gap(px(8.0)).w_full().child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(palette().fg))
                        .child(label),
                );
                block = block.child(
                    Input::new(input)
                        .aria_label(label)
                        .h(px(40.0))
                        .rounded(px(10.0)),
                );
                if self.add_error.as_deref() == Some(FORM_NEED_TEXT) {
                    block = block.child(form_error_line(FORM_NEED_TEXT));
                }
                if let Some(hint) = paste_hint(mode, entry.warning) {
                    block = block.child(
                        div()
                            .text_size(px(12.0))
                            .line_height(px(18.0))
                            .text_color(rgb(palette().os_muted))
                            .child(hint.to_string()),
                    );
                }
                block = block.child(commit_button(
                    "accounts-login-save",
                    save_label(mode),
                    save_busy_label(mode),
                    !has_session,
                    saving,
                    busy.is_some(),
                    view.clone(),
                    move |this, cx| this.save_secret(mode, cx),
                ));
                rows = rows.child(block);
            }
        }
        rows
    }

    fn region_picks(&self, regions: &[&str], view: WeakEntity<Self>) -> Div {
        let selected = self
            .add_region
            .as_deref()
            .unwrap_or_else(|| regions.first().copied().unwrap_or(""));
        let mut row = div().flex().gap(px(8.0)).w_full();
        for id in regions {
            let on = *id == selected;
            let pick = (*id).to_string();
            let target = view.clone();
            row = row.child(
                div()
                    .id(ElementId::Name(format!("accounts-region-{id}").into()))
                    .flex_1()
                    .h(px(36.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(rgb(if on {
                        palette().accent
                    } else {
                        palette().os_edge
                    }))
                    .bg(rgb(if on {
                        palette().os_fill
                    } else {
                        palette().panel
                    }))
                    .text_size(px(13.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(if on { palette().fg } else { palette().os_muted }))
                    .cursor_pointer()
                    .child(region_name(id).to_string())
                    .on_click(move |_, _, cx| {
                        let pick = pick.clone();
                        let _ = target.update(cx, |this, cx| {
                            this.add_region = Some(pick);
                            this.revise(cx);
                        });
                    }),
            );
        }
        row
    }

    pub(crate) fn render_warn(&self, kind: WarnKind, view: WeakEntity<Self>) -> impl IntoElement {
        let (title, body, ok_label, danger) = match kind {
            WarnKind::Logout => (
                "退出账号",
                "退出后这个账号会从列表里删除，需要重新登录才能再读取额度。",
                "退出",
                true,
            ),
        };
        let dismiss = view.clone();
        let confirm = view.clone();
        let card = dialog_card(440.0)
            .child(dialog_head("", title, "", dismiss.clone()))
            .child(
                div()
                    .px(px(24.0))
                    .text_size(px(13.0))
                    .text_color(rgb(palette().os_muted))
                    .child(body),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .px(px(24.0))
                    .child(ghost_button(
                        "accounts-warn-cancel",
                        "取消",
                        dismiss,
                        |this, cx| {
                            this.confirm_delete_id = None;
                            this.confirm_reset_id = None;
                            this.revise(cx);
                        },
                    ))
                    .child(primary_button(
                        "accounts-warn-ok",
                        ok_label,
                        danger,
                        confirm,
                        move |this, cx| {
                            if let Some(id) = this.confirm_delete_id.take() {
                                this.run(AccountAction::Delete(id), cx);
                            }
                        },
                    )),
            );
        overlay(view, card)
    }
}

pub(crate) enum WarnKind {
    Logout,
}

fn auth_url_of(start: &OAuthStartDto) -> String {
    if start.auth_url.starts_with("http") {
        start.auth_url.clone()
    } else {
        start.verification_uri.clone().unwrap_or_default()
    }
}

fn secret_placeholder(entry: &ss_usage::catalog::CatalogEntry) -> &'static str {
    if entry.auth_modes.contains(&AuthMode::ApiKey) {
        "API key"
    } else if entry.auth_modes.contains(&AuthMode::Cookie) {
        "Cookie"
    } else if entry.auth_modes.contains(&AuthMode::TokenImport) {
        "卡密 / JSON / refresh token"
    } else if entry.auth_modes.contains(&AuthMode::Manual) {
        "显示名称"
    } else {
        ""
    }
}

fn save_label(mode: AuthMode) -> &'static str {
    match mode {
        AuthMode::ApiKey => "保存密钥",
        AuthMode::Cookie => "保存 Cookie",
        AuthMode::TokenImport => "导入凭证",
        AuthMode::Manual => "保存",
        AuthMode::OAuth => "继续",
    }
}

fn save_busy_label(mode: AuthMode) -> &'static str {
    match mode {
        AuthMode::TokenImport => "正在核对凭证",
        AuthMode::ApiKey | AuthMode::Cookie | AuthMode::Manual | AuthMode::OAuth => "正在保存",
    }
}
