//! Grok (xAI) OAuth + billing fetcher.
//!
//! Mirrors the Grok CLI flow used by CLIProxyAPI:
//! 1. Open `https://auth.x.ai/oauth2/authorize` with PKCE and
//!    `redirect_uri=http://127.0.0.1:56121/callback`.
//! 2. Exchange `code` at `https://auth.x.ai/oauth2/token`.
//! 3. GET `https://cli-chat-proxy.grok.com/v1/billing` for credit-quota data.
//!
//! Grok exposes two distinct allowances (mirrored in its CLI `/usage`): a
//! monthly numeric credit quota (`monthlyLimit`/`used`, USD cents) from the
//! default view, and a weekly soft-limit progress (`creditUsagePercent` +
//! `currentPeriod`) from the `?format=credits` view. We render both. The real
//! default payload shape is:
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
use serde_json::Value;
use std::sync::LazyLock;
use std::time::Duration;
use url::Url;

use super::common::SubscriptionBuilder;
use crate::crypto;
use crate::oauth::local_server;
use crate::oauth::pkce::PkcePair;
use crate::oauth::token_endpoint::{self, TokenResponse};
use crate::oauth::token_refresh;
use crate::oauth_clients;
use crate::storage;
use crate::subscription::{CreditInfo, Subscription, SubscriptionUsage, UsageWindow};
use crate::{UsageError, UsageResult};

#[path = "reset.rs"]
mod reset;
#[cfg(test)]
pub(super) use reset::{
    GrokResetToken, decode_grpc_web_frames, decode_remaining_resets_response,
    encode_grpc_web_frame, encode_redeem_reset_request, encode_varint, select_reset_token,
};
use reset::{redeem_available_reset, remaining_reset_credits};

const AUTHORIZE_URL: &str = "https://auth.x.ai/oauth2/authorize";
const TOKEN_URL: &str = "https://auth.x.ai/oauth2/token";
static CLIENT_ID: LazyLock<String> = LazyLock::new(|| {
    oauth_clients::client_id!(
        "xai",
        "SKILLSTAR_XAI_CLIENT_ID",
        "b1a00492-073a-47ea-816f-4c329264a828"
    )
});

/// The resolved Grok OAuth `client_id` (honours env / file overrides). The CLI
/// account switch (`skillstar_app::usage_switch`) keys `~/.grok/auth.json` by
/// `https://auth.x.ai::<this id>`, so it must read the same resolved value the
/// fetcher uses rather than a separate hard-coded copy.
pub fn client_id() -> &'static str {
    CLIENT_ID.as_str()
}
const SCOPES: &str = concat!(
    "openid profile email offline_access ",
    "grok-cli:access conversations:read conversations:write api:access"
);
const CALLBACK_PORT: u16 = 56121;
const CALLBACK_PATH: &str = "/callback";
const BILLING_URL: &str = "https://cli-chat-proxy.grok.com/v1/billing";
/// Grok's web client exposes a separate consumer-billing gRPC-Web service for
/// spending a reset credit. This is intentionally not the JSON billing
/// endpoint above: querying billing only redraws the existing quota.
const CONSUMER_UI_SERVICE_URL: &str = "https://grok.com/prod.mc.billing.ConsumerUiSvc";
/// The `credits`-format billing view. Same endpoint, different projection: it
/// drops the `monthlyLimit`/`used` numbers but adds `currentPeriod`
/// (`{ type: USAGE_PERIOD_TYPE_WEEKLY|_MONTHLY, start, end }`) plus a
/// `creditUsagePercent` (the weekly soft-limit usage, 0–100, omitted by proto3
/// when 0). These are the authoritative source for the *weekly* progress bar:
/// whether this account resets weekly, exactly when, and how much of the weekly
/// allowance is consumed. The numbers (`monthlyLimit`/`used`) are NOT in this
/// view — they come from the default view and drive the *monthly* numeric
/// quota. Fetching both lets us show the two distinct bars the Grok CLI's own
/// `/usage` shows (weekly limit left + monthly limit).
const BILLING_CREDITS_URL: &str = "https://cli-chat-proxy.grok.com/v1/billing?format=credits";
const DEFAULT_PLAN_NAME: &str = "Grok";
const GROK_RESET_CREDITS: &str = "grok-reset-credits";

