//! Grok (xAI) OAuth + billing fetcher.
//!
//! Interactive login follows the Grok CLI: post a device-code grant to
//! `https://auth.x.ai/oauth2/device/code`, open the verification page, and
//! poll `https://auth.x.ai/oauth2/token`. The loopback authorize URL on
//! `127.0.0.1:56121` is only the fallback when that device endpoint is
//! missing (HTTP 404). Billing is
//! `GET https://cli-chat-proxy.grok.com/v1/billing`.
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

use std::sync::LazyLock;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde_json::Value;

use super::common::SubscriptionBuilder;
use crate::crypto;
use crate::local_import::upsert_oauth_subscription;
use crate::oauth::token_endpoint::{self, TokenResponse};
use crate::oauth::token_refresh;
use crate::oauth_clients;
use crate::storage;
use crate::subscription::{
    CreditInfo, GROK_RESET_CARD, GROK_RESET_CREDITS, Subscription, SubscriptionUsage, UsageUnit,
    UsageWindow,
};
use crate::{UsageError, UsageResult};

#[path = "xai_billing.rs"]
mod billing;
use billing::plan_from_access_token;
#[path = "xai_device.rs"]
mod device;
#[path = "reset.rs"]
mod reset;
#[cfg(test)]
pub(super) use reset::{
    GrokResetToken, decode_grpc_web_frames, decode_remaining_resets_response,
    encode_grpc_web_frame, encode_redeem_reset_request, encode_varint, select_reset_token,
};
use reset::{live_reset_tokens, redeem_available_reset};

pub(super) const TOKEN_URL: &str = "https://auth.x.ai/oauth2/token";
static CLIENT_ID: LazyLock<String> = LazyLock::new(|| {
    oauth_clients::client_id!(
        "xai",
        "SKILLSTAR_XAI_CLIENT_ID",
        "b1a00492-073a-47ea-816f-4c329264a828"
    )
});

/// The resolved Grok OAuth `client_id` (honours env / file overrides). The CLI
/// account switch (`crate::usage_switch`) keys `~/.grok/auth.json` by
/// `https://auth.x.ai::<this id>`, so it must read the same resolved value the
/// fetcher uses rather than a separate hard-coded copy.
pub fn client_id() -> &'static str {
    CLIENT_ID.as_str()
}

/// The `auth.json` key the Grok CLI files a login under: the OIDC issuer plus
/// the resolved client id. The switch engine (`crate::usage_switch`) reads and
/// writes the same key, so both sides must build it here rather than each
/// keeping a formatting copy.
pub(crate) fn scope_key() -> String {
    format!("https://auth.x.ai::{}", client_id())
}
const SCOPES: &str = concat!(
    "openid profile email offline_access ",
    "grok-cli:access conversations:read conversations:write api:access ",
    "workspaces:read workspaces:write"
);
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
    match device::begin_device_flow(CLIENT_ID.as_str(), SCOPES).await? {
        device::DeviceBegin::Ready(session) => {
            Ok(device::start_device_login(session, target_subscription_id))
        }
        device::DeviceBegin::Unavailable => {
            device::start_loopback_login(target_subscription_id).await
        }
    }
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
            plan_name: plan_from_access_token(&access_token),
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

