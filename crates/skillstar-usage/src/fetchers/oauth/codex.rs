//! Codex (ChatGPT/OpenAI) OAuth fetcher.
//!
//! Implements the PKCE + local-callback flow described in cockpit-tools
//! `codex_oauth.rs`:
//! 1. Generate PKCE pair + state.
//! 2. Open `https://auth.openai.com/oauth/authorize` with `redirect_uri =
//!    http://127.0.0.1:1455/auth/callback`.
//! 3. Local server catches `?code=...&state=...`.
//! 4. POST `https://auth.openai.com/oauth/token` (form-encoded) to swap.
//! 5. `id_token` JWT carries `chatgpt_plan_type`. `access_token` is used to
//!    call `https://chatgpt.com/backend-api/wham/usage` (with header
//!    `ChatGPT-Account-Id` extracted from JWT).
//!
//! NOTE: The repo already has a Codex account manager in
//! `crates/skillstar-model-config/src/codex_accounts.rs` — long term that
//! state store should be the single source of truth. v1 keeps a separate
//! Subscription record so the usage page is self-contained; v1.1 will
//! reconcile via a shared id.

use chrono::Utc;
use serde::Deserialize;
use serde_json::Value;
use std::time::Duration;

use crate::catalog::AuthMode;
use crate::crypto;
use crate::oauth::local_server;
use crate::oauth::pkce::PkcePair;
use crate::oauth::token_refresh;
use crate::storage;
use crate::subscription::{BillingCycle, Subscription, SubscriptionUsage, UsageWindow};
use crate::{UsageError, UsageResult};

const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const AUTHORIZE_URL: &str = "https://auth.openai.com/oauth/authorize";
const TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
const CALLBACK_PORT: u16 = 1455;
const SCOPES: &str = "openid profile email offline_access";

#[derive(Debug, Deserialize, Default)]
struct TokenResponse {
    #[serde(default)]
    id_token: Option<String>,
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

#[derive(Debug, Deserialize, Default)]
struct UsageResponse {
    #[serde(default)]
    plan_type: Option<String>,
    #[serde(default)]
    rate_limit: Option<RateLimit>,
}

#[derive(Debug, Deserialize, Default)]
struct RateLimit {
    #[serde(default)]
    primary_window: Option<Window>,
    #[serde(default)]
    secondary_window: Option<Window>,
}

#[derive(Debug, Deserialize, Default)]
struct Window {
    #[serde(default)]
    used_percent: Option<i32>,
    #[serde(default)]
    reset_at: Option<i64>,
}

pub async fn start_login(_region: Option<&str>) -> UsageResult<super::OAuthStartInfo> {
    let pkce = PkcePair::generate();
    let state = crate::oauth::pkce::random_state();
    let redirect_uri = format!("http://127.0.0.1:{}/auth/callback", CALLBACK_PORT);
    let auth_url = format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&code_challenge={}&code_challenge_method=S256&state={}",
        AUTHORIZE_URL,
        CLIENT_ID,
        urlencoding(&redirect_uri),
        urlencoding(SCOPES),
        pkce.challenge,
        state,
    );

    let pending_id = crate::oauth::pending_state::register("codex", None, auth_url.clone());

