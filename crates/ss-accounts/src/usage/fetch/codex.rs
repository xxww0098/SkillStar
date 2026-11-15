//! Codex quota: the official usage endpoint behind the ChatGPT login.
//!
//! GET /backend-api/wham/usage serves two rate-limit windows (the 5-hour rule
//! and the weekly cap), the prepaid credit balance, and the plan name. The
//! windows are slotted by the length they report, not by position: a ChatGPT
//! Pro account serves the weekly window as primary_window with
//! secondary_window null, so slotting by position would label a window that
//! resets in days as "5h".

use serde_json::Value;

use crate::usage::credentials::{Credentials, UsageOutcome};
use crate::usage::error::{UsageError, UsageResult};
use crate::usage::snapshot::{Credits, UsageSnapshot, UsageUnit, UsageWindow};

use super::parse;

/// What the account layer sees when a read of this endpoint fails.
const ENDPOINT_LABEL: &str = "codex wham/usage";
const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";

/// Credits are points, not money; the label is the caller's to localise.
pub(crate) const CREDITS_LABEL: &str = "Credits";

const FIVE_HOUR_KEY: &str = "five_hour";
const FIVE_HOUR_LABEL: &str = "5h";
const WEEK_KEY: &str = "week";
const WEEK_LABEL: &str = "7d";
const SECONDS_PER_DAY: i64 = 86_400;

/// Read the Codex quota for one account.
pub fn fetch(creds: &Credentials) -> UsageResult<UsageOutcome> {
    let reading = super::tokens_for_read("codex", creds)?;
    let mut headers = vec![
        ("Accept", "application/json".to_string()),
        (
            "Authorization",
            format!("Bearer {}", reading.tokens.access_token),
        ),
    ];
    if let Some(account_id) = parse::text_at(creds.provider_state.as_ref(), "/account_id") {
        headers.push(("ChatGPT-Account-Id", account_id));
    }
    let answer = super::http_get(ENDPOINT_LABEL, USAGE_URL, &headers)?;
    let snapshot = interpret(
        answer.status,
        &answer.body,
        super::unix_now(),
        CREDITS_LABEL,
    )?;
    Ok(UsageOutcome {
        snapshot,
        tokens: reading.tokens,
        provider_state: reading.provider_state,
    })
}

/// What one usage reply means: status and body in, snapshot out.
fn interpret(status: u16, body: &str, now: i64, credits_label: &str) -> UsageResult<UsageSnapshot> {
    if !(200..300).contains(&status) {
        return Err(super::classify_status(ENDPOINT_LABEL, status, body));
    }
    parse_usage(body, now, credits_label)
}

/// Response JSON to snapshot. Pure: the fixtures drive it without a socket.
pub(crate) fn parse_usage(body: &str, now: i64, credits_label: &str) -> UsageResult<UsageSnapshot> {
    let root: Value = serde_json::from_str(body).map_err(|error| {
        UsageError::Unreadable(format!(
            "{ENDPOINT_LABEL}: cannot parse the response: {error}: {}",
            parse::snippet(body)
        ))
    })?;
    let windows = rate_limit_windows(&root);
    let credits = credit_balance(&root, credits_label);
    if windows.is_empty() && credits.is_none() {
        return Err(UsageError::Unreadable(format!(
            "{ENDPOINT_LABEL}: the response carries no quota window: {}",
            parse::snippet(body)
        )));
    }
    Ok(UsageSnapshot {
        fetched_at: now,
        windows,
        plan: parse::text(root.get("plan_type")),
        credits,
        error: None,
        gcp_tos: None,
    })
}

/// Slot the two wham windows onto the hourly and weekly cards by their
/// reported length, falling back to position only when the length is absent.
fn rate_limit_windows(root: &Value) -> Vec<UsageWindow> {
    let rate_limit = root.get("rate_limit");
    let mut hourly: Option<UsageWindow> = None;
    let mut weekly: Option<UsageWindow> = None;
    let slots = [
        (
            rate_limit.and_then(|limit| limit.get("primary_window")),
            (FIVE_HOUR_KEY, FIVE_HOUR_LABEL),
        ),
        (
            rate_limit.and_then(|limit| limit.get("secondary_window")),
            (WEEK_KEY, WEEK_LABEL),
        ),
    ];
    for (raw, positional) in slots {
        let Some(raw) = raw.filter(|value| value.is_object()) else {
            continue;
        };
        let Some(window) = parse_window(raw, positional) else {
            continue;
        };
        let (preferred, other) = if window.key == WEEK_KEY {
            (&mut weekly, &mut hourly)
        } else {
            (&mut hourly, &mut weekly)
        };
        if preferred.is_none() {
            *preferred = Some(window);
        } else if other.is_none() {
            *other = Some(window);
        }
    }
    hourly.into_iter().chain(weekly).collect()
}

/// One rate-limit window: used_percent in, remaining share out.
fn parse_window(raw: &Value, positional: (&'static str, &'static str)) -> Option<UsageWindow> {
    let used_percent = parse::number(raw.get("used_percent"))?;
    let seconds = parse::integer(raw.get("limit_window_seconds")).filter(|value| *value > 0);
    let (key, label) = match seconds {
        Some(seconds) if seconds >= SECONDS_PER_DAY => (WEEK_KEY, parse::window_label(seconds)),
        Some(seconds) => (FIVE_HOUR_KEY, parse::window_label(seconds)),
        None => (positional.0, positional.1.to_string()),
    };
    Some(UsageWindow {
        key: key.to_string(),
        label,
        remaining_percent: parse::remaining_from_used(used_percent),
        used: None,
        total: None,
        unit: UsageUnit::Percent,
        resets_at: parse::epoch_seconds(raw.get("reset_at")),
    })
}

