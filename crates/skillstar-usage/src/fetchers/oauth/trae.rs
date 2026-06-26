//! Trae OAuth fetcher (region-aware).

use chrono::Utc;
use serde::Deserialize;
use serde_json::{Value, json};
use std::time::Duration;

use crate::catalog::AuthMode;
use crate::crypto;
use crate::oauth::local_server;
use crate::oauth::pkce;
use crate::oauth::token_refresh;
use crate::storage;
use crate::subscription::{BillingCycle, Subscription, SubscriptionUsage, UsageWindow};
use crate::{UsageError, UsageResult};

const CLIENT_ID: &str = "ono9krqynydwx5";
const CLIENT_SECRET: &str = "-";
const CALLBACK_PORT: u16 = 1456;
const LOGIN_PAGE: &str = "https://www.trae.ai/login";
const EXCHANGE_PATH: &str = "/cloudide/api/v3/trae/oauth/ExchangeToken";
const PAY_STATUS_PATH: &str = "/trae/api/v1/pay/ide_user_pay_status";
const ENT_USAGE_PATH: &str = "/trae/api/v1/pay/ide_user_ent_usage";

const PRODUCT_PROMO: i64 = 3;
const PRODUCT_FREE: i64 = 0;
const PRODUCT_PRO: i64 = 1;
const PRODUCT_PRO_PLUS: i64 = 4;
const PRODUCT_ULTRA: i64 = 6;
const PRODUCT_LITE: i64 = 8;
const PRODUCT_TRIAL: i64 = 9;

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

pub async fn start_login(region: Option<&str>) -> UsageResult<super::OAuthStartInfo> {
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

    let pending_id = crate::oauth::pending_state::register("trae", Some(&region), auth_url.clone());
    let pid = pending_id.clone();
    let region_for_task = region.clone();
    tokio::spawn(async move {
        let result = drive_login(state, region_for_task).await;
        if let Some(tx) = crate::oauth::pending_state::take_sender(&pid) {
            let _ = tx.send(result);
        }
    });

    Ok(super::OAuthStartInfo::browser(auth_url, pending_id))
}

async fn drive_login(state: String, region: String) -> UsageResult<Subscription> {
    let code =
        local_server::wait_for_callback(CALLBACK_PORT, state, Some(Duration::from_secs(300)))
            .await?;
    let tokens = exchange_code(&region, Some(code.as_str()), None).await?;
    let access_token = tokens
        .access_token
        .ok_or_else(|| UsageError::Other("Trae 缺少 accessToken".into()))?;
    let expires_at = tokens
        .expires_in
        .map(|s| Utc::now().timestamp() + s)
        .or_else(|| token_refresh::jwt_exp(&access_token));

    finalize(access_token, tokens.refresh_token, expires_at, region).await
}

