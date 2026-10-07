//! Command Code identity, credit pools and quota windows.
use super::super::account_json::{number, read, stamp};
use crate::subscription::{MonetaryBalance, SubscriptionUsage, UsageUnit, UsageWindow};
use crate::{UsageError, UsageResult};
use serde_json::Value;
const BASE: &str = "https://api.commandcode.ai/alpha";

pub async fn fetch(id: &str, key: &str) -> UsageResult<SubscriptionUsage> {
    if key.len() < 8 || key.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(UsageError::Other("Command Code API Key 无效".into()));
    }
    let client = super::super::http_client()?;
    let get = |url: String| {
        client
            .get(url)
            .bearer_auth(key)
            .header("x-cli-environment", "production")
    };
    let who = read(get(format!("{BASE}/whoami?limits=1"))).await?;
    if !who["user"].is_object() {
        return Err(UsageError::Fetcher("Command Code 缺少账号信息".into()));
    }
    let org = who["org"]["id"].as_str();
    let query = org
        .map(|s| format!("?orgId={}", crate::urlencode::encode(s)))
        .unwrap_or_default();
    let (credits, subscription) = tokio::join!(
        read(get(format!("{BASE}/billing/credits{query}"))),
        read(get(format!("{BASE}/billing/subscriptions{query}")))
    );
    let (credits, subscription) = (credits?, subscription?);
    let mut url = reqwest::Url::parse(&format!("{BASE}/usage/summary")).expect("constant URL");
    if let Some(org) = org {
        url.query_pairs_mut().append_pair("orgId", org);
    }
    if let Some(since) = subscription["data"]["currentPeriodStart"].as_str() {
        url.query_pairs_mut().append_pair("since", since);
    }
    let summary = read(get(url.into())).await?;
    Ok(parse(id, &credits, &subscription, &summary))
}

fn window(value: &Value, label: &str) -> Option<UsageWindow> {
    let total = number(&value["cap"]).filter(|v| *v > 0.0)?;
    let used = number(&value["used"]).unwrap_or(0.0).max(0.0);
    Some(UsageWindow {
        label: label.into(),
        used: (used * 100.0).round() as i64,
        total: Some((total * 100.0).round() as i64),
        percent: Some((used / total * 100.0).clamp(0.0, 100.0).round() as i32),
        reset_at: stamp(value.get("resetAt").unwrap_or(&value["reset"])),
        breakdown: vec![],

        unit: UsageUnit::Count,
    })
}

fn parse(id: &str, credits: &Value, subscription: &Value, summary: &Value) -> SubscriptionUsage {
    let pools = &credits["credits"];
    let monthly = number(&pools["monthlyCredits"]).unwrap_or(0.0).max(0.0);
    let purchased = number(&pools["purchasedCredits"]).unwrap_or(0.0).max(0.0);
    let free = number(&pools["freeCredits"]).unwrap_or(0.0).max(0.0);
    let remaining = monthly + purchased + free;
    let data = &subscription["data"];
    let plan = data["planId"].as_str();
    // Same plan table as DSH/CLI; longest prefix prevents pro-v1 matching pro.
    let allowance: Option<f64> = plan
        .and_then(|p| {
            let p = p.to_lowercase().replace('_', "-");
            [
                ("individual-provider", 15.0),
                ("individual-pro-v1", 80.0),
                ("individual-goat", 70.0),
                ("individual-ultra", 300.0),
                ("individual-pro", 30.0),
                ("individual-max", 150.0),
                ("individual-go", 10.0),
                ("teams-pro", 40.0),
            ]
            .into_iter()
            .find(|(k, _)| p.starts_with(k))
            .map(|(_, n)| n)
        })
        .filter(|_| {
            matches!(
                data["status"].as_str(),
                Some("active" | "trialing" | "past_due")
            )
        });
    let total = allowance
        .map(|v: f64| v.max(monthly) + purchased + free)
        .unwrap_or(number(&summary["totalCost"]).unwrap_or(0.0).max(0.0) + remaining);
    SubscriptionUsage {
        subscription_id: id.into(),
        fetched_at: chrono::Utc::now().timestamp(),
        plan_name: plan.map(str::to_string),
        hourly: window(&credits["windowLimits"]["fiveHour"], "5h"),
        weekly: window(&credits["windowLimits"]["weekly"], "7d"),
        monthly: (total > 0.0).then(|| UsageWindow {
            label: "Credits".into(),
            used: ((total - remaining).max(0.0) * 100.0).round() as i64,
            total: Some((total * 100.0).round() as i64),
            percent: Some(
                ((total - remaining).max(0.0) / total * 100.0)
                    .clamp(0.0, 100.0)
                    .round() as i32,
            ),
            reset_at: stamp(&data["currentPeriodEnd"]),
            breakdown: vec![],

            unit: UsageUnit::Count,
        }),
        balance: credits["credits"].is_object().then_some(MonetaryBalance {
            currency: "USD".into(),
            total: remaining,
            granted: monthly + free,
            topped_up: purchased,
            is_available: None,
        }),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn credits_use_longest_plan_prefix_and_topups() {
        let usage = parse(
            "c",
            &serde_json::json!({"credits":{"monthlyCredits":40,"purchasedCredits":5},"windowLimits":{"fiveHour":{"used":2,"cap":10}}}),
            &serde_json::json!({"data":{"planId":"individual-pro-v1","status":"active"}}),
            &Value::Null,
        );
        assert_eq!(usage.balance.unwrap().total, 45.0);
        assert_eq!(usage.monthly.unwrap().total, Some(8500));
        assert_eq!(usage.hourly.unwrap().percent, Some(20));
        assert!(
            parse("c", &Value::Null, &Value::Null, &Value::Null)
                .monthly
                .is_none()
        );
    }
}
