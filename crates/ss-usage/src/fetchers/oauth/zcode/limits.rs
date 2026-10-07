//! GLM Coding Plan windows from `GET /api/monitor/usage/quota/limit`.
//!
//! The official quota strip is three bars: the 5-hour pool, the weekly pool,
//! and ZCode MCP. `percentage` is the consumed share. `currentValue` /
//! `usage` are used / ceiling when the row carries absolute units.
//! A duplicated window is omitted rather than guessed.

use serde_json::Value;

use crate::subscription::{SubscriptionUsage, UsageUnit, UsageWindow};

const SESSION_LABEL: &str = "Five Hour Limit";
const WEEKLY_LABEL: &str = "Weekly Limit";
const MCP_LABEL: &str = "ZCode MCP";

pub(super) struct PlanLimits {
    pub hourly: Option<UsageWindow>,
    pub weekly: Option<UsageWindow>,
    pub monthly: Option<UsageWindow>,
    pub level: Option<String>,
}

pub(super) fn parse(body: &Value) -> Option<PlanLimits> {
    if !envelope_ok(body) {
        return None;
    }
    let data = body
        .get("data")
        .filter(|value| value.is_object())
        .unwrap_or(body);
    let level = data
        .get("level")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string);
    let (hourly, weekly, monthly) = match data.get("limits").and_then(Value::as_array) {
        Some(rows) => parse_rows(rows),
        None => parse_legacy(data),
    };
    if hourly.is_none() && weekly.is_none() && monthly.is_none() && level.is_none() {
        return None;
    }
    Some(PlanLimits {
        hourly,
        weekly,
        monthly,
        level,
    })
}

/// ZCode MCP is `GET /api/v1/mcp/usage`, not a `TIME_LIMIT` row.
/// `total_usage.limit` is the ceiling. `percentage` stays the consumed share.
pub(super) fn mcp_window(body: &Value) -> Option<UsageWindow> {
    if code_number(body.get("code")?) != Some(0) {
        return None;
    }
    let usage = body.get("data")?.get("total_usage")?;
    let total = json_i64(usage.get("limit")?)?;
    if total <= 0 {
        return None;
    }
    let used = usage.get("used").and_then(json_i64).unwrap_or(0);
    let remaining = usage
        .get("remaining")
        .and_then(json_i64)
        .unwrap_or(0)
        .clamp(0, total);
    let percent = (100.0 - remaining as f64 / total as f64 * 100.0)
        .round()
        .clamp(0.0, 100.0) as i32;
    Some(UsageWindow {
        label: MCP_LABEL.to_string(),
        used,
        total: Some(total),
        percent: Some(percent),
        reset_at: reset_seconds(
            body.get("data")
                .and_then(|data| data.get("next_refresh_at")),
        ),
        breakdown: Vec::new(),
        unit: UsageUnit::Count,
    })
}

/// Keep a billing balance only as the 5-hour bar's absolute counts when it
/// is one row and agrees with that bar. Otherwise the three plan bars replace
/// the collapsed Quota bar.
pub(super) fn apply(usage: &mut SubscriptionUsage, limits: PlanLimits) {
    if usage.plan_name.is_none() {
        usage.plan_name = limits.level;
    }
    if limits.hourly.is_none() && limits.weekly.is_none() && limits.monthly.is_none() {
        return;
    }
    let billing = usage.monthly.take();
    let mut hourly = limits.hourly;
    if let Some(billing) = billing {
        if let Some(slot) = hourly.as_mut() {
            enrich_session(slot, &billing);
        } else {
            hourly = Some(billing);
        }
    }
    usage.hourly = hourly;
    usage.weekly = limits.weekly;
    usage.monthly = limits.monthly;
}

fn enrich_session(session: &mut UsageWindow, billing: &UsageWindow) {
    if !billing.breakdown.is_empty() {
        return;
    }
    let Some(total) = billing.total.filter(|total| *total > 100) else {
        return;
    };
    let billing_pct = billing
        .percent
        .unwrap_or_else(|| percent(billing.used, total));
    if session
        .percent
        .is_some_and(|pct| (pct - billing_pct).abs() > 1)
    {
        return;
    }
    session.used = billing.used;
    session.total = Some(total);
    session.percent = Some(billing_pct.clamp(0, 100));
    if session.reset_at.is_none() {
        session.reset_at = billing.reset_at;
    }
}