/// The current usage period reported by `?format=credits`. `weekly` is
/// `Some(true)` for a weekly-reset plan, `Some(false)` for monthly, and `None`
/// when the type string is unrecognised. `usage_percent` is the weekly
/// `creditUsagePercent` (0–100), `None`/0 when the proxy omits it (no usage yet
/// this week).
#[derive(Debug, Clone, Default)]
struct CurrentPeriod {
    weekly: Option<bool>,
    end: Option<i64>,
    usage_percent: Option<f64>,
}

pub async fn start_login(
    _region: Option<&str>,
    target_subscription_id: Option<&str>,
) -> UsageResult<super::OAuthStartInfo> {
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

    let pending_id = register_pending_login(auth_url.clone(), target_subscription_id);
    let pid = pending_id.clone();
    let verifier = pkce.verifier.clone();
    let state_for_task = state.clone();
    tokio::spawn(async move {
        let target_subscription_id = crate::oauth::pending_state::target_subscription_id(&pid);
        let result = drive_login(
            state_for_task,
            verifier,
            redirect_uri,
            target_subscription_id,
        )
        .await;
        if let Some(tx) = crate::oauth::pending_state::take_sender(&pid) {
            let _ = tx.send(result);
        }
    });

    Ok(super::OAuthStartInfo::browser(auth_url, pending_id))
}

fn register_pending_login(auth_url: String, target_subscription_id: Option<&str>) -> String {
    let pending_id = crate::oauth::pending_state::register("xai", None, auth_url);
    crate::oauth::pending_state::set_target_subscription_id(
        &pending_id,
        target_subscription_id.map(str::to_string),
    );
    pending_id
}

async fn drive_login(
    state: String,
    verifier: String,
    redirect_uri: String,
    target_subscription_id: Option<String>,
) -> UsageResult<Subscription> {
    let code =
        local_server::wait_for_callback(CALLBACK_PORT, state, Some(Duration::from_secs(300)))
            .await?;
    let tokens = exchange_code(&code, &verifier, &redirect_uri).await?;
    crate::refresh_guard::with_catalog_lock("xai", || async {
        finalize(tokens, target_subscription_id.as_deref()).await
    })
    .await?
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
            .append_pair("client_id", CLIENT_ID.as_str())
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
    token_endpoint::post_token(
        TOKEN_URL,
        &[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", CLIENT_ID.as_str()),
            ("code_verifier", verifier),
        ],
        "Grok token",
    )
    .await
}

async fn finalize(
    tokens: TokenResponse,
    target_subscription_id: Option<&str>,
) -> UsageResult<Subscription> {
    let existing = match target_subscription_id {
        Some(id) => {
            let subscription = storage::get_subscription(id)?;
            if subscription.catalog_id != "xai"
                || subscription.auth_mode != crate::catalog::AuthMode::OAuth
            {
                return Err(UsageError::Other(format!(
                    "Grok 重新授权目标 {id} 不是 xAI OAuth 订阅"
                )));
            }
            Some(subscription)
        }
        None => None,
    };
    let sub = build_subscription(tokens, existing.as_ref())?;
    let account_changed = existing.as_ref().is_some_and(|existing| {
        let old_identity = subscription_account_identity(existing);
        let new_identity = subscription_account_identity(&sub);
        old_identity.is_some() && new_identity.is_some() && old_identity != new_identity
    });
    if account_changed {
        // The row id stays stable during targeted reauthorization, but usage
        // windows belong to the account identity. Do not carry the previous
        // account's weekly fallback into the newly bound account.
        storage::delete_usage_snapshot(&sub.id)?;
    }
    let access_token =
        crate::fetchers::decrypt_required(&sub.access_token_encrypted, "access_token")?;

    let usage = fetch_with_token(&sub.id, &access_token)
        .await
        .unwrap_or_else(|error| SubscriptionUsage {
            subscription_id: sub.id.clone(),
            fetched_at: Utc::now().timestamp(),
            plan_name: Some(DEFAULT_PLAN_NAME.to_string()),
            error: Some(format!("Grok 已重新授权，但用量刷新失败: {error}")),
            ..Default::default()
        });
    storage::save_usage_snapshot(usage).ok();
    storage::upsert_subscription(sub)
        .map_err(|e| UsageError::Other(format!("Grok 订阅保存失败: {}", e)))
}

