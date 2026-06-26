//! Antigravity (Google IDE) OAuth fetcher.
//!
//! Google OAuth + Cloud Code Assist (`loadCodeAssist` + `fetchAvailableModels`).

use chrono::Utc;
use serde::Deserialize;
use std::time::Duration;

use crate::catalog::AuthMode;
use crate::cloud_code::{self, LoadCodeAssistResult};
use crate::crypto;
use crate::oauth::local_server;
use crate::oauth::token_refresh;
use crate::storage;
use crate::subscription::{BillingCycle, Subscription, SubscriptionUsage, UsageWindow};
use crate::{UsageError, UsageResult};

use crate::antigravity_oauth_config::antigravity_oauth_config;

const AUTHORIZE_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v2/userinfo?alt=json";
const SCOPES: &str = "https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/userinfo.email https://www.googleapis.com/auth/userinfo.profile https://www.googleapis.com/auth/cclog https://www.googleapis.com/auth/experimentsandconfigs";
const CALLBACK_PORT: u16 = 51121;

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
    let state = crate::oauth::pkce::random_state();
    let redirect = format!("http://localhost:{}/oauth-callback", CALLBACK_PORT);
    let auth_url = build_auth_url(&redirect, &state, &antigravity_oauth_config()?.client_id);

    let pending_id = crate::oauth::pending_state::register("antigravity", None, auth_url.clone());
    let pid = pending_id.clone();
    let state_for_task = state.clone();
    tokio::spawn(async move {
        let result = drive_login(state_for_task, redirect).await;
        if let Some(tx) = crate::oauth::pending_state::take_sender(&pid) {
            let _ = tx.send(result);
        }
    });

    Ok(super::OAuthStartInfo::browser(auth_url, pending_id))
}

fn build_auth_url(redirect: &str, state: &str, client_id: &str) -> String {
    format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&access_type=offline&prompt=consent&state={}",
        AUTHORIZE_URL,
        client_id,
        urlencoding(&redirect),
        urlencoding(SCOPES),
        state,
    )
}

async fn drive_login(state: String, redirect_uri: String) -> UsageResult<Subscription> {
    let code =
        local_server::wait_for_callback(CALLBACK_PORT, state, Some(Duration::from_secs(300)))
            .await?;
    let tokens = exchange_code(&code, &redirect_uri).await?;
    let access_token = tokens
        .access_token
        .ok_or_else(|| UsageError::Other("Antigravity 缺少 access_token".into()))?;
    let expires_at = tokens
        .expires_in
        .map(|s| Utc::now().timestamp() + s)
        .or_else(|| token_refresh::jwt_exp(&access_token));
    let email = fetch_email(&access_token).await.or_else(|| {
        tokens
            .id_token
            .as_deref()
            .and_then(|jwt| token_refresh::jwt_string(jwt, &["email"]))
    });

    finalize(access_token, tokens.refresh_token, expires_at, email).await
}

async fn fetch_email(access_token: &str) -> Option<String> {
    let client = crate::http_client::usage_reqwest_with_active_fingerprint().ok()?;
    let resp = client
        .get(USERINFO_URL)
        .bearer_auth(access_token)
        .send()
        .await
        .ok()?;
    let value = resp.json::<serde_json::Value>().await.ok()?;
    value
        .get("email")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
}

async fn exchange_code(code: &str, redirect_uri: &str) -> UsageResult<TokenResponse> {
    let oauth = antigravity_oauth_config()?;
    let client = crate::http_client::usage_reqwest_with_active_fingerprint()?;
    let resp = client
        .post(TOKEN_URL)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("client_id", oauth.client_id.as_str()),
            ("client_secret", oauth.client_secret.as_str()),
            ("redirect_uri", redirect_uri),
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
        platform_token_encrypted: None,
        access_token_encrypted: Some(crypto::encrypt(&access_token)),
        refresh_token_encrypted: refresh_token.as_deref().map(crypto::encrypt),
        access_token_expires_at: expires_at,
        id_token_encrypted: None,
        oauth_account_id: email,
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
    if let Ok(usage) = build_usage(&sub.id, &access_token, None).await {
        storage::save_usage_snapshot(usage).ok();
    }
    storage::upsert_subscription(sub)
        .map_err(|e| UsageError::Other(format!("Antigravity 订阅保存失败：{}", e)))
}

pub async fn fetch(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    let fp_id = subscription.fingerprint_id.clone();
    crate::http_client::with_fingerprint(fp_id, fetch_inner(subscription)).await
}

