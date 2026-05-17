//! Cursor OAuth fetcher.
//!
//! Flow (PKCE + polling, derived from cockpit-tools `cursor_oauth.rs`):
//! 1. Generate PKCE verifier/challenge + UUID.
//! 2. Open `https://cursor.com/loginDeepControl?challenge={c}&uuid={u}&mode=login` in browser.
//! 3. Poll `https://api2.cursor.sh/auth/poll?uuid={u}&verifier={v}` every 2s
//!    (max 150 attempts = 5 minutes). 404 = pending, 200 = success.
//! 4. Response yields `{accessToken, refreshToken, authId}`.
//! 5. For usage refresh: GET `cursor.com/api/usage-summary` (using WorkOS
//!    cookie derived from JWT `sub`) + GET `api2.cursor.sh/auth/stripe_profile`
//!    (Bearer auth) for the plan tier.

use chrono::Utc;
use serde::Deserialize;
use serde_json::Value;
use std::time::Duration;

use crate::catalog::AuthMode;
use crate::crypto;
use crate::oauth::pkce::PkcePair;
use crate::oauth::poll_flow::{Poll, PollConfig, run};
use crate::oauth::token_refresh;
use crate::storage;
use crate::subscription::{Subscription, SubscriptionUsage, UsageWindow};
use crate::{UsageError, UsageResult};

const LOGIN_URL: &str = "https://cursor.com/loginDeepControl";
const POLL_ENDPOINT: &str = "https://api2.cursor.sh/auth/poll";
const USAGE_SUMMARY_URL: &str = "https://cursor.com/api/usage-summary";
const STRIPE_PROFILE_URL: &str = "https://api2.cursor.sh/auth/stripe_profile";
const OAUTH_TOKEN_URL: &str = "https://api2.cursor.sh/oauth/token";
const CLIENT_ID: &str = "KbZUR41cY7W6zRSdpSUJ7I7mLYBKOCmB";
const POLL_INTERVAL_MS: u64 = 2000;
const POLL_MAX_ATTEMPTS: usize = 150;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PollResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    auth_id: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct StripeProfile {
    membership_type: Option<String>,
    individual_membership_type: Option<String>,
    is_team_member: Option<bool>,
    is_enterprise: Option<bool>,
}

#[derive(Debug, Deserialize, Default)]
struct RefreshTokenResponse {
    #[serde(alias = "accessToken")]
    access_token: Option<String>,
    #[serde(alias = "refreshToken")]
    refresh_token: Option<String>,
}

/// Spawn the browser-driven login. Returns `(auth_url, pending_id)`.
pub async fn start_login(_region: Option<&str>) -> UsageResult<(String, String)> {
    let pkce = PkcePair::generate();
    let uuid = uuid::Uuid::new_v4().to_string();
    let auth_url = format!(
        "{}?challenge={}&uuid={}&mode=login",
        LOGIN_URL, pkce.challenge, uuid
    );

    let pending_id = crate::oauth::pending_state::register("cursor", None, auth_url.clone());

    // Spawn the polling task; it'll resolve the oneshot inside pending_state.
    let pid = pending_id.clone();
    let verifier = pkce.verifier.clone();
    let session_uuid = uuid.clone();
    tokio::spawn(async move {
        let result = poll_for_tokens(session_uuid, verifier).await;
        if let Some(tx) = crate::oauth::pending_state::take_sender(&pid) {
            let _ = tx.send(result);
        }
    });

    Ok((auth_url, pending_id))
}

async fn poll_for_tokens(uuid: String, verifier: String) -> UsageResult<Subscription> {
    let client = http_client()?;
    let poll_url = format!("{}?uuid={}&verifier={}", POLL_ENDPOINT, uuid, verifier);

    let config = PollConfig::new(POLL_INTERVAL_MS, POLL_MAX_ATTEMPTS);
    let tokens = run(config, |_attempt| {
        let client = client.clone();
        let url = poll_url.clone();
        async move {
            let resp = match client
                .get(&url)
                .header(reqwest::header::ACCEPT, "application/json")
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => return Ok(Poll::Failed(format!("Cursor 轮询失败：{}", e))),
            };
            let status = resp.status();
            if status == reqwest::StatusCode::NOT_FOUND {
                return Ok(Poll::Pending);
            }
            if !status.is_success() {
                // Transient — keep polling rather than abort.
                tracing::warn!("[cursor] poll returned {}", status);
                return Ok(Poll::Pending);
            }
            let body: PollResponse = match resp.json().await {
                Ok(b) => b,
                Err(_) => return Ok(Poll::Pending),
            };
            match (body.access_token, body.refresh_token) {
                (Some(at), Some(rt)) => Ok(Poll::Ready((at, rt, body.auth_id))),
                _ => Ok(Poll::Pending),
            }
        }
    })
    .await?;

    let (access_token, refresh_token, auth_id) = tokens;
    finalize_subscription(access_token, refresh_token, auth_id).await
}

