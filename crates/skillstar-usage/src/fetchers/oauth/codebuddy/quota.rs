//! Quota from dosage, payment type, and user-resource (or enterprise usage).
//!
//! A window exists only when `used` is known. Missing amounts omit that
//! package. An explicit zero stays zero. Enterprise accounts call
//! `get-enterprise-user-usage` because user-resource returns an empty list.

use chrono::{DateTime, Local, NaiveDateTime, Utc};
use serde_json::{Value, json};

use super::http::{self, QuotaContext};
use super::{
    DOSAGE_PATH, ENTERPRISE_USAGE_PATH, Enterprise, Host, PAYMENT_PATH, USER_RESOURCE_PATH,
    endpoint, json_f64, nonempty, object_string,
};
use crate::subscription::{Subscription, SubscriptionUsage, UsageWindow};
use crate::{UsageError, UsageResult};

const ADDON: &str = "TCACA_code_009_0XmEQc2xOf";
const ACTIVITY: &str = "TCACA_code_007_nzdH5h4Nl0";
const ENTERPRISE_CODE: &str = "TCACA_code_enterprise";
const BASE: &[&str] = &[
    "TCACA_code_001_PqouKr6QWV",
    "TCACA_code_006_DbXS0lrypC",
    "TCACA_code_008_cfWoLwvjU4",
];
const PRO: &[&str] = &["TCACA_code_002_AkiJS3ZHF5", "TCACA_code_003_FAnt7lcmRT"];

const TOTAL_KEYS: &[&str] = &[
    "CycleCapacitySizePrecise",
    "CycleCapacitySize",
    "CapacitySizePrecise",
    "CapacitySize",
    "limit_num",
    "limitNum",
    "total",
];
const REMAIN_KEYS: &[&str] = &[
    "CycleCapacityRemainPrecise",
    "CycleCapacityRemain",
    "CapacityRemainPrecise",
    "CapacityRemain",
    "remain",
    "remaining",
];
const USED_KEYS: &[&str] = &[
    "CycleCapacityUsedPrecise",
    "CycleCapacityUsed",
    "used_num",
    "usedNum",
    "credit",
    "used",
];

const ITEM_PATHS: &[&[&str]] = &[
    &["data", "resources"],
    &["data", "data", "resources"],
    &["data", "Response", "Data", "Accounts"],
    &["data", "data", "Response", "Data", "Accounts"],
    &["Response", "Data", "Accounts"],
];

struct Package {
    code: Option<String>,
    name: Option<String>,
    used: i64,
    total: Option<i64>,
    reset_at: Option<i64>,
}

pub(super) async fn fetch_quota(
    host: &Host,
    subscription: &mut Subscription,
    refresh_first: bool,
) -> UsageResult<SubscriptionUsage> {
    let client = crate::fetchers::http_client()?;
    fetch_with_client(&client, host, host.origin, subscription, refresh_first).await
}

