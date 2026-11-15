//! ZCode 配额响应的纯解析（夹具直接驱动）。
//!
//! 三份响应对应三类信息：
//!
//! * 计费余额（zcode.z.ai 的 balance）：plans 里 active 的那条给套餐名，
//!   balances 里的每一行给一个额度窗口（总数 / 已用 / 剩余 / 期末）。
//! * 上游配额上限（quota/limit）：没有 balances 时的兜底窗口。
//! * 用户信息（profile）：套餐名与账号 id 的最后兜底。
//!
//! 数字语义：快照里的窗口一律记"剩余"，所以 remaining = 1 - used/total。
//! 余额行的 used 可能只给了 total 与 remaining，这时由两者相减得出（别反过来算）。

use serde_json::Value;

use crate::usage::snapshot::{UsageSnapshot, UsageUnit, UsageWindow};

use super::parse;

/// 响应可能裹在 data 里。
fn document(body: &Value) -> &Value {
    body.get("data")
        .filter(|value| value.is_object())
        .unwrap_or(body)
}

/// 一条字符串字段（按顺序找第一个非空）。
fn text_in(node: &Value, paths: &[&[&str]]) -> Option<String> {
    for path in paths {
        let mut current = node;
        let mut found = true;
        for key in *path {
            match current.get(*key) {
                Some(next) => current = next,
                None => {
                    found = false;
                    break;
                }
            }
        }
        if found
            && let Some(text) = current
                .as_str()
                .map(str::trim)
                .filter(|text| !text.is_empty())
        {
            return Some(text.to_string());
        }
    }
    None
}

/// 一条数值字段：数字或数字字符串，非负。
fn units(node: &Value, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|key| {
        let value = node.get(*key)?;
        let number = match value {
            Value::Number(number) => number.as_f64(),
            Value::String(text) => text.trim().parse::<f64>().ok(),
            _ => None,
        }?;
        (number.is_finite() && number >= 0.0).then_some(number)
    })
}

fn timestamp(node: &Value, keys: &[&str]) -> Option<i64> {
    let raw = units(node, keys)?;
    let seconds = if raw > 10_000_000_000.0 {
        raw / 1000.0
    } else {
        raw
    };
    (seconds > 0.0).then_some(seconds as i64)
}

/// 套餐名：billing 里 active 的 plan → profile 里的 plan 字段。
pub(crate) fn plan_name(profile: Option<&Value>, billing: Option<&Value>) -> Option<String> {
    if let Some(billing) = billing
        && let Some(plans) = document(billing).get("plans").and_then(Value::as_array)
    {
        let plan = plans
            .iter()
            .find(|plan| plan.get("status").and_then(Value::as_str) == Some("active"))
            .or_else(|| plans.first());
        if let Some(name) = plan.and_then(|plan| {
            text_in(plan, &[&["name"], &["plan_id"], &["plan_name"]])
        }) {
            return Some(name);
        }
    }
    profile.and_then(|profile| {
        text_in(
            document(profile),
            &[
                &["plan_type"],
                &["planName"],
                &["plan_name"],
                &["plan"],
                &["subscription", "name"],
            ],
        )
    })
}

/// 一条余额行 → 一个窗口。没有"已用"也没有(total, remaining)时跳过。
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
        (remaining <= total).then_some(total - remaining)
    })?;
    let label = text_in(
        item,
        &[
            &["show_name"],
            &["showName"],
            &["model"],
            &["model_name"],
            &["modelName"],
            &["name"],
            &["title"],
        ],
    )
    .unwrap_or_else(|| {
        if index == 0 {
            "Quota".to_string()
        } else {
            format!("Quota {}", index + 1)
        }
    });
    Some(UsageWindow {
        key: format!("quota-{index}"),
        label,
        remaining_percent: match total {
            Some(total) => parse::remaining_from_amounts(used, total),
            None => 0.0,
        },
        used: Some(used),
        total,
        unit: UsageUnit::Count,
        resets_at: timestamp(
            item,
            &["period_end", "expires_at", "periodEnd", "expiresAt"],
        ),
    })
}

/// billing 的 balances：每一行一个窗口。
pub(crate) fn billing_windows(billing: &Value) -> Vec<UsageWindow> {
    let Some(balances) = document(billing).get("balances").and_then(Value::as_array) else {
        return Vec::new();
    };
    balances
        .iter()
        .enumerate()
        .filter_map(|(index, item)| balance_window(item, index))
        .collect()
}

