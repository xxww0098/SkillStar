//! Grok（xAI）计费响应的纯解析（夹具直接驱动，不碰网络）。
//!
//! 两个视图、两种语义，别弄反：
//!
//! * 默认视图（/v1/billing）给**月度数字额度**：monthlyLimit / usage.totalUsed
//!   都是**美元分**，重置点看 billingCycle.billingPeriodEnd。
//! * credits 视图（?format=credits）给**周进度**：creditUsagePercent 是**已用**
//!   百分比（proto3 在 0 时省略），周期看 currentPeriod.{type,end}。
//!
//! 快照里的窗口一律记"剩余"，所以周进度要换算成 100 - 已用；月度额度按分/分
//! 算剩余比例。金额字段可能挂在根上（xAI 自己的形状），也可能裹在 config 里
//! （镜像/旧夹具），两处都读。

use serde_json::Value;

use crate::usage::snapshot::{Credits, UsageSnapshot, UsageUnit, UsageWindow};

use super::parse;

/// 当前计费周期：周计划、重置时间、已用百分比。
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct CurrentPeriod {
    /// Some(true) 周重置，Some(false) 月重置，None 类型不认识。
    pub(crate) weekly: Option<bool>,
    pub(crate) end: Option<i64>,
    /// creditUsagePercent：**已用** 0..=100，缺失按 0 处理。
    pub(crate) usage_percent: Option<f64>,
}

/// 金额字段可能挂在根上，也可能在 config 包装里；按优先级返回候选根。
pub(crate) fn candidate_roots(payload: &Value) -> Vec<&Value> {
    let mut roots = vec![payload];
    if let Some(config) = payload.get("config").filter(|value| value.is_object()) {
        roots.push(config);
    }
    roots
}

/// 逐条路径试读，先根后 config。
pub(crate) fn pick_value_multi<'a>(roots: &[&'a Value], paths: &[&[&str]]) -> Option<&'a Value> {
    for root in roots {
        for path in paths {
            if let Some(value) = get_path(root, path) {
                return Some(value);
            }
        }
    }
    None
}

fn get_path<'a>(root: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = root;
    for key in path {
        current = current.get(*key)?;
    }
    Some(current)
}

fn pick_cent_multi(roots: &[&Value], paths: &[&[&str]]) -> Option<f64> {
    pick_value_multi(roots, paths).and_then(cent_value)
}

/// 金额：数字、数字字符串，或 { val: ... } 包装。
pub(crate) fn cent_value(value: &Value) -> Option<f64> {
    if let Some(object) = value.as_object()
        && let Some(inner) = object.get("val")
    {
        return cent_value(inner);
    }
    if let Some(number) = value.as_f64().filter(|number| number.is_finite()) {
        return Some(number);
    }
    value
        .as_str()
        .and_then(|text| text.trim().parse::<f64>().ok())
        .filter(|number| number.is_finite())
}

/// 百分比：与金额同形状，只是语义是 0..=100。
pub(crate) fn percent_value(value: &Value) -> Option<f64> {
    cent_value(value)
}

/// 时间点：epoch 秒/毫秒、数字字符串，或 RFC3339。
pub(crate) fn parse_timestamp(value: &Value) -> Option<i64> {
    let raw = match value {
        Value::Number(number) => number.as_i64(),
        Value::String(text) => {
            let trimmed = text.trim();
            if let Ok(number) = trimmed.parse::<i64>() {
                return normalize_timestamp(number);
            }
            return chrono::DateTime::parse_from_rfc3339(trimmed)
                .ok()
                .map(|at| at.timestamp());
        }
        _ => None,
    }?;
    normalize_timestamp(raw)
}

fn normalize_timestamp(raw: i64) -> Option<i64> {
    if raw <= 0 {
        return None;
    }
    Some(if raw > 10_000_000_000 {
        raw / 1000
    } else {
        raw
    })
}

