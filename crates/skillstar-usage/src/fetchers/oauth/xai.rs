//! Grok (xAI) OAuth + billing fetcher.
//!
//! Mirrors the Grok CLI flow used by CLIProxyAPI:
//! 1. Open `https://auth.x.ai/oauth2/authorize` with PKCE and
//!    `redirect_uri=http://127.0.0.1:56121/callback`.
//! 2. Exchange `code` at `https://auth.x.ai/oauth2/token`.
//! 3. GET `https://cli-chat-proxy.grok.com/v1/billing` for credit-quota data.
//!
//! xAI bills a single credit pool; the reset window (7d vs 30d) only decides
//! the UI label. The real payload shape is:
//! ```json
//! {
//!   "billingCycle": { "billingPeriodEnd": "..." },
//!   "monthlyLimit": { "val": 99900 },
//!   "onDemandCap":  { "val": 0 },
//!   "usage": { "totalUsed": { "val": 12345 } }
//! }
//! ```
//! Amount fields live at the root (the official shape). A `config` wrapper is
//! also tolerated for proxy mirrors and older fixtures.

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;
use std::time::Duration;
use url::Url;

use crate::catalog::AuthMode;
use crate::crypto;
use crate::oauth::local_server;
use crate::oauth::pkce::PkcePair;
use crate::oauth::token_refresh;
use crate::storage;
use crate::subscription::{BillingCycle, CreditInfo, Subscription, SubscriptionUsage, UsageWindow};
use crate::{UsageError, UsageResult};

const AUTHORIZE_URL: &str = "https://auth.x.ai/oauth2/authorize";
const TOKEN_URL: &str = "https://auth.x.ai/oauth2/token";
const CLIENT_ID: &str = "b1a00492-073a-47ea-816f-4c329264a828";
const SCOPES: &str = "openid profile email offline_access grok-cli:access api:access";
const CALLBACK_PORT: u16 = 56121;
const CALLBACK_PATH: &str = "/callback";
const BILLING_URL: &str = "https://cli-chat-proxy.grok.com/v1/billing";
const DEFAULT_PLAN_NAME: &str = "Grok";

