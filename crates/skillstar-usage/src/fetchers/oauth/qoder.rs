//! Qoder OAuth fetcher (Device Flow variant, PKCE).
//!
//! Flow (derived from cockpit-tools `qoder_oauth.rs`):
//! 1. Generate PKCE verifier + login nonce.
//! 2. Open `https://qoder.com/device/selectAccounts?nonce=...&challenge=...&method=S256`
//!    in browser.
//! 3. Poll `https://openapi.qoder.sh/api/v1/deviceToken/poll` with PKCE
//!    `code_verifier` + nonce (1s interval, max 600 attempts = 10 min).
//! 4. Successful response yields tokens; then GET `/api/v2/user/plan` for the
//!    plan name and `/api/v2/quota/usage` for credit usage.

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
use crate::subscription::{BillingCycle, Subscription, SubscriptionUsage, UsageWindow};
use crate::{UsageError, UsageResult};

const LOGIN_BASE_URL: &str = "https://qoder.com/device/selectAccounts";
const OPENAPI_BASE_URL: &str = "https://openapi.qoder.sh";
const POLL_PATH: &str = "/api/v1/deviceToken/poll";
const USER_PLAN_PATH: &str = "/api/v2/user/plan";
const CREDIT_USAGE_PATH: &str = "/api/v2/quota/usage";
const POLL_INTERVAL_MS: u64 = 1500;
const POLL_MAX_ATTEMPTS: usize = 400;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PollResponse {
    #[serde(default)]
    code: i64,
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
    #[serde(default)]
    msg: Option<String>,
}

pub async fn start_login(_region: Option<&str>) -> UsageResult<(String, String)> {
    let pkce = PkcePair::generate();
    let nonce = crate::oauth::pkce::random_state();
    let auth_url = format!(
        "{}?nonce={}&challenge={}&method=S256",
        LOGIN_BASE_URL, nonce, pkce.challenge
    );

    let pending_id =
        crate::oauth::pending_state::register("qoder", None, auth_url.clone());

    let pid = pending_id.clone();
    let verifier = pkce.verifier.clone();
    let nonce_for_task = nonce.clone();
    tokio::spawn(async move {
        let result = poll_for_tokens(nonce_for_task, verifier).await;
        if let Some(tx) = crate::oauth::pending_state::take_sender(&pid) {
            let _ = tx.send(result);
        }
    });

    Ok((auth_url, pending_id))
}

async fn poll_for_tokens(nonce: String, verifier: String) -> UsageResult<Subscription> {
    let client = http_client()?;
    let url = format!("{}{}", OPENAPI_BASE_URL, POLL_PATH);

    let config = PollConfig::new(POLL_INTERVAL_MS, POLL_MAX_ATTEMPTS);
    let tokens = run(config, |_attempt| {
        let client = client.clone();
        let url = url.clone();
        let nonce = nonce.clone();
        let verifier = verifier.clone();
        async move {
            let resp = match client
                .post(&url)
                .header(reqwest::header::ACCEPT, "application/json")
                .json(&serde_json::json!({
                    "nonce": nonce,
                    "codeVerifier": verifier,
                }))
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => return Ok(Poll::Failed(format!("Qoder 轮询失败：{}", e))),
            };
            let status = resp.status();
            if !status.is_success() && status != reqwest::StatusCode::ACCEPTED {
                return Ok(Poll::Pending);
            }
            let body: PollResponse = match resp.json().await {
                Ok(b) => b,
                Err(_) => return Ok(Poll::Pending),
            };
            if body.access_token.is_some() {
                return Ok(Poll::Ready(body));
            }
            if body.code == 401 {
                return Ok(Poll::Failed(format!(
                    "Qoder 认证失败：{}",
                    body.msg.unwrap_or_default()
                )));
            }
            Ok(Poll::Pending)
        }
    })
    .await?;

    let access_token = tokens
        .access_token
        .ok_or_else(|| UsageError::Other("Qoder 缺少 accessToken".into()))?;
    let refresh_token = tokens.refresh_token;
    let expires_at = tokens
        .expires_in
        .map(|s| Utc::now().timestamp() + s)
        .or_else(|| token_refresh::jwt_exp(&access_token));

    finalize_subscription(access_token, refresh_token, expires_at).await
}

