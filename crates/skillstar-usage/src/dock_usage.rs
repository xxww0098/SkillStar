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

#[cfg(test)]
mod tests {
    use super::*;

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
}
