//! Devin Desktop（Windsurf）额度响应的纯解析。
//!
//! 座位服务的回答是 Connect/JSON（也可能是一段 planStatus protobuf，本 crate
//! 只读 JSON——解不开就当 Unreadable，不猜）。两个量：
//!
//! * 提示额度（User Prompt credits）：availablePromptCredits / usedPromptCredits /
//!   monthlyPromptCredits，重置于套餐周期。
//! * 附加额度（Add-on prompt credits）：*flex* / *add_on* / *top_up* 三组别名。
//!
//! 快照里的窗口一律记"剩余"：remaining = 1 - used/total。

use serde_json::Value;

use crate::usage::snapshot::{UsageSnapshot, UsageUnit, UsageWindow};

use super::parse;

/// 两条窗口的标签（UI 直接显示，与 Cockpit Tools 一致）。
pub(crate) const PROMPT_LABEL: &str = "User Prompt credits";
pub(crate) const ADD_ON_LABEL: &str = "Add-on prompt credits";

fn number_in(node: &Value, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|key| parse::number(node.get(*key)))
}

fn text_in(node: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| parse::text(node.get(*key)))
}

/// 额度可能挂在 planStatus 里，也可能在新版响应的 planInfo 里。
fn plan_info<'a>(root: &'a Value, status: Option<&'a Value>) -> Option<&'a Value> {
    status
        .and_then(|status| status.get("planInfo").or_else(|| status.get("plan_info")))
        .filter(|value| value.is_object())
        .or_else(|| root.get("planInfo").or_else(|| root.get("plan_info")))
        .filter(|value| value.is_object())
}

/// 套餐名：planInfo 的 planName → status 的 planName → 根上的。
pub(crate) fn plan_name(root: &Value, status: Option<&Value>) -> Option<String> {
    let info = plan_info(root, status);
    info.and_then(|info| text_in(info, &["planName", "plan_name", "teamsTier", "teams_tier"]))
        .or_else(|| status.and_then(|status| text_in(status, &["planName", "plan_name"])))
        .or_else(|| text_in(root, &["planName", "plan_name"]))
}

/// 一组 (used, total)。used 缺失但 total 与 available 都在时由两者相减得出。
fn amounts(
    root: &Value,
    info: Option<&Value>,
    used_keys: &[&str],
    available_keys: &[&str],
    total_keys: &[&str],
) -> Option<(f64, Option<f64>)> {
    let lookup = |keys: &[&str]| {
        info.and_then(|info| number_in(info, keys))
            .or_else(|| number_in(root, keys))
    };
    let total = lookup(total_keys);
    let available = lookup(available_keys);
    let used = lookup(used_keys).or_else(|| match (total, available) {
        (Some(total), Some(available)) if available <= total => Some(total - available),
        _ => None,
    })?;
    Some((used, total))
}

