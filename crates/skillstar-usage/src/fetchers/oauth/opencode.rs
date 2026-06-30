//! OpenCode first-party services — OAuth PKCE fetcher.
//!
//! Flow (PKCE + local-server callback):
//! 1. Generate PKCE challenge / verifier + state.
//! 2. Open `https://auth.opencode.ai/authorize` with our own redirect_uri.
//! 3. User clicks "Continue with GitHub" / "Continue with Google".
//! 4. After GitHub/Google OAuth, `auth.opencode.ai` redirects to our
//!    `http://127.0.0.1:{PORT}/auth/callback?code=...&state=...`.
//! 5. Local server catches the code.
//! 6. POST `https://auth.opencode.ai/token` to exchange code → CLI OAuth tokens.
//!
//! Refresh: `POST auth.opencode.ai/token` with `grant_type=refresh_token`.

use chrono::Utc;
use serde::Deserialize;
use std::sync::LazyLock;
use std::time::Duration;

use crate::catalog::AuthMode;
use crate::crypto;
use crate::oauth::local_server;
use crate::oauth::pkce::PkcePair;
use crate::oauth::token_refresh;
use crate::oauth_clients;
use crate::storage;
use crate::subscription::{BillingCycle, Subscription, SubscriptionUsage};
use crate::{UsageError, UsageResult};

const AUTHORIZE_URL: &str = "https://auth.opencode.ai/authorize";
const TOKEN_URL: &str = "https://auth.opencode.ai/token";
const CALLBACK_PORT: u16 = 1457;
static CLIENT_ID: LazyLock<String> =
    LazyLock::new(|| oauth_clients::client_id!("opencode", "SKILLSTAR_OPENCODE_CLIENT_ID", "app"));

fn oauth_usage_unavailable() -> String {
    "OpenCode 官方 OAuth token 只适用于 CLI 授权，不能读取 opencode.ai 控制台用量；请在订阅设置中切换到 Cookie 模式，并从 opencode.ai 控制台请求复制 Cookie。".to_string()
}