/// credits 视图里的当前周期。
pub(crate) fn current_period(payload: &Value) -> Option<CurrentPeriod> {
    let roots = candidate_roots(payload);
    let period = pick_value_multi(&roots, &[&["currentPeriod"], &["current_period"]])?;
    let kind = period.get("type").and_then(Value::as_str).unwrap_or("");
    let weekly = if kind.contains("WEEKLY") {
        Some(true)
    } else if kind.contains("MONTHLY") {
        Some(false)
    } else {
        None
    };
    let end = period
        .get("end")
        .and_then(parse_timestamp)
        .or_else(|| period.get("billingPeriodEnd").and_then(parse_timestamp));
    // creditUsagePercent 是 currentPeriod 的兄弟字段，不是它的子节点。
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

/// 月度数字额度窗口。没有上限（monthlyLimit 缺失）时不画窗口：快照的
/// remaining_percent 是必填项，编一个 0 或 100 都是假话。
pub(crate) fn monthly_window(payload: &Value, period: Option<&CurrentPeriod>) -> Option<UsageWindow> {
    let roots = candidate_roots(payload);
    let limit = pick_cent_multi(&roots, &[&["monthlyLimit"], &["monthly_limit"]])?;
    let used = pick_cent_multi(
        &roots,
        &[&["usage", "totalUsed"], &["usage", "total_used"], &["used"]],
    )
    .unwrap_or(0.0);
    let resets_at = pick_value_multi(
        &roots,
        &[
            &["billingCycle", "billingPeriodEnd"],
            &["billing_cycle", "billing_period_end"],
            &["billingPeriodEnd"],
            &["billing_period_end"],
        ],
    )
    .and_then(parse_timestamp)
    .or_else(|| match period {
        Some(period) if period.weekly == Some(false) => period.end,
        _ => None,
    });
    let used = used.max(0.0);
    Some(UsageWindow {
        key: "month".to_string(),
        label: "Monthly credits".to_string(),
        remaining_percent: parse::remaining_from_amounts(used, limit),
        used: Some(used),
        total: Some(limit),
        unit: UsageUnit::UsdCents,
        resets_at,
    })
}

/// 周进度窗口：只有周计划的账号才有，且只有百分比（provider 不给绝对数）。
pub(crate) fn weekly_window(period: Option<&CurrentPeriod>) -> Option<UsageWindow> {
    let period = period.filter(|period| period.weekly == Some(true))?;
    let used = period.usage_percent.unwrap_or(0.0).clamp(0.0, 100.0);
    Some(UsageWindow {
        key: "week".to_string(),
        label: "Weekly credits".to_string(),
        remaining_percent: parse::remaining_from_used(used),
        used: Some(used),
        total: Some(100.0),
        unit: UsageUnit::Percent,
        resets_at: period.end,
    })
}

/// 按需付费上限：0 表示没开，不是一条值得占位的额度。
pub(crate) fn on_demand_credits(payload: &Value) -> Option<Credits> {
    let roots = candidate_roots(payload);
    let cap = pick_cent_multi(&roots, &[&["onDemandCap"], &["on_demand_cap"]])
        .filter(|cap| *cap > 0.0)?;
    Some(Credits {
        label: "On-demand cap".to_string(),
        remaining: Some(cap),
        unlimited: false,
        unit: UsageUnit::UsdCents,
    })
}

/// GET /v1/settings 里的套餐显示名。品牌串 Grok 不是套餐名。
pub(crate) fn plan_from_settings(payload: &Value) -> Option<String> {
    let roots = candidate_roots(payload);
    for key in [
        "subscription_tier_display",
        "subscriptionTierDisplay",
        "subscription_tier",
        "subscriptionTier",
    ] {
        if let Some(name) = pick_value_multi(&roots, &[&[key]])
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty() && !name.eq_ignore_ascii_case("grok"))
        {
            return Some(name.to_string());
        }
    }
    None
}

/// settings 不可达时的兜底：访问令牌里的 tier 数字。
pub(crate) fn plan_from_access_token(access_token: &str) -> Option<String> {
    // tier 是数字 claim（不是字符串），所以直接读 claims 而不是 jwt_claim。
    let claims = crate::identity::jwt_claims(access_token)?;
    let tier = claims.get("tier")?;
    let tier = tier
        .as_i64()
        .or_else(|| tier.as_u64().map(|value| value as i64))
        .or_else(|| tier.as_str().and_then(|text| text.trim().parse().ok()))?;
    let name = match tier {
        0 => "Free",
        1 => "SuperGrok",
        2 => "X Basic",
        3 => "X Premium",
        4 => "X Premium+",
        5 => "SuperGrok Heavy",
        6 => "SuperGrok Lite",
        7 => "SuperGrok Plus",
        _ => return None,
    };
    Some(name.to_string())
}

/// 一次读取的完整快照：月度数字 + 周进度 + 按需上限。
pub(crate) fn snapshot_from(
    billing: &Value,
    period: Option<&CurrentPeriod>,
    plan: Option<String>,
    now: i64,
) -> UsageSnapshot {
    let mut windows = Vec::new();
    if let Some(monthly) = monthly_window(billing, period) {
        windows.push(monthly);
    }
    if let Some(weekly) = weekly_window(period) {
        windows.push(weekly);
    }
    UsageSnapshot {
        fetched_at: now,
        windows,
        plan,
        credits: on_demand_credits(billing),
        error: None,
        gcp_tos: None,
    }
}

