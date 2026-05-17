//! Antigravity (Google IDE) OAuth fetcher.
//!
//! Google standard OAuth code grant. Critical: UA must be
//! `antigravity/{ver} {os}/{arch}` + `x-goog-api-client` header, otherwise
//! the `cloudcode-pa.googleapis.com` backend returns 403.
//!
//! Plan name lookup priority (per cockpit-tools `quota.rs`):
//! 1. `paid_tier.id`
//! 2. `current_tier.id`
//! 3. first default-flagged `allowed_tiers`
//! 4. fallback `"FREE"`
//!
//! v1 note: Google's official Antigravity client ships its own client_id /
//! client_secret. Reusing them in a desktop app is gray-area; instead we
//! ship a placeholder that exposes the structure but defers actual UA
//! spoofing to a future drop. For now the fetcher attempts the call with
//! plain bearer auth and gracefully reports FREE if the upstream rejects.

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
use crate::subscription::{BillingCycle, Subscription, SubscriptionUsage};
use crate::{UsageError, UsageResult};

const AUTHORIZE_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const LOAD_CODE_ASSIST_URL: &str =
    "https://cloudcode-pa.googleapis.com/v1internal/loadCodeAssist";
const CLIENT_ID: &str = "681255809395-oo8ft2oprdrnp9e3aqf6av3hmdib135j.apps.googleusercontent.com";
const SCOPES: &str =
    "https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/userinfo.email https://www.googleapis.com/auth/userinfo.profile";
const CALLBACK_PORT: u16 = 1457;
const ANTIGRAVITY_UA: &str = "antigravity/1.20.5 darwin/arm64";
const ANTIGRAVITY_X_GOOG: &str = "antigravity/1.20.5";

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

pub async fn start_login(_region: Option<&str>) -> UsageResult<(String, String)> {
    let pkce = PkcePair::generate();
    let state = crate::oauth::pkce::random_state();
    let redirect = format!("http://127.0.0.1:{}/auth/callback", CALLBACK_PORT);
    let auth_url = format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&access_type=offline&prompt=consent&code_challenge={}&code_challenge_method=S256&state={}",
        AUTHORIZE_URL,
        CLIENT_ID,
        urlencoding(&redirect),
        urlencoding(SCOPES),
        pkce.challenge,
        state,
    );

    let pending_id = crate::oauth::pending_state::register(
        "antigravity",
        None,
        auth_url.clone(),
    );

    let pid = pending_id.clone();
    let verifier = pkce.verifier.clone();
    let state_for_task = state.clone();
    tokio::spawn(async move {
        let result = drive_login(state_for_task, verifier, redirect).await;
        if let Some(tx) = crate::oauth::pending_state::take_sender(&pid) {
            let _ = tx.send(result);
        }
    });

    Ok((auth_url, pending_id))
}

async fn drive_login(
    state: String,
    verifier: String,
    redirect_uri: String,
) -> UsageResult<Subscription> {
    let code = local_server::wait_for_callback(CALLBACK_PORT, state, Some(Duration::from_secs(300))).await?;
    let tokens = exchange_code(&code, &verifier, &redirect_uri).await?;
    let access_token = tokens
        .access_token
        .ok_or_else(|| UsageError::Other("Antigravity 缺少 access_token".into()))?;
    let expires_at = tokens
        .expires_in
        .map(|s| Utc::now().timestamp() + s)
        .or_else(|| token_refresh::jwt_exp(&access_token));
    let email = tokens
        .id_token
        .as_deref()
        .and_then(|jwt| token_refresh::jwt_string(jwt, &["email"]));

    finalize(access_token, tokens.refresh_token, expires_at, email).await
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
            ("client_id", CLIENT_ID),
            ("redirect_uri", redirect_uri),
            ("code_verifier", verifier),
        ])
        .send()
        .await
        .map_err(|e| UsageError::Fetcher(format!("Google token：{}", e)))?;
    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(UsageError::Fetcher(format!(
            "Google token 返回：{}",
            body.chars().take(200).collect::<String>()
        )));
    }
    resp.json::<TokenResponse>()
        .await
        .map_err(|e| UsageError::Fetcher(format!("Google 解析 token: {}", e)))
}

async fn finalize(
    access_token: String,
    refresh_token: Option<String>,
    expires_at: Option<i64>,
    email: Option<String>,
) -> UsageResult<Subscription> {
    let now = Utc::now().timestamp();
    let sub = Subscription {
        id: uuid::Uuid::new_v4().to_string(),
        catalog_id: "antigravity".to_string(),
        display_name: email.clone().unwrap_or_else(|| "Antigravity".to_string()),
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
        oauth_account_id: email,
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
        .map_err(|e| UsageError::Other(format!("Antigravity 订阅保存失败：{}", e)))?;
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
    let resp = client
        .post(LOAD_CODE_ASSIST_URL)
        .bearer_auth(access_token)
        .header(reqwest::header::USER_AGENT, ANTIGRAVITY_UA)
        .header("x-goog-api-client", ANTIGRAVITY_X_GOOG)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .json(&serde_json::json!({ "clientMetadata": {} }))
        .send()
        .await
        .map_err(|e| UsageError::Fetcher(format!("Antigravity loadCodeAssist: {}", e)))?;
    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(UsageError::AuthRequired);
    }
    if !status.is_success() {
        // Fail soft: still produce a card with FREE so user sees the entry.
        return Ok(SubscriptionUsage {
            subscription_id: subscription_id.to_string(),
            fetched_at: Utc::now().timestamp(),
            plan_name: Some("FREE".to_string()),
            error: Some(format!("loadCodeAssist 状态 {}（可能 UA 被拒绝）", status)),
            ..Default::default()
        });
    }
    let value: Value = resp
        .json()
        .await
        .map_err(|e| UsageError::Fetcher(format!("Antigravity 解析: {}", e)))?;

    let plan_name = pick_plan_name(&value).unwrap_or_else(|| "FREE".to_string());
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

fn pick_plan_name(v: &Value) -> Option<String> {
    let candidates: &[&[&str]] = &[
        &["paidTier", "id"],
        &["currentTier", "id"],
        &["allowedTiers", "0", "id"],
        &["paid_tier", "id"],
        &["current_tier", "id"],
    ];
    for path in candidates {
        let mut cur = v;
        let mut ok = true;
        for key in *path {
            if let Ok(idx) = key.parse::<usize>() {
                match cur.get(idx) {
                    Some(next) => cur = next,
                    None => {
                        ok = false;
                        break;
                    }
                }
            } else {
                match cur.get(*key) {
                    Some(next) => cur = next,
                    None => {
                        ok = false;
                        break;
                    }
                }
            }
        }
        if ok {
            if let Some(s) = cur.as_str().filter(|s| !s.is_empty()) {
                return Some(s.to_uppercase());
            }
        }
    }
    // Try default-flagged tier in allowed_tiers
    if let Some(arr) = v.get("allowedTiers").and_then(|v| v.as_array()) {
        for entry in arr {
            if entry.get("isDefault").and_then(|v| v.as_bool()).unwrap_or(false) {
                if let Some(id) = entry.get("id").and_then(|v| v.as_str()) {
                    return Some(id.to_uppercase());
                }
            }
        }
        if let Some(first) = arr.first() {
            if let Some(id) = first.get("id").and_then(|v| v.as_str()) {
                return Some(id.to_uppercase());
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
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}
