//! GitHub Copilot OAuth fetcher (GitHub Device Flow + Copilot internal APIs).

use chrono::Utc;
use serde::Deserialize;
use serde_json::Value;

use crate::catalog::AuthMode;
use crate::crypto;
use crate::oauth::pending_state;
use crate::oauth::poll_flow::{Poll, PollConfig, run};
use crate::storage;
use crate::subscription::{BillingCycle, Subscription, SubscriptionUsage, UsageWindow};
use crate::{UsageError, UsageResult};

const DEVICE_CODE_URL: &str = "https://github.com/login/device/code";
const DEVICE_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
const GITHUB_USER_URL: &str = "https://api.github.com/user";
const GITHUB_EMAILS_URL: &str = "https://api.github.com/user/emails";
const COPILOT_TOKEN_URL: &str = "https://api.github.com/copilot_internal/v2/token";
const COPILOT_USER_URL: &str = "https://api.github.com/copilot_internal/user";
const CLIENT_ID: &str = "01ab8ac9400c4e429b23";
const SCOPE: &str = "read:user user:email repo workflow";
const USER_AGENT: &str = "SkillStar";
const POLL_MAX_ATTEMPTS: usize = 180;

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    #[serde(default)]
    verification_uri_complete: Option<String>,
    #[serde(default)]
    interval: Option<u64>,
}

#[derive(Debug, Deserialize, Default)]
struct DeviceTokenResponse {
    access_token: Option<String>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    error_description: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct CopilotTokenResponse {
    token: Option<String>,
    #[serde(default)]
    sku: Option<String>,
    #[serde(default)]
    limited_user_quotas: Option<Value>,
    #[serde(default)]
    limited_user_reset_date: Option<i64>,
}

#[derive(Debug, Deserialize, Default)]
struct CopilotUserResponse {
    #[serde(default, rename = "copilot_plan")]
    copilot_plan: Option<String>,
    #[serde(default)]
    quota_snapshots: Option<Value>,
}

pub async fn start_login(_region: Option<&str>) -> UsageResult<super::OAuthStartInfo> {
    let client = crate::fetchers::http_client()?;
    let device: DeviceCodeResponse = client
        .post(DEVICE_CODE_URL)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(reqwest::header::ACCEPT, "application/json")
        .form(&[("client_id", CLIENT_ID), ("scope", SCOPE)])
        .send()
        .await
        .map_err(|e| UsageError::Fetcher(format!("GitHub 设备码：{}", e)))?
        .error_for_status()
        .map_err(|e| UsageError::Fetcher(format!("GitHub 设备码 HTTP：{}", e)))?
        .json()
        .await
        .map_err(|e| UsageError::Fetcher(format!("GitHub 设备码解析：{}", e)))?;

    let auth_url = device
        .verification_uri_complete
        .clone()
        .unwrap_or_else(|| format!("{}?user_code={}", device.verification_uri, device.user_code));

    let pending_id = pending_state::register("github-copilot", None, auth_url.clone());
    let pid = pending_id.clone();
    let device_code = device.device_code.clone();
    let interval = device.interval.unwrap_or(5).max(1);

    tokio::spawn(async move {
        let result = poll_device_and_finalize(device_code, interval).await;
        if let Some(tx) = pending_state::take_sender(&pid) {
            let _ = tx.send(result);
        }
    });

    Ok(super::OAuthStartInfo::browser(auth_url, pending_id)
        .with_user_code(device.user_code.clone())
        .with_verification_uri(device.verification_uri.clone()))
}

async fn poll_device_and_finalize(
    device_code: String,
    interval_secs: u64,
) -> UsageResult<Subscription> {
    let client = crate::fetchers::http_client()?;
    let config = PollConfig::new(interval_secs * 1000, POLL_MAX_ATTEMPTS);

    let github_token = run(config, |_n| {
        let client = client.clone();
        let device_code = device_code.clone();
        async move {
            let resp = client
                .post(DEVICE_TOKEN_URL)
                .header(reqwest::header::USER_AGENT, USER_AGENT)
                .header(reqwest::header::ACCEPT, "application/json")
                .form(&[
                    ("client_id", CLIENT_ID),
                    ("device_code", device_code.as_str()),
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ])
                .send()
                .await
                .map_err(|e| UsageError::Fetcher(format!("GitHub token：{}", e)))?;

            let body: DeviceTokenResponse = resp
                .json()
                .await
                .map_err(|e| UsageError::Fetcher(format!("GitHub token 解析：{}", e)))?;

            if let Some(token) = body.access_token {
                return Ok(Poll::Ready(token));
            }
            if body.error.as_deref() == Some("authorization_pending") {
                return Ok(Poll::Pending);
            }
            if body.error.as_deref() == Some("slow_down") {
                return Ok(Poll::Pending);
            }
            Ok(Poll::Failed(format!(
                "GitHub 授权失败：{}",
                body.error_description
                    .or(body.error)
                    .unwrap_or_else(|| "unknown".into())
            )))
        }
    })
    .await?;

    let display = fetch_github_display(&client, &github_token).await;
    finalize_subscription(github_token, display).await
}

async fn fetch_github_display(client: &reqwest::Client, token: &str) -> String {
    if let Ok(resp) = client
        .get(GITHUB_USER_URL)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .header(reqwest::header::AUTHORIZATION, format!("Bearer {}", token))
        .send()
        .await
        && let Ok(user) = resp.json::<Value>().await
    {
        if let Some(login) = user.get("login").and_then(|v| v.as_str()) {
            return login.to_string();
        }
        if let Some(email) = user
            .get("email")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            return email.to_string();
        }
    }

