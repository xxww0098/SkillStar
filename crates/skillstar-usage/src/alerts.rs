//! Threshold-based alert computation.
//!
//! Pure function: given the current subscription set + last usage snapshots
//! + dismissed alert ids, returns the alerts that should be shown right now.

use chrono::Utc;

use crate::UsageResult;
use crate::storage;
use crate::subscription::{
    AlertKind, AlertSeverity, Subscription, SubscriptionAlert, SubscriptionUsage,
};

const SECONDS_PER_DAY: i64 = 86_400;

pub fn compute_alerts() -> UsageResult<Vec<SubscriptionAlert>> {
    let subs = storage::list_subscriptions()?;
    let snapshots = storage::list_usage_snapshots()?;
    let dismissed = storage::dismissed_alert_ids().unwrap_or_default();
    let now = Utc::now().timestamp();

    let mut alerts: Vec<SubscriptionAlert> = Vec::new();
    for sub in &subs {
        let usage = snapshots.get(&sub.id);
        for alert in alerts_for(sub, usage, now) {
            if !dismissed.contains(&alert.id) {
                alerts.push(alert);
            }
        }
    }
    Ok(alerts)
}

fn alerts_for(
    sub: &Subscription,
    usage: Option<&SubscriptionUsage>,
    now: i64,
) -> Vec<SubscriptionAlert> {
    let mut out = Vec::new();

    // OAuth re-auth required
    if sub.requires_reauth {
        out.push(SubscriptionAlert {
            id: format!("{}::needs-reauth", sub.id),
            subscription_id: sub.id.clone(),
            severity: AlertSeverity::Danger,
            kind: AlertKind::NeedsReauth,
            message: format!("{} 登录已失效，请重新授权。", sub.display_name),
        });
    }

    // Renew window
    if sub.renew_date > 0 {
        let remaining = sub.renew_date - now;
        if remaining < 0 {
            out.push(SubscriptionAlert {
                id: format!("{}::expired", sub.id),
                subscription_id: sub.id.clone(),
                severity: AlertSeverity::Danger,
                kind: AlertKind::Expired,
                message: format!("{} 订阅已到期。", sub.display_name),
            });
        } else if remaining < 7 * SECONDS_PER_DAY {
            let days = (remaining / SECONDS_PER_DAY).max(0);
            out.push(SubscriptionAlert {
                id: format!("{}::renew-soon", sub.id),
                subscription_id: sub.id.clone(),
                severity: AlertSeverity::Info,
                kind: AlertKind::RenewSoon,
                message: format!("{} 将在 {} 天后续费。", sub.display_name, days),
            });
        }
    }

    // Quota windows
    if let Some(usage) = usage {
        for window in [&usage.hourly, &usage.weekly, &usage.monthly]
            .iter()
            .copied()
            .flatten()
        {
            if let Some(pct) = window.percent {
                // `percent` is "used" — alert when remaining < threshold.
                let remaining = (100 - pct).max(0);
                if remaining < 5 {
                    out.push(SubscriptionAlert {
                        id: format!("{}::critical::{}", sub.id, window.label),
                        subscription_id: sub.id.clone(),
                        severity: AlertSeverity::Danger,
                        kind: AlertKind::QuotaCritical,
                        message: format!(
                            "{} 的 {} 用量仅剩 {}%。",
                            sub.display_name, window.label, remaining
                        ),
                    });
                } else if remaining < 20 {
                    out.push(SubscriptionAlert {
                        id: format!("{}::low::{}", sub.id, window.label),
                        subscription_id: sub.id.clone(),
                        severity: AlertSeverity::Warning,
                        kind: AlertKind::QuotaLow,
                        message: format!(
                            "{} 的 {} 用量剩余 {}%。",
                            sub.display_name, window.label, remaining
                        ),
                    });
                }
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::AuthMode;
    use crate::subscription::{BillingCycle, UsageWindow};

    /// A fixed "now" keeps every renew-window boundary assertion deterministic.
    const NOW: i64 = 1_700_000_000;

    fn subscription(id: &str) -> Subscription {
        Subscription {
            id: id.into(),
            catalog_id: "kimi".into(),
            display_name: "Kimi 主号".into(),
            auth_mode: AuthMode::ApiKey,
            plan_tier: None,
            monthly_price: None,
            currency: "CNY".into(),
            billing_cycle: BillingCycle::Monthly,
            start_date: 0,
            renew_date: 0,
            auto_renew: false,
            api_key_encrypted: None,
            platform_token_encrypted: None,
            access_token_encrypted: None,
            refresh_token_encrypted: None,
            access_token_expires_at: None,
            id_token_encrypted: None,
            oauth_account_id: None,
            oauth_region: None,
            requires_reauth: false,
            cookie_jar_encrypted: None,
            cookie_session_expires_at: None,
            manual_quota: None,
            note: None,
            sort_index: 0,
            created_at: 0,
            updated_at: 0,
        }
    }

    fn window(label: &str, percent: Option<i32>) -> UsageWindow {
        UsageWindow {
            label: label.into(),
            used: 0,
            total: None,
            percent,
            reset_at: None,
            breakdown: Vec::new(),
        }
    }

    fn usage_with_monthly(percent: Option<i32>) -> SubscriptionUsage {
        SubscriptionUsage {
            subscription_id: "sub-1".into(),
            fetched_at: NOW,
            monthly: Some(window("30d", percent)),
            ..Default::default()
        }
    }

    #[test]
    fn reauth_flag_raises_danger_alert_with_stable_dismissable_id() {
        let mut sub = subscription("sub-1");
        sub.requires_reauth = true;

        let alerts = alerts_for(&sub, None, NOW);

        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].id, "sub-1::needs-reauth");
        assert_eq!(alerts[0].kind, AlertKind::NeedsReauth);
        assert_eq!(alerts[0].severity, AlertSeverity::Danger);
        // Recomputation must yield the same id, or dismissals would not stick.
        assert_eq!(alerts_for(&sub, None, NOW)[0].id, alerts[0].id);
    }

    #[test]
    fn past_renew_date_is_expired_while_unset_renew_date_stays_quiet() {
        let mut sub = subscription("sub-1");
        sub.renew_date = NOW - 1;
        let alerts = alerts_for(&sub, None, NOW);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].kind, AlertKind::Expired);
        assert_eq!(alerts[0].severity, AlertSeverity::Danger);

        sub.renew_date = 0;
        assert!(alerts_for(&sub, None, NOW).is_empty());
    }

    #[test]
    fn renewal_inside_seven_days_warns_with_remaining_day_count() {
        let mut sub = subscription("sub-1");
        sub.renew_date = NOW + 3 * SECONDS_PER_DAY + 3600;

        let alerts = alerts_for(&sub, None, NOW);

        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].kind, AlertKind::RenewSoon);
        assert_eq!(alerts[0].severity, AlertSeverity::Info);
        assert!(alerts[0].message.contains("3 天"), "{}", alerts[0].message);
    }

    #[test]
    fn seven_day_window_boundary_is_exclusive() {
        let mut sub = subscription("sub-1");
        sub.renew_date = NOW + 7 * SECONDS_PER_DAY;
        assert!(alerts_for(&sub, None, NOW).is_empty());

        sub.renew_date = NOW + 7 * SECONDS_PER_DAY - 1;
        let alerts = alerts_for(&sub, None, NOW);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].kind, AlertKind::RenewSoon);
        assert!(alerts[0].message.contains("6 天"), "{}", alerts[0].message);
    }

    #[test]
    fn quota_bands_split_at_5_and_20_percent_remaining() {
        let sub = subscription("sub-1");
        let kind_at = |used_percent: i32| {
            alerts_for(&sub, Some(&usage_with_monthly(Some(used_percent))), NOW)
                .first()
                .map(|a| a.kind)
        };

        // percent is "used": remaining 4% → critical, 5% / 19% → low, 20% → none.
        assert_eq!(kind_at(96), Some(AlertKind::QuotaCritical));
        assert_eq!(kind_at(95), Some(AlertKind::QuotaLow));
        assert_eq!(kind_at(81), Some(AlertKind::QuotaLow));
        assert_eq!(kind_at(80), None);
    }

    #[test]
    fn overconsumed_quota_clamps_remaining_to_zero_percent() {
        let sub = subscription("sub-1");
        let alerts = alerts_for(&sub, Some(&usage_with_monthly(Some(130))), NOW);

        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].kind, AlertKind::QuotaCritical);
        assert!(alerts[0].message.contains("0%"), "{}", alerts[0].message);
    }

    #[test]
    fn window_without_percent_never_alerts() {
        let sub = subscription("sub-1");
        assert!(alerts_for(&sub, Some(&usage_with_monthly(None)), NOW).is_empty());
    }

    #[test]
    fn each_quota_window_alerts_independently_with_label_scoped_ids() {
        let sub = subscription("sub-1");
        let usage = SubscriptionUsage {
            subscription_id: "sub-1".into(),
            fetched_at: NOW,
            hourly: Some(window("5h", Some(97))),
            monthly: Some(window("30d", Some(85))),
            ..Default::default()
        };

        let alerts = alerts_for(&sub, Some(&usage), NOW);

        assert_eq!(alerts.len(), 2);
        assert_eq!(alerts[0].id, "sub-1::critical::5h");
        assert_eq!(alerts[1].id, "sub-1::low::30d");
    }

    #[test]
    fn compute_alerts_hides_dismissed_ids() {
        let _guard = crate::test_env_lock().lock().expect("env lock");
        let temp = tempfile::tempdir().expect("tempdir");
        // SAFETY: serialized by the crate-wide test_env_lock.
        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
        }

        let mut sub = subscription("sub-dismiss");
        sub.requires_reauth = true;
        storage::upsert_subscription(sub).expect("seed subscription");

        let before = compute_alerts().expect("compute");
        assert!(before.iter().any(|a| a.id == "sub-dismiss::needs-reauth"));

        storage::dismiss_alert("sub-dismiss::needs-reauth").expect("dismiss");
        let after = compute_alerts().expect("recompute");
        assert!(!after.iter().any(|a| a.id == "sub-dismiss::needs-reauth"));

        // SAFETY: still serialized by the crate-wide test_env_lock.
        unsafe {
            std::env::remove_var("SKILLSTAR_DATA_DIR");
        }
    }
}
