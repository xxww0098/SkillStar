//! Trae OAuth fetcher (region-aware, OAuth code grant with local callback).
//!
//! Trae's official OAuth handshake (derived from cockpit-tools `trae_oauth.rs`):
//! 1. Pick region base URL → `grow-normal.trae.ai` (CN), `growsg-normal.trae.ai`
//!    (SG), `growva-normal.trae.ai` (US), or `grow-normal.traeapi.us` (TTP).
//! 2. Open `https://www.trae.ai/login?platform=ide&redirect=http%3A%2F%2F127.0.0.1%3A{port}%2Fauthorize`.
//! 3. Local server catches `/authorize?code=...&loginRegion=...`.
//! 4. POST `/cloudide/api/v3/trae/oauth/ExchangeToken` to swap code for tokens.
//! 5. Plan name comes from `/trae/api/v1/pay/ide_user_pay_status` →
//!    `identityStr` (multi-path fallback).
//!
//! v1 is a usable subset — the full implementation in cockpit-tools spans
//! 2,000+ lines (machine_id, entitlements, multi-region failover). We open
//! the right URL and parse `identityStr` from `pay_status` after the user
//! completes login.

use chrono::Utc;
use serde::Deserialize;
use serde_json::Value;
use std::time::Duration;

use crate::catalog::AuthMode;
use crate::crypto;
use crate::oauth::local_server;
use crate::oauth::pkce;
use crate::oauth::token_refresh;
use crate::storage;
use crate::subscription::{BillingCycle, Subscription, SubscriptionUsage};
use crate::{UsageError, UsageResult};

const CLIENT_ID: &str = "ono9krqynydwx5";
const CALLBACK_PORT: u16 = 1456;
const LOGIN_PAGE: &str = "https://www.trae.ai/login";
const EXCHANGE_PATH: &str = "/cloudide/api/v3/trae/oauth/ExchangeToken";
const PAY_STATUS_PATH: &str = "/trae/api/v1/pay/ide_user_pay_status";

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ExchangeResponse {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

fn region_base_url(region: &str) -> &'static str {
    match region {
        "sg" => "https://growsg-normal.trae.ai",
        "us" => "https://growva-normal.trae.ai",
        "ttp" => "https://grow-normal.traeapi.us",
        _ => "https://grow-normal.trae.ai",
    }
}

pub async fn start_login(region: Option<&str>) -> UsageResult<(String, String)> {
    let region = region.unwrap_or("cn").to_string();
    let state = pkce::random_state();
    let redirect = format!("http://127.0.0.1:{}/authorize", CALLBACK_PORT);
    let auth_url = format!(
        "{}?platform=ide&clientId={}&state={}&loginRegion={}&redirect={}",
        LOGIN_PAGE,
        CLIENT_ID,
        state,
        region,
        urlencoding(&redirect),
    );

    let pending_id =
        crate::oauth::pending_state::register("trae", Some(&region), auth_url.clone());

    let pid = pending_id.clone();
    let region_for_task = region.clone();
    tokio::spawn(async move {
        let result = drive_login(state, region_for_task).await;
        if let Some(tx) = crate::oauth::pending_state::take_sender(&pid) {
            let _ = tx.send(result);
        }
    });

    Ok((auth_url, pending_id))
}

async fn drive_login(state: String, region: String) -> UsageResult<Subscription> {
    let code = local_server::wait_for_callback(CALLBACK_PORT, state, Some(Duration::from_secs(300))).await?;
    let base = region_base_url(&region);
    let tokens = exchange_code(base, &code).await?;
    let access_token = tokens
        .access_token
        .ok_or_else(|| UsageError::Other("Trae 缺少 accessToken".into()))?;
    let expires_at = tokens
        .expires_in
        .map(|s| Utc::now().timestamp() + s)
        .or_else(|| token_refresh::jwt_exp(&access_token));

    finalize(access_token, tokens.refresh_token, expires_at, region).await
}

async fn exchange_code(base: &str, code: &str) -> UsageResult<ExchangeResponse> {
    let client = http_client()?;
    let resp = client
        .post(format!("{}{}", base, EXCHANGE_PATH))
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .json(&serde_json::json!({
            "code": code,
            "clientId": CLIENT_ID,
        }))
        .send()
        .await
        .map_err(|e| UsageError::Fetcher(format!("Trae 换 token 失败：{}", e)))?;
    if !resp.status().is_success() {
        return Err(UsageError::Fetcher(format!(
            "Trae 换 token 状态码 {}",
            resp.status()
        )));
    }
    resp.json::<ExchangeResponse>()
        .await
        .map_err(|e| UsageError::Fetcher(format!("Trae 解析 token: {}", e)))
}

async fn finalize(
    access_token: String,
    refresh_token: Option<String>,
    expires_at: Option<i64>,
    region: String,
) -> UsageResult<Subscription> {
    let now = Utc::now().timestamp();
    let sub = Subscription {
        id: uuid::Uuid::new_v4().to_string(),
        catalog_id: "trae".to_string(),
        display_name: "Trae".to_string(),
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
        oauth_region: Some(region.clone()),
        requires_reauth: false,
        manual_quota: None,
        note: None,
        sort_index: 0,
        created_at: now,
        updated_at: now,
    };
    if let Ok(usage) = fetch_with_token(&sub.id, &access_token, &region).await {
        storage::save_usage_snapshot(usage).ok();
    }
    let saved = storage::upsert_subscription(sub)
        .map_err(|e| UsageError::Other(format!("Trae 订阅保存失败：{}", e)))?;
    Ok(saved)
}

pub async fn fetch(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    let access_token = decrypt_required(&subscription.access_token_encrypted)?;
    let region = subscription
        .oauth_region
        .as_deref()
        .unwrap_or("cn")
        .to_string();
    fetch_with_token(&subscription.id, &access_token, &region).await
}

async fn fetch_with_token(
    subscription_id: &str,
    access_token: &str,
    region: &str,
) -> UsageResult<SubscriptionUsage> {
    let client = http_client()?;
    let base = region_base_url(region);
    let url = format!("{}{}", base, PAY_STATUS_PATH);

    let resp = client
        .get(&url)
        .bearer_auth(access_token)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|e| UsageError::Fetcher(format!("Trae pay_status 请求：{}", e)))?;
    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(UsageError::AuthRequired);
    }
    if !status.is_success() {
        return Err(UsageError::Fetcher(format!("Trae pay_status 状态码 {}", status)));
    }
    let value: Value = resp
        .json()
        .await
        .map_err(|e| UsageError::Fetcher(format!("Trae 解析 pay_status: {}", e)))?;

    let plan_name = extract_plan_name(&value).unwrap_or_else(|| "FREE".to_string());

    Ok(SubscriptionUsage {
        subscription_id: subscription_id.to_string(),
        fetched_at: Utc::now().timestamp(),
        plan_name: Some(plan_name),
        hourly: None,
        weekly: None,
        monthly: None,
        balance: None,
        error: None,
    })
}

fn extract_plan_name(value: &Value) -> Option<String> {
    let candidates: &[&[&str]] = &[
        &["identityStr"],
        &["identity_str"],
        &["user_pay_identity_str"],
        &["entitlementInfo", "identityStr"],
        &["data", "identityStr"],
        &["data", "user_pay_identity_str"],
        &["data", "entitlementInfo", "identityStr"],
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
            if let Some(s) = cur.as_str().map(str::trim).filter(|s| !s.is_empty()) {
                return Some(s.to_string());
            }
        }
    }
    None
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

fn urlencoding(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}