    let pid = pending_id.clone();
    let verifier = pkce.verifier.clone();
    let redirect = redirect_uri.clone();
    let state_for_task = state.clone();
    tokio::spawn(async move {
        let result = drive_login(state_for_task, verifier, redirect).await;
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
) -> UsageResult<Subscription> {
    let code =
        local_server::wait_for_callback(CALLBACK_PORT, state, Some(Duration::from_secs(300)))
            .await?;
    let tokens = exchange_code(&code, &verifier, &redirect_uri).await?;
    let access_token = tokens
        .access_token
        .ok_or_else(|| UsageError::Other("Codex 缺少 access_token".into()))?;
    let id_token = tokens
        .id_token
        .ok_or_else(|| UsageError::Other("Codex 缺少 id_token".into()))?;
    let expires_at = tokens
        .expires_in
        .map(|s| Utc::now().timestamp() + s)
        .or_else(|| token_refresh::jwt_exp(&access_token));

    let account_id = token_refresh::jwt_string(
        &id_token,
        &["https://api.openai.com/auth", "chatgpt_account_id"],
    )
    .or_else(|| token_refresh::jwt_string(&id_token, &["chatgpt_account_id"]))
    .or_else(|| token_refresh::jwt_string(&id_token, &["sub"]));

    finalize(access_token, tokens.refresh_token, expires_at, account_id).await
}

async fn exchange_code(
    code: &str,
    verifier: &str,
    redirect_uri: &str,
) -> UsageResult<TokenResponse> {
    let client = http_client()?;
    let resp = client
        .post(TOKEN_URL)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", CLIENT_ID),
            ("code_verifier", verifier),
        ])
        .send()
        .await
        .map_err(|e| UsageError::Fetcher(format!("Codex token 交换：{}", e)))?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(UsageError::Fetcher(format!(
            "Codex token 返回 {}：{}",
            status,
            body.chars().take(200).collect::<String>()
        )));
    }
    resp.json::<TokenResponse>()
        .await
        .map_err(|e| UsageError::Fetcher(format!("Codex 解析 token: {}", e)))
}

async fn finalize(
    access_token: String,
    refresh_token: Option<String>,
    expires_at: Option<i64>,
    account_id: Option<String>,
) -> UsageResult<Subscription> {
    let now = Utc::now().timestamp();
    let sub = Subscription {
        id: uuid::Uuid::new_v4().to_string(),
        catalog_id: "codex".to_string(),
        display_name: "Codex".to_string(),
        auth_mode: AuthMode::OAuth,
        plan_tier: None,
        monthly_price: None,
        currency: "USD".to_string(),
        billing_cycle: BillingCycle::Monthly,
        start_date: 0,
        renew_date: 0,
        auto_renew: false,
        api_key_encrypted: None,
        access_token_encrypted: Some(crypto::encrypt(&access_token)),
        refresh_token_encrypted: refresh_token.as_deref().map(crypto::encrypt),
        access_token_expires_at: expires_at,
        oauth_account_id: account_id.clone(),
        oauth_region: None,
        requires_reauth: false,
        fingerprint_id: None,
        cookie_jar_encrypted: None,
        cookie_session_expires_at: None,
        manual_quota: None,
        note: None,
        sort_index: 0,
        created_at: now,
        updated_at: now,
    };

    if let Ok(usage) = fetch_with_token(&sub.id, &access_token, account_id.as_deref()).await {
        storage::save_usage_snapshot(usage).ok();
    }
    let saved = storage::upsert_subscription(sub)
        .map_err(|e| UsageError::Other(format!("Codex 订阅保存失败：{}", e)))?;
    Ok(saved)
}

pub async fn fetch(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    let fp_id = subscription.fingerprint_id.clone();
    crate::http_client::with_fingerprint(fp_id, fetch_inner(subscription)).await
}

async fn fetch_inner(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    if token_refresh::needs_refresh(subscription.access_token_expires_at) {
        refresh_codex_tokens(subscription).await?;
    }
    let access_token = decrypt_required(&subscription.access_token_encrypted)?;
    let account_id = subscription.oauth_account_id.clone();
    match fetch_with_token(&subscription.id, &access_token, account_id.as_deref()).await {
        Err(UsageError::AuthRequired) => {
            refresh_codex_tokens(subscription).await?;
            let access_token = decrypt_required(&subscription.access_token_encrypted)?;
            fetch_with_token(
                &subscription.id,
                &access_token,
                subscription.oauth_account_id.as_deref(),
            )
            .await
        }
        other => other,
    }
}

