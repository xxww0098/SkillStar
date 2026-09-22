//! Quota from `openapi.qoder.sh`: userinfo, user status, plan, and credit usage.
//!
//! A credits window exists only when `used` is present. A missing amount omits
//! the window. A present zero stays zero. `total` is filled from `used +
//! remaining` only when both of those were actually returned and `total` was
//! not. Plan text is the provider's raw string.

use chrono::{DateTime, Utc};
use serde_json::Value;

use super::http;
use super::{OPENAPI_BASE, QoderMachine, object_string, pick_string};
use crate::subscription::{Subscription, SubscriptionUsage, UsageWindow};
use crate::{UsageError, UsageResult};

pub(crate) const LABEL_CREDITS: &str = "Credits";

const USER_INFO: &str = "/api/v1/userinfo";
const USER_STATUS: &str = "/api/v3/user/status";
const USER_PLAN: &str = "/api/v2/user/plan";
const CREDIT_USAGE: &str = "/api/v2/quota/usage";

#[derive(Debug, Clone, Default)]
pub(super) struct Profile {
    pub user: Option<Value>,
    pub status: Option<Value>,
    pub plan: Option<Value>,
    pub usage: Option<Value>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct Identity {
    pub email: Option<String>,
    pub user_id: Option<String>,
    pub name: Option<String>,
}

pub(super) async fn fetch_quota(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    let client = crate::fetchers::http_client()?;
    let access = super::decrypt_optional(&subscription.access_token_encrypted)
        .ok_or(UsageError::AuthRequired)?;
    let machine = QoderMachine::from_subscription(subscription);
    let profile = read_profile(&client, OPENAPI_BASE, &access, &machine, true).await?;
    apply_identity(subscription, &profile);
    Ok(usage_from_profile(&subscription.id, &profile))
}

/// `strict` is the refresh path: non-404 failures propagate. Login passes
/// `false` so a plan or usage blip still keeps the token. 401 and
/// `LoginExpire` always fail. 404 on plan or usage omits that body.
pub(super) async fn read_profile(
    client: &reqwest::Client,
    openapi_base: &str,
    access_token: &str,
    machine: &QoderMachine,
    strict: bool,
) -> UsageResult<Profile> {
    let user = match get(
        client,
        openapi_base,
        USER_INFO,
        access_token,
        machine,
        "Qoder userinfo",
    )
    .await
    {
        Ok(value) => Some(value),
        Err(UsageError::AuthRequired) => return Err(UsageError::AuthRequired),
        Err(err) if strict => return Err(err),
        Err(_) => None,
    };
    let status = match get(
        client,
        openapi_base,
        USER_STATUS,
        access_token,
        machine,
        "Qoder status",
    )
    .await
    {
        Ok(value) => {
            if let Some(err) = status_failure(&value) {
                return Err(err);
            }
            Some(value)
        }
        Err(UsageError::AuthRequired) => return Err(UsageError::AuthRequired),
        Err(err) if strict => return Err(err),
        Err(_) => None,
    };
    let plan = optional(
        client,
        openapi_base,
        USER_PLAN,
        access_token,
        machine,
        "Qoder plan",
        strict,
    )
    .await?;
    let usage = optional(
        client,
        openapi_base,
        CREDIT_USAGE,
        access_token,
        machine,
        "Qoder 用量",
        strict,
    )
    .await?;
    Ok(Profile {
        user,
        status,
        plan,
        usage,
    })
}

pub(super) fn identity_of(profile: &Profile) -> Identity {
    let roots = [
        profile.user.as_ref(),
        profile.status.as_ref(),
        profile.plan.as_ref(),
    ];
    Identity {
        email: roots.iter().find_map(|root| root.and_then(email_of)),
        user_id: roots.iter().find_map(|root| root.and_then(user_id_of)),
        name: roots.iter().find_map(|root| {
            root.and_then(|value| {
                pick_string(value, &["name", "nickname", "displayName", "display_name"])
            })
        }),
    }
}

pub(super) fn usage_from_profile(subscription_id: &str, profile: &Profile) -> SubscriptionUsage {
    let mut usage = SubscriptionUsage {
        subscription_id: subscription_id.to_string(),
        fetched_at: Utc::now().timestamp(),
        plan_name: plan_name(profile),
        ..SubscriptionUsage::default()
    };
    if let Some(window) = credits_window(profile) {
        usage.monthly = Some(window);
    }
    usage
}

pub(super) fn status_failure(status: &Value) -> Option<UsageError> {
    let whitelist = pick_string(status, &["whitelistStatus", "whitelist_status"])?;
    match whitelist.as_str() {
        "LoginExpire" => Some(UsageError::AuthRequired),
        "NoIpPermission" => Some(UsageError::Fetcher(
            "企业设置了 IP 白名单，当前 IP 无法登录".into(),
        )),
        "AppDisable" => Some(UsageError::Fetcher("Qoder 应用已被停用，无法登录".into())),
        "NotAllow" | "NOT_ALLOW" => Some(UsageError::Fetcher("当前账号暂无 Qoder 使用权限".into())),
        _ => None,
    }
}

fn apply_identity(subscription: &mut Subscription, profile: &Profile) {
    let identity = identity_of(profile);
    crate::fetchers::oauth::common::apply_email_title(
        subscription,
        identity.email.as_deref(),
        &["Qoder"],
    );
    if subscription
        .oauth_account_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .is_none()
        && let Some(user_id) = identity.user_id
    {
        subscription.oauth_account_id = Some(user_id);
    }
}

fn credits_window(profile: &Profile) -> Option<UsageWindow> {
    let used = number_in(profile.usage.as_ref(), USED_KEYS)
        .or_else(|| number_in(profile.plan.as_ref(), USED_KEYS))?;
    let total = number_in(profile.usage.as_ref(), TOTAL_KEYS)
        .or_else(|| number_in(profile.plan.as_ref(), TOTAL_KEYS))
        .or_else(|| {
            let remaining = number_in(profile.usage.as_ref(), REMAIN_KEYS)
                .or_else(|| number_in(profile.plan.as_ref(), REMAIN_KEYS))?;
            Some(used + remaining)
        });
    let reset = timestamp_in(profile.usage.as_ref(), RESET_KEYS)
        .or_else(|| timestamp_in(profile.plan.as_ref(), RESET_KEYS));
    Some(window(
        LABEL_CREDITS,
        as_units(used),
        total.map(as_units),
        reset,
    ))
}

fn plan_name(profile: &Profile) -> Option<String> {
    const KEYS: &[&str] = &[
        "plan",
        "planType",
        "plan_type",
        "planName",
        "plan_name",
        "tierName",
        "packageName",
        "userTag",
    ];
    profile
        .plan
        .as_ref()
        .and_then(|value| pick_string(value, KEYS))
        .or_else(|| {
            profile
                .usage
                .as_ref()
                .and_then(|value| pick_string(value, KEYS))
        })
        .or_else(|| {
            profile
                .user
                .as_ref()
                .and_then(|value| pick_string(value, KEYS))
        })
        .or_else(|| {
            profile
                .status
                .as_ref()
                .and_then(|value| pick_string(value, KEYS))
        })
}

const USED_KEYS: &[&str] = &["used", "usedCredits", "creditsUsed", "consumed", "consume"];
const TOTAL_KEYS: &[&str] = &["total", "creditsTotal", "totalCredits", "quota", "limit"];
const REMAIN_KEYS: &[&str] = &["remaining", "remain", "left", "available"];
const RESET_KEYS: &[&str] = &[
    "resetAt",
    "resetOn",
    "resetTime",
    "nextResetAt",
    "nextResetTime",
];

fn number_in(root: Option<&Value>, keys: &[&str]) -> Option<f64> {
    let root = root?;
    for key in keys {
        if let Some(number) = find_number(root, key) {
            return Some(number);
        }
    }
    None
}

fn find_number(root: &Value, key: &str) -> Option<f64> {
    let mut found = None;
    walk_numbers(root, key, &mut found);
    found
}

fn walk_numbers(value: &Value, key: &str, found: &mut Option<f64>) {
    if found.is_some() {
        return;
    }
    let Some(map) = value.as_object() else {
        if let Some(items) = value.as_array() {
            for item in items {
                walk_numbers(item, key, found);
            }
        }
        return;
    };
    for (name, item) in map {
        if name.eq_ignore_ascii_case(key)
            && !banned_key(name)
            && let Some(number) = as_number(item)
        {
            *found = Some(number);
            return;
        }
    }
    for (name, item) in map {
        if banned_key(name) {
            continue;
        }
        walk_numbers(item, key, found);
        if found.is_some() {
            return;
        }
    }
}

fn banned_key(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    ["percent", "rate", "ratio"]
        .iter()
        .any(|ban| lower.contains(ban))
}

fn as_number(value: &Value) -> Option<f64> {
    let number = match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse::<f64>().ok(),
        _ => None,
    }?;
    number
        .is_finite()
        .then_some(number)
        .filter(|number| *number >= 0.0)
}