async fn finalize_subscription(
    access_token: String,
    refresh_token: String,
    auth_id: Option<String>,
) -> UsageResult<Subscription> {
    let now = Utc::now().timestamp();
    let display = auth_id
        .as_deref()
        .filter(|s| s.contains('@'))
        .map(str::to_string)
        .unwrap_or_else(|| "Cursor".to_string());

    let mut sub = Subscription {
        id: uuid::Uuid::new_v4().to_string(),
        catalog_id: "cursor".to_string(),
        display_name: display,
        auth_mode: AuthMode::OAuth,
        plan_tier: None,
        monthly_price: None,
        currency: "USD".to_string(),
        billing_cycle: crate::subscription::BillingCycle::Monthly,
        start_date: 0,
        renew_date: 0,
        auto_renew: false,
        api_key_encrypted: None,
        access_token_encrypted: Some(crypto::encrypt(&access_token)),
        refresh_token_encrypted: Some(crypto::encrypt(&refresh_token)),
        access_token_expires_at: token_refresh::jwt_exp(&access_token),
        oauth_account_id: auth_id,
        oauth_region: None,
        requires_reauth: false,
        manual_quota: None,
        note: None,
        sort_index: 0,
        created_at: now,
        updated_at: now,
    };

    // First-time refresh — capture usage + plan name. Failures are non-fatal:
    // the subscription gets created either way, and the user can retry.
    if let Ok(usage) = fetch_with_tokens(&sub.id, &access_token).await {
        storage::save_usage_snapshot(usage).ok();
    }

    let saved = storage::upsert_subscription(sub.clone()).map_err(|e| {
        sub.requires_reauth = true;
        UsageError::Other(format!("Cursor 订阅保存失败：{}", e))
    })?;
    Ok(saved)
}

pub async fn fetch(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    let mut access_token = decrypt_required(&subscription.access_token_encrypted)?;
    if token_refresh::needs_refresh(subscription.access_token_expires_at) {
        if let Some(rt_cipher) = subscription.refresh_token_encrypted.as_deref() {
            let refresh_token = crypto::decrypt(rt_cipher);
            if !refresh_token.is_empty() {
                if let Ok((at, rt_new)) = exchange_refresh(&refresh_token).await {
                    subscription.access_token_encrypted = Some(crypto::encrypt(&at));
                    subscription.access_token_expires_at = token_refresh::jwt_exp(&at);
                    if let Some(rt) = rt_new {
                        subscription.refresh_token_encrypted = Some(crypto::encrypt(&rt));
                    }
                    access_token = at;
                }
            }
        }
    }
    match fetch_with_tokens(&subscription.id, &access_token).await {
        Ok(usage) => Ok(usage),
        Err(UsageError::AuthRequired) => Err(UsageError::AuthRequired),
        Err(e) => Err(e),
    }
}

async fn fetch_with_tokens(
    subscription_id: &str,
    access_token: &str,
) -> UsageResult<SubscriptionUsage> {
    let client = http_client()?;

    // Stripe profile — for plan tier.
    let profile = fetch_stripe_profile(&client, access_token).await.ok();
    let plan_name = resolve_plan_name(profile.as_ref());

    // Usage summary — needs WorkOS cookie + browser UA.
    let usage_json = fetch_usage_summary(&client, access_token).await.ok();
    let monthly = usage_json.as_ref().and_then(parse_monthly_window);

    Ok(SubscriptionUsage {
        subscription_id: subscription_id.to_string(),
        fetched_at: Utc::now().timestamp(),
        plan_name: Some(plan_name),
        hourly: None,
        weekly: None,
        monthly,
        balance: None,
        error: None,
    })
}

async fn fetch_stripe_profile(
    client: &reqwest::Client,
    access_token: &str,
) -> UsageResult<StripeProfile> {
    let resp = client
        .get(STRIPE_PROFILE_URL)
        .bearer_auth(access_token)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|e| UsageError::Fetcher(format!("Cursor stripe_profile: {}", e)))?;
    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(UsageError::AuthRequired);
    }
    if !status.is_success() {
        return Err(UsageError::Fetcher(format!(
            "Cursor stripe_profile 状态码 {}",
            status
        )));
    }
    resp.json::<StripeProfile>()
        .await
        .map_err(|e| UsageError::Fetcher(format!("解析 stripe_profile: {}", e)))
}