async fn finalize_subscription(
    access_token: String,
    refresh_token: Option<String>,
    expires_at: Option<i64>,
) -> UsageResult<Subscription> {
    let now = Utc::now().timestamp();
    let sub = Subscription {
        id: uuid::Uuid::new_v4().to_string(),
        catalog_id: "qoder".to_string(),
        display_name: "Qoder".to_string(),
        auth_mode: AuthMode::OAuth,
        plan_tier: None,
        monthly_price: None,
        currency: "CNY".to_string(),
        billing_cycle: BillingCycle::Monthly,
        start_date: 0,
        renew_date: 0,
        auto_renew: false,
        api_key_encrypted: None,
        access_token_encrypted: Some(crypto::encrypt(&access_token)),
        refresh_token_encrypted: refresh_token.as_deref().map(crypto::encrypt),
        access_token_expires_at: expires_at,
        oauth_account_id: None,
        oauth_region: None,
        requires_reauth: false,
        manual_quota: None,
        note: None,
        sort_index: 0,
        created_at: now,
        updated_at: now,
    };

    if let Ok(usage) = fetch_with_token(&sub.id, &access_token).await {
        storage::save_usage_snapshot(usage).ok();
    }
    let saved = storage::upsert_subscription(sub)
        .map_err(|e| UsageError::Other(format!("Qoder 订阅保存失败：{}", e)))?;
    Ok(saved)
}

pub async fn fetch(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    let access_token = decrypt_required(&subscription.access_token_encrypted)?;
    fetch_with_token(&subscription.id, &access_token).await
}

async fn fetch_with_token(
    subscription_id: &str,
    access_token: &str,
) -> UsageResult<SubscriptionUsage> {
    let client = http_client()?;

    let plan_json = fetch_json(&client, &format!("{}{}", OPENAPI_BASE_URL, USER_PLAN_PATH), access_token)
        .await
        .ok();
    let usage_json = fetch_json(&client, &format!("{}{}", OPENAPI_BASE_URL, CREDIT_USAGE_PATH), access_token)
        .await
        .ok();

    let plan_name = plan_json
        .as_ref()
        .and_then(extract_plan_name)
        .unwrap_or_else(|| "FREE".to_string());

    let monthly = usage_json.as_ref().and_then(parse_usage_window);

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

async fn fetch_json(
    client: &reqwest::Client,
    url: &str,
    access_token: &str,
) -> UsageResult<Value> {
    let resp = client
        .get(url)
        .bearer_auth(access_token)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|e| UsageError::Fetcher(format!("Qoder GET {} 失败：{}", url, e)))?;
    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(UsageError::AuthRequired);
    }
    if !status.is_success() {
        return Err(UsageError::Fetcher(format!(
            "Qoder GET {} 状态码 {}",
            url, status
        )));
    }
    resp.json::<Value>()
        .await
        .map_err(|e| UsageError::Fetcher(format!("Qoder 解析 JSON: {}", e)))
}

fn extract_plan_name(value: &Value) -> Option<String> {
    // Try a chain of common paths used by the Qoder /user/plan response.
    let candidates: &[&[&str]] = &[
        &["data", "planName"],
        &["data", "plan", "name"],
        &["data", "plan", "displayName"],
        &["data", "plan", "level"],
        &["data", "subscription", "name"],
        &["planName"],
        &["plan"],
    ];
    for path in candidates {
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
            if let Some(s) = cur.as_str() {
                let t = s.trim();
                if !t.is_empty() {
                    return Some(t.to_string());
                }
            }
        }
    }
    None
}

fn parse_usage_window(value: &Value) -> Option<UsageWindow> {
    let data = value.get("data").unwrap_or(value);
    let used = data
        .get("used")
        .or_else(|| data.get("usedCredits"))
        .and_then(|v| v.as_i64())?;
    let total = data
        .get("total")
        .or_else(|| data.get("totalCredits"))
        .and_then(|v| v.as_i64());
    let percent = total
        .filter(|t| *t > 0)
        .map(|t| ((used as f64 / t as f64) * 100.0).round() as i32);
    Some(UsageWindow {
        label: "本月".to_string(),
        used,
        total,
        percent,
        reset_at: data.get("resetAt").and_then(|v| v.as_i64()),
    })
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
