//! Antigravity (Google IDE) OAuth fetcher.
//!
//! Google OAuth + Cloud Code Assist (`loadCodeAssist` + `fetchAvailableModels`).

use chrono::Utc;
use std::time::Duration;

use super::common::SubscriptionBuilder;
use crate::cloud_code::{self, LoadCodeAssistResult};
use crate::crypto;
use crate::oauth::local_server;
use crate::oauth::token_endpoint::{self, TokenResponse};
use crate::oauth::token_refresh;
use crate::storage;
use crate::subscription::{Subscription, SubscriptionUsage, UsageWindow};
use crate::urlencode;
use crate::{UsageError, UsageResult};

use crate::antigravity_oauth_config::antigravity_oauth_config;

const ANTIGRAVITY_TITLE_PLACEHOLDERS: &[&str] = &["Antigravity"];

const AUTHORIZE_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v2/userinfo?alt=json";
const SCOPES: &str = "https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/userinfo.email https://www.googleapis.com/auth/userinfo.profile https://www.googleapis.com/auth/cclog https://www.googleapis.com/auth/experimentsandconfigs";
const CALLBACK_PORT: u16 = 51121;

pub async fn start_login(
    _region: Option<&str>,
    target_subscription_id: Option<&str>,
) -> UsageResult<super::OAuthStartInfo> {
    let state = crate::oauth::pkce::random_state();
    let redirect = format!("http://localhost:{}/oauth-callback", CALLBACK_PORT);
    let auth_url = build_auth_url(&redirect, &state, &antigravity_oauth_config()?.client_id);

    let pending_id = crate::oauth::pending_state::register("antigravity", None, auth_url.clone());
    crate::oauth::pending_state::set_target_subscription_id(
        &pending_id,
        target_subscription_id.map(str::to_string),
    );
    let pid = pending_id.clone();
    let state_for_task = state.clone();
    tokio::spawn(async move {
        let target_subscription_id = crate::oauth::pending_state::target_subscription_id(&pid);
        let result = drive_login(state_for_task, redirect, target_subscription_id).await;
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
        urlencode::encode(redirect),
        urlencode::encode(SCOPES),
        state,
    )
}

async fn drive_login(
    state: String,
    redirect_uri: String,
    target_subscription_id: Option<String>,
) -> UsageResult<Subscription> {
    let code =
        local_server::wait_for_callback(CALLBACK_PORT, state, Some(Duration::from_secs(300)))
            .await?;
    let tokens = exchange_code(&code, &redirect_uri).await?;
    let access_token = tokens
        .access_token()
        .ok_or_else(|| UsageError::Other("Antigravity 缺少 access_token".into()))?
        .to_string();
    let expires_at = tokens.expires_at();
    let email = fetch_email(&access_token).await.or_else(|| {
        tokens
            .id_token()
            .and_then(|jwt| token_refresh::jwt_string(jwt, &["email"]))
    });
    let refresh_token = tokens.refresh_token().map(str::to_string);

    // Same catalog lock the refresh path takes, so a concurrent Antigravity
    // refresh cannot overwrite the credentials this login just stored.
    crate::refresh_guard::with_catalog_lock("antigravity", || async {
        finalize(
            access_token,
            refresh_token,
            expires_at,
            email,
            target_subscription_id.as_deref(),
        )
        .await
    })
    .await?
}

async fn fetch_email(access_token: &str) -> Option<String> {
    let client = crate::http_client::usage_http_client().ok()?;
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
    token_endpoint::post_token(
        TOKEN_URL,
        &[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("client_id", oauth.client_id.as_str()),
            ("client_secret", oauth.client_secret.as_str()),
            ("redirect_uri", redirect_uri),
        ],
        "Google token",
    )
    .await
}

async fn finalize(
    access_token: String,
    refresh_token: Option<String>,
    expires_at: Option<i64>,
    email: Option<String>,
    target_subscription_id: Option<&str>,
) -> UsageResult<Subscription> {
    let display_name = email.clone().unwrap_or_else(|| "Antigravity".to_string());
    let mut sub = SubscriptionBuilder::new(
        "antigravity",
        display_name,
        "USD",
        &access_token,
        expires_at,
    )
    .refresh_token(refresh_token)
    .oauth_account_id(email)
    .build();

    // Re-authorizing an existing card must land back on that card
    // (docs/features/usage/README.md), not add a second one beside it.
    let cached_project = match super::common::reauth_target("antigravity", target_subscription_id) {
        Some(existing) => {
            let cached_project = existing.note.clone();
            super::common::carry_over_user_metadata(
                &mut sub,
                &existing,
                ANTIGRAVITY_TITLE_PLACEHOLDERS,
            );
            cached_project
        }
        None => None,
    };

    if let Ok(usage) = build_usage(&sub.id, &access_token, cached_project.as_deref()).await {
        storage::save_usage_snapshot(usage).ok();
    }
    storage::upsert_subscription(sub)
        .map_err(|e| UsageError::Other(format!("Antigravity 订阅保存失败：{}", e)))
}

super::common::impl_oauth_fetch!();

async fn fetch_inner(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    ensure_fresh_access_token(subscription).await?;
    let access_token =
        crate::fetchers::decrypt_required(&subscription.access_token_encrypted, "access_token")?;
    // Upgrade "Antigravity" placeholders once userinfo (or cached oauth_account_id) yields email.
    if let Some(email) = fetch_email(&access_token).await.or_else(|| {
        subscription
            .oauth_account_id
            .as_deref()
            .filter(|s| super::common::looks_like_email(s))
            .map(str::to_string)
    }) {
        super::common::apply_email_title(
            subscription,
            Some(&email),
            ANTIGRAVITY_TITLE_PLACEHOLDERS,
        );
        if subscription.oauth_account_id.is_none() {
            subscription.oauth_account_id = Some(email);
        }
    }
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
    // Google answers a revoked grant with 400 + `invalid_grant`, so the shared
    // token endpoint (not the status code) is what turns that into
    // `AuthRequired` and gives the card its "重新授权" affordance.
    let tokens = cloud_code::refresh_antigravity_access_token(&refresh).await?;
    if let Some(at) = tokens.access_token() {
        subscription.access_token_encrypted = Some(crypto::encrypt(at));
        subscription.access_token_expires_at = tokens.expires_at();
    }
    if let Some(rt) = tokens.refresh_token() {
        subscription.refresh_token_encrypted = Some(crypto::encrypt(rt));
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
