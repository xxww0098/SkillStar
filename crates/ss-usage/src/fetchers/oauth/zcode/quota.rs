//! Quota from the provider profile plus the zcode billing balance.
//!
//! z.ai uses `GET chat.z.ai/api/oauth/userinfo` with `Bearer` access token.
//! BigModel uses `GET open.bigmodel.cn/api/biz/customer/getCustomerInfo` with
//! the raw access token as `Authorization` (cockpit does not add `Bearer`).
//! Balances use `Bearer` zcode JWT. A window exists only when `used` is
//! present. Missing numbers are omitted, not stored as zero. The three Coding
//! Plan bars (5-hour, weekly, ZCode MCP) come from the provider quota-limit
//! endpoint and replace a collapsed Quota bar when that response has windows.

use serde_json::Value;

use super::http::{self, pick_string};
use super::{APP_VERSION, Endpoints, PLACEHOLDER_NAME, decrypt_optional, nonempty, provider_kind};
use crate::subscription::{Subscription, SubscriptionUsage, UsageUnit, UsageWindow};
use crate::{UsageError, UsageResult};

pub(super) async fn fetch_quota(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    let client = crate::fetchers::http_client()?;
    let endpoints = Endpoints::production();
    let mut usage = read_quota(&client, &endpoints, subscription).await?;
    usage.credits = super::reset::fetch(&client, &endpoints, subscription)
        .await
        .unwrap_or_default();
    Ok(usage)
}

pub(super) async fn read_quota(
    client: &reqwest::Client,
    endpoints: &Endpoints,
    subscription: &mut Subscription,
) -> UsageResult<SubscriptionUsage> {
    if provider_kind(subscription) == "api_key" {
        subscription.plan_tier = Some("API Key".into());
        return Ok(api_key_usage(&subscription.id));
    }
    let provider = super::normalize_provider(subscription.oauth_region.as_deref())?;
    let access =
        decrypt_optional(&subscription.access_token_encrypted).ok_or(UsageError::AuthRequired)?;
    let jwt = decrypt_optional(&subscription.id_token_encrypted);
    let profile = fetch_profile(client, endpoints, provider, &access).await?;
    apply_identity(subscription, &profile);
    let billing = match jwt.as_deref() {
        Some(jwt) => Some(fetch_billing(client, endpoints, jwt).await?),
        None => None,
    };
    let mut usage = usage_from_bodies(&subscription.id, Some(&profile), billing.as_ref())?;
    if let Some(limits) = fetch_plan_limits(client, endpoints, provider, &access)
        .await
        .and_then(|body| super::limits::parse(&body))
    {
        super::limits::apply(&mut usage, limits);
    }
    if let Some(body) = fetch_mcp_usage(client, endpoints, jwt.as_deref(), &access).await
        && let Some(window) = super::limits::mcp_window(&body)
    {
        usage.monthly = Some(window);
    }
    if let Some(plan) = usage.plan_name.clone() {
        subscription.plan_tier = Some(plan);
    }
    Ok(usage)
}

async fn fetch_plan_limits(
    client: &reqwest::Client,
    endpoints: &Endpoints,
    provider: &str,
    access_token: &str,
) -> Option<Value> {
    http::request_json(
        client,
        reqwest::Method::GET,
        endpoints.quota_limit_url(provider),
        &[
            ("accept", "application/json".into()),
            ("authorization", format!("Bearer {}", access_token.trim())),
        ],
        None,
        "GLM 配额",
    )
    .await
    .ok()
}

async fn fetch_mcp_usage(
    client: &reqwest::Client,
    endpoints: &Endpoints,
    jwt: Option<&str>,
    access_token: &str,
) -> Option<Value> {
    let jwt = jwt.map(str::trim).filter(|token| !token.is_empty())?;
    http::request_json(
        client,
        reqwest::Method::GET,
        &endpoints.mcp_usage,
        &[
            ("accept", "application/json".into()),
            ("authorization", format!("Bearer {jwt}")),
            (
                "x-bigmodel-authorization",
                format!("Bearer {}", access_token.trim()),
            ),
            ("bigmodel-target-type", "PERSONAL".into()),
        ],
        None,
        "GLM MCP 配额",
    )
    .await
    .ok()
}