async fn fetch_inner(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    ensure_fresh_access_token(subscription).await?;
    let access_token = crate::fetchers::decrypt_required(&subscription.access_token_encrypted, "access_token")?;
    let cached_project = subscription.note.clone();
    let (load, cleared_cached_project) = match load_code_assist_with_project_fallback(
        &access_token,
        cached_project.as_deref(),
    )
    .await
    {
        Ok(v) => v,
        Err(UsageError::AuthRequired) => return Err(UsageError::AuthRequired),
        Err(e) => {
            return Ok(SubscriptionUsage {
                subscription_id: subscription.id.clone(),
                fetched_at: Utc::now().timestamp(),
                plan_name: Some("FREE".to_string()),
                credits: Vec::new(),
                error: Some(e.to_string()),
                ..Default::default()
            });
        }
    };
    if cleared_cached_project {
        subscription.note = None;
    }
    if let Some(pid) = &load.project_id {
        subscription.note = Some(pid.clone());
    }
    let breakdown = cloud_code::fetch_model_quotas(&access_token, load.project_id.as_deref())
        .await
        .unwrap_or_default();

    Ok(usage_from_load(&subscription.id, &load, breakdown))
}

async fn load_code_assist_with_project_fallback(
    access_token: &str,
    cached_project: Option<&str>,
) -> UsageResult<(LoadCodeAssistResult, bool)> {
    match cloud_code::load_code_assist(access_token, cached_project).await {
        Ok(load) => Ok((load, false)),
        Err(UsageError::AuthRequired) => Err(UsageError::AuthRequired),
        Err(first_err)
            if cached_project.filter(|s| !s.is_empty()).is_some() && is_bad_request(&first_err) =>
        {
            cloud_code::load_code_assist(access_token, None)
                .await
                .map(|load| (load, true))
        }
        Err(e) => Err(e),
    }
}

fn is_bad_request(error: &UsageError) -> bool {
    matches!(error, UsageError::Fetcher(message) if message.contains("400") || message.contains("Bad Request"))
}

async fn build_usage(
    subscription_id: &str,
    access_token: &str,
    cached_project_id: Option<&str>,
) -> UsageResult<SubscriptionUsage> {
    let load = match cloud_code::load_code_assist(access_token, cached_project_id).await {
        Ok(v) => v,
        Err(UsageError::AuthRequired) => return Err(UsageError::AuthRequired),
        Err(e) => {
            return Ok(SubscriptionUsage {
                subscription_id: subscription_id.to_string(),
                fetched_at: Utc::now().timestamp(),
                plan_name: Some("FREE".to_string()),
                credits: Vec::new(),
                error: Some(e.to_string()),
                ..Default::default()
            });
        }
    };

    let breakdown = cloud_code::fetch_model_quotas(access_token, load.project_id.as_deref())
        .await
        .unwrap_or_default();

    Ok(usage_from_load(subscription_id, &load, breakdown))
}

async fn ensure_fresh_access_token(subscription: &mut Subscription) -> UsageResult<()> {
    if !token_refresh::needs_refresh(subscription.access_token_expires_at) {
        return Ok(());
    }
    let rt_cipher = subscription
        .refresh_token_encrypted
        .as_deref()
        .ok_or(UsageError::AuthRequired)?;
    let refresh = crypto::decrypt(rt_cipher);
    if refresh.is_empty() {
        return Err(UsageError::AuthRequired);
    }
    let tokens = cloud_code::refresh_antigravity_access_token(&refresh).await?;
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

fn usage_from_load(
    subscription_id: &str,
    load: &LoadCodeAssistResult,
    breakdown: Vec<UsageWindow>,
) -> SubscriptionUsage {
    let monthly = if breakdown.is_empty() {
        None
    } else {
        let avg =
            breakdown.iter().filter_map(|w| w.percent).sum::<i32>() / breakdown.len().max(1) as i32;
        Some(UsageWindow {
            label: "模型额度".to_string(),
            used: (100 - avg).max(0) as i64,
            total: Some(100),
            percent: Some(avg),
            reset_at: None,
            breakdown,
        })
    };

    SubscriptionUsage {
        subscription_id: subscription_id.to_string(),
        fetched_at: Utc::now().timestamp(),
        plan_name: Some(load.plan_name.clone()),
        hourly: None,
        weekly: None,
        monthly,
        balance: None,
        credits: load.credits.clone(),
        error: None,
        api_keys: Vec::new(),
        deepseek_analytics: None,
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_url_uses_antigravity_client_without_pkce() {
        let url = build_auth_url(
            "http://localhost:51121/oauth-callback",
            "state-123",
            "test-client-id.apps.googleusercontent.com",
        );

        assert!(url.contains("client_id=test-client-id.apps.googleusercontent.com"));
        assert!(url.contains("redirect_uri=http%3A%2F%2Flocalhost%3A51121%2Foauth-callback"));
        assert!(url.contains("https%3A%2F%2Fwww.googleapis.com%2Fauth%2Fcclog"));
        assert!(url.contains("https%3A%2F%2Fwww.googleapis.com%2Fauth%2Fexperimentsandconfigs"));
        assert!(!url.contains("681255809395-oo8ft2oprdrnp9e3aqf6av3hmdib135j"));
        assert!(!url.contains("code_challenge"));
    }
}