    if let Ok(resp) = client
        .get(GITHUB_EMAILS_URL)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .header(reqwest::header::AUTHORIZATION, format!("Bearer {}", token))
        .send()
        .await
        && let Ok(emails) = resp.json::<Vec<Value>>().await
        && let Some(email) = emails
            .iter()
            .find(|e| e.get("primary").and_then(|v| v.as_bool()) == Some(true))
            .and_then(|e| e.get("email").and_then(|v| v.as_str()))
    {
        return email.to_string();
    }

    "GitHub Copilot".to_string()
}

async fn finalize_subscription(
    github_token: String,
    display_name: String,
) -> UsageResult<Subscription> {
    let now = Utc::now().timestamp();
    let sub = Subscription {
        id: uuid::Uuid::new_v4().to_string(),
        catalog_id: "github-copilot".to_string(),
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
        access_token_encrypted: None,
        refresh_token_encrypted: Some(crypto::encrypt(&github_token)),
        access_token_expires_at: None,
        id_token_encrypted: None,
        oauth_account_id: None,
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

    if let Ok(usage) = fetch_with_github_token(&sub.id, &github_token).await {
        storage::save_usage_snapshot(usage).ok();
    }

    let saved = storage::upsert_subscription(sub)
        .map_err(|e| UsageError::Other(format!("GitHub Copilot 订阅保存失败：{}", e)))?;
    Ok(saved)
}

pub async fn fetch(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    let fp_id = subscription.fingerprint_id.clone();
    crate::http_client::with_fingerprint(fp_id, fetch_inner(subscription)).await
}

async fn fetch_inner(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    let github_token =
        crate::fetchers::decrypt_required(&subscription.refresh_token_encrypted, "GitHub token")?;
    fetch_with_github_token(&subscription.id, &github_token).await
}

async fn fetch_with_github_token(
    subscription_id: &str,
    github_token: &str,
) -> UsageResult<SubscriptionUsage> {
    let client = crate::fetchers::http_client()?;

    let copilot_resp = client
        .get(COPILOT_TOKEN_URL)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(reqwest::header::ACCEPT, "application/json")
        .header("X-GitHub-Api-Version", "2025-04-01")
        .header(
            reqwest::header::AUTHORIZATION,
            format!("token {}", github_token),
        )
        .send()
        .await
        .map_err(|e| UsageError::Fetcher(format!("Copilot token：{}", e)))?;

    if copilot_resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(UsageError::AuthRequired);
    }
    if !copilot_resp.status().is_success() {
        return Err(UsageError::Fetcher(format!(
            "Copilot token 状态码 {}",
            copilot_resp.status()
        )));
    }

    let copilot: CopilotTokenResponse = copilot_resp
        .json()
        .await
        .map_err(|e| UsageError::Fetcher(format!("Copilot token 解析：{}", e)))?;

    let user_info: CopilotUserResponse = match client
        .get(COPILOT_USER_URL)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(reqwest::header::ACCEPT, "application/json")
        .header("X-GitHub-Api-Version", "2025-04-01")
        .header(
            reqwest::header::AUTHORIZATION,
            format!("token {}", github_token),
        )
        .send()
        .await
    {
        Ok(resp) => resp.json().await.unwrap_or_default(),
        Err(_) => CopilotUserResponse::default(),
    };

    let plan_name = user_info
        .copilot_plan
        .clone()
        .or(copilot.sku.clone())
        .unwrap_or_else(|| "Copilot".to_string());

    let breakdown = build_quota_breakdown(
        copilot.limited_user_quotas.as_ref(),
        user_info.quota_snapshots.as_ref(),
        copilot.token.as_deref(),
    );

    let monthly = if breakdown.is_empty() {
        None
    } else {
        let avg =
            breakdown.iter().filter_map(|w| w.percent).sum::<i32>() / breakdown.len().max(1) as i32;
        Some(UsageWindow {
            label: "额度".to_string(),
            used: (100 - avg).max(0) as i64,
            total: Some(100),
            percent: Some(avg),
            reset_at: copilot.limited_user_reset_date,
            breakdown,
        })
    };

    Ok(SubscriptionUsage {
        subscription_id: subscription_id.to_string(),
        fetched_at: Utc::now().timestamp(),
        plan_name: Some(plan_name),
        hourly: None,
        weekly: None,
        monthly,
        balance: None,
        credits: Vec::new(),
        error: None,
        api_keys: Vec::new(),
        deepseek_analytics: None,
    })
}