pub(super) async fn fetch_profile(
    client: &reqwest::Client,
    endpoints: &Endpoints,
    provider: &str,
    access_token: &str,
) -> UsageResult<Value> {
    let (url, label, authorization) = if provider == "zai" {
        (
            endpoints.zai_userinfo.as_str(),
            "Z.ai 用户信息",
            format!("Bearer {}", access_token.trim()),
        )
    } else {
        (
            endpoints.bigmodel_customer.as_str(),
            "BigModel 用户信息",
            access_token.trim().to_string(),
        )
    };
    let body = http::request_json(
        client,
        reqwest::Method::GET,
        url,
        &[
            ("accept", "application/json".into()),
            ("authorization", authorization),
        ],
        None,
        label,
    )
    .await?;
    http::require_profile(&body, label)?;
    Ok(body)
}

async fn fetch_billing(
    client: &reqwest::Client,
    endpoints: &Endpoints,
    jwt: &str,
) -> UsageResult<Value> {
    let body = http::request_json(
        client,
        reqwest::Method::GET,
        &endpoints.billing_url(),
        &[
            ("accept", "application/json".into()),
            ("authorization", format!("Bearer {}", jwt.trim())),
            ("user-agent", format!("ZCode/{APP_VERSION}")),
            ("http-referer", "https://zcode.z.ai".into()),
            // billing/balance answers 400 code 3001 without it (device.rs).
            ("x-device-mid", super::device::device_mid()),
        ],
        None,
        "ZCode 配额",
    )
    .await?;
    http::require_billing(&body)?;
    Ok(body)
}

pub(super) fn apply_identity(subscription: &mut Subscription, profile: &Value) {
    let doc = document(profile);
    if subscription
        .oauth_account_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .is_none()
        && let Some(user_id) = user_id(doc)
    {
        subscription.oauth_account_id = Some(user_id);
    }
    let email = pick_string(doc, &[&["email"]]);
    crate::fetchers::oauth::common::apply_email_title(
        subscription,
        email.as_deref(),
        &[PLACEHOLDER_NAME],
    );
}

pub(super) fn usage_from_bodies(
    subscription_id: &str,
    profile: Option<&Value>,
    billing: Option<&Value>,
) -> UsageResult<SubscriptionUsage> {
    if let Some(billing) = billing {
        http::require_billing(billing)?;
    }
    let plan = billing
        .and_then(plan_from_billing)
        .or_else(|| profile.and_then(plan_from_profile));
    let monthly = billing.and_then(windows_from_billing);
    Ok(SubscriptionUsage {
        subscription_id: subscription_id.to_string(),
        fetched_at: chrono::Utc::now().timestamp(),
        plan_name: plan,
        monthly,
        ..SubscriptionUsage::default()
    })
}

fn api_key_usage(subscription_id: &str) -> SubscriptionUsage {
    SubscriptionUsage {
        subscription_id: subscription_id.to_string(),
        fetched_at: chrono::Utc::now().timestamp(),
        plan_name: Some("API Key".into()),
        ..SubscriptionUsage::default()
    }
}

fn plan_from_billing(body: &Value) -> Option<String> {
    let plans = document(body).get("plans")?.as_array()?;
    let plan = plans
        .iter()
        .find(|plan| plan.get("status").and_then(Value::as_str) == Some("active"))
        .or_else(|| plans.first())?;
    nonempty(pick_string(plan, &[&["name"], &["plan_id"], &["plan_name"]]).as_deref())
}

fn plan_from_profile(body: &Value) -> Option<String> {
    let doc = document(body);
    if let Some(text) = pick_string(
        doc,
        &[
            &["plan_type"],
            &["planName"],
            &["plan_name"],
            &["plan"],
            &["subscription", "name"],
        ],
    ) {
        return nonempty(Some(text.as_str()));
    }
    None
}

fn windows_from_billing(body: &Value) -> Option<UsageWindow> {
    let balances = document(body).get("balances")?.as_array()?.clone();
    let mut rows = Vec::new();
    for (index, item) in balances.iter().enumerate() {
        let Some(row) = balance_window(item, index) else {
            continue;
        };
        rows.push(row);
    }
    if rows.is_empty() {
        return None;
    }
    if rows.len() == 1 {
        return rows.pop();
    }
    let used = rows.iter().map(|row| row.used).sum();
    let total = rows
        .iter()
        .map(|row| row.total)
        .collect::<Option<Vec<_>>>()
        .map(|values| values.into_iter().sum());
    let reset_at = rows.iter().filter_map(|row| row.reset_at).min();
    let mut parent = window("Quota", used, total, reset_at);
    parent.breakdown = rows;
    Some(parent)
}

