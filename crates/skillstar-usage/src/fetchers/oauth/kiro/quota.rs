//! CodeWhisperer `getUsageLimits` on `q.{region}.amazonaws.com`.
//!
//! Credits land on the monthly window and free trial on the weekly window.
//! A missing amount omits that window instead of inventing a zero quota.
//! `resetOn` / `resetAt` attach to a window that exists; they do not create one.

use chrono::{DateTime, Utc};
use serde_json::Value;

use super::http;
use super::{KiroState, resolve_region, runtime_endpoint, string_field};
use crate::oauth::token_refresh;
use crate::subscription::{Subscription, SubscriptionUsage, UsageWindow};
use crate::{UsageError, UsageResult};

pub(crate) const LABEL_CREDITS: &str = "Credits";
pub(crate) const LABEL_FREE_TRIAL: &str = "Free trial";

pub(super) async fn fetch_quota(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    let client = crate::fetchers::http_client()?;
    ensure_access(&client, subscription).await?;
    match load_usage(&client, subscription).await {
        Err(UsageError::AuthRequired) => {
            refresh_tokens(&client, subscription).await?;
            load_usage(&client, subscription).await
        }
        other => other,
    }
}

pub(super) async fn fetch_limits(
    client: &reqwest::Client,
    endpoint: &str,
    access_token: &str,
    profile_arn: &str,
) -> UsageResult<Value> {
    let mut url = reqwest::Url::parse(&format!(
        "{}/getUsageLimits",
        endpoint.trim_end_matches('/')
    ))
    .map_err(|err| UsageError::Other(format!("Kiro 用量地址无效: {err}")))?;
    url.query_pairs_mut()
        .append_pair("origin", "AI_EDITOR")
        .append_pair("profileArn", profile_arn)
        .append_pair("resourceType", "AGENTIC_REQUEST")
        .append_pair("isEmailRequired", "true");
    http::get_bearer(client, url.as_str(), access_token, "Kiro 用量").await
}

pub(super) fn usage_from_payload(subscription_id: &str, body: &Value) -> SubscriptionUsage {
    let root = resolve_usage_root(body);
    let breakdown = pick_breakdown(root);
    let credits = credit_amounts(root, breakdown);
    let trial = trial_amounts(root, breakdown);
    let reset = reset_timestamp(root, breakdown);
    let mut usage = SubscriptionUsage {
        subscription_id: subscription_id.to_string(),
        fetched_at: Utc::now().timestamp(),
        plan_name: plan_name(root, breakdown),
        ..SubscriptionUsage::default()
    };
    if let Some((used, total)) = credits {
        usage.monthly = Some(window(LABEL_CREDITS, used, total, reset));
    }
    if let Some((used, total)) = trial {
        let trial_reset =
            trial_expiry(breakdown).or(if usage.monthly.is_none() { reset } else { None });
        usage.weekly = Some(window(LABEL_FREE_TRIAL, used, total, trial_reset));
    }
    usage
}

async fn load_usage(
    client: &reqwest::Client,
    subscription: &mut Subscription,
) -> UsageResult<SubscriptionUsage> {
    let access = super::decrypt_optional(&subscription.access_token_encrypted)
        .ok_or(UsageError::AuthRequired)?;
    let state = KiroState::from_subscription(subscription);
    let profile_arn = state
        .profile_arn
        .clone()
        .ok_or_else(|| UsageError::Other("Kiro 用量缺少 profileArn".into()))?;
    let region = state
        .region
        .clone()
        .unwrap_or_else(|| resolve_region(None, Some(&profile_arn)));
    let endpoint = runtime_endpoint(&region);
    let body = fetch_limits(client, &endpoint, &access, &profile_arn).await?;
    apply_usage_identity(subscription, &body);
    Ok(usage_from_payload(&subscription.id, &body))
}

async fn ensure_access(
    client: &reqwest::Client,
    subscription: &mut Subscription,
) -> UsageResult<()> {
    let missing = super::decrypt_optional(&subscription.access_token_encrypted).is_none();
    if missing || token_refresh::needs_refresh(subscription.access_token_expires_at) {
        refresh_tokens(client, subscription).await?;
    }
    Ok(())
}

