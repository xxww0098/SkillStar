//! Ollama Cloud usage fetcher.
//!
//! Local Ollama (`localhost:11434`) has no quota. This catalog is the
//! ollama.com Cloud account behind an API key from
//! `https://ollama.com/settings/keys`.
//!
//! `GET https://ollama.com/api/usage` with `Authorization: Bearer <key>`.
//! The documented chat/generate APIs do not expose account quota; this
//! endpoint exists (401 without a key) and is community-verified to return
//! 0–1 consumed fractions on `limits.session.usage` / `limits.weekly.usage`.
//! It does **not** report reset timestamps — those are a global grid
//! (5h from Unix epoch; weekly Monday 00:00 UTC) predicted locally.
//!
//! Unknown extra fields are ignored. A window that cannot be read is
//! dropped; the account only fails when **neither** window parses.

use chrono::{DateTime, TimeDelta, Utc};
use serde_json::Value;
use skillstar_providers::balance;

use crate::subscription::{SubscriptionUsage, UsageWindow};
use crate::{UsageError, UsageResult};

const SESSION_PERIOD: TimeDelta = TimeDelta::hours(5);
/// Unix epoch is Thursday; Monday 00:00 UTC is epoch + 4 days.
const WEEKLY_PHASE: TimeDelta = TimeDelta::days(4);
const WEEKLY_PERIOD: TimeDelta = TimeDelta::days(7);

pub async fn fetch(subscription_id: &str, api_key: &str) -> UsageResult<SubscriptionUsage> {
    let body: Value = super::fetch_spec(&balance::OLLAMA, api_key).await?;
    if let Some(err) = body.get("error").and_then(Value::as_str) {
        let err = err.trim();
        if !err.is_empty() {
            return Err(UsageError::Fetcher(format!("Ollama Cloud: {err}")));
        }
    }

    let parsed = parse_usage(&body, Utc::now());
    if parsed.hourly.is_none() && parsed.weekly.is_none() {
        return Err(UsageError::Fetcher(
            "Ollama Cloud 未返回可展示的额度窗口。".into(),
        ));
    }

    Ok(SubscriptionUsage {
        subscription_id: subscription_id.to_string(),
        fetched_at: Utc::now().timestamp(),
        plan_name: parsed.plan_name,
        hourly: parsed.hourly,
        weekly: parsed.weekly,
        monthly: None,
        balance: None,
        credits: Vec::new(),
        error: None,
        api_keys: Vec::new(),
        deepseek_analytics: None,
    })
}

#[derive(Debug, Default)]
struct ParsedUsage {
    plan_name: Option<String>,
    hourly: Option<UsageWindow>,
    weekly: Option<UsageWindow>,
}

fn parse_usage(body: &Value, now: DateTime<Utc>) -> ParsedUsage {
    let limits = body.get("limits");
    ParsedUsage {
        plan_name: pick_plan_name(body),
        hourly: fraction_window(
            limits.and_then(|l| l.get("session")),
            "5h",
            now,
            SESSION_PERIOD,
            TimeDelta::zero(),
        ),
        weekly: fraction_window(
            limits.and_then(|l| l.get("weekly")),
            "7d",
            now,
            WEEKLY_PERIOD,
            WEEKLY_PHASE,
        ),
    }
}

fn fraction_window(
    node: Option<&Value>,
    label: &str,
    now: DateTime<Utc>,
    period: TimeDelta,
    phase: TimeDelta,
) -> Option<UsageWindow> {
    let percent = consumed_percent(node.and_then(|n| n.get("usage")))?;
    Some(UsageWindow {
        label: label.to_string(),
        used: i64::from(percent),
        total: Some(100),
        percent: Some(percent),
        reset_at: Some(next_reset_epoch_secs(now, period, phase)),
        breakdown: Vec::new(),
    })
}

/// `usage` is a 0–1 consumed fraction. Values above 1.5 are treated as
/// already-percent (defensive; the verified payload is fractional).
fn consumed_percent(value: Option<&Value>) -> Option<i32> {
    let raw = match value? {
        Value::Number(n) => n.as_f64()?,
        Value::String(text) => text.trim().parse::<f64>().ok()?,
        _ => return None,
    };
    if !raw.is_finite() || raw < 0.0 {
        return None;
    }
    let percent = if raw <= 1.5 { raw * 100.0 } else { raw };
    Some(percent.round().clamp(0.0, 100.0) as i32)
}