fn build_quota_breakdown(
    limited: Option<&Value>,
    snapshots: Option<&Value>,
    copilot_token: Option<&str>,
) -> Vec<UsageWindow> {
    let mut out = Vec::new();
    if let Some(lim) = limited.and_then(|v| v.as_object()) {
        let token_map = parse_copilot_token_map(copilot_token);
        if let Some(rem) = lim.get("completions").and_then(json_f64) {
            let total = token_map
                .get("cq")
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(rem);
            if let Some(pct) = remaining_percent(rem, total) {
                out.push(window_from_percent("Inline Suggestions", pct));
            }
        }
        if let Some(rem) = lim.get("chat").and_then(json_f64) {
            let total = token_map
                .get("tq")
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(rem);
            if let Some(pct) = remaining_percent(rem, total) {
                out.push(window_from_percent("Chat Messages", pct));
            }
        }
    }
    if let Some(snap) = snapshots.and_then(|v| v.as_object()) {
        let premium = snap
            .get("premium_interactions")
            .or_else(|| snap.get("premium_models"));
        if let Some(obj) = premium.and_then(|v| v.as_object()) {
            if obj.get("unlimited").and_then(|v| v.as_bool()) == Some(true) {
                out.push(window_from_percent("Premium", 100));
            } else if let Some(pct) = obj.get("percent_remaining").and_then(json_f64) {
                out.push(window_from_percent("Premium", pct.round() as i32));
            }
        }
    }
    out
}

fn window_from_percent(label: &str, percent: i32) -> UsageWindow {
    UsageWindow {
        label: label.to_string(),
        used: (100 - percent).max(0) as i64,
        total: Some(100),
        percent: Some(percent),
        reset_at: None,
        breakdown: Vec::new(),
    }
}

fn remaining_percent(remaining: f64, total: f64) -> Option<i32> {
    if total <= 0.0 {
        return None;
    }
    Some(((remaining.max(0.0) / total) * 100.0).round() as i32)
}

fn parse_copilot_token_map(token: Option<&str>) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let Some(raw) = token else {
        return map;
    };
    for part in raw.split(';') {
        let mut kv = part.splitn(2, '=');
        if let (Some(k), Some(v)) = (kv.next(), kv.next()) {
            map.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    map
}

fn json_f64(v: &Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
}
