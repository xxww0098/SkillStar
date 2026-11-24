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
const RESET_CREDITS_LABEL: &str = "codex wham/rate-limit-reset-credits";
const RESET_CREDITS_URL: &str = "https://chatgpt.com/backend-api/wham/rate-limit-reset-credits";

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
    let headers = request_headers(
        creds,
        &reading.tokens,
        reading.provider_state.as_ref().or(creds.provider_state.as_ref()),
    );
    let answer = super::http_get(ENDPOINT_LABEL, USAGE_URL, &headers)?;
    let now = super::unix_now();
    let mut snapshot = interpret(answer.status, &answer.body, now, CREDITS_LABEL)?;
    if snapshot.reset_credits.is_none() {
        snapshot.reset_credits = fetch_reset_credits(&headers, now);
    }
    Ok(UsageOutcome {
        snapshot,
        tokens: reading.tokens,
        provider_state: reading.provider_state,
        identity: None,
    })
}

fn request_headers(
    creds: &Credentials,
    tokens: &crate::oauth::TokenSet,
    provider_state: Option<&Value>,
) -> Vec<(&'static str, String)> {
    let mut headers = vec![
        ("Accept", "application/json".to_string()),
        ("Authorization", format!("Bearer {}", tokens.access_token)),
    ];
    let account_id = creds
        .account_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .or_else(|| parse::text_at(provider_state, "/account_id"))
        .or_else(|| {
            (crate::agents::spec(crate::Agent::Codex).provider_account_id)(
                &tokens.access_token,
                tokens.id_token.as_deref(),
            )
        });
    if let Some(account_id) = account_id {
        headers.push(("ChatGPT-Account-Id", account_id));
    }
    headers
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
    let reset_credits = inline_reset_credits(&root);
    if windows.is_empty() && credits.is_none() && reset_credits.is_none() {
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
        reset_credits,
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

/// Inline banked reset-card count from `/backend-api/wham/usage`.
fn inline_reset_credits(root: &Value) -> Option<u64> {
    let block = root
        .get("rate_limit_reset_credits")
        .or_else(|| root.get("rateLimitResetCredits"))
        .filter(|value| value.is_object())?;
    let count = block
        .get("available_count")
        .or_else(|| block.get("availableCount"));
    parse::integer(count).and_then(|value| u64::try_from(value).ok())
}

/// Best-effort fallback to `/backend-api/wham/rate-limit-reset-credits` when
/// `/wham/usage` omits `rate_limit_reset_credits`. Never fails the quota read.
fn fetch_reset_credits(headers: &[(&str, String)], now: i64) -> Option<u64> {
    let answer = super::http_get(RESET_CREDITS_LABEL, RESET_CREDITS_URL, headers)
        .map_err(|error| {
            tracing::debug!(target: "accounts", error = %error, "codex reset-credits fallback failed");
            error
        })
        .ok()?;
    if !(200..300).contains(&answer.status) {
        tracing::debug!(
            target: "accounts",
            status = answer.status,
            "codex reset-credits fallback returned non-2xx"
        );
        return None;
    }
    parse_reset_credits(&answer.body, now)
}

/// Pure parser for `/backend-api/wham/rate-limit-reset-credits`.
pub(crate) fn parse_reset_credits(body: &str, now: i64) -> Option<u64> {
    let root: Value = serde_json::from_str(body).ok()?;
    let data = root.get("data").filter(|value| value.is_object());
    let direct = [Some(&root), data].into_iter().flatten().find_map(|node| {
        let count = node
            .get("available_count")
            .or_else(|| node.get("availableCount"));
        parse::integer(count).and_then(|value| u64::try_from(value).ok())
    });
    if direct.is_some() {
        return direct;
    }
    let credits = [Some(&root), data]
        .into_iter()
        .flatten()
        .find_map(|node| node.get("credits").and_then(Value::as_array))?;
    let available = credits
        .iter()
        .filter(|item| is_available_reset_credit(item, now))
        .count();
    u64::try_from(available).ok()
}

fn is_available_reset_credit(item: &Value, now: i64) -> bool {
    let Some(obj) = item.as_object() else {
        return false;
    };
    let status = parse::text_in(item, &["status", "state"])
        .unwrap_or_else(|| "available".to_string())
        .to_ascii_lowercase();
    if matches!(
        status.as_str(),
        "redeemed" | "used" | "consumed" | "expired"
    ) {
        return false;
    }
    if let Some(expires_at) = parse::epoch_seconds(
        obj.get("expires_at").or_else(|| obj.get("expiresAt")),
    ) {
        return expires_at > now;
    }
    true
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

    #[test]
    fn parses_inline_reset_credits_from_wham_usage() {
        let with_cards = r#"{
            "plan_type": "pro",
            "rate_limit": {
                "primary_window": { "used_percent": 47, "limit_window_seconds": 604800 }
            },
            "rate_limit_reset_credits": { "available_count": 2 }
        }"#;
        let snapshot = parse_usage(with_cards, NOW, CREDITS_LABEL).expect("parse with reset cards");
        assert_eq!(snapshot.reset_credits, Some(2));

        let with_zero = r#"{
            "rate_limit_reset_credits": { "available_count": 0 }
        }"#;
        let zero_snapshot =
            parse_usage(with_zero, NOW, CREDITS_LABEL).expect("zero count still valid");
        assert_eq!(zero_snapshot.reset_credits, Some(0));

        let absent = parse_usage(USAGE_FULL, NOW, CREDITS_LABEL).expect("fixture");
        assert_eq!(absent.reset_credits, None);
    }

    #[test]
    fn parses_reset_credits_endpoint_direct_count_and_credit_rows() {
        assert_eq!(
            parse_reset_credits(r#"{"available_count": 3}"#, NOW),
            Some(3)
        );
        assert_eq!(
            parse_reset_credits(r#"{"data": {"availableCount": 0}}"#, NOW),
            Some(0)
        );
        let rows = r#"{
            "credits": [
                { "status": "available", "expires_at": 1760000100 },
                { "status": "redeemed", "expires_at": 1760000100 },
                { "status": "available", "expires_at": 1759999900 },
                { "state": "available" }
            ]
        }"#;
        assert_eq!(parse_reset_credits(rows, NOW), Some(2));
        assert_eq!(parse_reset_credits("not json", NOW), None);
        assert_eq!(parse_reset_credits("{}", NOW), None);
    }

    #[test]
    fn codex_request_headers_include_chatgpt_account_id_from_account_or_state() {
        let mut creds = Credentials {
            tokens: crate::oauth::TokenSet {
                access_token: "at-1".to_string(),
                ..Default::default()
            },
            account_id: Some("ws-from-account".to_string()),
            provider_state: Some(serde_json::json!({ "account_id": "ws-from-state" })),
            needs_identity: false,
        };
        let headers = request_headers(&creds, &creds.tokens, creds.provider_state.as_ref());
        assert!(
            headers.contains(&("ChatGPT-Account-Id", "ws-from-account".to_string())),
            "{headers:?}"
        );

        creds.account_id = None;
        let headers_state = request_headers(&creds, &creds.tokens, creds.provider_state.as_ref());
        assert!(
            headers_state.contains(&("ChatGPT-Account-Id", "ws-from-state".to_string())),
            "{headers_state:?}"
        );

        creds.provider_state = None;
        let headers_none = request_headers(&creds, &creds.tokens, None);
        assert!(!headers_none.iter().any(|(k, _)| *k == "ChatGPT-Account-Id"));
    }
}
