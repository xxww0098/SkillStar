//! Kimi Code quota windows (the Moonshot platform balance is a different product).
use super::super::account_json::{number, read, stamp};
use crate::subscription::{SubscriptionUsage, UsageUnit, UsageWindow};
use crate::{UsageError, UsageResult};
use serde_json::Value;

pub async fn fetch(id: &str, api_key: &str) -> UsageResult<SubscriptionUsage> {
    let client = super::super::http_client()?;
    let get = |path| {
        client
            .get(path)
            .bearer_auth(api_key)
            .header("User-Agent", "SkillStar")
            .header("x-msh-platform", "skillstar")
            .header("x-msh-version", "SkillStar")
            .header("x-msh-device-id", id)
            .header("x-msh-device-name", "SkillStar")
            .header("x-msh-device-model", std::env::consts::ARCH)
            .header("x-msh-os-version", std::env::consts::OS)
    };
    let (usage, me) = tokio::join!(
        read(get(crate::providers::balance::KIMI.endpoint)),
        read(get("https://api.kimi.com/coding/v1/me"))
    );
    // Keep a hard quota failure visible instead of fabricating an empty balance.
    let usage = usage?;
    if !usage.is_object() {
        return Err(UsageError::Fetcher("Kimi 额度响应无效".into()));
    }
    Ok(parse(id, &usage, &me.unwrap_or(Value::Null)))
}

fn first_number(v: &Value, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|key| number(&v[*key]))
}

fn bag(v: &Value) -> Option<(Option<f64>, Option<f64>, Option<f64>)> {
    if let Some(items) = v.as_array() {
        return items.iter().find_map(bag);
    }
    let total = first_number(v, &["total", "limit", "cap", "allocation", "amount"]);
    let used = first_number(v, &["used", "spent", "consumed", "usage"]);
    let remaining = first_number(v, &["remaining", "balance", "left"]);
    if total.is_none() && used.is_none() && remaining.is_none() {
        return v.get("bags").or(v.get("items")).and_then(bag);
    }
    Some((
        total,
        used.or_else(|| Some((total? - remaining?).max(0.0))),
        remaining,
    ))
}

fn window(v: &Value, label: &str) -> Option<UsageWindow> {
    let (total, used, remaining) = bag(v)?;
    let used = used.unwrap_or(0.0).max(0.0);
    let percent = total.filter(|n| *n > 0.0).map(|total| {
        let left = remaining.unwrap_or((total - used).max(0.0));
        ((1.0 - left / total) * 100.0).clamp(0.0, 100.0).round() as i32
    });
    let reset = [
        "reset_at",
        "resetAt",
        "resets_at",
        "resetsAt",
        "reset_time",
        "resetTime",
    ]
    .iter()
    .find_map(|key| stamp(&v[*key]));
    Some(UsageWindow {
        label: v["name"]
            .as_str()
            .or(v["title"].as_str())
            .unwrap_or(label)
            .into(),
        used: used.round() as i64,
        total: total.map(|v| v.round() as i64),
        percent,
        reset_at: reset,
        breakdown: vec![],

        unit: UsageUnit::Count,
    })
}

fn parse(id: &str, value: &Value, me: &Value) -> SubscriptionUsage {
    let mut usage = SubscriptionUsage {
        subscription_id: id.into(),
        fetched_at: chrono::Utc::now().timestamp(),
        plan_name: ["user_level_name", "userLevelName", "plan", "plan_type"]
            .iter()
            .find_map(|key| me[*key].as_str())
            .map(str::to_string),
        weekly: window(&value["usage"], "7d"),
        ..Default::default()
    };
    for (i, limit) in value["limits"].as_array().into_iter().flatten().enumerate() {
        let unit = limit["window"]["timeUnit"]
            .as_str()
            .or(limit["window"]["time_unit"].as_str())
            .unwrap_or("")
            .to_uppercase();
        let slot = if unit.contains("WEEK") {
            &mut usage.weekly
        } else if unit.contains("DAY") {
            &mut usage.monthly
        } else if unit.contains("HOUR") || unit.contains("MINUTE") || i == 0 {
            &mut usage.hourly
        } else {
            &mut usage.monthly
        };
        if let Some(row) = window(
            limit.get("detail").unwrap_or(limit),
            if unit.contains("WEEK") { "7d" } else { "5h" },
        ) {
            if let Some(parent) = slot {
                parent.breakdown.push(row);
            } else {
                *slot = Some(row);
            }
        }
    }
    usage
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn code_windows_decode_bags_and_numeric_strings() {
        let value = serde_json::json!({"usage":{"total":"100","remaining":"80"},"limits":[{"window":{"timeUnit":"HOUR","duration":5},"detail":{"bags":[{"total":{"val":50},"used":20}]}}]});
        let usage = parse("k", &value, &serde_json::json!({"user_level_name":"Pro"}));
        assert_eq!(usage.plan_name.as_deref(), Some("Pro"));
        assert_eq!(usage.weekly.unwrap().percent, Some(20));
        assert_eq!(usage.hourly.unwrap().percent, Some(40));
        assert!(parse("k", &Value::Null, &Value::Null).weekly.is_none());
    }
}