/// 一条窗口。
fn window(
    key: &str,
    label: &str,
    used: f64,
    total: f64,
    resets_at: Option<i64>,
) -> UsageWindow {
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

/// 重置时间：planInfo 上的优先，其次根上的。
fn reset_at(root: &Value, info: Option<&Value>) -> Option<i64> {
    const KEYS: [&str; 4] = ["planEnd", "plan_end", "subscriptionEnd", "nextResetAt"];
    KEYS.iter()
        .find_map(|key| info.and_then(|info| parse::epoch_seconds(info.get(*key))))
        .or_else(|| {
            KEYS.iter()
                .find_map(|key| parse::epoch_seconds(root.get(*key)))
        })
}

/// 一次读取的窗口。
pub(crate) fn windows(root: &Value, status: Option<&Value>) -> Vec<UsageWindow> {
    let info = plan_info(root, status);
    let reset = reset_at(root, info);
    let mut windows = Vec::new();
    if let Some((used, total)) = amounts(
        root,
        info,
        &["usedPromptCredits", "used_prompt_credits"],
        &["availablePromptCredits", "available_prompt_credits"],
        &["monthlyPromptCredits", "monthly_prompt_credits"],
    ) && let Some(total) = total.filter(|total| *total > 0.0)
    {
        windows.push(window("prompt", PROMPT_LABEL, used, total, reset));
    }
    if let Some((used, total)) = amounts(
        root,
        info,
        &[
            "used_flex_credits",
            "usedFlexCredits",
            "used_add_on_credits",
            "usedAddOnCredits",
            "used_top_up_credits",
            "usedTopUpCredits",
        ],
        &[
            "available_flex_credits",
            "availableFlexCredits",
            "available_add_on_credits",
            "availableAddOnCredits",
            "available_top_up_credits",
            "availableTopUpCredits",
        ],
        &[
            "monthly_add_on_credits",
            "monthlyAddOnCredits",
            "monthly_top_up_credits",
            "monthlyTopUpCredits",
            "monthly_flex_credits",
            "monthlyFlexCredits",
        ],
    ) && let Some(total) = total.filter(|total| *total > 0.0)
    {
        windows.push(window("add_on", ADD_ON_LABEL, used, total, reset));
    }
    windows
}

/// 响应里的 email（对账用；导入路径也会读它）。
pub(crate) fn email_of(root: &Value) -> Option<String> {
    text_in(root, &["email", "emailAddress", "email_address"])
}

/// 完整快照。
pub(crate) fn snapshot_from(root: &Value, status: Option<&Value>, now: i64) -> UsageSnapshot {
    UsageSnapshot {
        fetched_at: now,
        windows: windows(root, status),
        plan: plan_name(root, status),
        credits: None,
        error: None,
        gcp_tos: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn status() -> Value {
        json!({
            "planInfo": {
                "planName": "Pro",
                "monthlyPromptCredits": 500,
                "availablePromptCredits": 380,
                "usedPromptCredits": 120,
                "planEnd": "2026-12-01T00:00:00Z"
            }
        })
    }

    #[test]
    fn the_prompt_credits_become_a_remaining_window() {
        let windows = windows(&status(), Some(&status()));
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].key, "prompt");
        assert_eq!(windows[0].label, PROMPT_LABEL);
        assert_eq!(windows[0].used, Some(120.0));
        assert_eq!(windows[0].total, Some(500.0));
        assert_eq!(windows[0].remaining_percent, 76.0, "remaining, not used");
        assert_eq!(windows[0].resets_at, Some(1_796_083_200));
        assert_eq!(plan_name(&status(), Some(&status())).as_deref(), Some("Pro"));
    }

    #[test]
    fn available_credits_are_also_accepted_and_used_is_derived() {
        let body = json!({
            "planInfo": {
                "monthlyPromptCredits": 100,
                "availablePromptCredits": 80
            }
        });
        let windows = windows(&body, None);
        assert_eq!(windows[0].used, Some(20.0), "used = total - available");
        assert_eq!(windows[0].remaining_percent, 80.0);
    }

    #[test]
    fn the_add_on_credits_are_a_second_window_with_their_aliases() {
        let body = json!({
            "planInfo": {
                "monthly_add_on_credits": 50,
                "used_add_on_credits": 10
            }
        });
        let windows = windows(&body, None);
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].key, "add_on");
        assert_eq!(windows[0].remaining_percent, 80.0);
    }

    #[test]
    fn a_reply_without_numbers_yields_no_window() {
        assert!(windows(&json!({ "planInfo": { "planName": "Free" } }), None).is_empty());
        // total 为 0 不是"满了"，是没有额度。
        assert!(
            windows(
                &json!({ "planInfo": { "monthlyPromptCredits": 0, "usedPromptCredits": 0 } }),
                None
            )
            .is_empty()
        );
        assert_eq!(email_of(&json!({ "email": "dev@example.com" })).as_deref(), Some("dev@example.com"));
    }
}