pub(super) async fn refresh_tokens(
    client: &reqwest::Client,
    subscription: &mut Subscription,
) -> UsageResult<()> {
    let refresh = super::decrypt_optional(&subscription.refresh_token_encrypted)
        .ok_or(UsageError::AuthRequired)?;
    let mut state = KiroState::from_subscription(subscription);
    let grant = if state.has_idc_client() {
        let region = state
            .region
            .clone()
            .unwrap_or_else(|| resolve_region(None, state.profile_arn.as_deref()));
        super::idc::refresh_idc_token(
            client,
            &region,
            &refresh,
            state.client_id.as_deref().unwrap_or(""),
            state.client_secret.as_deref().unwrap_or(""),
        )
        .await?
    } else {
        super::portal::refresh_portal_token(client, &refresh).await?
    };
    state.overlay_token(&grant.raw);
    if state.region.is_none() {
        state.region = Some(resolve_region(None, state.profile_arn.as_deref()));
    }
    apply_grant(subscription, &grant, &state);
    Ok(())
}

pub(super) fn apply_grant(subscription: &mut Subscription, grant: &TokenGrant, state: &KiroState) {
    if !grant.access_token.trim().is_empty() {
        subscription.access_token_encrypted = Some(crate::crypto::encrypt(&grant.access_token));
    }
    if let Some(refresh) = grant
        .refresh_token
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        subscription.refresh_token_encrypted = Some(crate::crypto::encrypt(refresh));
    }
    if let Some(seconds) = grant.expires_in.filter(|seconds| *seconds > 0) {
        subscription.access_token_expires_at = Some(Utc::now().timestamp() + seconds);
    } else if let Some(exp) = token_refresh::jwt_exp(&grant.access_token) {
        subscription.access_token_expires_at = Some(exp);
    }
    if let Some(json) = state.to_json() {
        subscription.provider_state_encrypted = Some(crate::crypto::encrypt(&json));
    }
    if let Some(region) = state.region.clone() {
        subscription.oauth_region = Some(region);
    }
}

fn apply_usage_identity(subscription: &mut Subscription, body: &Value) {
    let root = resolve_usage_root(body);
    let email = string_field(root, &["email"]).or_else(|| {
        root.and_then(|value| value.get("userInfo"))
            .and_then(|info| string_field(Some(info), &["email"]))
    });
    crate::fetchers::oauth::common::apply_email_title(subscription, email.as_deref(), &["Kiro"]);
    if subscription
        .oauth_account_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .is_none()
    {
        let user_id = string_field(root, &["userId", "user_id", "sub"]).or_else(|| {
            root.and_then(|value| value.get("userInfo"))
                .and_then(|info| string_field(Some(info), &["userId", "user_id", "sub"]))
        });
        if let Some(user_id) = user_id {
            subscription.oauth_account_id = Some(user_id);
        }
    }
    if let Some(arn) = string_field(root, &["profileArn", "profile_arn", "arn"]) {
        let mut state = KiroState::from_subscription(subscription);
        if state.profile_arn.is_none() {
            state.profile_arn = Some(arn);
            if let Some(json) = state.to_json() {
                subscription.provider_state_encrypted = Some(crate::crypto::encrypt(&json));
            }
        }
    }
}

fn resolve_usage_root(body: &Value) -> Option<&Value> {
    body.get("kiro.resourceNotifications.usageState")
        .or_else(|| body.get("usageState"))
        .or(Some(body))
}

fn pick_breakdown(root: Option<&Value>) -> Option<&Value> {
    let list = root?
        .get("usageBreakdownList")
        .or_else(|| root?.get("usageBreakdowns"))
        .and_then(Value::as_array)?;
    list.iter()
        .find(|item| {
            item.get("type")
                .and_then(Value::as_str)
                .is_some_and(|kind| kind.eq_ignore_ascii_case("credit"))
        })
        .or_else(|| list.first())
}

fn credit_amounts(root: Option<&Value>, breakdown: Option<&Value>) -> Option<(i64, Option<i64>)> {
    let total = first_number(
        root,
        &[
            &["estimatedUsage", "total"],
            &["estimatedUsage", "creditsTotal"],
            &["usageBreakdowns", "plan", "totalCredits"],
            &["usageBreakdowns", "covered", "total"],
            &["credits", "total"],
            &["totalCredits"],
        ],
    )
    .or_else(|| {
        first_number(
            breakdown,
            &[
                &["usageLimitWithPrecision"],
                &["usageLimit"],
                &["limit"],
                &["total"],
                &["totalCredits"],
            ],
        )
    });
    let used = first_number(
        root,
        &[
            &["estimatedUsage", "used"],
            &["estimatedUsage", "creditsUsed"],
            &["usageBreakdowns", "plan", "usedCredits"],
            &["usageBreakdowns", "covered", "used"],
            &["credits", "used"],
            &["usedCredits"],
        ],
    )
    .or_else(|| {
        first_number(
            breakdown,
            &[
                &["currentUsageWithPrecision"],
                &["currentUsage"],
                &["used"],
                &["usedCredits"],
            ],
        )
    });
    pair(used, total)
}