/// 一次读取是否读到了可展示的东西：两个视图都空时不要给一张空白卡。
pub(crate) fn has_numbers(billing: &Value, period: Option<&CurrentPeriod>) -> bool {
    monthly_window(billing, period).is_some() || weekly_window(period).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// xAI 默认视图的真实形状。
    fn billing() -> Value {
        json!({
            "billingCycle": { "billingPeriodEnd": "2026-12-01T00:00:00Z" },
            "monthlyLimit": { "val": 99900 },
            "onDemandCap": { "val": 2500 },
            "usage": { "totalUsed": { "val": 12345 } }
        })
    }

    #[test]
    fn the_monthly_window_reads_remaining_from_used_and_limit() {
        let window = monthly_window(&billing(), None).expect("monthly");
        assert_eq!(window.key, "month");
        assert_eq!(window.unit, UsageUnit::UsdCents);
        assert_eq!(window.used, Some(12345.0));
        assert_eq!(window.total, Some(99900.0));
        // 12345/99900 已用 → 剩余约 87.64%
        assert!(
            (f64::from(window.remaining_percent) - 87.644).abs() < 0.5,
            "{}",
            window.remaining_percent
        );
        assert_eq!(window.resets_at, Some(1_796_083_200));
    }

    #[test]
    fn a_config_wrapped_payload_reads_the_same_numbers() {
        let wrapped = json!({ "config": billing() });
        let window = monthly_window(&wrapped, None).expect("monthly");
        assert_eq!(window.total, Some(99900.0));
        assert_eq!(
            on_demand_credits(&wrapped).and_then(|credits| credits.remaining),
            Some(2500.0)
        );
    }

    #[test]
    fn a_limit_less_account_gets_no_monthly_bar_instead_of_a_fake_one() {
        let no_limit = json!({ "usage": { "totalUsed": { "val": 500 } } });
        assert!(monthly_window(&no_limit, None).is_none());
        assert!(!has_numbers(&no_limit, None));
    }

    #[test]
    fn the_weekly_period_is_remaining_not_used() {
        let payload = json!({
            "currentPeriod": {
                "type": "USAGE_PERIOD_TYPE_WEEKLY",
                "start": "2026-11-09T00:00:00Z",
                "end": "2026-11-16T00:00:00Z"
            },
            "creditUsagePercent": 35
        });
        let period = current_period(&payload).expect("period");
        assert_eq!(period.weekly, Some(true));
        assert_eq!(period.usage_percent, Some(35.0));
        let window = weekly_window(Some(&period)).expect("weekly");
        assert_eq!(window.key, "week");
        assert_eq!(window.unit, UsageUnit::Percent);
        assert_eq!(window.used, Some(35.0));
        assert_eq!(
            window.remaining_percent, 65.0,
            "creditUsagePercent is used, not remaining"
        );
        assert_eq!(window.resets_at, Some(1_794_787_200));
    }

    #[test]
    fn a_monthly_plan_has_no_weekly_bar() {
        let payload = json!({
            "currentPeriod": { "type": "USAGE_PERIOD_TYPE_MONTHLY", "end": "2026-12-01T00:00:00Z" }
        });
        let period = current_period(&payload).expect("period");
        assert_eq!(period.weekly, Some(false));
        assert!(weekly_window(Some(&period)).is_none());
        // 月重置时间可以给月度窗口用（billingCycle 缺失时）。
        let no_cycle = json!({ "monthlyLimit": { "val": 1000 }, "usage": { "totalUsed": { "val": 250 } } });
        let window = monthly_window(&no_cycle, Some(&period)).expect("monthly");
        assert_eq!(window.resets_at, Some(1_796_083_200));
        assert_eq!(window.remaining_percent, 75.0);
    }

    #[test]
    fn an_unknown_period_type_is_still_a_reset_hint() {
        let payload = json!({ "currentPeriod": { "end": 1_796_083_200_i64 } });
        let period = current_period(&payload).expect("period");
        assert_eq!(period.weekly, None);
        assert_eq!(period.end, Some(1_796_083_200));
        assert!(current_period(&json!({})).is_none());
    }

    #[test]
    fn the_plan_name_never_repeats_the_brand() {
        assert_eq!(
            plan_from_settings(&json!({ "subscription_tier_display": "SuperGrok Heavy" })).as_deref(),
            Some("SuperGrok Heavy")
        );
        assert_eq!(plan_from_settings(&json!({ "subscription_tier_display": "Grok" })), None);
        assert_eq!(plan_from_settings(&json!({})), None);
    }

    #[test]
    fn the_token_tier_is_the_fallback_plan() {
        let token = crate::test_support::jwt_with_claims(json!({ "tier": 4 }));
        assert_eq!(plan_from_access_token(&token).as_deref(), Some("X Premium+"));
        assert_eq!(plan_from_access_token("opaque"), None);
    }

    #[test]
    fn a_snapshot_keeps_the_two_windows_and_the_cap_apart() {
        let credits_view = json!({
            "currentPeriod": { "type": "USAGE_PERIOD_TYPE_WEEKLY", "end": "2026-11-16T00:00:00Z" },
            "creditUsagePercent": 10
        });
        let period = current_period(&credits_view);
        let snapshot = snapshot_from(&billing(), period.as_ref(), Some("SuperGrok".to_string()), 7);
        assert_eq!(snapshot.fetched_at, 7);
        assert_eq!(snapshot.plan.as_deref(), Some("SuperGrok"));
        assert_eq!(snapshot.windows.len(), 2);
        assert_eq!(snapshot.windows[0].key, "month");
        assert_eq!(snapshot.windows[1].key, "week");
        assert_eq!(snapshot.credits.expect("cap").remaining, Some(2500.0));
        assert_eq!(snapshot.gcp_tos, None);
    }

    #[test]
    fn a_zero_on_demand_cap_is_not_a_credit_chip() {
        let payload = json!({ "onDemandCap": { "val": 0 } });
        assert!(on_demand_credits(&payload).is_none());
    }
}