/// Consume one of the account's available Grok usage-reset credits, then
/// return the newly-reset billing snapshot.
///
/// Grok's web client first lists unexpired reset tokens and then redeems one
/// explicit token id. Keeping that two-step flow here is important: the reset
/// is a real provider-side mutation, not a local refresh or a billing-cache
/// invalidation.
pub(crate) async fn consume_reset(subscription: &mut Subscription) -> UsageResult<()> {
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
    match reset_result {
        Ok(()) => (),
        Err(UsageError::AuthRequired) => {
            refresh_xai_tokens(subscription).await?;
            maybe_upgrade_xai_title(subscription);
            let refreshed_token = crate::fetchers::decrypt_required(
                &subscription.access_token_encrypted,
                "access_token",
            )?;
            redeem_available_reset(&refreshed_token).await?;
        }
        Err(error) => return Err(error),
    };

    // The web client gives the billing projection two seconds to converge
    // after RedeemReset. Match that provider-side propagation window before
    // rebuilding the card snapshot.
    tokio::time::sleep(Duration::from_secs(2)).await;
    Ok(())
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
    // Credits and settings are independent of the legacy monthly view. A
    // transport failure on the default URL must not discard a weekly window
    // or the real plan name that those two calls already returned.
    let (billing, period, settings_plan, reset_tokens) = tokio::join!(
        billing::fetch_billing_payload(&client, access_token),
        billing::fetch_current_period(&client, access_token),
        billing::fetch_settings_plan(&client, access_token),
        live_reset_tokens(access_token),
    );
    let plan = settings_plan.or_else(|| plan_from_access_token(access_token));
    let payload = match billing {
        Ok(payload) => payload,
        Err(UsageError::AuthRequired) => return Err(UsageError::AuthRequired),
        Err(_) if period.as_ref().and_then(|item| item.weekly) == Some(true) => {
            // Weekly credits already describe the card. Keep going so one
            // flaky monthly call does not stick the previous error on it.
            Value::Null
        }
        Err(error) => return Err(error),
    };

    let mut usage = build_subscription_usage(subscription_id, &payload, period, plan)?;

    // Count stays on `grok-reset-credits` for the React footer. Each live
    // card's expiry is its own row so the reset stack can name the earliest
    // one. Token ids are not stored.
    if let Ok(tokens) = reset_tokens {
        usage.credits.push(CreditInfo {
            credit_type: GROK_RESET_CREDITS.to_string(),
            credit_amount: Some(tokens.len().to_string()),
            minimum_credit_amount_for_usage: None,
        });
        for token in tokens {
            usage.credits.push(CreditInfo {
                credit_type: GROK_RESET_CARD.to_string(),
                credit_amount: Some(token.validity_end.to_string()),
                minimum_credit_amount_for_usage: None,
            });
        }
    }

    // Card-shape stability: a weekly Grok plan must keep its weekly bar across
    // refreshes. The credits view is slow (~2.5s) and best-effort, so a single
    // transient miss would otherwise collapse the card from two bars to one
    // (the "2 kinds of cards" symptom). When this round produced no weekly bar,
    // reuse the subscription's last known weekly window instead of dropping it.
    if let Ok(Some(prev)) = storage::get_usage_snapshot(subscription_id) {
        if usage.weekly.is_none() {
            usage.weekly = prev.weekly;
        }
        if usage.monthly.is_none() {
            usage.monthly = prev.monthly;
        }
        if usage.plan_name.is_none() && prev.plan_name.as_deref().is_some_and(|name| name != "Grok")
        {
            usage.plan_name = prev.plan_name;
        }
    }
    if let Some(monthly) = usage.monthly.as_mut() {
        billing::fill_missing_percent(monthly);
    }

    Ok(usage)
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

/// Build Grok usage as a weekly progress window and a monthly credit window,
/// mirroring the Grok CLI `/usage`:
///
/// * **Monthly numeric quota** (`monthly`) — `used`/`monthlyLimit` (USD cents)
///   from the default billing view, resetting on the monthly billing cycle.
///   Always present when the account exposes numbers. The account card shows
///   dollars and remaining percent, without a progress bar.
/// * **Weekly progress bar** (`weekly`) — only for weekly-reset plans. Driven by
///   `creditUsagePercent` (a percent, no absolute number exists) from the
///   `?format=credits` view, resetting on `currentPeriod.end`. Percent-only, so
///   the UI renders it as a plain progress bar.
fn build_subscription_usage(
    subscription_id: &str,
    payload: &Value,
    period: Option<CurrentPeriod>,
    plan_name: Option<String>,
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
        // A present limit of 0 is a real allowance, not a missing percent.
        // The remaining caption needs `percent`. The account card omits the
        // monthly bar.
        let percent = total_cents.map(|total| {
            if total <= 0 {
                0
            } else {
                ((used_cents as f64 / total as f64) * 100.0).round() as i32
            }
        });
        Some(UsageWindow {
            label: "Monthly credits".to_string(),
            used: used_cents,
            total: total_cents,
            percent,
            reset_at: monthly_reset,
            breakdown: Vec::new(),
            unit: UsageUnit::UsdCents,
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

            unit: UsageUnit::Count,
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
        plan_name,
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

// ── Local import ─────────────────────────────────────────────────────────────

/// The sign-in fields SkillStar snapshots out of a `~/.grok/auth.json` entry.
#[derive(Debug, Default)]
struct ImportedGrokCredential {
    access_token: String,
    refresh_token: Option<String>,
    expires_at: Option<i64>,
    email: Option<String>,
    subject: Option<String>,
}

/// Import the sign-in the Grok CLI keeps in `~/.grok/auth.json` (honouring
/// `GROK_HOME`), the way magpie reads it: read-only, never touching the file —
/// the CLI rotates its tokens itself, and SkillStar snapshots the current ones
/// into a subscription row.
pub(crate) async fn import_from_local() -> UsageResult<Subscription> {
    let path = crate::tool_paths::switch_grok_auth_path();
    if !path.exists() {
        return Err(UsageError::Other(
            "Grok 未登录：未找到 ~/.grok/auth.json（可用 GROK_HOME 指定）".into(),
        ));
    }
    let content = std::fs::read_to_string(&path).map_err(UsageError::Io)?;
    let root: Value = serde_json::from_str(&content).map_err(UsageError::Serde)?;
    let credential = credential_from_root(&root).ok_or_else(|| {
        UsageError::Other("auth.json 缺少含 key 的登录条目，请先运行 grok login".into())
    })?;

    let display_name = credential
        .email
        .clone()
        .unwrap_or_else(|| DEFAULT_PLAN_NAME.to_string());
    upsert_oauth_subscription(
        "xai",
        display_name,
        credential.access_token,
        credential.refresh_token,
        credential.expires_at,
        "USD",
        credential.subject.or(credential.email),
    )
    .await
}

/// Pick the CLI's entry out of `auth.json`. The file is a map keyed by OIDC
/// scope URL; our own scope entry wins, then — as magpie reads it — any entry
/// with a non-empty `key`, in sorted key order for a deterministic choice.
fn credential_from_root(root: &Value) -> Option<ImportedGrokCredential> {
    let map = root.as_object()?;
    let entry = match map.get(&scope_key()) {
        Some(entry) if entry_has_key(entry) => entry,
        _ => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            keys.into_iter()
                .map(|key| &map[key])
                .find(|entry| entry_has_key(entry))?
        }
    };

    let access_token = entry
        .get("key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|token| !token.is_empty())?
        .to_string();

    // Same expiry policy as the switch engine's read: trust the stored
    // RFC3339 and the JWT claim, take the earlier one.
    let stored = entry.get("expires_at").and_then(parse_timestamp);
    let jwt = token_refresh::jwt_exp(&access_token);
    let expires_at = match (stored, jwt) {
        (Some(stored), Some(jwt)) => Some(stored.min(jwt)),
        (stored, jwt) => stored.or(jwt),
    };

    // Identity mirrors the switch engine's read: the mirrored claims first,
    // the access token's own JWT claims as fallback.
    let email = string_field(entry, "email")
        .or_else(|| token_refresh::jwt_string(&access_token, &["email"]));
    let subject = ["user_id", "principal_id", "sub"]
        .into_iter()
        .find_map(|field| string_field(entry, field))
        .or_else(|| token_refresh::jwt_string(&access_token, &["sub"]));

    Some(ImportedGrokCredential {
        access_token,
        refresh_token: entry
            .get("refresh_token")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .map(str::to_string),
        expires_at,
        email,
        subject,
    })
}

fn entry_has_key(entry: &Value) -> bool {
    entry
        .get("key")
        .and_then(Value::as_str)
        .is_some_and(|key| !key.trim().is_empty())
}

fn string_field(entry: &Value, field: &str) -> Option<String> {
    entry
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
#[path = "xai_tests.rs"]
mod tests;