fn balance_window(item: &Value, index: usize) -> Option<UsageWindow> {
    let total = units(item, &["total_units", "totalUnits"]);
    let remaining = units(
        item,
        &[
            "remaining_units",
            "available_units",
            "remainingUnits",
            "availableUnits",
        ],
    );
    let used = units(item, &["used_units", "usedUnits"]).or_else(|| {
        let total = total?;
        let remaining = remaining?;
        (remaining <= total).then_some(total.saturating_sub(remaining))
    })?;
    let label = pick_string(
        item,
        &[
            &["show_name"],
            &["showName"],
            &["model"],
            &["model_name"],
            &["modelName"],
            &["model_id"],
            &["name"],
            &["product"],
            &["sku"],
            &["title"],
        ],
    )
    .unwrap_or_else(|| {
        if index == 0 {
            "Quota".into()
        } else {
            format!("Quota {}", index + 1)
        }
    });
    let reset_at = timestamp(
        item,
        &["period_end", "expires_at", "periodEnd", "expiresAt"],
    );
    Some(window(&label, used, total, reset_at))
}

fn window(label: &str, used: i64, total: Option<i64>, reset_at: Option<i64>) -> UsageWindow {
    UsageWindow {
        label: label.to_string(),
        used,
        total,
        percent: total.and_then(|total| (total > 0).then(|| percent(used, total))),
        reset_at,
        breakdown: Vec::new(),
        unit: UsageUnit::Count,
    }
}

fn percent(used: i64, total: i64) -> i32 {
    let value = ((used as f64 / total as f64) * 100.0).round() as i32;
    value.clamp(0, 100)
}

fn document(body: &Value) -> &Value {
    body.get("data")
        .filter(|value| value.is_object())
        .unwrap_or(body)
}

fn user_id(doc: &Value) -> Option<String> {
    pick_string(doc, &[&["user_id"], &["id"], &["customerNumber"], &["sub"]])
}

fn units(item: &Value, keys: &[&str]) -> Option<i64> {
    for key in keys {
        if let Some(raw) = item.get(*key)
            && let Some(number) = json_units(raw)
        {
            return Some(number);
        }
    }
    None
}

fn json_units(value: &Value) -> Option<i64> {
    let number = match value {
        Value::Number(number) => number.as_i64().or_else(|| float_units(number.as_f64()?)),
        Value::String(text) => float_units(text.trim().parse::<f64>().ok()?),
        _ => None,
    }?;
    (number >= 0).then_some(number)
}

fn float_units(value: f64) -> Option<i64> {
    if !value.is_finite() || value < 0.0 || value > i64::MAX as f64 {
        return None;
    }
    Some(value.trunc() as i64)
}

fn timestamp(item: &Value, keys: &[&str]) -> Option<i64> {
    let raw = units_any(item, keys)?;
    let seconds = if raw > 10_000_000_000 {
        raw / 1000
    } else {
        raw
    };
    (seconds > 0).then_some(seconds)
}