#[derive(Debug, Deserialize, Default)]
struct TokenResponse {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    id_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

pub async fn start_login(_region: Option<&str>) -> UsageResult<super::OAuthStartInfo> {
    let pkce = PkcePair::generate();
    let state = crate::oauth::pkce::random_state();
    let nonce = crate::oauth::pkce::random_state();
    let redirect_uri = format!("http://127.0.0.1:{}{}", CALLBACK_PORT, CALLBACK_PATH);
    let auth_url = build_authorize_url(
        AUTHORIZE_URL,
        &redirect_uri,
        &pkce.challenge,
        &state,
        &nonce,
    )?;

    let pending_id = crate::oauth::pending_state::register("xai", None, auth_url.clone());
    let pid = pending_id.clone();
    let verifier = pkce.verifier.clone();
    let state_for_task = state.clone();
    tokio::spawn(async move {
        let result = drive_login(state_for_task, verifier, redirect_uri).await;
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
    finalize(tokens).await
}

fn build_authorize_url(
    endpoint: &str,
    redirect_uri: &str,
    code_challenge: &str,
    state: &str,
    nonce: &str,
) -> UsageResult<String> {
    let mut url = Url::parse(endpoint)
        .map_err(|e| UsageError::Fetcher(format!("Grok authorize URL 无效: {}", e)))?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs
            .append_pair("response_type", "code")
            .append_pair("client_id", CLIENT_ID)
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("scope", SCOPES)
            .append_pair("code_challenge", code_challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", state)
            .append_pair("nonce", nonce)
            .append_pair("plan", "generic")
            .append_pair("referrer", "skillstar");
    }
    Ok(url.to_string())
}

async fn exchange_code(
    code: &str,
    verifier: &str,
    redirect_uri: &str,
) -> UsageResult<TokenResponse> {
    let client = crate::fetchers::http_client()?;
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
        .map_err(|e| UsageError::Fetcher(format!("Grok token 交换失败: {}", e)))?;

    parse_token_response(resp, "Grok token").await
}

async fn parse_token_response(resp: reqwest::Response, label: &str) -> UsageResult<TokenResponse> {
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(UsageError::AuthRequired);
        }
        return Err(UsageError::Fetcher(format!(
            "{} 状态码 {}: {}",
            label,
            status,
            body.chars().take(200).collect::<String>()
        )));
    }
    let tokens: TokenResponse = serde_json::from_str(&body)
        .map_err(|e| UsageError::Fetcher(format!("{} 响应解析失败: {}", label, e)))?;
    if trim_opt(tokens.access_token.as_deref()).is_none() {
        return Err(UsageError::AuthRequired);
    }
    Ok(tokens)
}

async fn finalize(tokens: TokenResponse) -> UsageResult<Subscription> {
    let access_token = trim_opt(tokens.access_token.as_deref()).ok_or(UsageError::AuthRequired)?;
    let refresh_token = trim_opt(tokens.refresh_token.as_deref());
    let id_token = trim_opt(tokens.id_token.as_deref());
    let email = id_token.and_then(|token| token_refresh::jwt_string(token, &["email"]));
    let subject = id_token.and_then(|token| token_refresh::jwt_string(token, &["sub"]));
    let display_name = email
        .as_ref()
        .map(|email| format!("Grok · {}", email))
        .unwrap_or_else(|| DEFAULT_PLAN_NAME.to_string());
    let expires_at = tokens
        .expires_in
        .map(|s| Utc::now().timestamp() + s)
        .or_else(|| token_refresh::jwt_exp(access_token))
        .or_else(|| id_token.and_then(token_refresh::jwt_exp));

    let now = Utc::now().timestamp();
    let sub = Subscription {
        id: uuid::Uuid::new_v4().to_string(),
        catalog_id: "xai".to_string(),
        display_name,
        auth_mode: AuthMode::OAuth,
        plan_tier: None,
        monthly_price: None,
        currency: "USD".to_string(),
        billing_cycle: BillingCycle::Monthly,
        start_date: 0,
        renew_date: 0,
        auto_renew: false,
        api_key_encrypted: None,
        platform_token_encrypted: None,
        access_token_encrypted: Some(crypto::encrypt(access_token)),
        refresh_token_encrypted: refresh_token.map(crypto::encrypt),
        access_token_expires_at: expires_at,
        id_token_encrypted: None,
        oauth_account_id: subject.or(email),
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

    if let Ok(usage) = fetch_with_token(&sub.id, access_token).await {
        storage::save_usage_snapshot(usage).ok();
    }
    storage::upsert_subscription(sub)
        .map_err(|e| UsageError::Other(format!("Grok 订阅保存失败: {}", e)))
}

pub async fn fetch(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    let fp_id = subscription.fingerprint_id.clone();
    crate::http_client::with_fingerprint(fp_id, fetch_inner(subscription)).await
}

async fn fetch_inner(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    if token_refresh::needs_refresh(subscription.access_token_expires_at) {
        refresh_xai_tokens(subscription).await?;
    }

    let access_token =
        crate::fetchers::decrypt_required(&subscription.access_token_encrypted, "access_token")?;
    match fetch_with_token(&subscription.id, &access_token).await {
        Err(UsageError::AuthRequired) => {
            refresh_xai_tokens(subscription).await?;
            let access_token =
                crate::fetchers::decrypt_required(&subscription.access_token_encrypted, "access_token")?;
            fetch_with_token(&subscription.id, &access_token).await
        }
        other => other,
    }
}

async fn refresh_xai_tokens(subscription: &mut Subscription) -> UsageResult<()> {
    let rt_cipher = subscription
        .refresh_token_encrypted
        .as_deref()
        .ok_or(UsageError::AuthRequired)?;
    let refresh_token = crypto::decrypt(rt_cipher);
    if refresh_token.trim().is_empty() {
        return Err(UsageError::AuthRequired);
    }

    let client = crate::fetchers::http_client()?;
    let resp = client
        .post(TOKEN_URL)
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", CLIENT_ID),
            ("refresh_token", refresh_token.trim()),
        ])
        .send()
        .await
        .map_err(|e| UsageError::Fetcher(format!("Grok refresh 失败: {}", e)))?;
    let tokens = parse_token_response(resp, "Grok refresh").await?;
    let access_token = trim_opt(tokens.access_token.as_deref()).ok_or(UsageError::AuthRequired)?;