fn build_subscription(
    tokens: TokenResponse,
    existing: Option<&Subscription>,
) -> UsageResult<Subscription> {
    let access_token = tokens.access_token().ok_or(UsageError::AuthRequired)?;
    let refresh_token = tokens.refresh_token();
    let id_token = tokens.id_token();
    // Some xAI token exchanges omit `id_token` (especially targeted
    // reauthorization). In that case the new access token is still the
    // authoritative account identity; carrying the old row's account id would
    // bind a fresh token to the wrong Grok card.
    let email = id_token
        .and_then(|token| token_refresh::jwt_string(token, &["email"]))
        .or_else(|| token_refresh::jwt_string(access_token, &["email"]));
    let subject = id_token
        .and_then(|token| token_refresh::jwt_string(token, &["sub"]))
        .or_else(|| token_refresh::jwt_string(access_token, &["sub"]));
    // Card already shows provider branding (logo / plan badge); title is the account only.
    let display_name = email
        .clone()
        .unwrap_or_else(|| DEFAULT_PLAN_NAME.to_string());
    let expires_at = tokens.expires_at();

    let mut sub = SubscriptionBuilder::new("xai", display_name, "USD", access_token, expires_at)
        .refresh_token(refresh_token.map(str::to_string))
        .id_token(id_token.map(str::to_string))
        .oauth_account_id(subject.or(email))
        .build();

    if let Some(existing) = existing {
        let existing_identity = subscription_account_identity(existing);
        let new_identity = subscription_account_identity(&sub);
        let same_identity_proven = existing_identity.is_some() && existing_identity == new_identity;
        sub.oauth_account_id = new_identity;
        sub.id = existing.id.clone();
        sub.plan_tier = existing.plan_tier.clone();
        sub.monthly_price = existing.monthly_price;
        sub.currency = existing.currency.clone();
        sub.billing_cycle = existing.billing_cycle;
        sub.start_date = existing.start_date;
        sub.renew_date = existing.renew_date;
        sub.auto_renew = existing.auto_renew;
        if same_identity_proven {
            sub.refresh_token_encrypted = sub
                .refresh_token_encrypted
                .or_else(|| existing.refresh_token_encrypted.clone());
            sub.id_token_encrypted = sub
                .id_token_encrypted
                .or_else(|| existing.id_token_encrypted.clone());
        }
        sub.manual_quota = existing.manual_quota.clone();
        sub.note = existing.note.clone();
        sub.sort_index = existing.sort_index;
        sub.created_at = existing.created_at;
    }

    Ok(sub)
}

fn subscription_account_identity(subscription: &Subscription) -> Option<String> {
    for encrypted in [
        subscription.access_token_encrypted.as_deref(),
        subscription.id_token_encrypted.as_deref(),
    ] {
        if let Some(identity) = encrypted
            .map(crypto::decrypt)
            .filter(|token| !token.is_empty())
            .and_then(|token| {
                token_refresh::jwt_string(&token, &["sub"])
                    .or_else(|| token_refresh::jwt_string(&token, &["email"]))
            })
        {
            return Some(identity.trim().to_ascii_lowercase());
        }
    }
    subscription
        .oauth_account_id
        .as_deref()
        .map(str::trim)
        .filter(|identity| !identity.is_empty())
        .map(str::to_ascii_lowercase)
}

super::common::impl_oauth_fetch!();

