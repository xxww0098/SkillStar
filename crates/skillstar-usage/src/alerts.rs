//! Threshold-based alert computation.
//!
//! Pure function: given the current subscription set + last usage snapshots
//! + dismissed alert ids, returns the alerts that should be shown right now.

use chrono::Utc;

use crate::storage;
use crate::subscription::{
    AlertKind, AlertSeverity, Subscription, SubscriptionAlert, SubscriptionUsage,
};
use crate::{UsageError, UsageResult};

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

#[allow(dead_code)]
fn _unused_marker(_: UsageError) {}