fn next_reset_epoch_secs(now: DateTime<Utc>, period: TimeDelta, phase: TimeDelta) -> i64 {
    let now_ms = now.timestamp_millis();
    let period_ms = period.num_milliseconds().max(1);
    let phase_ms = phase.num_milliseconds();
    let elapsed = (now_ms - phase_ms).rem_euclid(period_ms);
    let remain = period_ms - elapsed;
    (now_ms + remain) / 1000
}

fn pick_plan_name(body: &Value) -> Option<String> {
    const PATHS: &[&[&str]] = &[
        &["plan"],
        &["tier"],
        &["plan_name"],
        &["subscription", "plan"],
        &["subscription", "tier"],
    ];
    for path in PATHS {
        let mut node = body;
        let mut found = true;
        for key in *path {
            match node.get(*key) {
                Some(next) => node = next,
                None => {
                    found = false;
                    break;
                }
            }
        }
        if !found {
            continue;
        }
        if let Some(text) = node.as_str().map(str::trim).filter(|s| !s.is_empty()) {
            return Some(text.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, TimeZone};
    use serde_json::json;

    fn ts(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).single().expect("utc")
    }

    #[test]
    fn parse_verified_fraction_payload() {
        let body = json!({
            "plan": "pro",
            "limits": {
                "session": { "usage": 0.246 },
                "weekly": { "usage": 0.066 },
                "extra": { "ignored": true }
            }
        });
        // 2026-05-27 10:15:00 UTC — mid 5h slot, Wednesday.
        let now = ts(1_748_338_500);
        let parsed = parse_usage(&body, now);
        assert_eq!(parsed.plan_name.as_deref(), Some("pro"));

        let hourly = parsed.hourly.expect("session");
        assert_eq!(hourly.label, "5h");
        assert_eq!(hourly.percent, Some(25));
        assert_eq!(hourly.used, 25);
        assert_eq!(hourly.total, Some(100));
        let reset = hourly.reset_at.expect("predicted reset");
        assert!(reset > now.timestamp());
        assert_eq!(reset % SESSION_PERIOD.num_seconds(), 0);

        let weekly = parsed.weekly.expect("weekly");
        assert_eq!(weekly.label, "7d");
        assert_eq!(weekly.percent, Some(7));
        let week_reset = weekly.reset_at.expect("weekly reset");
        let week_dt = ts(week_reset);
        assert_eq!(week_dt.weekday(), chrono::Weekday::Mon);
        assert_eq!(week_dt.time(), chrono::NaiveTime::MIN);
    }

    #[test]
    fn missing_window_is_dropped_not_an_account_failure() {
        let body = json!({ "limits": { "session": { "usage": 0.5 } } });
        let parsed = parse_usage(&body, ts(1_700_000_000));
        assert!(parsed.hourly.is_some());
        assert!(parsed.weekly.is_none());
    }

    #[test]
    fn unreadable_usage_drops_that_bar() {
        let body = json!({
            "limits": {
                "session": { "usage": "nope" },
                "weekly": { "usage": 0.1 }
            }
        });
        let parsed = parse_usage(&body, ts(1_700_000_000));
        assert!(parsed.hourly.is_none());
        assert_eq!(parsed.weekly.unwrap().percent, Some(10));
    }

    #[test]
    fn empty_payload_drops_both_windows() {
        let parsed = parse_usage(&json!({}), ts(1_700_000_000));
        assert!(parsed.hourly.is_none());
        assert!(parsed.weekly.is_none());
        assert!(parsed.plan_name.is_none());
    }

    #[test]
    fn fraction_one_is_fully_consumed_and_large_values_are_already_percent() {
        assert_eq!(consumed_percent(Some(&json!(1.0))), Some(100));
        assert_eq!(consumed_percent(Some(&json!(0.0))), Some(0));
        assert_eq!(consumed_percent(Some(&json!(42))), Some(42));
        assert_eq!(consumed_percent(Some(&json!("0.5"))), Some(50));
        assert_eq!(consumed_percent(Some(&json!(-0.1))), None);
    }

    #[test]
    fn session_reset_is_epoch_aligned_5h() {
        // Exactly on a 5h boundary → next is one period later.
        let on_grid = ts(0);
        assert_eq!(
            next_reset_epoch_secs(on_grid, SESSION_PERIOD, TimeDelta::zero()),
            SESSION_PERIOD.num_seconds()
        );
        let just_before = ts(SESSION_PERIOD.num_seconds() - 1);
        assert_eq!(
            next_reset_epoch_secs(just_before, SESSION_PERIOD, TimeDelta::zero()),
            SESSION_PERIOD.num_seconds()
        );
    }
}