async fn fetch_inner(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    if token_refresh::needs_refresh(subscription.access_token_expires_at) {
        refresh_xai_tokens(subscription).await?;
    }
    // Legacy "Grok · email" / bare "Grok" → bare email when oauth_account_id or id_token has it.
    maybe_upgrade_xai_title(subscription);

    let access_token =
        crate::fetchers::decrypt_required(&subscription.access_token_encrypted, "access_token")?;
    match fetch_with_token(&subscription.id, &access_token).await {
        Err(UsageError::AuthRequired) => {
            refresh_xai_tokens(subscription).await?;
            maybe_upgrade_xai_title(subscription);
            let access_token = crate::fetchers::decrypt_required(
                &subscription.access_token_encrypted,
                "access_token",
            )?;
            fetch_with_token(&subscription.id, &access_token).await
        }
        other => other,
    }
}

fn maybe_upgrade_xai_title(subscription: &mut Subscription) {
    let email = subscription
        .id_token_encrypted
        .as_deref()
        .map(crypto::decrypt)
        .filter(|s| !s.is_empty())
        .and_then(|jwt| super::common::email_from_jwt(&jwt))
        .or_else(|| {
            subscription
                .oauth_account_id
                .as_deref()
                .filter(|s| super::common::looks_like_email(s))
                .map(str::to_string)
        })
        // Strip legacy "Grok · user@x.com" already stored as display_name.
        .or_else(|| {
            subscription
                .display_name
                .strip_prefix("Grok · ")
                .map(str::trim)
                .filter(|s| super::common::looks_like_email(s))
                .map(str::to_string)
        });
    super::common::apply_email_title(subscription, email.as_deref(), &["Grok"]);
}

/// Refresh Grok OAuth material when the account-switch transaction has already
/// determined that the effective expiry (stored metadata plus JWT `exp`) is
/// near/unknown. This deliberately does not fetch billing data.
pub async fn refresh_for_cli_switch(subscription: &mut Subscription) -> UsageResult<()> {
    if subscription.catalog_id != "xai" {
        return Err(UsageError::Other(format!(
            "Grok credential refresh received catalog {}",
            subscription.catalog_id
        )));
    }
    refresh_xai_tokens(subscription).await?;
    maybe_upgrade_xai_title(subscription);
    Ok(())
}