fn units_any(item: &Value, keys: &[&str]) -> Option<i64> {
    for key in keys {
        if let Some(raw) = item.get(*key)
            && let Some(number) = json_units(raw)
        {
            return Some(number);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetchers::oauth::common::SubscriptionBuilder;
    use serde_json::json;

    fn row(region: &str, access: &str, jwt: Option<&str>) -> Subscription {
        let mut builder = SubscriptionBuilder::new("zcode", "ZCode", "USD", access, None)
            .oauth_region(Some(region.into()))
            .provider_state(super::super::provider_state_json("oauth"));
        if let Some(jwt) = jwt {
            builder = builder.id_token(Some(jwt.into()));
        }
        builder.build()
    }

    #[test]
    fn missing_balance_fields_omit_windows_and_models_become_breakdown() {
        let plan_only = usage_from_bodies(
            "sub",
            None,
            Some(&json!({"code": 0, "data": {"plans": [{"status": "active", "name": "Pro"}]}})),
        )
        .unwrap();
        assert_eq!(plan_only.plan_name.as_deref(), Some("Pro"));
        assert!(plan_only.monthly.is_none());
        assert!(plan_only.weekly.is_none());

        let usage = usage_from_bodies(
            "sub",
            Some(&json!({"plan_type": "Fallback"})),
            Some(&json!({
                "code": 0,
                "data": {
                    "plans": [{"status": "expired", "name": "Old"}, {"status": "active", "name": "Pro"}],
                    "balances": [
                        {"model": "glm-4.6", "used_units": 3, "total_units": 10, "period_end": 1900000000},
                        {"name": "glm-4.5", "total_units": 4, "remaining_units": 1},
                        {"model": "skipped"}
                    ]
                }
            })),
        )
        .unwrap();
        assert_eq!(usage.plan_name.as_deref(), Some("Pro"));
        let monthly = usage.monthly.unwrap();
        assert_eq!(monthly.label, "Quota");
        assert_eq!(monthly.used, 6);
        assert_eq!(monthly.total, Some(14));
        assert_eq!(monthly.breakdown.len(), 2);
        assert_eq!(monthly.breakdown[0].label, "glm-4.6");
        assert_eq!(monthly.breakdown[0].used, 3);
        assert_eq!(monthly.breakdown[0].reset_at, Some(1_900_000_000));
        assert_eq!(monthly.breakdown[1].label, "glm-4.5");
        assert_eq!(monthly.breakdown[1].used, 3);

        let profile_plan = usage_from_bodies(
            "sub",
            Some(&json!({"planName": "Lite"})),
            Some(&json!({"code": 0, "data": {}})),
        )
        .unwrap();
        assert_eq!(profile_plan.plan_name.as_deref(), Some("Lite"));
        assert!(profile_plan.monthly.is_none());

        let bad =
            usage_from_bodies("sub", None, Some(&json!({"code": 7, "msg": "no"}))).unwrap_err();
        assert!(matches!(bad, UsageError::Fetcher(_)), "{bad:?}");
        assert!(!bad.is_transient());
    }

    #[tokio::test]
    async fn quota_hits_the_provider_profile_and_jwt_billing() {
        use super::super::http::scripted::{self, ScriptedHttp};

        // fetch_billing resolves the device mid from the real ZCode home and
        // the real data root; both stay out of this test.
        let sandbox = tempfile::tempdir().unwrap();
        let _env = crate::test_support::EnvGuard::set(&[
            ("SKILLSTAR_TOOL_SYNC_HOME", sandbox.path()),
            ("SKILLSTAR_DATA_DIR", sandbox.path()),
        ]);

        let zai = ScriptedHttp::start(vec![
            (200, r#"{"email":"a@b.c","user_id":"user-9"}"#.into()),
            (
                200,
                r#"{"code":0,"data":{"plans":[{"status":"active","name":"Pro"}],"balances":[{"model":"glm-4.6","used_units":1,"total_units":2}]}}"#.into(),
            ),
            (500, "down".into()),
            (500, "mcp down".into()),
        ]);
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .no_proxy()
            .build()
            .unwrap();
        let endpoints = Endpoints::from_base(&zai.base);
        let mut sub = row("zai", "access-z", Some("jwt-z"));
        let usage = read_quota(&client, &endpoints, &mut sub).await.unwrap();
        assert_eq!(usage.plan_name.as_deref(), Some("Pro"));
        assert_eq!(usage.monthly.unwrap().label, "glm-4.6");
        assert_eq!(sub.oauth_account_id.as_deref(), Some("user-9"));
        assert_eq!(sub.display_name, "a@b.c");
        let seen = zai.seen();
        assert_eq!(seen[0].path, "/api/oauth/userinfo");
        assert_eq!(
            scripted::header(&seen[0], "authorization"),
            Some("Bearer access-z")
        );
        assert!(
            seen[1]
                .path
                .starts_with("/api/v1/zcode-plan/billing/balance?app_version=")
        );
        assert_eq!(
            scripted::header(&seen[1], "authorization"),
            Some("Bearer jwt-z")
        );
        assert_eq!(seen.len(), 4);
        assert_eq!(seen[2].path, "/api/monitor/usage/quota/limit");
        assert_eq!(
            scripted::header(&seen[2], "authorization"),
            Some("Bearer access-z")
        );
        assert_eq!(seen[3].path, "/api/v1/mcp/usage");
        assert_eq!(
            scripted::header(&seen[3], "authorization"),
            Some("Bearer jwt-z")
        );
        assert_eq!(
            scripted::header(&seen[3], "x-bigmodel-authorization"),
            Some("Bearer access-z")
        );
        assert_eq!(
            scripted::header(&seen[3], "bigmodel-target-type"),
            Some("PERSONAL")
        );
        let mid = scripted::header(&seen[1], "x-device-mid").expect("device mid on billing");
        uuid::Uuid::parse_str(mid).expect("device mid is a uuid");
        let kept = std::fs::read_to_string(
            sandbox
                .path()
                .join("state")
                .join("usage")
                .join("zcode-device-mid"),
        )
        .unwrap();
        assert_eq!(mid, kept.trim());

        let bigmodel = ScriptedHttp::start(vec![
            (
                200,
                r#"{"code":200,"data":{"customerNumber":"c1","email":"b@c.d"}}"#.into(),
            ),
            (200, r#"{"code":0,"data":{"balances":[]}}"#.into()),
            (
                200,
                r#"{"code":200,"success":true,"data":{"level":"pro","limits":[{"type":"TOKENS_LIMIT","unit":3,"number":5,"percentage":0},{"type":"TOKENS_LIMIT","unit":6,"number":1,"percentage":38,"nextResetTime":1787641095989},{"type":"TIME_LIMIT","usage":4000,"currentValue":0,"percentage":0}]}}"#.into(),
            ),
            (
                200,
                r#"{"code":0,"data":{"next_refresh_at":1791388800,"total_usage":{"used":0,"limit":1000,"remaining":1000}}}"#.into(),
            ),
        ]);
        let endpoints = Endpoints::from_base(&bigmodel.base);
        let mut sub = row("bigmodel", "raw-token", Some("jwt-b"));
        let usage = read_quota(&client, &endpoints, &mut sub).await.unwrap();
        assert_eq!(usage.plan_name.as_deref(), Some("pro"));
        assert_eq!(usage.hourly.as_ref().unwrap().label, "Five Hour Limit");
        assert_eq!(usage.weekly.as_ref().unwrap().percent, Some(38));
        assert_eq!(usage.monthly.as_ref().unwrap().label, "ZCode MCP");
        assert_eq!(usage.monthly.as_ref().unwrap().used, 0);
        assert_eq!(usage.monthly.as_ref().unwrap().total, Some(1000));
        assert_eq!(
            usage.monthly.as_ref().unwrap().reset_at,
            Some(1_791_388_800)
        );
        assert_eq!(sub.oauth_account_id.as_deref(), Some("c1"));
        let seen = bigmodel.seen();
        assert_eq!(seen[0].path, "/api/biz/customer/getCustomerInfo");
        assert_eq!(
            scripted::header(&seen[0], "authorization"),
            Some("raw-token")
        );
        assert_ne!(
            scripted::header(&seen[0], "authorization"),
            Some("Bearer raw-token")
        );
        assert_eq!(seen[2].path, "/api/monitor/usage/quota/limit");
        assert_eq!(
            scripted::header(&seen[2], "authorization"),
            Some("Bearer raw-token")
        );

        let forbidden = ScriptedHttp::start(vec![(403, "no".into())]);
        let endpoints = Endpoints::from_base(&forbidden.base);
        let mut sub = row("zai", "access", Some("jwt"));
        let error = read_quota(&client, &endpoints, &mut sub).await.unwrap_err();
        assert!(matches!(error, UsageError::Fetcher(_)), "{error:?}");
        assert!(!matches!(error, UsageError::AuthRequired));

        let mut api_key = SubscriptionBuilder::new("zcode", "Key", "USD", "", None)
            .provider_state(super::super::provider_state_json("api_key"))
            .build();
        api_key.access_token_encrypted = None;
        api_key.api_key_encrypted = Some(crate::crypto::encrypt("sk"));
        let usage = read_quota(&client, &Endpoints::production(), &mut api_key)
            .await
            .unwrap();
        assert_eq!(usage.plan_name.as_deref(), Some("API Key"));
        assert!(usage.monthly.is_none());
    }
}