/// Prepaid credits from the same payload: unlimited, or a point balance the
/// provider writes as either a string or a number.
fn credit_balance(root: &Value, label: &str) -> Option<Credits> {
    let credits = root.get("credits").filter(|value| value.is_object())?;
    let unlimited = credits
        .get("unlimited")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let remaining = if unlimited {
        None
    } else {
        Some(parse::number(credits.get("balance"))?)
    };
    Some(Credits {
        label: label.to_string(),
        remaining,
        unlimited,
        unit: UsageUnit::Count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_760_000_000;
    const JUNK: &str = "<!doctype html><html><body>Just a moment...</body></html>";
    const USAGE_FULL: &str = include_str!("../../../tests/fixtures/codex/usage_full.json");
    const USAGE_WEEKLY_PRIMARY: &str =
        include_str!("../../../tests/fixtures/codex/usage_weekly_primary.json");
    const USAGE_MISSING_SECONDS: &str =
        include_str!("../../../tests/fixtures/codex/usage_missing_window_seconds.json");

    #[test]
    fn parses_both_windows_and_string_credits() {
        let snapshot = parse_usage(USAGE_FULL, NOW, CREDITS_LABEL).expect("fixture must parse");
        assert_eq!(snapshot.fetched_at, NOW);
        assert_eq!(snapshot.plan.as_deref(), Some("plus"));
        assert_eq!(snapshot.windows.len(), 2);

        let five_hour = &snapshot.windows[0];
        assert_eq!(five_hour.key, "five_hour");
        assert_eq!(five_hour.label, "5h");
        assert_eq!(five_hour.remaining_percent, 88.0);
        assert_eq!(five_hour.used, None);
        assert_eq!(five_hour.total, None);
        assert_eq!(five_hour.unit, UsageUnit::Percent);
        assert_eq!(five_hour.resets_at, Some(1_760_405_400));

        let week = &snapshot.windows[1];
        assert_eq!(week.key, "week");
        assert_eq!(week.label, "7d");
        assert_eq!(week.remaining_percent, 60.0);

        let credits = snapshot.credits.expect("credit row");
        assert_eq!(credits.remaining, Some(12.5));
        assert!(!credits.unlimited);
        assert_eq!(credits.unit, UsageUnit::Count);
        assert_eq!(credits.label, CREDITS_LABEL);
    }

    #[test]
    fn weekly_primary_slots_by_length_and_unlimited_credits() {
        let snapshot =
            parse_usage(USAGE_WEEKLY_PRIMARY, NOW, CREDITS_LABEL).expect("fixture must parse");
        assert_eq!(
            snapshot.windows.len(),
            1,
            "null secondary_window adds no row"
        );
        let week = &snapshot.windows[0];
        assert_eq!(week.key, "week");
        assert_eq!(week.label, "7d");
        assert_eq!(week.remaining_percent, 70.0);
        assert_eq!(week.resets_at, Some(1_791_594_863));

        let credits = snapshot.credits.expect("credit row");
        assert!(credits.unlimited);
        assert_eq!(credits.remaining, None);
    }

    #[test]
    fn missing_window_seconds_falls_back_to_positional_slots() {
        let snapshot =
            parse_usage(USAGE_MISSING_SECONDS, NOW, CREDITS_LABEL).expect("fixture must parse");
        assert_eq!(snapshot.windows.len(), 2);
        assert_eq!(snapshot.windows[0].key, "five_hour");
        assert_eq!(snapshot.windows[0].label, "5h");
        assert_eq!(snapshot.windows[1].key, "week");
        assert_eq!(snapshot.windows[1].label, "7d");
        assert_eq!(
            snapshot.credits.expect("credit row").remaining,
            Some(250.0),
            "balance written as a number"
        );
    }

    #[test]
    fn rejected_and_busy_statuses_classify() {
        let rejected = interpret(401, "", NOW, CREDITS_LABEL).expect_err("401 must fail");
        assert!(matches!(rejected, UsageError::AuthRequired(_)));
        assert!(!rejected.is_transient());

        let busy = interpret(429, "slow down", NOW, CREDITS_LABEL).expect_err("429 must fail");
        assert!(busy.is_transient());

        let down = interpret(503, "", NOW, CREDITS_LABEL).expect_err("5xx must fail");
        assert!(down.is_transient());

        let gone = interpret(404, "nope", NOW, CREDITS_LABEL).expect_err("404 must fail");
        assert!(matches!(gone, UsageError::Unreadable(_)));
    }

    #[test]
    fn html_body_is_unreadable() {
        let error = interpret(200, JUNK, NOW, CREDITS_LABEL).expect_err("HTML is not quota");
        assert!(matches!(error, UsageError::Unreadable(_)));
        assert!(!error.is_transient());

        let empty =
            interpret(200, "{}", NOW, CREDITS_LABEL).expect_err("no windows is a shape change");
        assert!(matches!(empty, UsageError::Unreadable(_)));
    }
}
