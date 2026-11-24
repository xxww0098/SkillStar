//! Kiro（CodeWhisperer getUsageLimits）响应解析，纯函数驱动。
//!
//! 两家额度、两种窗口，Cockpit Tools 的映法：
//!
//! * Credits（月度）→ 本 crate 的 month 窗口：额度单位是"积分"（Count）。
//! * Free trial（周/试用期）→ 本 crate 的 trial 窗口，重置点看 freeTrialInfo.expiryDate。
//!
//! 缺金额就不画那条窗口：快照的 remaining_percent 是必填，编一个 0 或 100 都是假话。
//! resetOn/resetAt 只挂到已经存在的窗口上，不凭空造窗口。

use serde_json::Value;

use crate::usage::snapshot::{UsageSnapshot, UsageUnit, UsageWindow};

use super::parse;

/// Credits 窗口的标签（与 Cockpit Tools 一致，UI 直接显示）。
pub(crate) const LABEL_CREDITS: &str = "Credits";
/// 试用期窗口的标签。
pub(crate) const LABEL_FREE_TRIAL: &str = "Free trial";

/// 用量响应里可能有 kiro.resourceNotifications.usageState 包装，也可能直接是根。
fn usage_root(body: &Value) -> &Value {
    body.get("kiro.resourceNotifications.usageState")
        .or_else(|| body.get("usageState"))
        .unwrap_or(body)
}

/// usageBreakdownList 里挑 credit 那一条，其次第一条。
fn breakdown(root: &Value) -> Option<&Value> {
    let list = root
        .get("usageBreakdownList")
        .or_else(|| root.get("usageBreakdowns"))
        .and_then(Value::as_array)?;
    list.iter()
        .find(|item| {
            item.get("type")
                .and_then(Value::as_str)
                .is_some_and(|kind| kind.eq_ignore_ascii_case("credit"))
        })
        .or_else(|| list.first())
}

/// 数字字段：数字或数字字符串（AWS 会回字符串化的浮点）。
fn number_at(node: Option<&Value>, paths: &[&[&str]]) -> Option<f64> {
    let node = node?;
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
        if found && let Some(value) = parse::number(Some(current)) {
            return Some(value);
        }
    }
    None
}

/// 一组 (used, total)：两边都没有就是这条额度不存在。
fn amounts(
    root: &Value,
    breakdown: Option<&Value>,
    used_paths: &[&[&str]],
    total_paths: &[&[&str]],
    trial: bool,
) -> Option<(f64, f64)> {
    let scoped = if trial {
        breakdown.and_then(|item| {
            item.get("freeTrialInfo")
                .or_else(|| item.get("freeTrialUsage"))
        })
    } else {
        breakdown
    };
    let used = number_at(Some(root), used_paths)
        .or_else(|| number_at(scoped, &[&["currentUsageWithPrecision"], &["currentUsage"], &["used"], &["usedCredits"]]));
    let total = number_at(Some(root), total_paths).or_else(|| {
        number_at(
            scoped,
            &[&["usageLimitWithPrecision"], &["usageLimit"], &["limit"], &["total"], &["totalCredits"]],
        )
    });
    match (used, total) {
        (None, None) => None,
        (used, total) => Some((used.unwrap_or(0.0).max(0.0), total.unwrap_or(0.0).max(0.0))),
    }
}

fn timestamp_at(node: Option<&Value>, keys: &[&str]) -> Option<i64> {
    let node = node?;
    keys.iter().find_map(|key| {
        let value = node.get(*key)?;
        if let Some(raw) = value.as_i64() {
            return normalize_epoch(raw);
        }
        if let Some(raw) = value.as_u64() {
            return normalize_epoch(raw as i64);
        }
        let text = value.as_str()?.trim();
        if let Ok(raw) = text.parse::<i64>() {
            return normalize_epoch(raw);
        }
        chrono::DateTime::parse_from_rfc3339(text)
            .ok()
            .map(|at| at.timestamp())
    })
}

fn normalize_epoch(raw: i64) -> Option<i64> {
    if raw <= 0 {
        return None;
    }
    Some(if raw > 10_000_000_000 {
        raw / 1000
    } else {
        raw
    })
}

fn window(key: &str, label: &str, used: f64, total: f64, resets_at: Option<i64>) -> UsageWindow {
    UsageWindow {
        key: key.to_string(),
        label: label.to_string(),
        remaining_percent: parse::remaining_from_amounts(used, total),
        used: Some(used),
        total: Some(total),
        unit: UsageUnit::Count,
        resets_at,
    }
}

