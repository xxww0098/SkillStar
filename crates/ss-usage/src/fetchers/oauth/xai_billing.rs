//! Proxy calls for Grok billing, the credits view, and the settings plan name.
//!
//! The CLI session token is only accepted when `x-xai-token-auth` names the
//! Grok CLI authenticator. The default billing view and `?format=credits` are
//! independent: a transport failure on one must not discard the other.

use serde_json::Value;
use std::time::Duration;

use super::{BILLING_CREDITS_URL, BILLING_URL, CurrentPeriod, UsageError, UsageResult};
use crate::oauth::token_refresh;
use crate::subscription::UsageWindow;

/// CLI session tokens are rejected by the proxy unless this header names the
/// Grok CLI authenticator. Official clients send it on every proxy call.
const TOKEN_AUTH_HEADER: &str = "x-xai-token-auth";
const TOKEN_AUTH_VALUE: &str = "xai-grok-cli";
const SETTINGS_URL: &str = "https://cli-chat-proxy.grok.com/v1/settings";

pub(super) fn grok_get(
    client: &reqwest::Client,
    url: &str,
    access_token: &str,
) -> reqwest::RequestBuilder {
    client
        .get(url)
        .bearer_auth(access_token.trim())
        .header(reqwest::header::ACCEPT, "application/json")
        .header(TOKEN_AUTH_HEADER, TOKEN_AUTH_VALUE)
}

fn transport_detail(error: &reqwest::Error) -> String {
    let mut detail = error.to_string();
    let mut source = std::error::Error::source(error);
    while let Some(next) = source {
        let text = next.to_string();
        if !detail.contains(&text) {
            detail.push_str(": ");
            detail.push_str(&text);
        }
        source = std::error::Error::source(next);
    }
    detail
}

/// Remaining percent needs `percent`. A known total, including an explicit
/// zero allowance, is enough to compute it. The account card omits the
/// Monthly credits bar.
pub(super) fn fill_missing_percent(window: &mut UsageWindow) {
    if window.percent.is_some() {
        return;
    }
    let Some(total) = window.total else {
        return;
    };
    window.percent = Some(if total <= 0 {
        0
    } else {
        ((window.used as f64 / total as f64) * 100.0).round() as i32
    });
}

/// Default billing view, retried once on transport failure. 401 is terminal.
pub(super) async fn fetch_billing_payload(
    client: &reqwest::Client,
    access_token: &str,
) -> UsageResult<Value> {
    let mut last_transport = None;
    for attempt in 0..2 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(400)).await;
        }
        match fetch_billing_once(client, access_token).await {
            Ok(payload) => return Ok(payload),
            Err(UsageError::AuthRequired) => return Err(UsageError::AuthRequired),
            Err(error) if error.is_transient() => last_transport = Some(error),
            Err(error) => return Err(error),
        }
    }
    Err(last_transport
        .unwrap_or_else(|| UsageError::Transient("Grok billing 请求失败".to_string())))
}

async fn fetch_billing_once(client: &reqwest::Client, access_token: &str) -> UsageResult<Value> {
    let resp = grok_get(client, BILLING_URL, access_token)
        .send()
        .await
        .map_err(|error| UsageError::transport("Grok billing", transport_detail(&error)))?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(UsageError::AuthRequired);
    }
    if !status.is_success() {
        return Err(UsageError::http_status(
            "Grok billing",
            status.as_u16(),
            &body,
        ));
    }
    serde_json::from_str(&body)
        .map_err(|e| UsageError::Fetcher(format!("Grok billing JSON 解析失败: {}", e)))
}

pub(super) async fn fetch_settings_plan(
    client: &reqwest::Client,
    access_token: &str,
) -> Option<String> {
    let resp = grok_get(client, SETTINGS_URL, access_token)
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body = resp.text().await.ok()?;
    let payload: Value = serde_json::from_str(&body).ok()?;
    plan_from_settings(&payload)
}

/// Fetch `?format=credits` and extract the current usage period. Returns `None`
/// only after a retry also fails, so the caller degrades gracefully. The retry
/// matters because the credits view is slow and occasionally flaky under the
/// parallel multi-account refresh — a silent miss would drop the weekly bar.
pub(super) async fn fetch_current_period(
    client: &reqwest::Client,
    access_token: &str,
) -> Option<CurrentPeriod> {
    for attempt in 0..2 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(400)).await;
        }
        if let Some(period) = fetch_current_period_once(client, access_token).await {
            return Some(period);
        }
    }
    None
}

async fn fetch_current_period_once(
    client: &reqwest::Client,
    access_token: &str,
) -> Option<CurrentPeriod> {
    let resp = grok_get(client, BILLING_CREDITS_URL, access_token)
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body = resp.text().await.ok()?;
    let payload: Value = serde_json::from_str(&body).ok()?;
    super::parse_current_period(&payload)
}

/// Display name from `GET /v1/settings`. The brand string `Grok` is not a plan.
pub(super) fn plan_from_settings(payload: &Value) -> Option<String> {
    let roots = super::candidate_roots(payload);
    for key in [
        "subscription_tier_display",
        "subscriptionTierDisplay",
        "subscription_tier",
        "subscriptionTier",
    ] {
        if let Some(name) = super::pick_value_multi(&roots, &[&[key]])
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty() && !name.eq_ignore_ascii_case("grok"))
        {
            return Some(name.to_string());
        }
    }
    None
}

/// JWT `tier` numbers used by the Grok CLI when settings is unreachable.
/// Display strings match `subscription_tier_display`, not the snake_case claim.
pub(super) fn plan_from_access_token(token: &str) -> Option<String> {
    let tier = token_refresh::decode_jwt_payload(token)?
        .get("tier")?
        .clone();
    let tier = tier
        .as_i64()
        .or_else(|| tier.as_u64().map(|value| value as i64))
        .or_else(|| tier.as_str().and_then(|text| text.trim().parse().ok()))?;
    let name = match tier {
        0 => "Free",
        1 => "SuperGrok",
        2 => "X Basic",
        3 => "X Premium",
        4 => "X Premium+",
        5 => "SuperGrok Heavy",
        6 => "SuperGrok Lite",
        7 => "SuperGrok Plus",
        _ => return None,
    };
    Some(name.to_string())
}