fn timestamp_in(root: Option<&Value>, keys: &[&str]) -> Option<i64> {
    let text = pick_string(root?, keys)?;
    if let Ok(number) = text.parse::<i64>() {
        return http::parse_expiry(&number.to_string());
    }
    DateTime::parse_from_rfc3339(&text)
        .ok()
        .map(|value| value.timestamp())
}

fn as_units(value: f64) -> i64 {
    value.round().clamp(0.0, i64::MAX as f64) as i64
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

fn email_of(value: &Value) -> Option<String> {
    pick_string(value, &["email", "mail"])
        .filter(|email| crate::fetchers::oauth::common::looks_like_email(email))
}

fn user_id_of(value: &Value) -> Option<String> {
    pick_string(value, &["userId", "user_id", "uid"]).or_else(|| {
        // Top-level `id` only. A nested id is often an org or plan id.
        object_string(value, &["id"]).filter(|id| !id.contains('@'))
    })
}

async fn get(
    client: &reqwest::Client,
    base: &str,
    path: &str,
    token: &str,
    machine: &QoderMachine,
    label: &str,
) -> UsageResult<Value> {
    http::get_json(
        client,
        &http::endpoint(base, path),
        Some(token),
        machine,
        label,
    )
    .await
}

async fn optional(
    client: &reqwest::Client,
    base: &str,
    path: &str,
    token: &str,
    machine: &QoderMachine,
    label: &str,
    strict: bool,
) -> UsageResult<Option<Value>> {
    match get(client, base, path, token, machine, label).await {
        Ok(value) => Ok(Some(value)),
        Err(err) if is_not_found(&err) => Ok(None),
        Err(err) if strict => Err(err),
        Err(_) => Ok(None),
    }
}

fn is_not_found(err: &UsageError) -> bool {
    matches!(err, UsageError::Fetcher(message) if message.contains("状态码 404"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn client() -> reqwest::Client {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .expect("client")
    }

    fn machine() -> QoderMachine {
        QoderMachine {
            machine_token: Some("machine-token".into()),
            machine_id: Some("machine-id".into()),
            machine_type: Some("desktop".into()),
            machine_code: Some("code".into()),
            hostname: Some("host".into()),
            os: Some("aarch64_darwin".into()),
            cosy_version: Some("1.27.1".into()),
        }
    }

    #[test]
    fn qoder_quota_omits_missing_fields_and_keeps_a_real_zero() {
        let full = usage_from_profile(
            "sub",
            &Profile {
                user: Some(
                    serde_json::json!({"id": "user-1", "email": "ada@qoder.dev", "name": "Ada"}),
                ),
                plan: Some(serde_json::json!({"plan": "Pro Plus"})),
                usage: Some(serde_json::json!({
                    "used": 12,
                    "total": 40,
                    "remaining": 28,
                    "resetAt": "2026-10-01T00:00:00Z"
                })),
                status: None,
            },
        );
        assert_eq!(full.plan_name.as_deref(), Some("Pro Plus"));
        let monthly = full.monthly.expect("credits");
        assert_eq!(monthly.label, LABEL_CREDITS);
        assert_eq!(monthly.used, 12);
        assert_eq!(monthly.total, Some(40));
        assert_eq!(monthly.percent, Some(30));
        assert!(monthly.reset_at.is_some());

        let zero = usage_from_profile(
            "sub",
            &Profile {
                usage: Some(serde_json::json!({"used": 0, "total": 10})),
                ..Profile::default()
            },
        );
        let monthly = zero.monthly.expect("present zero");
        assert_eq!(monthly.used, 0);
        assert_eq!(monthly.total, Some(10));

        let used_only = usage_from_profile(
            "sub",
            &Profile {
                usage: Some(serde_json::json!({"used": 4})),
                ..Profile::default()
            },
        );
        assert_eq!(
            used_only.monthly.as_ref().map(|window| window.used),
            Some(4)
        );
        assert_eq!(
            used_only.monthly.as_ref().and_then(|window| window.total),
            None
        );

        let derived = usage_from_profile(
            "sub",
            &Profile {
                usage: Some(serde_json::json!({"used": 2, "remaining": 3})),
                ..Profile::default()
            },
        );
        assert_eq!(
            derived.monthly.as_ref().and_then(|window| window.total),
            Some(5)
        );

        for body in [
            serde_json::json!({"remaining": 4, "total": 10}),
            serde_json::json!({"usagePercent": 50, "total": 10}),
            serde_json::json!({"resetAt": "2026-10-01T00:00:00Z"}),
            serde_json::json!({}),
        ] {
            let usage = usage_from_profile(
                "sub",
                &Profile {
                    usage: Some(body),
                    ..Profile::default()
                },
            );
            assert!(usage.monthly.is_none(), "{usage:?}");
            assert!(usage.weekly.is_none());
            assert!(usage.hourly.is_none());
        }
    }

    #[tokio::test]
    async fn qoder_quota_http_sends_bearer_and_cosy_headers() {
        let server = super::http::scripted::ScriptedHttp::start(vec![
            (
                200,
                r#"{"id":"user-9","email":"ada@qoder.dev","name":"Ada"}"#.into(),
            ),
            (200, r#"{"id":"user-9","whitelistStatus":"PASS"}"#.into()),
            (404, "missing plan".into()),
            (
                200,
                r#"{"code":0,"data":{"used":3,"total":9,"plan":"Team"}}"#.into(),
            ),
        ]);
        let profile = read_profile(&client(), &server.base, "access-token", &machine(), true)
            .await
            .expect("profile");
        let usage = usage_from_profile("sub", &profile);
        assert_eq!(usage.plan_name.as_deref(), Some("Team"));
        assert_eq!(usage.monthly.as_ref().map(|window| window.used), Some(3));
        assert_eq!(
            usage.monthly.as_ref().and_then(|window| window.total),
            Some(9)
        );
        let identity = identity_of(&profile);
        assert_eq!(identity.user_id.as_deref(), Some("user-9"));
        assert_eq!(identity.email.as_deref(), Some("ada@qoder.dev"));

        let seen = server.seen();
        assert_eq!(seen.len(), 4);
        assert!(
            seen[0].path.contains("/api/v1/userinfo"),
            "{}",
            seen[0].path
        );
        assert!(
            seen[1].path.contains("/api/v3/user/status"),
            "{}",
            seen[1].path
        );
        assert!(
            seen[2].path.contains("/api/v2/user/plan"),
            "{}",
            seen[2].path
        );
        assert!(
            seen[3].path.contains("/api/v2/quota/usage"),
            "{}",
            seen[3].path
        );
        for request in &seen {
            assert_eq!(
                super::http::scripted::header(request, "authorization"),
                Some("Bearer access-token")
            );
            assert_eq!(
                super::http::scripted::header(request, "Cosy-MachineToken"),
                Some("machine-token")
            );
            assert_eq!(
                super::http::scripted::header(request, "Cosy-MachineId"),
                Some("machine-id")
            );
            assert_eq!(
                super::http::scripted::header(request, "Cosy-ClientType"),
                Some("0")
            );
        }
    }

    #[tokio::test]
    async fn qoder_quota_classifies_auth_forbidden_and_business_code() {
        let unauthorized = super::http::scripted::ScriptedHttp::start(vec![(401, "nope".into())]);
        let auth = read_profile(&client(), &unauthorized.base, "t", &machine(), true)
            .await
            .expect_err("401");
        assert!(matches!(auth, UsageError::AuthRequired), "{auth:?}");

        let forbidden = super::http::scripted::ScriptedHttp::start(vec![
            (200, r#"{"id":"u"}"#.into()),
            (200, r#"{"id":"u"}"#.into()),
            (200, "{}".into()),
            (403, r#"{"message":"no"}"#.into()),
        ]);
        let err = read_profile(&client(), &forbidden.base, "t", &machine(), true)
            .await
            .expect_err("403");
        assert!(matches!(err, UsageError::Fetcher(_)), "{err:?}");
        assert!(!err.is_transient());

        let business = super::http::scripted::ScriptedHttp::start(vec![
            (200, r#"{"id":"u"}"#.into()),
            (200, r#"{"id":"u"}"#.into()),
            (200, r#"{"code":1001,"message":"denied"}"#.into()),
        ]);
        let err = read_profile(&client(), &business.base, "t", &machine(), true)
            .await
            .expect_err("code");
        assert!(matches!(err, UsageError::Fetcher(_)), "{err:?}");
        assert!(!err.is_transient());
        assert!(err.to_string().contains("1001"), "{err}");

        let expired = super::http::scripted::ScriptedHttp::start(vec![
            (200, r#"{"id":"u"}"#.into()),
            (200, r#"{"id":"u","whitelistStatus":"LoginExpire"}"#.into()),
        ]);
        let err = read_profile(&client(), &expired.base, "t", &machine(), true)
            .await
            .expect_err("expire");
        assert!(matches!(err, UsageError::AuthRequired), "{err:?}");

        let limited = super::http::scripted::ScriptedHttp::start(vec![(429, "slow".into())]);
        let err = read_profile(&client(), &limited.base, "t", &machine(), true)
            .await
            .expect_err("429");
        assert!(matches!(err, UsageError::Transient(_)), "{err:?}");
    }
}