pub(super) async fn fetch_with_client(
    client: &reqwest::Client,
    host: &Host,
    origin: &str,
    subscription: &mut Subscription,
    refresh_first: bool,
) -> UsageResult<SubscriptionUsage> {
    let mut access = super::decrypt_optional(&subscription.access_token_encrypted)
        .ok_or(UsageError::AuthRequired)?;
    let mut refresh = super::decrypt_optional(&subscription.refresh_token_encrypted);
    let mut enterprise = Enterprise::from_subscription(subscription);
    let mut expires = subscription.access_token_expires_at;
    if refresh_first && let Some(token) = refresh.clone() {
        let issued = http::refresh_access(
            client,
            origin,
            &access,
            &token,
            enterprise.domain.as_deref(),
            &format!("{} refresh", host.display_name),
        )
        .await?;
        access = issued.access_token;
        if let Some(next) = issued.refresh_token {
            refresh = Some(next);
        }
        if let Some(domain) = issued.domain {
            enterprise.domain = Some(domain);
        }
        if enterprise.enterprise_id.is_none() {
            enterprise.enterprise_id = issued.enterprise_id;
        }
        if enterprise.enterprise_name.is_none() {
            enterprise.enterprise_name = issued.enterprise_name;
        }
        expires = issued.expires_at.or(expires);
        apply_tokens(subscription, &access, refresh.as_deref(), expires);
        if let Some(json) = enterprise.to_json() {
            subscription.provider_state_encrypted = Some(crate::crypto::encrypt(&json));
        }
        if subscription
            .oauth_account_id
            .as_deref()
            .map(str::trim)
            .is_none_or(str::is_empty)
            && let Some(uid) = issued.uid
        {
            subscription.oauth_account_id = Some(uid);
        }
    }
    let context = QuotaContext {
        user_id: subscription.oauth_account_id.clone(),
        enterprise_id: enterprise.enterprise_id.clone(),
        domain: enterprise.domain.clone(),
    };
    let dosage = post_quota(
        client,
        host,
        origin,
        &access,
        &context,
        DOSAGE_PATH,
        None,
        "dosage",
    )
    .await?;
    let payment = post_quota(
        client,
        host,
        origin,
        &access,
        &context,
        PAYMENT_PATH,
        None,
        "payment",
    )
    .await?;
    let resource = if nonempty(context.enterprise_id.as_deref()).is_some() {
        post_quota(
            client,
            host,
            origin,
            &access,
            &context,
            ENTERPRISE_USAGE_PATH,
            Some(&json!({})),
            "enterprise-usage",
        )
        .await?
    } else {
        post_quota(
            client,
            host,
            origin,
            &access,
            &context,
            USER_RESOURCE_PATH,
            Some(&user_resource_body(Local::now())),
            "user-resource",
        )
        .await?
    };
    if subscription.oauth_region.as_deref() != Some(host.oauth_region) {
        subscription.oauth_region = Some(host.oauth_region.to_string());
    }
    Ok(usage_from_bodies(
        &subscription.id,
        &dosage,
        &payment,
        &resource,
    ))
}

pub(super) fn user_resource_body(now: DateTime<Local>) -> Value {
    let end = now + chrono::Duration::days(365 * 101);
    json!({
        "PageNumber": 1,
        "PageSize": 100,
        "ProductCode": "p_tcaca",
        "Status": [0, 3],
        "PackageEndTimeRangeBegin": now.format("%Y-%m-%d %H:%M:%S").to_string(),
        "PackageEndTimeRangeEnd": end.format("%Y-%m-%d %H:%M:%S").to_string(),
    })
}

pub(super) fn usage_from_bodies(
    subscription_id: &str,
    dosage: &Value,
    payment: &Value,
    resource: &Value,
) -> SubscriptionUsage {
    let packages = packages_from_body(resource);
    let mut usage = SubscriptionUsage {
        subscription_id: subscription_id.to_string(),
        fetched_at: Utc::now().timestamp(),
        ..SubscriptionUsage::default()
    };
    let Some(primary_at) = primary_index(&packages) else {
        usage.plan_name = plan_name(payment, dosage, None);
        return usage;
    };
    let primary = &packages[primary_at];
    let label = package_label(primary);
    usage.plan_name = plan_name(payment, dosage, Some(label.as_str()));
    let mut primary_window = window(&label, primary.used, primary.total, primary.reset_at);
    primary_window.breakdown = packages
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != primary_at)
        .map(|(_, package)| {
            window(
                &package_label(package),
                package.used,
                package.total,
                package.reset_at,
            )
        })
        .collect();
    usage.monthly = Some(primary_window);
    usage
}

fn packages_from_body(body: &Value) -> Vec<Package> {
    let items = resource_items(body);
    if !items.is_empty() {
        return items.iter().filter_map(package_from_item).collect();
    }
    wrap_enterprise(body)
        .map(|wrapped| {
            resource_items(&wrapped)
                .iter()
                .filter_map(package_from_item)
                .collect()
        })
        .unwrap_or_default()
}

fn plan_name(payment: &Value, dosage: &Value, package: Option<&str>) -> Option<String> {
    payment_label(payment)
        .or_else(|| {
            package
                .filter(|label| *label != "Usage")
                .map(str::to_string)
        })
        .or_else(|| dosage_label(dosage))
        .or_else(|| package.map(str::to_string))
}