async fn exchange_code(
    region: &str,
    code: Option<&str>,
    refresh_token: Option<&str>,
) -> UsageResult<ExchangeResponse> {
    let client = crate::http_client::usage_reqwest_with_active_fingerprint()?;
    let base = region_base_url(region);
    let body = if let Some(c) = code {
        json!({ "code": c, "clientId": CLIENT_ID })
    } else {
        json!({
            "ClientID": CLIENT_ID,
            "clientId": CLIENT_ID,
            "RefreshToken": refresh_token.unwrap_or(""),
            "refreshToken": refresh_token.unwrap_or(""),
            "refresh_token": refresh_token.unwrap_or(""),
            "ClientSecret": CLIENT_SECRET,
        })
    };
    let resp = client
        .post(format!("{}{}", base, EXCHANGE_PATH))
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| UsageError::Fetcher(format!("Trae ExchangeToken：{}", e)))?;
    if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(UsageError::AuthRequired);
    }
    if !resp.status().is_success() {
        return Err(UsageError::Fetcher(format!(
            "Trae ExchangeToken 状态 {}",
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
        platform_token_encrypted: None,
        access_token_encrypted: Some(crypto::encrypt(&access_token)),
        refresh_token_encrypted: refresh_token.as_deref().map(crypto::encrypt),
        access_token_expires_at: expires_at,
        id_token_encrypted: None,
        oauth_account_id: None,
        oauth_region: Some(region.clone()),
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
    if let Ok(usage) = fetch_usage_for_subscription(&sub, &access_token, &region).await {
        storage::save_usage_snapshot(usage).ok();
    }
    storage::upsert_subscription(sub)
        .map_err(|e| UsageError::Other(format!("Trae 订阅保存失败：{}", e)))
}

pub async fn fetch(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    let fp_id = subscription.fingerprint_id.clone();
    crate::http_client::with_fingerprint(fp_id, fetch_inner(subscription)).await
}

async fn fetch_inner(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    let region = subscription
        .oauth_region
        .as_deref()
        .unwrap_or("cn")
        .to_string();

    if token_refresh::needs_refresh(subscription.access_token_expires_at) {
        refresh_tokens(subscription, &region).await?;
    }

    let access_token =
        crate::fetchers::decrypt_required(&subscription.access_token_encrypted, "access_token")?;
    match fetch_usage_for_subscription(subscription, &access_token, &region).await {
        Err(UsageError::AuthRequired) => {
            refresh_tokens(subscription, &region).await?;
            let access_token =
                crate::fetchers::decrypt_required(&subscription.access_token_encrypted, "access_token")?;
            fetch_usage_for_subscription(subscription, &access_token, &region).await
        }
        other => other,
    }
}

async fn refresh_tokens(subscription: &mut Subscription, region: &str) -> UsageResult<()> {
    let rt_cipher = subscription
        .refresh_token_encrypted
        .as_deref()
        .ok_or(UsageError::AuthRequired)?;
    let refresh = crypto::decrypt(rt_cipher);
    if refresh.is_empty() {
        return Err(UsageError::AuthRequired);
    }
    let tokens = exchange_code(region, None, Some(&refresh)).await?;
    if let Some(at) = tokens.access_token {
        subscription.access_token_encrypted = Some(crypto::encrypt(&at));
        subscription.access_token_expires_at = tokens
            .expires_in
            .map(|s| Utc::now().timestamp() + s)
            .or_else(|| token_refresh::jwt_exp(&at));
    }
    if let Some(rt) = tokens.refresh_token {
        subscription.refresh_token_encrypted = Some(crypto::encrypt(&rt));
    }
    Ok(())
}

async fn fetch_usage_for_subscription(
    subscription: &Subscription,
    access_token: &str,
    region: &str,
) -> UsageResult<SubscriptionUsage> {
    fetch_with_token(&subscription.id, access_token, region).await
}

async fn fetch_with_token(
    subscription_id: &str,
    access_token: &str,
    region: &str,
) -> UsageResult<SubscriptionUsage> {
    let client = crate::http_client::usage_reqwest_with_active_fingerprint()?;
    let base = region_base_url(region);

    let pay = post_json(
        &client,
        &format!("{}{}", base, PAY_STATUS_PATH),
        access_token,
        json!({}),
    )
    .await
    .ok();
    let usage = post_json(
        &client,
        &format!("{}{}", base, ENT_USAGE_PATH),
        access_token,
        json!({ "require_usage": true }),
    )
    .await?;

    let plan_name = pay
        .as_ref()
        .and_then(extract_plan_name)
        .or_else(|| parse_usage_identity(&usage))
        .unwrap_or_else(|| "Free".to_string());

    let monthly = parse_usage_windows(&usage);

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

async fn post_json(
    client: &reqwest::Client,
    url: &str,
    access_token: &str,
    body: Value,
) -> UsageResult<Value> {
    let resp = client
        .post(url)
        .bearer_auth(access_token)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(reqwest::header::ACCEPT, "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| UsageError::Fetcher(format!("Trae POST：{}", e)))?;
    if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(UsageError::AuthRequired);
    }
    if !resp.status().is_success() {
        return Err(UsageError::Fetcher(format!("Trae 状态 {}", resp.status())));
    }
    resp.json()
        .await
        .map_err(|e| UsageError::Fetcher(format!("Trae 解析：{}", e)))
}

fn extract_plan_name(value: &Value) -> Option<String> {
    const PATHS: &[&[&str]] = &[
        &["user_pay_identity_str"],
        &["identityStr"],
        &["data", "user_pay_identity_str"],
        &["data", "identityStr"],
    ];
    for path in PATHS {
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
        if ok && let Some(s) = cur.as_str().map(str::trim).filter(|s| !s.is_empty()) {
            return Some(s.to_string());
        }
    }
    None
}

fn parse_usage_identity(usage: &Value) -> Option<String> {
    let pack = select_usage_pack(usage)?;
    let product_type = pack_product_type(pack)?;
    Some(identity_from_product_type(product_type).to_string())
}

fn parse_usage_windows(usage: &Value) -> Option<UsageWindow> {
    let pack = select_usage_pack(usage)?;

    let mut breakdown = Vec::new();
    let basic_quota = pick_f64(
        pack,
        &["entitlement_base_info", "quota", "basic_usage_limit"],
    );
    let basic_usage = pick_f64(pack, &["usage", "basic_usage_amount"]).unwrap_or(0.0);
    if let Some(total) = basic_quota.filter(|t| *t > 0.0) {
        let pct = ((total - basic_usage).max(0.0) / total * 100.0).round() as i32;
        breakdown.push(UsageWindow {
            label: "基础额度".to_string(),
            used: basic_usage.round() as i64,
            total: Some(total.round() as i64),
            percent: Some(pct),
            reset_at: pack_reset_at(pack),
            breakdown: Vec::new(),
        });
    }

    let bonus_quota = pick_f64(
        pack,
        &["entitlement_base_info", "quota", "bonus_usage_limit"],
    );
    let bonus_usage = pick_f64(pack, &["usage", "bonus_usage_amount"]).unwrap_or(0.0);
    if let Some(total) = bonus_quota.filter(|t| *t > 0.0) {
        let pct = ((total - bonus_usage).max(0.0) / total * 100.0).round() as i32;
        breakdown.push(UsageWindow {
            label: "赠送额度".to_string(),
            used: bonus_usage.round() as i64,
            total: Some(total.round() as i64),
            percent: Some(pct),
            reset_at: None,
            breakdown: Vec::new(),
        });
    }

    if breakdown.is_empty() {
        return None;
    }

    let avg =
        breakdown.iter().filter_map(|w| w.percent).sum::<i32>() / breakdown.len().max(1) as i32;
    Some(UsageWindow {
        label: "本月".to_string(),
        used: (100 - avg).max(0) as i64,
        total: Some(100),
        percent: Some(avg),
        reset_at: breakdown.first().and_then(|w| w.reset_at),
        breakdown,
    })
}

fn select_usage_pack(usage: &Value) -> Option<&Value> {
    if let Some(code) = usage.get("code").and_then(|v| v.as_i64())
        && code != 0
    {
        return None;
    }
    let packs = usage.get("user_entitlement_pack_list")?.as_array()?;
    let filtered: Vec<&Value> = packs
        .iter()
        .filter(|p| pack_product_type(p) != Some(PRODUCT_PROMO))
        .collect();
    for t in [
        PRODUCT_ULTRA,
        PRODUCT_PRO_PLUS,
        PRODUCT_PRO,
        PRODUCT_TRIAL,
        PRODUCT_LITE,
        PRODUCT_FREE,
    ] {
        if let Some(p) = filtered.iter().find(|p| pack_product_type(p) == Some(t)) {
            return Some(p);
        }
    }
    filtered.first().copied()
}

fn pack_product_type(pack: &Value) -> Option<i64> {
    pick_f64(pack, &["entitlement_base_info", "product_type"])
        .map(|v| v as i64)
        .or_else(|| pack.get("product_type").and_then(|v| v.as_i64()))
}

fn identity_from_product_type(product_type: i64) -> &'static str {
    match product_type {
        PRODUCT_ULTRA => "Ultra",
        PRODUCT_PRO_PLUS => "Pro+",
        PRODUCT_PRO | PRODUCT_TRIAL => "Pro",
        PRODUCT_LITE => "Lite",
        PRODUCT_FREE => "Free",
        _ => "Free",
    }
}

fn pack_reset_at(pack: &Value) -> Option<i64> {
    let end = pick_f64(pack, &["entitlement_base_info", "end_time"])?;
    if end <= 0.0 {
        return None;
    }
    Some((end as i64) + 1)
}

fn pick_f64(value: &Value, path: &[&str]) -> Option<f64> {
    let mut cur = value;
    for key in path {
        cur = cur.get(*key)?;
    }
    cur.as_f64()
        .or_else(|| cur.as_str().and_then(|s| s.parse().ok()))
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