fn trial_amounts(root: Option<&Value>, breakdown: Option<&Value>) -> Option<(i64, Option<i64>)> {
    let trial = breakdown.and_then(|item| {
        item.get("freeTrialInfo")
            .or_else(|| item.get("freeTrialUsage"))
    });
    let total =
        first_number(root, &[&["bonusCredits", "total"], &["bonus", "total"]]).or_else(|| {
            first_number(
                trial,
                &[
                    &["usageLimitWithPrecision"],
                    &["usageLimit"],
                    &["limit"],
                    &["total"],
                    &["totalCredits"],
                ],
            )
        });
    let used = first_number(root, &[&["bonusCredits", "used"], &["bonus", "used"]]).or_else(|| {
        first_number(
            trial,
            &[
                &["currentUsageWithPrecision"],
                &["currentUsage"],
                &["used"],
                &["usedCredits"],
            ],
        )
    });
    pair(used, total)
}

fn pair(used: Option<f64>, total: Option<f64>) -> Option<(i64, Option<i64>)> {
    if used.is_none() && total.is_none() {
        return None;
    }
    Some((as_units(used.unwrap_or(0.0)), total.map(as_units)))
}

fn plan_name(root: Option<&Value>, breakdown: Option<&Value>) -> Option<String> {
    string_field(root, &["planName", "currentPlanName"])
        .or_else(|| nested_string(root, &["subscriptionInfo", "subscriptionTitle"]))
        .or_else(|| nested_string(root, &["subscriptionInfo", "subscriptionName"]))
        .or_else(|| string_field(breakdown, &["displayName", "displayNamePlural"]))
}

fn reset_timestamp(root: Option<&Value>, breakdown: Option<&Value>) -> Option<i64> {
    timestamp_at(root, &["resetAt"])
        .or_else(|| timestamp_at(root, &["resetTime"]))
        .or_else(|| timestamp_at(root, &["resetOn"]))
        .or_else(|| timestamp_at(root, &["nextDateReset"]))
        .or_else(|| timestamp_at(breakdown, &["resetDate"]))
        .or_else(|| timestamp_at(breakdown, &["resetAt"]))
        .or_else(|| timestamp_at(breakdown, &["resetOn"]))
}

fn trial_expiry(breakdown: Option<&Value>) -> Option<i64> {
    let trial = breakdown?
        .get("freeTrialInfo")
        .or_else(|| breakdown?.get("freeTrialUsage"));
    timestamp_at(trial, &["expiryDate"]).or_else(|| timestamp_at(trial, &["freeTrialExpiry"]))
}

fn window(label: &str, used: i64, total: Option<i64>, reset_at: Option<i64>) -> UsageWindow {
    UsageWindow {
        label: label.to_string(),
        used,
        total,
        percent: total.and_then(|total| percent(used, total)),
        reset_at,
        breakdown: Vec::new(),
    }
}

fn percent(used: i64, total: i64) -> Option<i32> {
    if total <= 0 {
        return None;
    }
    Some(((used.clamp(0, total) as i128) * 100 / total as i128) as i32)
}

fn as_units(value: f64) -> i64 {
    if !value.is_finite() || value < 0.0 {
        0
    } else {
        value.round() as i64
    }
}

fn first_number(root: Option<&Value>, paths: &[&[&str]]) -> Option<f64> {
    let root = root?;
    paths.iter().find_map(|path| number_at(root, path))
}

fn number_at(root: &Value, path: &[&str]) -> Option<f64> {
    let mut current = root;
    for key in path {
        current = current.as_object()?.get(*key)?;
    }
    current
        .as_f64()
        .filter(|value| value.is_finite())
        .or_else(|| current.as_i64().map(|value| value as f64))
        .or_else(|| current.as_u64().map(|value| value as f64))
        .or_else(|| {
            current
                .as_str()
                .and_then(|text| text.trim().parse::<f64>().ok())
                .filter(|value| value.is_finite())
        })
}