fn payment_label(body: &Value) -> Option<String> {
    let data = body.get("data").unwrap_or(body);
    if let Some(text) = data.as_str() {
        return nonempty(Some(text));
    }
    object_string(data, &["paymentType", "payment_type"])
}

fn dosage_label(body: &Value) -> Option<String> {
    let data = body
        .get("data")
        .filter(|value| value.is_object())
        .unwrap_or(body);
    object_string(
        data,
        &[
            "dosageNotifyZh",
            "dosage_notify_zh",
            "dosageNotifyEn",
            "dosage_notify_en",
        ],
    )
}

fn package_label(package: &Package) -> String {
    match package.code.as_deref() {
        Some(ADDON) => "Add-on".to_string(),
        Some(ACTIVITY) => "Activity".to_string(),
        Some(ENTERPRISE_CODE) => "Enterprise".to_string(),
        Some(code) if BASE.contains(&code) => "Basic".to_string(),
        Some(code) if PRO.contains(&code) => "Pro".to_string(),
        _ => package
            .name
            .clone()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| "Usage".to_string()),
    }
}

fn primary_index(packages: &[Package]) -> Option<usize> {
    packages
        .iter()
        .enumerate()
        .min_by_key(|(_, package)| match package.code.as_deref() {
            Some(code) if PRO.contains(&code) => 0,
            Some(code) if BASE.contains(&code) => 1,
            Some(ENTERPRISE_CODE) => 2,
            Some(ADDON) => 4,
            _ => 3,
        })
        .map(|(index, _)| index)
}

fn package_from_item(item: &Value) -> Option<Package> {
    if !status_active(item) {
        return None;
    }
    let total_raw = lookup_f64(item, TOTAL_KEYS);
    let unlimited = is_unlimited(item, total_raw);
    let used_raw = lookup_f64(item, USED_KEYS).filter(|value| *value >= 0.0);
    let remain_raw = lookup_f64(item, REMAIN_KEYS).filter(|value| *value >= 0.0);
    let (used, total) = if unlimited {
        (used_raw?, None)
    } else {
        let total = total_raw.filter(|value| *value > 0.0);
        let used = match used_raw {
            Some(used) => used,
            None => {
                let total = total?;
                let remain = remain_raw?;
                (total - remain).max(0.0)
            }
        };
        let total = total.or_else(|| remain_raw.map(|remain| used + remain));
        (used, total.filter(|value| *value > 0.0))
    };
    Some(Package {
        code: object_string(item, &["PackageCode", "packageCode", "package_code"]),
        name: object_string(item, &["PackageName", "packageName", "package_name"]),
        used: as_units(used),
        total: total.map(as_units),
        reset_at: reset_of(item),
    })
}

fn status_active(item: &Value) -> bool {
    let Some(raw) = item.get("Status").or_else(|| item.get("status")) else {
        return true;
    };
    match super::json_i64(Some(raw)) {
        Some(0 | 3) => true,
        Some(_) => false,
        None => true,
    }
}

fn is_unlimited(item: &Value, total: Option<f64>) -> bool {
    if total == Some(-1.0) {
        return true;
    }
    match item.get("Unlimited").or_else(|| item.get("unlimited")) {
        Some(Value::Bool(flag)) => *flag,
        Some(Value::Number(number)) => number.as_i64() == Some(1),
        Some(Value::String(text)) => text.eq_ignore_ascii_case("true"),
        _ => false,
    }
}

fn resource_items(body: &Value) -> Vec<Value> {
    for path in ITEM_PATHS {
        if let Some(items) = array_at(body, path)
            && !items.is_empty()
        {
            return items
                .iter()
                .filter(|item| item.is_object())
                .cloned()
                .collect();
        }
    }
    Vec::new()
}

fn array_at<'a>(body: &'a Value, path: &[&str]) -> Option<&'a Vec<Value>> {
    let mut cursor = body;
    for key in path {
        cursor = cursor.get(*key)?;
    }
    cursor.as_array()
}