/// 套餐名：planName / subscriptionInfo 的标题 / breakdown 的显示名。
pub(crate) fn plan_name(body: &Value) -> Option<String> {
    let root = usage_root(body);
    let breakdown = breakdown(root);
    parse::text_in(root, &["planName", "currentPlanName"])
        .or_else(|| parse::text_at(Some(root), "/subscriptionInfo/subscriptionTitle"))
        .or_else(|| parse::text_at(Some(root), "/subscriptionInfo/subscriptionName"))
        .or_else(|| breakdown.and_then(|item| parse::text_in(item, &["displayName", "displayNamePlural"])))
}

/// 一次读取的窗口：Credits（月度）与 Free trial（试用期）。
pub(crate) fn windows(body: &Value) -> Vec<UsageWindow> {
    let root = usage_root(body);
    let breakdown = breakdown(root);
    let reset = timestamp_at(Some(root), &["resetAt", "resetTime", "resetOn", "nextDateReset"])
        .or_else(|| timestamp_at(breakdown, &["resetDate", "resetAt", "resetOn"]));
    let mut windows = Vec::new();
    if let Some((used, total)) = amounts(
        root,
        breakdown,
        &[
            &["estimatedUsage", "used"],
            &["estimatedUsage", "creditsUsed"],
            &["credits", "used"],
            &["usedCredits"],
        ],
        &[
            &["estimatedUsage", "total"],
            &["estimatedUsage", "creditsTotal"],
            &["credits", "total"],
            &["totalCredits"],
        ],
        false,
    ) && total > 0.0
    {
        windows.push(window("month", LABEL_CREDITS, used, total, reset));
    }
    if let Some((used, total)) = amounts(
        root,
        breakdown,
        &[&["bonusCredits", "used"], &["bonus", "used"]],
        &[&["bonusCredits", "total"], &["bonus", "total"]],
        true,
    ) && total > 0.0
    {
        let expiry = breakdown
            .and_then(|item| item.get("freeTrialInfo").or_else(|| item.get("freeTrialUsage")))
            .and_then(|trial| timestamp_at(Some(trial), &["expiryDate", "freeTrialExpiry"]))
            .or(if windows.is_empty() { reset } else { None });
        windows.push(window("trial", LABEL_FREE_TRIAL, used, total, expiry));
    }
    windows
}

/// 响应里的 profileArn（下次读额度要用，缺了就得用户先跑一次 Kiro）。
pub(crate) fn profile_arn(body: &Value) -> Option<String> {
    let root = usage_root(body);
    parse::text_in(root, &["profileArn", "profile_arn", "arn"])
}

/// 响应里带回的身份（userInfo.email / userInfo.userId 或根级回退）。
pub(crate) fn identity_of(body: &Value) -> Option<crate::oauth::Identity> {
    let root = usage_root(body);
    let user_info = root.get("userInfo").or_else(|| root.get("user"));
    let email = user_info
        .and_then(|node| parse::text_in(node, &["email", "userEmail"]))
        .or_else(|| parse::text_in(root, &["email", "userEmail"]));
    let account_id = user_info
        .and_then(|node| parse::text_in(node, &["userId", "user_id", "sub", "id"]))
        .or_else(|| parse::text_in(root, &["userId", "user_id", "sub"]));
    if email.is_none() && account_id.is_none() {
        return None;
    }
    Some(crate::oauth::Identity {
        display_name: email.clone(),
        email,
        plan: None,
        account_id,
    })
}