fn nested_string(root: Option<&Value>, path: &[&str]) -> Option<String> {
    let mut current = root?;
    for key in path {
        current = current.as_object()?.get(*key)?;
    }
    current
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn timestamp_at(root: Option<&Value>, path: &[&str]) -> Option<i64> {
    let mut current = root?;
    for key in path {
        current = current.as_object()?.get(*key)?;
    }
    parse_timestamp(current)
}

fn parse_timestamp(value: &Value) -> Option<i64> {
    if let Some(seconds) = value.as_i64() {
        return normalize_timestamp(seconds);
    }
    if let Some(seconds) = value.as_u64() {
        return normalize_timestamp(seconds as i64);
    }
    if let Some(seconds) = value.as_f64().filter(|n| n.is_finite()) {
        return normalize_timestamp(seconds.round() as i64);
    }
    let text = value.as_str()?.trim();
    if text.is_empty() {
        return None;
    }
    if let Ok(number) = text.parse::<i64>() {
        return normalize_timestamp(number);
    }
    if let Ok(parsed) = DateTime::parse_from_rfc3339(text) {
        return Some(parsed.timestamp());
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

use super::http::TokenGrant;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn kiro_quota_maps_credits_and_free_trial_and_omits_gaps() {
        let body = json!({
            "subscriptionInfo": {"subscriptionTitle": "Kiro Pro"},
            "usageBreakdownList": [{
                "type": "CREDIT",
                "displayName": "Credits",
                "currentUsageWithPrecision": 12,
                "usageLimitWithPrecision": 50,
                "resetOn": "2026-10-01T00:00:00Z",
                "freeTrialInfo": {
                    "currentUsageWithPrecision": 3,
                    "usageLimitWithPrecision": 10
                }
            }]
        });
        let usage = usage_from_payload("sub", &body);
        assert_eq!(usage.plan_name.as_deref(), Some("Kiro Pro"));
        let monthly = usage.monthly.expect("credits");
        assert_eq!(monthly.label, LABEL_CREDITS);
        assert_eq!(monthly.used, 12);
        assert_eq!(monthly.total, Some(50));
        assert_eq!(monthly.percent, Some(24));
        assert!(monthly.reset_at.is_some());
        let weekly = usage.weekly.expect("trial");
        assert_eq!(weekly.label, LABEL_FREE_TRIAL);
        assert_eq!(weekly.used, 3);
        assert_eq!(weekly.total, Some(10));
        assert_eq!(weekly.percent, Some(30));

        let used_only = usage_from_payload("sub", &json!({"estimatedUsage": {"used": 4}}));
        assert_eq!(
            used_only.monthly.as_ref().map(|window| window.used),
            Some(4)
        );
        assert_eq!(
            used_only.monthly.as_ref().and_then(|window| window.total),
            None
        );
        assert!(used_only.weekly.is_none());

        let reset_only = usage_from_payload("sub", &json!({"resetOn": "2026-10-01T00:00:00Z"}));
        assert!(reset_only.monthly.is_none());
        assert!(reset_only.weekly.is_none());

        let trial_only = usage_from_payload(
            "sub",
            &json!({"usageBreakdownList": [{"freeTrialInfo": {"usageLimit": 8}}]}),
        );
        assert!(trial_only.monthly.is_none());
        assert_eq!(
            trial_only.weekly.as_ref().and_then(|window| window.total),
            Some(8)
        );
    }

    #[tokio::test]
    async fn kiro_quota_http_403_is_not_auth_and_401_is() {
        let forbidden = super::super::http::scripted::ScriptedHttp::start(vec![(
            403,
            r#"{"message":"no"}"#.into(),
        )]);
        let client = reqwest::Client::new();
        let err = fetch_limits(
            &client,
            &forbidden.base,
            "access-token",
            "arn:aws:codewhisperer:us-east-1:1:profile/p",
        )
        .await
        .expect_err("403");
        assert!(matches!(err, UsageError::Fetcher(_)), "{err:?}");
        assert!(
            forbidden.seen()[0].contains("profileArn="),
            "{:?}",
            forbidden.seen()
        );

        let unauthorized =
            super::super::http::scripted::ScriptedHttp::start(vec![(401, "nope".into())]);
        let err = fetch_limits(&client, &unauthorized.base, "access-token", "arn")
            .await
            .expect_err("401");
        assert!(matches!(err, UsageError::AuthRequired), "{err:?}");
    }
}