    subscription.access_token_encrypted = Some(crypto::encrypt(access_token));
    if let Some(rt) = trim_opt(tokens.refresh_token.as_deref()) {
        subscription.refresh_token_encrypted = Some(crypto::encrypt(rt));
    }
    subscription.access_token_expires_at = tokens
        .expires_in
        .map(|s| Utc::now().timestamp() + s)
        .or_else(|| token_refresh::jwt_exp(access_token))
        .or_else(|| trim_opt(tokens.id_token.as_deref()).and_then(token_refresh::jwt_exp));

    if let Some(id_token) = trim_opt(tokens.id_token.as_deref()) {
        let account_id = token_refresh::jwt_string(id_token, &["sub"])
            .or_else(|| token_refresh::jwt_string(id_token, &["email"]));
        if account_id.is_some() {
            subscription.oauth_account_id = account_id;
        }
    }

    Ok(())
}

async fn fetch_with_token(
    subscription_id: &str,
    access_token: &str,
) -> UsageResult<SubscriptionUsage> {
    let client = crate::fetchers::http_client()?;
    let resp = client
        .get(BILLING_URL)
        .bearer_auth(access_token.trim())
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|e| UsageError::Fetcher(format!("Grok billing 请求失败: {}", e)))?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(UsageError::AuthRequired);
    }
    if !status.is_success() {
        return Err(UsageError::Fetcher(format!(
            "Grok billing 状态码 {}: {}",
            status,
            body.chars().take(200).collect::<String>()
        )));
    }

    let payload: Value = serde_json::from_str(&body)
        .map_err(|e| UsageError::Fetcher(format!("Grok billing JSON 解析失败: {}", e)))?;
    build_subscription_usage(subscription_id, &payload)
}

/// Amount fields may sit at the payload root (the real xAI shape) or under a
/// `config` wrapper (some proxy mirrors / older fixtures). Return roots in
/// lookup priority order: root first, `config` fallback.
fn candidate_roots(payload: &Value) -> Vec<&Value> {
    let mut roots = vec![payload];
    if let Some(config) = payload.get("config").filter(|v| v.is_object()) {
        roots.push(config);
    }
    roots
}

fn pick_value_multi<'a>(roots: &[&'a Value], paths: &[&[&str]]) -> Option<&'a Value> {
    for root in roots {
        for path in paths {
            if let Some(value) = get_path_value(root, path) {
                return Some(value);
            }
        }
    }
    None
}

fn pick_cent_multi(roots: &[&Value], paths: &[&[&str]]) -> Option<f64> {
    pick_value_multi(roots, paths).and_then(cent_value)
}

/// xAI bills a single credit pool; the reset-window length decides the label.
/// A sub-10-day window renders as "Weekly", otherwise "Monthly" — matching
/// how the Grok CLI dashboard names the same bar.
fn period_label(reset_at: i64) -> String {
    let days = ((reset_at - Utc::now().timestamp()) / 86_400).max(0);
    if days > 0 && days <= 10 {
        "Weekly credits".to_string()
    } else {
        "Monthly credits".to_string()
    }
}