async fn fetch_usage_summary(
    client: &reqwest::Client,
    access_token: &str,
) -> UsageResult<Value> {
    let cookie = build_session_cookie(access_token)
        .ok_or_else(|| UsageError::Other("无法从 access_token 解析 WorkOS user id".into()))?;
    let resp = client
        .get(USAGE_SUMMARY_URL)
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::COOKIE, &cookie)
        .header(
            reqwest::header::USER_AGENT,
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)",
        )
        .send()
        .await
        .map_err(|e| UsageError::Fetcher(format!("Cursor usage-summary: {}", e)))?;
    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(UsageError::AuthRequired);
    }
    if !status.is_success() {
        return Err(UsageError::Fetcher(format!(
            "Cursor usage-summary 状态码 {}",
            status
        )));
    }
    resp.json::<Value>()
        .await
        .map_err(|e| UsageError::Fetcher(format!("解析 usage-summary: {}", e)))
}

async fn exchange_refresh(refresh_token: &str) -> UsageResult<(String, Option<String>)> {
    let client = http_client()?;
    let resp = client
        .post(OAUTH_TOKEN_URL)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .json(&serde_json::json!({
            "grant_type": "refresh_token",
            "client_id": CLIENT_ID,
            "refresh_token": refresh_token,
        }))
        .send()
        .await
        .map_err(|e| UsageError::Fetcher(format!("Cursor refresh: {}", e)))?;
    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(UsageError::AuthRequired);
    }
    if !status.is_success() {
        return Err(UsageError::Fetcher(format!("Cursor refresh 状态码 {}", status)));
    }
    let body: RefreshTokenResponse = resp
        .json()
        .await
        .map_err(|e| UsageError::Fetcher(format!("解析 refresh 响应: {}", e)))?;
    let at = body
        .access_token
        .ok_or_else(|| UsageError::Fetcher("Cursor refresh 缺少 accessToken".into()))?;
    Ok((at, body.refresh_token))
}

fn http_client() -> UsageResult<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| UsageError::Other(format!("http client: {}", e)))
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

fn build_session_cookie(access_token: &str) -> Option<String> {
    let payload = token_refresh::decode_jwt_payload(access_token)?;
    let sub = payload.get("sub")?.as_str()?;
    let user_id = sub.rsplit('|').next().unwrap_or(sub);
    if !user_id.starts_with("user_") {
        return None;
    }
    Some(format!(
        "WorkosCursorSessionToken={}%3A%3A{}",
        user_id, access_token
    ))
}

fn resolve_plan_name(profile: Option<&StripeProfile>) -> String {
    let Some(p) = profile else {
        return "FREE".to_string();
    };
    if p.is_enterprise.unwrap_or(false) {
        return "ENTERPRISE".to_string();
    }
    if p.is_team_member.unwrap_or(false) {
        return "TEAM".to_string();
    }
    if let Some(m) = p
        .individual_membership_type
        .as_deref()
        .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("free"))
    {
        return m.to_uppercase();
    }
    p.membership_type
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(str::to_uppercase)
        .unwrap_or_else(|| "FREE".to_string())
}

fn parse_monthly_window(usage: &Value) -> Option<UsageWindow> {
    // Cursor's `usage-summary` is loosely structured; try a handful of common
    // shapes. We expose whatever we find as a `30d`-labeled bar.
    let used = pick_i64(
        usage,
        &[
            &["fastRequestsUsed"],
            &["totalRequests"],
            &["currentUsage", "used"],
            &["usage", "used"],
        ],
    )?;
    let total = pick_i64(
        usage,
        &[
            &["fastRequestsLimit"],
            &["limit"],
            &["currentUsage", "total"],
            &["usage", "total"],
        ],
    );
    let percent = total
        .filter(|t| *t > 0)
        .map(|t| ((used as f64 / t as f64) * 100.0).round() as i32);
    Some(UsageWindow {
        label: "30d".to_string(),
        used,
        total,
        percent,
        reset_at: pick_i64(
            usage,
            &[&["resetAt"], &["periodEnd"], &["billingCycleEnd"]],
        ),
    })
}

fn pick_i64(value: &Value, paths: &[&[&str]]) -> Option<i64> {
    for path in paths {
        let mut cur = value;
        let mut ok = true;
        for key in *path {
            match cur.get(*key) {
                Some(v) => cur = v,
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if ok {
            if let Some(n) = cur.as_i64() {
                return Some(n);
            }
            if let Some(f) = cur.as_f64() {
                return Some(f as i64);
            }
        }
    }
    None
}