fn parse_rows(
    rows: &[Value],
) -> (
    Option<UsageWindow>,
    Option<UsageWindow>,
    Option<UsageWindow>,
) {
    let mut session = Slot::default();
    let mut weekly = Slot::default();
    let mut mcp = Slot::default();
    for item in rows {
        let Some(kind) = classify(item) else {
            continue;
        };
        let Some(window) = row_window(item, kind.label()) else {
            continue;
        };
        match kind {
            Kind::Session => session.push(window),
            Kind::Weekly => weekly.push(window),
            Kind::Mcp => mcp.push(window),
        }
    }
    (session.window, weekly.window, mcp.window)
}

fn parse_legacy(
    data: &Value,
) -> (
    Option<UsageWindow>,
    Option<UsageWindow>,
    Option<UsageWindow>,
) {
    let nested = data.get("quota").filter(|value| value.is_object());
    (
        legacy_window(
            data,
            nested,
            &["fiveHourPercent", "fiveHourUsage", "fiveHourUsed"],
            SESSION_LABEL,
            "fiveHourResetAt",
        ),
        legacy_window(
            data,
            nested,
            &["weeklyPercent", "weeklyUsage", "weeklyUsed"],
            WEEKLY_LABEL,
            "weeklyResetAt",
        ),
        legacy_window(
            data,
            nested,
            &["monthlyPercent", "mcpPercent", "monthlyMCPUsage"],
            MCP_LABEL,
            "monthlyResetAt",
        ),
    )
}

fn legacy_window(
    data: &Value,
    nested: Option<&Value>,
    keys: &[&str],
    label: &str,
    reset_key: &str,
) -> Option<UsageWindow> {
    let percent = keys.iter().find_map(|key| {
        data.get(*key)
            .and_then(percent_points)
            .or_else(|| nested.and_then(|quota| quota.get(*key).and_then(percent_points)))
    })?;
    Some(percent_window(
        label,
        percent,
        reset_seconds(data.get(reset_key)),
    ))
}

#[derive(Clone, Copy)]
enum Kind {
    Session,
    Weekly,
    Mcp,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Self::Session => SESSION_LABEL,
            Self::Weekly => WEEKLY_LABEL,
            Self::Mcp => MCP_LABEL,
        }
    }
}

fn classify(item: &Value) -> Option<Kind> {
    match item.get("type").and_then(Value::as_str).unwrap_or("") {
        "TIME_LIMIT" => Some(Kind::Mcp),
        "TOKENS_LIMIT" | "CREDIT_LIMIT" => {
            match (field_i64(item, &["unit"]), field_i64(item, &["number"])) {
                (Some(3), None | Some(5)) => Some(Kind::Session),
                (Some(6), None | Some(1)) => Some(Kind::Weekly),
                _ => None,
            }
        }
        _ => None,
    }
}

fn row_window(item: &Value, label: &str) -> Option<UsageWindow> {
    let percent = used_percent(item)?;
    let (used, total) = absolute_units(item, percent);
    Some(UsageWindow {
        label: label.to_string(),
        used,
        total,
        percent: Some(percent),
        reset_at: reset_seconds(
            item.get("nextResetTime")
                .or_else(|| item.get("next_reset_time")),
        ),
        breakdown: tool_counts(item),

        unit: UsageUnit::Count,
    })
}

fn percent_window(label: &str, percent: i32, reset_at: Option<i64>) -> UsageWindow {
    UsageWindow {
        label: label.to_string(),
        used: i64::from(percent),
        total: Some(100),
        percent: Some(percent),
        reset_at,
        breakdown: Vec::new(),
        unit: UsageUnit::Count,
    }
}

fn used_percent(item: &Value) -> Option<i32> {
    if let Some(percent) = item.get("percentage").and_then(percent_points) {
        return Some(percent);
    }
    let used = field_i64(item, &["currentValue", "current_value"])?;
    let total = field_i64(item, &["usage"])?;
    (total > 0).then(|| percent(used, total))
}

fn absolute_units(item: &Value, percent: i32) -> (i64, Option<i64>) {
    match (
        field_i64(item, &["currentValue", "current_value"]),
        field_i64(item, &["usage"]),
    ) {
        (Some(used), Some(total)) if total > 0 && !(total == 100 && used == i64::from(percent)) => {
            (used, Some(total))
        }
        _ => (i64::from(percent), Some(100)),
    }
}