fn build_subscription_usage(
    subscription_id: &str,
    payload: &Value,
) -> UsageResult<SubscriptionUsage> {
    let roots = candidate_roots(payload);

    let monthly_limit = pick_cent_multi(&roots, &[&["monthlyLimit"], &["monthly_limit"]]);
    let used = pick_cent_multi(
        &roots,
        &[
            &["usage", "totalUsed"],
            &["usage", "total_used"],
            &["used"],
        ],
    );
    let on_demand_cap = pick_cent_multi(&roots, &[&["onDemandCap"], &["on_demand_cap"]]);
    let billing_period_end = pick_value_multi(
        &roots,
        &[
            &["billingCycle", "billingPeriodEnd"],
            &["billing_cycle", "billing_period_end"],
            &["billingPeriodEnd"],
            &["billing_period_end"],
        ],
    )
    .and_then(parse_timestamp);

    if monthly_limit.is_none()
        && used.is_none()
        && on_demand_cap.is_none()
        && billing_period_end.is_none()
    {
        return Err(UsageError::Fetcher(
            "Grok billing 未返回可展示额度字段".into(),
        ));
    }

    let label = billing_period_end
        .map(period_label)
        .unwrap_or_else(|| "Monthly credits".to_string());

    let monthly = if monthly_limit.is_some() || used.is_some() {
        let used_cents = used.unwrap_or(0.0).round().max(0.0) as i64;
        let total_cents = monthly_limit.map(|v| v.round().max(0.0) as i64);
        let percent = total_cents
            .filter(|total| *total > 0)
            .map(|total| ((used_cents as f64 / total as f64) * 100.0).round() as i32);
        Some(UsageWindow {
            label,
            used: used_cents,
            total: total_cents,
            percent,
            reset_at: billing_period_end,
            breakdown: Vec::new(),
        })
    } else {
        None
    };

    let mut credits = Vec::new();
    if let Some(cap) = on_demand_cap {
        credits.push(CreditInfo {
            credit_type: "Pay as you go cap".to_string(),
            credit_amount: Some(format_usd_cents(cap)),
            minimum_credit_amount_for_usage: None,
        });
    }

    Ok(SubscriptionUsage {
        subscription_id: subscription_id.to_string(),
        fetched_at: Utc::now().timestamp(),
        plan_name: Some(DEFAULT_PLAN_NAME.to_string()),
        hourly: None,
        weekly: None,
        monthly,
        balance: None,
        credits,
        error: None,
        api_keys: Vec::new(),
        deepseek_analytics: None,
    })
}

fn get_path_value<'a>(root: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = root;
    for key in path {
        current = current.get(*key)?;
    }
    Some(current)
}

fn cent_value(value: &Value) -> Option<f64> {
    if let Some(obj) = value.as_object()
        && let Some(val) = obj.get("val")
    {
        return cent_value(val);
    }
    if let Some(num) = value.as_f64()
        && num.is_finite()
    {
        return Some(num);
    }
    if let Some(text) = value.as_str()
        && let Ok(num) = text.trim().parse::<f64>()
        && num.is_finite()
    {
        return Some(num);
    }
    None
}

fn parse_timestamp(value: &Value) -> Option<i64> {
    if let Some(seconds) = value.as_i64() {
        return normalize_timestamp(seconds);
    }
    if let Some(seconds) = value.as_u64() {
        return normalize_timestamp(seconds as i64);
    }
    if let Some(text) = value.as_str() {
        let trimmed = text.trim();
        if let Ok(num) = trimmed.parse::<i64>() {
            return normalize_timestamp(num);
        }
        if let Ok(dt) = DateTime::parse_from_rfc3339(trimmed) {
            return Some(dt.timestamp());
        }
    }
    None
}

fn normalize_timestamp(raw: i64) -> Option<i64> {
    if raw <= 0 {
        return None;
    }
    if raw > 10_000_000_000 {
        Some(raw / 1000)
    } else {
        Some(raw)
    }
}

fn format_usd_cents(cents: f64) -> String {
    let cents = cents.round() as i64;
    if cents % 100 == 0 {
        format!("${}", cents / 100)
    } else {
        format!("${:.2}", cents as f64 / 100.0)
    }
}

