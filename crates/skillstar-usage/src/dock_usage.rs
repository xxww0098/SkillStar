//! Dock usage helpers: reduce a subscription's quota snapshot to a single
//! "remaining percent" for the macOS Dock right-click menu.
//!
//! The Dock menu lists one row per subscription ("<account> · 剩余 47%"), so
//! each snapshot collapses to the *fullest* window's remaining share — the
//! quota closest to running out is what the row should warn about. Building the
//! row text (names, ordering) and the native menu lives upward in `skillstar-app`
//! / `src-tauri`; this module stays a pure, testable reduction.

use crate::subscription::{SubscriptionUsage, UsageWindow};

/// Consumed share (0–100) of a single window: the fetcher-provided `percent`
/// when present, otherwise derived from `used` / `total`. `None` when neither
/// is available (e.g. percent-less balance rows).
fn window_used_percent(window: &UsageWindow) -> Option<i32> {
    if let Some(percent) = window.percent {
        return Some(percent.clamp(0, 100));
    }
    let total = window.total?;
    if total <= 0 {
        return None;
    }
    let percent = ((window.used as f64 / total as f64) * 100.0).round();
    Some(percent.clamp(0.0, 100.0) as i32)
}

/// Remaining share (0–100) of a snapshot's *most-consumed* quota window — the
/// one closest to its limit. `None` when no window exposes a percent (nothing
/// meaningful to show for this subscription).
pub fn snapshot_remaining_percent(usage: &SubscriptionUsage) -> Option<i32> {
    let max_used = [
        usage.hourly.as_ref(),
        usage.weekly.as_ref(),
        usage.monthly.as_ref(),
    ]
    .into_iter()
    .flatten()
    .filter_map(window_used_percent)
    .max()?;
    Some((100 - max_used).clamp(0, 100))
}

/// Detailed summary for menu items (macOS Dock / system tray), returning
/// `(sort_priority, summary_string)`.
///
/// Priority ordering (lower number = more urgent / higher on the list):
/// - `0..=100`: Percentage quotas (ordered least remaining first).
/// - `1000`: Monetary balance.
/// - `1001`: Credit points.
/// - `1500`: Plan name only.
/// - `2000`: Sync error.
pub fn snapshot_menu_summary(usage: &SubscriptionUsage, lang: &str) -> Option<(i32, String)> {
    let is_zh = lang.starts_with("zh");
    if let Some(remaining) = snapshot_remaining_percent(usage) {
        let text = if is_zh {
            format!("剩余 {remaining}%")
        } else {
            format!("{remaining}% left")
        };
        return Some((remaining, text));
    }

    if let Some(balance) = &usage.balance {
        let curr_symbol = match balance.currency.as_str() {
            "USD" => "$",
            "CNY" | "RMB" => "¥",
            "EUR" => "€",
            "GBP" => "£",
            other => other,
        };
        let text = if is_zh {
            if curr_symbol.len() == 1 {
                format!("余额 {curr_symbol}{:.2}", balance.total)
            } else {
                format!("余额 {:.2} {curr_symbol}", balance.total)
            }
        } else if curr_symbol.len() == 1 {
            format!("Balance {curr_symbol}{:.2}", balance.total)
        } else {
            format!("Balance {:.2} {curr_symbol}", balance.total)
        };
        return Some((1000, text));
    }

    if let Some(credit) = usage.credits.first()
        && let Some(amt) = &credit.credit_amount
    {
        let text = if is_zh {
            format!("剩余 {amt} 积分")
        } else {
            format!("{amt} credits")
        };
        return Some((1001, text));
    }

    if let Some(plan) = &usage.plan_name {
        return Some((1500, plan.clone()));
    }

    if usage.error.is_some() {
        let text = if is_zh { "同步失败" } else { "Sync failed" };
        return Some((2000, text.to_string()));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subscription::{CreditInfo, MonetaryBalance};

    fn window(percent: Option<i32>, used: i64, total: Option<i64>) -> UsageWindow {
        UsageWindow {
            label: "w".to_string(),
            used,
            total,
            percent,
            reset_at: None,
            breakdown: Vec::new(),
        }
    }

    fn usage(
        hourly: Option<UsageWindow>,
        weekly: Option<UsageWindow>,
        monthly: Option<UsageWindow>,
    ) -> SubscriptionUsage {
        SubscriptionUsage {
            subscription_id: "s".to_string(),
            fetched_at: 0,
            plan_name: None,
            hourly,
            weekly,
            monthly,
            balance: None,
            credits: Vec::new(),
            error: None,
            api_keys: Vec::new(),
            deepseek_analytics: None,
        }
    }

    #[test]
    fn remaining_is_100_minus_fullest_window() {
        // weekly 0% used, monthly 58% used → fullest is 58% → remaining 42%.
        let u = usage(
            None,
            Some(window(Some(0), 0, None)),
            Some(window(Some(58), 0, None)),
        );
        assert_eq!(snapshot_remaining_percent(&u), Some(42));
    }

    #[test]
    fn derives_percent_from_used_over_total() {
        let u = usage(None, None, Some(window(None, 7500, Some(10000))));
        assert_eq!(snapshot_remaining_percent(&u), Some(25));
    }

    #[test]
    fn none_without_any_percent_quota() {
        assert_eq!(snapshot_remaining_percent(&usage(None, None, None)), None);
    }

    #[test]
    fn menu_summary_supports_percent_balance_and_credits() {
        let u_pct = usage(None, Some(window(Some(10), 0, None)), None);
        assert_eq!(
            snapshot_menu_summary(&u_pct, "zh-CN"),
            Some((90, "剩余 90%".to_string()))
        );
        assert_eq!(
            snapshot_menu_summary(&u_pct, "en"),
            Some((90, "90% left".to_string()))
        );

        let mut u_bal = usage(None, None, None);
        u_bal.balance = Some(MonetaryBalance {
            currency: "USD".into(),
            total: 25.5,
            granted: 0.0,
            topped_up: 25.5,
            is_available: Some(true),
        });
        assert_eq!(
            snapshot_menu_summary(&u_bal, "zh-CN"),
            Some((1000, "余额 $25.50".to_string()))
        );
        assert_eq!(
            snapshot_menu_summary(&u_bal, "en"),
            Some((1000, "Balance $25.50".to_string()))
        );

        let mut u_credit = usage(None, None, None);
        u_credit.credits = vec![CreditInfo {
            credit_type: "tier".into(),
            credit_amount: Some("500".into()),
            minimum_credit_amount_for_usage: None,
        }];
        assert_eq!(
            snapshot_menu_summary(&u_credit, "zh-CN"),
            Some((1001, "剩余 500 积分".to_string()))
        );
        assert_eq!(
            snapshot_menu_summary(&u_credit, "en"),
            Some((1001, "500 credits".to_string()))
        );
    }
}