#[derive(Debug, Deserialize, Default)]
struct TokenResponse {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

pub async fn start_login(
    _region: Option<&str>,
    target_subscription_id: Option<&str>,
) -> UsageResult<super::OAuthStartInfo> {
    let pkce = PkcePair::generate();
    let state = crate::oauth::pkce::random_state();
    let redirect_uri = format!("http://127.0.0.1:{}/auth/callback", CALLBACK_PORT);

    let auth_url = format!(
        "{}?client_id={}&redirect_uri={}&response_type=code&code_challenge={}&code_challenge_method=S256&state={}",
        AUTHORIZE_URL,
        CLIENT_ID.as_str(),
        urlencoding(&redirect_uri),
        pkce.challenge,
        state,
    );

    let pending_id = crate::oauth::pending_state::register("opencode", None, auth_url.clone());
    crate::oauth::pending_state::set_target_subscription_id(
        &pending_id,
        target_subscription_id.map(str::to_string),
    );

    let pid = pending_id.clone();
    let verifier = pkce.verifier.clone();
    let redirect = redirect_uri.clone();
    let state_for_task = state.clone();
    tokio::spawn(async move {
        let target_subscription_id = crate::oauth::pending_state::target_subscription_id(&pid);
        let result = drive_login(state_for_task, verifier, redirect, target_subscription_id).await;
        if let Some(tx) = crate::oauth::pending_state::take_sender(&pid) {
            let _ = tx.send(result);
        }
    });

    Ok(super::OAuthStartInfo::browser(auth_url, pending_id))
}

async fn drive_login(
    state: String,
    verifier: String,
    redirect_uri: String,
    target_subscription_id: Option<String>,
) -> UsageResult<Subscription> {
    let code =
        local_server::wait_for_callback(CALLBACK_PORT, state, Some(Duration::from_secs(300)))
            .await?;

    let (access_token, refresh_token, expires_at) =
        exchange_code(&code, &verifier, &redirect_uri).await?;

    let display_name = token_refresh::jwt_string(&access_token, &["email"])
        .unwrap_or_else(|| "OpenCode".to_string());

    let now = Utc::now().timestamp();
    let existing = target_subscription_id
        .as_deref()
        .and_then(|id| storage::get_subscription(id).ok())
        .filter(|sub| sub.catalog_id == "opencode" && sub.auth_mode == AuthMode::OAuth);

    let sub = Subscription {
        id: existing
            .as_ref()
            .map(|sub| sub.id.clone())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        catalog_id: "opencode".to_string(),
        display_name,
        auth_mode: AuthMode::OAuth,
        plan_tier: existing.as_ref().and_then(|sub| sub.plan_tier.clone()),
        monthly_price: existing.as_ref().and_then(|sub| sub.monthly_price),
        currency: existing
            .as_ref()
            .map(|sub| sub.currency.clone())
            .unwrap_or_else(|| "USD".to_string()),
        billing_cycle: existing
            .as_ref()
            .map(|sub| sub.billing_cycle)
            .unwrap_or(BillingCycle::Monthly),
        start_date: existing.as_ref().map(|sub| sub.start_date).unwrap_or(0),
        renew_date: existing.as_ref().map(|sub| sub.renew_date).unwrap_or(0),
        auto_renew: existing.as_ref().map(|sub| sub.auto_renew).unwrap_or(false),
        api_key_encrypted: None,
        platform_token_encrypted: None,
        access_token_encrypted: Some(crypto::encrypt(&access_token)),
        refresh_token_encrypted: refresh_token.as_deref().map(crypto::encrypt),
        access_token_expires_at: expires_at,
        id_token_encrypted: None,
        oauth_account_id: None,
        oauth_region: None,
        requires_reauth: false,
        fingerprint_id: None,
        cookie_jar_encrypted: None,
        cookie_session_expires_at: None,
        manual_quota: existing.as_ref().and_then(|sub| sub.manual_quota.clone()),
        note: existing.as_ref().and_then(|sub| sub.note.clone()),
        sort_index: existing.as_ref().map(|sub| sub.sort_index).unwrap_or(0),
        created_at: existing.as_ref().map(|sub| sub.created_at).unwrap_or(now),
        updated_at: now,
    };

    let usage = authorized_snapshot_with_warning(&sub.id, oauth_usage_unavailable());
    storage::save_usage_snapshot(usage).ok();

    let saved = storage::upsert_subscription(sub)
        .map_err(|e| UsageError::Other(format!("OpenCode 订阅保存失败：{}", e)))?;
    Ok(saved)
}

fn authorized_snapshot_with_warning(subscription_id: &str, warning: String) -> SubscriptionUsage {
    SubscriptionUsage {
        subscription_id: subscription_id.to_string(),
        fetched_at: Utc::now().timestamp(),
        plan_name: Some("OpenCode".to_string()),
        hourly: None,
        weekly: None,
        monthly: None,
        balance: None,
        credits: Vec::new(),
        error: Some(format!(
            "OpenCode 已重新授权，但用量探测暂时不可用：{}",
            warning
        )),
        api_keys: Vec::new(),
        deepseek_analytics: None,
    }
}

async fn exchange_code(
    code: &str,
    verifier: &str,
    redirect_uri: &str,
) -> UsageResult<(String, Option<String>, Option<i64>)> {
    let client = crate::http_client::usage_reqwest_with_active_fingerprint()?;
    let resp = client
        .post(TOKEN_URL)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", CLIENT_ID.as_str()),
            ("code_verifier", verifier),
        ])
        .send()
        .await
        .map_err(|e| UsageError::Fetcher(format!("OpenCode token 交换：{}", e)))?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(UsageError::Fetcher(format!(
            "OpenCode token 返回 {}：{}",
            status,
            body.chars().take(200).collect::<String>()
        )));
    }

    let tokens: TokenResponse = resp
        .json()
        .await
        .map_err(|e| UsageError::Fetcher(format!("OpenCode 解析 token：{}", e)))?;

    let access_token = tokens
        .access_token
        .ok_or_else(|| UsageError::Other("OpenCode 缺少 access_token".into()))?;

    let expires_at = tokens
        .expires_in
        .map(|s| Utc::now().timestamp() + s)
        .or_else(|| token_refresh::jwt_exp(&access_token));

    Ok((access_token, tokens.refresh_token, expires_at))
}

pub async fn fetch(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    let fp_id = subscription.fingerprint_id.clone();
    crate::http_client::with_fingerprint(fp_id, fetch_inner(subscription)).await
}

async fn fetch_inner(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    Ok(authorized_snapshot_with_warning(
        &subscription.id,
        oauth_usage_unavailable(),
    ))
}

fn urlencoding(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}