/// 上游 quota/limit 的兜底窗口：limits 里出现的第一组 (used, total)。
pub(crate) fn limit_windows(limits: &Value) -> Vec<UsageWindow> {
    let doc = document(limits);
    let used = units(doc, &["used", "used_units", "usedUnits", "currentUsage"]);
    let total = units(doc, &["total", "total_units", "totalUnits", "limit", "quota"]);
    let Some(used) = used else {
        return Vec::new();
    };
    let label = text_in(doc, &[&["name"], &["model"], &["title"]])
        .unwrap_or_else(|| "Quota".to_string());
    vec![UsageWindow {
        key: "limit".to_string(),
        label,
        remaining_percent: match total {
            Some(total) => parse::remaining_from_amounts(used, total),
            None => 0.0,
        },
        used: Some(used),
        total,
        unit: UsageUnit::Count,
        resets_at: timestamp(doc, &["reset_at", "resetAt", "expires_at"]),
    }]
}

/// 完整快照：billing 的窗口优先，limits 只在没有窗口时补位。
pub(crate) fn snapshot_from(
    profile: &Value,
    billing: Option<&Value>,
    limits: Option<&Value>,
    now: i64,
) -> UsageSnapshot {
    let plan = plan_name(Some(profile), billing);
    let mut windows = billing.map(billing_windows).unwrap_or_default();
    if windows.is_empty()
        && let Some(limits) = limits
    {
        windows = limit_windows(limits);
    }
    UsageSnapshot {
        fetched_at: now,
        windows,
        plan,
        credits: None,
        error: None,
        gcp_tos: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn billing() -> Value {
        json!({
            "code": 0,
            "data": {
                "plans": [{ "name": "GLM Coding Max", "status": "active" }],
                "balances": [
                    {
                        "show_name": "5 小时额度",
                        "total_units": 100,
                        "used_units": 25,
                        "period_end": 1_796_083_200
                    },
                    {
                        "show_name": "周额度",
                        "total_units": 1000,
                        "remaining_units": 800
                    }
                ]
            }
        })
    }

    #[test]
    fn the_balance_rows_become_remaining_windows() {
        let windows = billing_windows(&billing());
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].key, "quota-0");
        assert_eq!(windows[0].label, "5 小时额度");
        assert_eq!(windows[0].used, Some(25.0));
        assert_eq!(windows[0].total, Some(100.0));
        assert_eq!(windows[0].remaining_percent, 75.0, "remaining, not used");
        assert_eq!(windows[0].resets_at, Some(1_796_083_200));
        // 只给了 total + remaining 的那一行，used 由两者相减得出。
        assert_eq!(windows[1].used, Some(200.0));
        assert_eq!(windows[1].remaining_percent, 80.0);
    }

    #[test]
    fn the_plan_name_is_the_active_plan_then_the_profile() {
        assert_eq!(
            plan_name(Some(&json!({})), Some(&billing())).as_deref(),
            Some("GLM Coding Max")
        );
        assert_eq!(
            plan_name(Some(&json!({ "data": { "plan_type": "lite" } })), None).as_deref(),
            Some("lite")
        );
        assert_eq!(plan_name(None, None), None);
    }

    #[test]
    fn the_limit_view_is_only_the_fallback() {
        let limits = json!({ "data": { "used": 30, "total": 120, "reset_at": 1_796_083_200 } });
        assert_eq!(limit_windows(&limits).len(), 1);
        assert_eq!(limit_windows(&limits)[0].remaining_percent, 75.0);

        let snapshot = snapshot_from(&json!({}), Some(&billing()), Some(&limits), 7);
        assert_eq!(snapshot.windows.len(), 2, "billing wins");
        assert_eq!(snapshot.fetched_at, 7);

        let fallback = snapshot_from(&json!({}), None, Some(&limits), 7);
        assert_eq!(fallback.windows.len(), 1);
        assert_eq!(fallback.windows[0].key, "limit");
    }

    #[test]
    fn a_row_without_any_number_is_skipped() {
        let empty = json!({ "data": { "balances": [{ "show_name": "无数据" }] } });
        assert!(billing_windows(&empty).is_empty());
        // 只给了 remaining 没有 total 的行也没有 percent 可算，但窗口仍然存在。
        let remaining_only = json!({ "data": { "balances": [{ "remaining_units": 5 }] } });
        assert!(billing_windows(&remaining_only).is_empty(), "used is required");
    }
}