/// 完整快照。
pub(crate) fn snapshot_from(body: &Value, now: i64) -> UsageSnapshot {
    UsageSnapshot {
        fetched_at: now,
        windows: windows(body),
        plan: plan_name(body),
        credits: None,
        error: None,
        gcp_tos: None,
        reset_credits: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn payload() -> Value {
        json!({
            "usageBreakdownList": [{
                "type": "CREDIT",
                "currentUsageWithPrecision": 120,
                "usageLimitWithPrecision": 500,
                "freeTrialInfo": {
                    "currentUsageWithPrecision": 10,
                    "usageLimitWithPrecision": 50,
                    "expiryDate": "2026-12-01T00:00:00Z"
                },
                "displayName": "Kiro Pro"
            }],
            "resetOn": "2026-12-01T00:00:00Z"
        })
    }

    #[test]
    fn credits_and_the_free_trial_are_two_windows_with_remaining_percentages() {
        let windows = windows(&payload());
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].key, "month");
        assert_eq!(windows[0].label, LABEL_CREDITS);
        assert_eq!(windows[0].used, Some(120.0));
        assert_eq!(windows[0].total, Some(500.0));
        assert_eq!(windows[0].remaining_percent, 76.0, "remaining, not used");
        assert_eq!(windows[0].resets_at, Some(1_796_083_200));

        assert_eq!(windows[1].key, "trial");
        assert_eq!(windows[1].label, LABEL_FREE_TRIAL);
        assert_eq!(windows[1].used, Some(10.0));
        assert_eq!(windows[1].remaining_percent, 80.0);
        assert_eq!(windows[1].resets_at, Some(1_796_083_200));
    }

    #[test]
    fn the_usage_state_wrapper_is_read_the_same_way() {
        let wrapped = json!({ "kiro.resourceNotifications.usageState": payload() });
        assert_eq!(windows(&wrapped).len(), 2);
        assert_eq!(plan_name(&wrapped).as_deref(), Some("Kiro Pro"));
    }

    #[test]
    fn a_missing_amount_omits_the_window_instead_of_inventing_a_zero() {
        let only_used = json!({
            "usageBreakdownList": [{ "type": "CREDIT", "currentUsageWithPrecision": 5 }]
        });
        assert!(windows(&only_used).is_empty());

        let zero_limit = json!({
            "usageBreakdownList": [{
                "type": "CREDIT",
                "currentUsageWithPrecision": 0,
                "usageLimitWithPrecision": 0
            }]
        });
        assert!(windows(&zero_limit).is_empty(), "a zero allowance is not a window");
    }

    #[test]
    fn the_plan_name_comes_from_the_plan_or_the_breakdown() {
        assert_eq!(
            plan_name(&json!({ "planName": "KIRO PRO+" })).as_deref(),
            Some("KIRO PRO+")
        );
        assert_eq!(
            plan_name(&json!({ "subscriptionInfo": { "subscriptionTitle": "Kiro Pro" } })).as_deref(),
            Some("Kiro Pro")
        );
        assert_eq!(plan_name(&json!({})), None);
    }

    #[test]
    fn the_profile_arn_comes_from_the_usage_reply() {
        assert_eq!(
            profile_arn(&json!({
                "profileArn": "arn:aws:codewhisperer:us-east-1:1:profile/ABC"
            }))
            .as_deref(),
            Some("arn:aws:codewhisperer:us-east-1:1:profile/ABC")
        );
        assert_eq!(
            profile_arn(&json!({
                "kiro.resourceNotifications.usageState": { "arn": "arn:aws:codewhisperer:eu-west-1:1:profile/X" }
            }))
            .as_deref(),
            Some("arn:aws:codewhisperer:eu-west-1:1:profile/X")
        );
        assert_eq!(profile_arn(&json!({})), None);
    }

    #[test]
    fn a_snapshot_carries_the_windows_the_plan_and_the_read_time() {
        let snapshot = snapshot_from(&payload(), 7);
        assert_eq!(snapshot.fetched_at, 7);
        assert_eq!(snapshot.plan.as_deref(), Some("Kiro Pro"));
        assert_eq!(snapshot.windows.len(), 2);
        assert_eq!(snapshot.credits, None);
        assert_eq!(snapshot.gcp_tos, None);
    }

    #[test]
    fn identity_is_extracted_from_user_info_with_root_fallback() {
        let nested = json!({
            "userInfo": {
                "email": "kiro@example.com",
                "userId": "kiro-user-1"
            }
        });
        let parsed = identity_of(&nested).expect("nested identity");
        assert_eq!(parsed.email.as_deref(), Some("kiro@example.com"));
        assert_eq!(parsed.display_name.as_deref(), Some("kiro@example.com"));
        assert_eq!(parsed.account_id.as_deref(), Some("kiro-user-1"));

        let root_only = json!({
            "kiro.resourceNotifications.usageState": {
                "email": "wrapped@example.com",
                "userId": "wrapped-user"
            }
        });
        let wrapped = identity_of(&root_only).expect("wrapped identity");
        assert_eq!(wrapped.email.as_deref(), Some("wrapped@example.com"));
        assert_eq!(wrapped.account_id.as_deref(), Some("wrapped-user"));

        assert_eq!(identity_of(&json!({})), None);
    }
}