pub(super) fn wrap_enterprise(body: &Value) -> Option<Value> {
    let data = body
        .pointer("/data/data")
        .or_else(|| body.get("data"))
        .unwrap_or(body);
    let limit = lookup_f64(data, &["limit_num", "limitNum"])?;
    let used = lookup_f64(data, &["used_num", "usedNum", "credit"])?;
    let unlimited = limit == -1.0;
    let remain = if unlimited {
        -1.0
    } else {
        (limit - used).max(0.0)
    };
    let reset = object_string(
        data,
        &[
            "cycle_reset_time",
            "cycleResetTime",
            "cycle_end_time",
            "cycleEndTime",
        ],
    )
    .unwrap_or_default();
    Some(json!({
        "code": 0,
        "data": {
            "Response": {
                "Data": {
                    "Accounts": [{
                        "PackageCode": ENTERPRISE_CODE,
                        "PackageName": "Enterprise",
                        "CycleCapacitySizePrecise": limit.to_string(),
                        "CycleCapacityRemainPrecise": remain.to_string(),
                        "CycleCapacityUsedPrecise": used.to_string(),
                        "CycleResetTime": reset,
                        "Unlimited": unlimited,
                        "Status": 0
                    }]
                }
            }
        }
    }))
}

fn lookup_f64(item: &Value, keys: &[&str]) -> Option<f64> {
    let map = item.as_object()?;
    for key in keys {
        if let Some((_, value)) = map.iter().find(|(name, _)| name.eq_ignore_ascii_case(key))
            && let Some(number) = json_f64(Some(value))
        {
            return Some(number);
        }
    }
    None
}

fn reset_of(item: &Value) -> Option<i64> {
    let text = object_string(
        item,
        &[
            "CycleResetTime",
            "cycleResetTime",
            "cycle_reset_time",
            "CycleEndTime",
            "cycleEndTime",
            "cycle_end_time",
            "ExpiredTime",
            "DeductionEndTime",
        ],
    )?;
    parse_reset(&text)
}

fn parse_reset(raw: &str) -> Option<i64> {
    let text = raw.trim();
    if text.is_empty() {
        return None;
    }
    if let Ok(number) = text.parse::<i64>() {
        return http::normalize_epoch_seconds(number);
    }
    if let Ok(parsed) = DateTime::parse_from_rfc3339(text) {
        return Some(parsed.timestamp());
    }
    NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S")
        .ok()
        .map(|value| value.and_utc().timestamp())
}

fn as_units(value: f64) -> i64 {
    value.round().clamp(0.0, i64::MAX as f64) as i64
}

fn window(label: &str, used: i64, total: Option<i64>, reset_at: Option<i64>) -> UsageWindow {
    UsageWindow {
        label: label.to_string(),
        used,
        total,
        percent: total
            .filter(|total| *total > 0)
            .map(|total| ((used.clamp(0, total) as i128) * 100 / total as i128) as i32),
        reset_at,
        breakdown: Vec::new(),
    }
}

/// One quota leg: caller, target and transport are all distinct inputs.
#[allow(clippy::too_many_arguments)]
async fn post_quota(
    client: &reqwest::Client,
    host: &Host,
    origin: &str,
    access_token: &str,
    context: &QuotaContext,
    path: &str,
    body: Option<&Value>,
    leg: &str,
) -> UsageResult<Value> {
    let label = format!("{} {leg}", host.display_name);
    let url = endpoint(origin, path);
    let (status, text) = http::execute(
        client,
        reqwest::Method::POST,
        &url,
        &http::quota_headers(access_token, context),
        body,
        &label,
    )
    .await?;
    http::classify_exchange(status, &text, &label)
}

fn apply_tokens(
    subscription: &mut Subscription,
    access: &str,
    refresh: Option<&str>,
    expires: Option<i64>,
) {
    subscription.access_token_encrypted = Some(crate::crypto::encrypt(access));
    if let Some(refresh) = refresh.map(str::trim).filter(|token| !token.is_empty()) {
        subscription.refresh_token_encrypted = Some(crate::crypto::encrypt(refresh));
    }
    if expires.is_some() {
        subscription.access_token_expires_at = expires;
    }
}

#[cfg(test)]
#[path = "quota_tests.rs"]
mod tests;