/// Consume one of the account's available Grok usage-reset credits, then
/// return the newly-reset billing snapshot.
///
/// Grok's web client first lists unexpired reset tokens and then redeems one
/// explicit token id. Keeping that two-step flow here is important: the reset
/// is a real provider-side mutation, not a local refresh or a billing-cache
/// invalidation.
pub async fn reset_quota(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    if subscription.catalog_id != "xai" {
        return Err(UsageError::Other(format!(
            "Grok quota reset received catalog {}",
            subscription.catalog_id
        )));
    }

    if token_refresh::needs_refresh(subscription.access_token_expires_at) {
        refresh_xai_tokens(subscription).await?;
        maybe_upgrade_xai_title(subscription);
    }

    let access_token =
        crate::fetchers::decrypt_required(&subscription.access_token_encrypted, "access_token")?;
    let reset_result = redeem_available_reset(&access_token).await;
    let access_token = match reset_result {
        Ok(()) => access_token,
        Err(UsageError::AuthRequired) => {
            refresh_xai_tokens(subscription).await?;
            maybe_upgrade_xai_title(subscription);
            let refreshed_token = crate::fetchers::decrypt_required(
                &subscription.access_token_encrypted,
                "access_token",
            )?;
            redeem_available_reset(&refreshed_token).await?;
            refreshed_token
        }
        Err(error) => return Err(error),
    };

    // The web client gives the billing projection two seconds to converge
    // after RedeemReset. Match that provider-side propagation window before
    // rebuilding the card snapshot.
    tokio::time::sleep(Duration::from_secs(2)).await;
    fetch_with_token(&subscription.id, &access_token).await
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

    let tokens = token_endpoint::post_token(
        TOKEN_URL,
        &[
            ("grant_type", "refresh_token"),
            ("client_id", CLIENT_ID.as_str()),
            ("refresh_token", refresh_token.trim()),
        ],
        "Grok refresh",
    )
    .await?;
    let access_token = tokens.access_token().ok_or(UsageError::AuthRequired)?;

    subscription.access_token_encrypted = Some(crypto::encrypt(access_token));
    if let Some(rt) = tokens.refresh_token() {
        subscription.refresh_token_encrypted = Some(crypto::encrypt(rt));
    }
    subscription.access_token_expires_at = tokens.expires_at();

    if let Some(id_token) = tokens.id_token() {
        if let Some(email) = super::common::email_from_jwt(id_token) {
            super::common::apply_email_title(subscription, Some(&email), &["Grok"]);
        }
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
        .map_err(|e| UsageError::transport("Grok billing", e))?;

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

    let payload: Value = serde_json::from_str(&body)
        .map_err(|e| UsageError::Fetcher(format!("Grok billing JSON 解析失败: {}", e)))?;

    // Best-effort: the monthly numbers come from the default view above; the
    // weekly progress bar (period type, reset, `creditUsagePercent`) comes from
    // the credits view.
    let (period, reset_credits) = tokio::join!(
        fetch_current_period(&client, access_token),
        remaining_reset_credits(access_token),
    );
    let mut usage = build_subscription_usage(subscription_id, &payload, period)?;

    // Keep reset credits in the existing provider-specific credits projection
    // so the frontend can show the real count without adding a generic field
    // to every provider's usage snapshot.
    if let Ok(count) = reset_credits {
        usage.credits.push(CreditInfo {
            credit_type: GROK_RESET_CREDITS.to_string(),
            credit_amount: Some(count.to_string()),
            minimum_credit_amount_for_usage: None,
        });
    }

    // Card-shape stability: a weekly Grok plan must keep its weekly bar across
    // refreshes. The credits view is slow (~2.5s) and best-effort, so a single
    // transient miss would otherwise collapse the card from two bars to one
    // (the "2 kinds of cards" symptom). When this round produced no weekly bar,
    // reuse the subscription's last known weekly window instead of dropping it.
    if usage.weekly.is_none()
        && let Ok(Some(prev)) = storage::get_usage_snapshot(subscription_id)
        && let Some(prev_weekly) = prev.weekly
    {
        usage.weekly = Some(prev_weekly);
    }

    Ok(usage)
}

/// Fetch `?format=credits` and extract the current usage period. Returns `None`
/// only after a retry also fails, so the caller degrades gracefully. The retry
/// matters because the credits view is slow and occasionally flaky under the
/// parallel multi-account refresh — a silent miss would drop the weekly bar.
async fn fetch_current_period(
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
    let resp = client
        .get(BILLING_CREDITS_URL)
        .bearer_auth(access_token.trim())
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body = resp.text().await.ok()?;
    let payload: Value = serde_json::from_str(&body).ok()?;
    parse_current_period(&payload)
}

/// Parse `currentPeriod` from a `?format=credits` billing payload. Tolerates
/// the root or `config`-wrapped shape (via [`candidate_roots`]).
fn parse_current_period(payload: &Value) -> Option<CurrentPeriod> {
    let roots = candidate_roots(payload);
    let cp = pick_value_multi(&roots, &[&["currentPeriod"], &["current_period"]])?;
    let typ = cp.get("type").and_then(Value::as_str).unwrap_or("");
    let weekly = if typ.contains("WEEKLY") {
        Some(true)
    } else if typ.contains("MONTHLY") {
        Some(false)
    } else {
        None
    };
    let end = cp
        .get("end")
        .and_then(parse_timestamp)
        .or_else(|| cp.get("billingPeriodEnd").and_then(parse_timestamp));
    // `creditUsagePercent` is a sibling of `currentPeriod` (not nested), a plain
    // float in 0..=100. proto3 omits it when 0 → treat absence as 0% downstream.
    let usage_percent = pick_value_multi(
        &roots,
        &[&["creditUsagePercent"], &["credit_usage_percent"]],
    )
    .and_then(percent_value);
    if weekly.is_none() && end.is_none() {
        return None;
    }
    Some(CurrentPeriod {
        weekly,
        end,
        usage_percent,
    })
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

/// Build Grok usage as up to two distinct bars, mirroring the Grok CLI `/usage`:
///
/// * **Monthly numeric quota** (`monthly`) — `used`/`monthlyLimit` (USD cents)
///   from the default billing view, resetting on the monthly billing cycle.
///   Always present when the account exposes numbers.
/// * **Weekly progress bar** (`weekly`) — only for weekly-reset plans. Driven by
///   `creditUsagePercent` (a percent, no absolute number exists) from the
///   `?format=credits` view, resetting on `currentPeriod.end`. Percent-only, so
///   the UI renders it as a plain progress bar.
fn build_subscription_usage(
    subscription_id: &str,
    payload: &Value,
    period: Option<CurrentPeriod>,
) -> UsageResult<SubscriptionUsage> {
    let roots = candidate_roots(payload);

    let monthly_limit = pick_cent_multi(&roots, &[&["monthlyLimit"], &["monthly_limit"]]);
    let used = pick_cent_multi(
        &roots,
        &[&["usage", "totalUsed"], &["usage", "total_used"], &["used"]],
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

    let is_weekly = matches!(period.as_ref().and_then(|p| p.weekly), Some(true));

    if monthly_limit.is_none()
        && used.is_none()
        && on_demand_cap.is_none()
        && billing_period_end.is_none()
        && !is_weekly
    {
        return Err(UsageError::Fetcher(
            "Grok billing 未返回可展示额度字段".into(),
        ));
    }

    // Monthly numeric quota: absolute credits, resets on the monthly billing
    // cycle (the default view's `billingPeriodEnd`; fall back to a monthly
    // `currentPeriod.end` if the calendar-month field is missing).
    let monthly_reset = billing_period_end.or_else(|| match period.as_ref() {
        Some(p) if p.weekly == Some(false) => p.end,
        _ => None,
    });
    let monthly = if monthly_limit.is_some() || used.is_some() {
        let used_cents = used.unwrap_or(0.0).round().max(0.0) as i64;
        let total_cents = monthly_limit.map(|v| v.round().max(0.0) as i64);
        let percent = total_cents
            .filter(|total| *total > 0)
            .map(|total| ((used_cents as f64 / total as f64) * 100.0).round() as i32);
        Some(UsageWindow {
            label: "Monthly credits".to_string(),
            used: used_cents,
            total: total_cents,
            percent,
            reset_at: monthly_reset,
            breakdown: Vec::new(),
        })
    } else {
        None
    };

    // Weekly progress bar: percent-only (no absolute weekly number is exposed),
    // resets on `currentPeriod.end`. Only for weekly-reset plans.
    let weekly = period.as_ref().filter(|_| is_weekly).map(|p| {
        let pct = p.usage_percent.unwrap_or(0.0).round().clamp(0.0, 100.0) as i32;
        UsageWindow {
            label: "Weekly credits".to_string(),
            used: 0,
            total: None,
            percent: Some(pct),
            reset_at: p.end,
            breakdown: Vec::new(),
        }
    });

    let mut credits = Vec::new();
    // Machine slug the frontend `GrokUsagePanel` matches on (`GROK_ON_DEMAND_CAP`);
    // the human label comes from i18n, not this key. Omit a $0 cap — a zero
    // pay-as-you-go ceiling is "not enabled", not a chip worth showing.
    if let Some(cap) = on_demand_cap.filter(|c| *c > 0.0) {
        credits.push(CreditInfo {
            credit_type: "grok-on-demand-cap".to_string(),
            credit_amount: Some(format_usd_cents(cap)),
            minimum_credit_amount_for_usage: None,
        });
    }

    Ok(SubscriptionUsage {
        subscription_id: subscription_id.to_string(),
        fetched_at: Utc::now().timestamp(),
        plan_name: Some(DEFAULT_PLAN_NAME.to_string()),
        hourly: None,
        weekly,
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

/// Read a percentage value (plain float or numeric string, or a `{ val }`
/// wrapper). Used for `creditUsagePercent` (0..=100).
fn percent_value(value: &Value) -> Option<f64> {
    cent_value(value).filter(|n| n.is_finite())
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

#[cfg(test)]
#[path = "xai_tests.rs"]
mod tests;