async fn refresh_codex_tokens(subscription: &mut Subscription) -> UsageResult<()> {
    let rt_cipher = subscription
        .refresh_token_encrypted
        .as_deref()
        .ok_or(UsageError::AuthRequired)?;
    let refresh = crypto::decrypt(rt_cipher);
    if refresh.is_empty() {
        return Err(UsageError::AuthRequired);
    }
    let client = crate::http_client::usage_reqwest_with_active_fingerprint()?;
    let resp = client
        .post(TOKEN_URL)
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh.as_str()),
            ("client_id", CLIENT_ID),
        ])
        .send()
        .await
        .map_err(|e| UsageError::Fetcher(format!("Codex refresh：{}", e)))?;
    if !resp.status().is_success() {
        return Err(UsageError::AuthRequired);
    }
    let tokens: TokenResponse = resp
        .json()
        .await
        .map_err(|e| UsageError::Fetcher(format!("Codex refresh 解析：{}", e)))?;
    let access_token = tokens.access_token.ok_or(UsageError::AuthRequired)?;
    subscription.access_token_encrypted = Some(crypto::encrypt(&access_token));
    if let Some(rt) = tokens.refresh_token {
        subscription.refresh_token_encrypted = Some(crypto::encrypt(&rt));
    }
    subscription.access_token_expires_at = tokens
        .expires_in
        .map(|s| Utc::now().timestamp() + s)
        .or_else(|| token_refresh::jwt_exp(&access_token));
    if let Some(id_token) = tokens.id_token.as_deref() {
        let account_id = token_refresh::jwt_string(id_token, &["chatgpt_account_id"])
            .or_else(|| token_refresh::jwt_string(id_token, &["sub"]));
        if account_id.is_some() {
            subscription.oauth_account_id = account_id;
        }
    }
    Ok(())
}

async fn fetch_with_token(
    subscription_id: &str,
    access_token: &str,
    account_id: Option<&str>,
) -> UsageResult<SubscriptionUsage> {
    let client = http_client()?;
    let mut req = client.get(USAGE_URL).bearer_auth(access_token);
    if let Some(account) = account_id {
        req = req.header("ChatGPT-Account-Id", account);
    }
    let resp = req
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|e| UsageError::Fetcher(format!("Codex wham/usage：{}", e)))?;
    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(UsageError::AuthRequired);
    }
    if !status.is_success() {
        return Err(UsageError::Fetcher(format!(
            "Codex wham/usage 状态码 {}",
            status
        )));
    }
    let body: UsageResponse = resp
        .json()
        .await
        .map_err(|e| UsageError::Fetcher(format!("Codex 解析 usage: {}", e)))?;

    let plan_name = body.plan_type.clone().unwrap_or_else(|| "FREE".to_string());
    let (hourly, weekly) = match &body.rate_limit {
        Some(rl) => (
            window(rl.primary_window.as_ref(), "5h"),
            window(rl.secondary_window.as_ref(), "7d"),
        ),
        None => (None, None),
    };

    Ok(SubscriptionUsage {
        subscription_id: subscription_id.to_string(),
        fetched_at: Utc::now().timestamp(),
        plan_name: Some(plan_name),
        hourly,
        weekly,
        monthly: None,
        balance: None,
        credits: Vec::new(),
        error: None,
        api_keys: Vec::new(),
    })
}

fn window(w: Option<&Window>, label: &str) -> Option<UsageWindow> {
    let w = w?;
    let percent = w.used_percent?;
    Some(UsageWindow {
        label: label.to_string(),
        used: percent as i64,
        total: Some(100),
        percent: Some(percent),
        reset_at: w.reset_at,
        breakdown: Vec::new(),
    })
}

fn http_client() -> UsageResult<reqwest::Client> {
    crate::http_client::usage_reqwest_with_active_fingerprint()
}

fn decrypt_required(cipher: &Option<String>) -> UsageResult<String> {
    let cipher = cipher
        .as_deref()
        .ok_or_else(|| UsageError::Other("缺少 access_token".into()))?;
    let pt = crypto::decrypt(cipher);
    if pt.is_empty() {
        return Err(UsageError::AuthRequired);
    }
    Ok(pt)
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

#[allow(dead_code)]
fn _unused(_: Value) {}