fn tool_counts(item: &Value) -> Vec<UsageWindow> {
    let Some(rows) = item
        .get("usageDetails")
        .or_else(|| item.get("usage_details"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(|row| {
            let label = row
                .get("modelCode")
                .or_else(|| row.get("model_code"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|text| !text.is_empty())?;
            let used = field_i64(row, &["usage"])?;
            (used > 0).then(|| UsageWindow {
                label: label.to_string(),
                used,
                total: None,
                percent: None,
                reset_at: None,
                breakdown: Vec::new(),

                unit: UsageUnit::Count,
            })
        })
        .collect()
}

#[derive(Default)]
struct Slot {
    window: Option<UsageWindow>,
    duplicate: bool,
}

impl Slot {
    fn push(&mut self, window: UsageWindow) {
        if self.duplicate {
            return;
        }
        if self.window.is_some() {
            self.window = None;
            self.duplicate = true;
            return;
        }
        self.window = Some(window);
    }
}

fn envelope_ok(body: &Value) -> bool {
    if body.get("success").and_then(Value::as_bool) == Some(false) {
        return false;
    }
    match body.get("code") {
        None | Some(Value::Null) => true,
        Some(value) => matches!(code_number(value), Some(0 | 200)),
    }
}

fn code_number(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number.as_i64(),
        Value::String(text) => match text.trim() {
            "" | "0" => Some(0),
            "200" => Some(200),
            _ => None,
        },
        _ => None,
    }
}

fn percent(used: i64, total: i64) -> i32 {
    let value = ((used as f64 / total as f64) * 100.0).round();
    value.clamp(0.0, 100.0) as i32
}

fn percent_points(value: &Value) -> Option<i32> {
    let raw = json_f64(value)?;
    (0.0..=101.0).contains(&raw).then(|| {
        let rounded = raw.round();
        rounded.clamp(0.0, 100.0) as i32
    })
}

fn reset_seconds(value: Option<&Value>) -> Option<i64> {
    let raw = value.and_then(json_i64)?;
    if raw <= 0 {
        return None;
    }
    let seconds = if raw > 10_000_000_000 {
        raw / 1000
    } else {
        raw
    };
    Some(seconds)
}

fn field_i64(item: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter()
        .find_map(|key| item.get(*key).and_then(json_i64))
}

fn json_i64(value: &Value) -> Option<i64> {
    let number = json_f64(value)?;
    (number >= 0.0 && number <= i64::MAX as f64 && number.fract() == 0.0).then_some(number as i64)
}

fn json_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64().filter(|number| number.is_finite()),
        Value::String(text) => text
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|number| number.is_finite()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn quota_bar(label: &str, used: i64, total: i64) -> UsageWindow {
        UsageWindow {
            label: label.to_string(),
            used,
            total: Some(total),
            percent: Some(percent(used, total)),
            reset_at: Some(1_700_000_000),
            breakdown: Vec::new(),
            unit: UsageUnit::Count,
        }
    }

    #[test]
    fn three_plan_bars_keep_absolute_counts_on_the_five_hour_window() {
        let body = json!({
            "code": 200,
            "success": true,
            "data": {
                "level": "max",
                "limits": [
                    {"type": "TIME_LIMIT", "unit": 5, "number": 1, "usage": 4000, "currentValue": 0, "remaining": 4000, "percentage": 0, "nextResetTime": 1788073095998_i64,
                     "usageDetails": [
                        {"modelCode": "search-prime", "usage": 3},
                        {"modelCode": "web-reader", "usage": 0}
                     ]},
                    {"type": "TOKENS_LIMIT", "unit": 6, "number": 1, "percentage": 38, "nextResetTime": 1787641095989_i64},
                    {"type": "TOKENS_LIMIT", "unit": 3, "number": 5, "percentage": 0}
                ]
            }
        });
        let limits = parse(&body).unwrap();
        let mut usage = SubscriptionUsage {
            subscription_id: "sub".into(),
            plan_name: Some("ZCODE TRUST BUILD".into()),
            monthly: Some(quota_bar("Quota", 0, 100_000_000)),
            ..SubscriptionUsage::default()
        };
        apply(&mut usage, limits);
        let session = usage.hourly.unwrap();
        assert_eq!(session.label, "Five Hour Limit");
        assert_eq!(session.used, 0);
        assert_eq!(session.total, Some(100_000_000));
        assert_eq!(session.percent, Some(0));
        assert_eq!(session.reset_at, Some(1_700_000_000));
        let weekly = usage.weekly.unwrap();
        assert_eq!(weekly.label, "Weekly Limit");
        assert_eq!(weekly.percent, Some(38));
        assert_eq!(weekly.reset_at, Some(1_787_641_095));
        let mcp = usage.monthly.unwrap();
        assert_eq!(mcp.label, "ZCode MCP");
        assert_eq!(mcp.used, 0);
        assert_eq!(mcp.total, Some(4000));
        assert_eq!(mcp.percent, Some(0));
        assert_eq!(mcp.breakdown.len(), 1);
        assert_eq!(mcp.breakdown[0].label, "search-prime");
        assert_eq!(mcp.breakdown[0].used, 3);
        assert_eq!(usage.plan_name.as_deref(), Some("ZCODE TRUST BUILD"));
    }

    #[test]
    fn disagreed_token_pool_does_not_overwrite_the_five_hour_percent() {
        let body = json!({
            "code": 0,
            "data": {"limits": [
                {"type": "CREDIT_LIMIT", "unit": 3, "number": 5, "percentage": 100, "usage": 12000, "currentValue": 12000},
                {"type": "CREDIT_LIMIT", "unit": 6, "number": 1, "usage": 60000, "currentValue": 22800, "percentage": 38}
            ], "level": "pro"}
        });
        let mut usage = SubscriptionUsage {
            monthly: Some(quota_bar("Quota", 0, 100_000_000)),
            ..SubscriptionUsage::default()
        };
        apply(&mut usage, parse(&body).unwrap());
        let session = usage.hourly.unwrap();
        assert_eq!(session.percent, Some(100));
        assert_eq!(session.total, Some(12000));
        assert_eq!(usage.weekly.unwrap().used, 22800);
        assert_eq!(usage.plan_name.as_deref(), Some("pro"));
        assert!(usage.monthly.is_none());
    }

    #[test]
    fn duplicates_unknown_units_and_failed_envelopes_drop_windows() {
        let swapped = parse(&json!({"data": {"limits": [
            {"type": "TOKENS_LIMIT", "unit": 4, "percentage": 99},
            {"type": "TOKENS_LIMIT", "unit": 6, "number": 1, "percentage": 15},
            {"type": "TOKENS_LIMIT", "unit": 3, "percentage": 4}
        ]}, "success": true}))
        .unwrap();
        assert_eq!(swapped.hourly.unwrap().percent, Some(4));
        assert_eq!(swapped.weekly.unwrap().percent, Some(15));

        let mixed = parse(&json!({"data": {"limits": [
            {"type": "TOKENS_LIMIT", "unit": 3, "number": 5, "percentage": 1},
            {"type": "TOKENS_LIMIT", "unit": 3, "number": 5, "percentage": 9},
            {"type": "TOKENS_LIMIT", "unit": 6, "number": 1, "percentage": 15}
        ]}}))
        .unwrap();
        assert!(mixed.hourly.is_none());
        assert_eq!(mixed.weekly.unwrap().percent, Some(15));
        assert!(
            parse(&json!({"data": {"limits": [
                {"type": "TIME_LIMIT", "percentage": 2},
                {"type": "TIME_LIMIT", "percentage": 8}
            ]}}))
            .is_none()
        );

        assert!(parse(&json!({"code": 500, "success": false, "msg": "no"})).is_none());
        assert!(parse(&json!({"data": {"limits": []}})).is_none());
    }

    #[test]
    fn legacy_fields_apply_only_when_limits_are_absent() {
        let legacy = parse(&json!({
            "quota": {"fiveHourPercent": 12.4},
            "weeklyPercent": 40,
            "monthlyMCPUsage": 7
        }))
        .unwrap();
        assert_eq!(legacy.hourly.unwrap().percent, Some(12));
        assert_eq!(legacy.weekly.unwrap().percent, Some(40));
        let monthly = legacy.monthly.unwrap();
        assert_eq!(monthly.label, "ZCode MCP");
        assert_eq!(monthly.percent, Some(7));

        let present = parse(&json!({
            "limits": [{"type": "TIME_LIMIT", "percentage": 3}],
            "fiveHourPercent": 90
        }))
        .unwrap();
        assert!(present.hourly.is_none());
        assert_eq!(present.monthly.unwrap().percent, Some(3));
    }

    #[test]
    fn mcp_usage_is_its_own_window() {
        let mcp = mcp_window(&json!({
            "code": 0,
            "data": {
                "next_refresh_at": 1_791_388_800,
                "total_usage": {"used": 0, "limit": 1000, "remaining": 1000}
            }
        }))
        .unwrap();
        assert_eq!(mcp.label, "ZCode MCP");
        assert_eq!(mcp.used, 0);
        assert_eq!(mcp.total, Some(1000));
        assert_eq!(mcp.percent, Some(0));
        assert_eq!(mcp.reset_at, Some(1_791_388_800));
        let partial = mcp_window(&json!({
            "code": 0,
            "data": {"total_usage": {"used": 250, "limit": 1000, "remaining": 750}}
        }))
        .unwrap();
        assert_eq!(partial.percent, Some(25));
        assert!(
            mcp_window(&json!({"code": 7, "data": {"total_usage": {"used": 0, "limit": 1, "remaining": 1}}})).is_none()
        );
        assert!(mcp_window(&json!({"code": 0, "data": {}})).is_none());
    }
}