fn trim_opt(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_root_shape_with_usage_total_used() {
        // Real xAI shape: fields at root, used under usage.totalUsed.
        let payload = json!({
            "billingCycle": {
                "billingPeriodStart": "2026-06-01T00:00:00Z",
                "billingPeriodEnd": "2026-07-01T00:00:00Z"
            },
            "monthlyLimit": { "val": 99900 },
            "onDemandCap": { "val": 2000 },
            "usage": {
                "includedUsed": { "val": 12345 },
                "onDemandUsed": { "val": 0 },
                "totalUsed": { "val": 12345 }
            }
        });

        let usage = build_subscription_usage("sub-xai", &payload).unwrap();
        let monthly = usage.monthly.unwrap();

        assert_eq!(monthly.used, 12345);
        assert_eq!(monthly.total, Some(99900));
        assert_eq!(monthly.percent, Some(12));
        assert_eq!(usage.credits.len(), 1);
        assert_eq!(usage.credits[0].credit_amount.as_deref(), Some("$20"));
    }

    #[test]
    fn parses_legacy_config_wrapper() {
        // Older fixtures / proxy mirrors wrap fields under `config`.
        let payload = json!({
            "config": {
                "monthlyLimit": { "val": "5000" },
                "used": { "val": 1250 },
                "onDemandCap": { "val": 2000 },
                "billingPeriodEnd": "2026-06-30T00:00:00Z"
            }
        });

        let usage = build_subscription_usage("sub-xai", &payload).unwrap();
        let monthly = usage.monthly.unwrap();

        assert_eq!(monthly.used, 1250);
        assert_eq!(monthly.total, Some(5000));
        assert_eq!(monthly.percent, Some(25));
        assert_eq!(usage.credits[0].credit_amount.as_deref(), Some("$20"));
    }

    #[test]
    fn prefers_root_usage_over_config_used() {
        // When both root (real) and config (legacy) shapes are present,
        // root usage.totalUsed must win over a stray config.used.
        let payload = json!({
            "config": { "used": { "val": 999 } },
            "monthlyLimit": { "val": 10000 },
            "usage": { "totalUsed": { "val": 2500 } }
        });
        let usage = build_subscription_usage("sub-xai", &payload).unwrap();
        assert_eq!(usage.monthly.unwrap().used, 2500);
    }

    #[test]
    fn weekly_label_for_short_reset_window() {
        // A ~7-day reset window should render as "Weekly credits".
        let soon = Utc::now().timestamp() + 6 * 86_400;
        let payload = json!({
            "billingCycle": { "billingPeriodEnd": soon },
            "monthlyLimit": { "val": 5000 },
            "usage": { "totalUsed": { "val": 1000 } }
        });
        let usage = build_subscription_usage("sub-xai", &payload).unwrap();
        assert_eq!(usage.monthly.unwrap().label, "Weekly credits");
    }

    #[test]
    fn monthly_label_for_long_reset_window() {
        // A ~30-day reset window should render as "Monthly credits".
        let later = Utc::now().timestamp() + 29 * 86_400;
        let payload = json!({
            "billingCycle": { "billingPeriodEnd": later },
            "monthlyLimit": { "val": 5000 },
            "usage": { "totalUsed": { "val": 1000 } }
        });
        let usage = build_subscription_usage("sub-xai", &payload).unwrap();
        assert_eq!(usage.monthly.unwrap().label, "Monthly credits");
    }

    #[test]
    fn defaults_monthly_label_without_reset() {
        // No billingPeriodEnd → still parses, defaults to Monthly label.
        let payload = json!({
            "monthlyLimit": { "val": 5000 },
            "usage": { "totalUsed": { "val": 1000 } }
        });
        let usage = build_subscription_usage("sub-xai", &payload).unwrap();
        assert_eq!(usage.monthly.unwrap().label, "Monthly credits");
    }

    #[test]
    fn rejects_empty_billing_config() {
        let payload = json!({ "config": {} });
        assert!(build_subscription_usage("sub-xai", &payload).is_err());
    }

    #[test]
    fn rejects_garbage_payload() {
        let payload = json!({ "randomField": "value" });
        assert!(build_subscription_usage("sub-xai", &payload).is_err());
    }
}
