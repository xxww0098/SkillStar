//! Claude Code quota: the plan windows behind the OAuth login.
//!
//! GET /api/oauth/usage needs the OAuth bearer token plus the oauth beta
//! header. The payload carries a limits array (the 5-hour session window, the
//! weekly cap, and per-model weekly scopes) and, on older deployments, the two
//! summary windows at the top level. The plan tier is not in this payload: it
//! comes from the provider state the login stored.

use serde_json::Value;

use crate::usage::credentials::{Credentials, UsageOutcome};
use crate::usage::error::{UsageError, UsageResult};
use crate::usage::snapshot::{UsageSnapshot, UsageUnit, UsageWindow};

use super::parse;

/// What the account layer sees when a read of this endpoint fails.
const ENDPOINT_LABEL: &str = "claude-code oauth usage";
const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
/// The OAuth token is only accepted on the beta surface it was minted for.
const OAUTH_BETA: &str = "oauth-2025-04-20";

const FIVE_HOUR_KEY: &str = "five_hour";
const FIVE_HOUR_LABEL: &str = "5h";
const WEEK_KEY: &str = "week";
const WEEK_LABEL: &str = "7d";

/// Read the Claude Code quota for one account.
pub fn fetch(creds: &Credentials) -> UsageResult<UsageOutcome> {
    let reading = super::tokens_for_read("claude-code", creds)?;
    // A rotated provider state is the freshest place the plan tier lives.
    let state = reading
        .provider_state
        .as_ref()
        .or(creds.provider_state.as_ref());
    let plan = parse::text_at(state, "/plan");
    let headers = [
        ("Accept", "application/json".to_string()),
        (
            "Authorization",
            format!("Bearer {}", reading.tokens.access_token),
        ),
        ("anthropic-beta", OAUTH_BETA.to_string()),
    ];
    let answer = super::http_get(ENDPOINT_LABEL, USAGE_URL, &headers)?;
    let snapshot = interpret(
        answer.status,
        &answer.body,
        super::unix_now(),
        plan.as_deref(),
    )?;
    Ok(UsageOutcome {
        snapshot,
        tokens: reading.tokens,
        provider_state: reading.provider_state,
        identity: None,
    })
}

/// What one usage reply means: status and body in, snapshot out.
fn interpret(status: u16, body: &str, now: i64, plan: Option<&str>) -> UsageResult<UsageSnapshot> {
    if !(200..300).contains(&status) {
        return Err(super::classify_status(ENDPOINT_LABEL, status, body));
    }
    parse_usage(body, now, plan)
}

/// Response JSON to snapshot. Pure: the fixtures drive it without a socket.
pub(crate) fn parse_usage(body: &str, now: i64, plan: Option<&str>) -> UsageResult<UsageSnapshot> {
    let root: Value = serde_json::from_str(body).map_err(|error| {
        UsageError::Unreadable(format!(
            "{ENDPOINT_LABEL}: cannot parse the response: {error}: {}",
            parse::snippet(body)
        ))
    })?;
    let mut windows: Vec<UsageWindow> = Vec::new();
    // limits[] is the newer, richer shape, so it wins for every key it names.
    if let Some(limits) = root.get("limits").and_then(Value::as_array) {
        for limit in limits {
            let Some(window) = parse_limit(limit) else {
                continue;
            };
            if !windows.iter().any(|known| known.key == window.key) {
                windows.push(window);
            }
        }
    }
    for (field, key, label) in [
        ("five_hour", FIVE_HOUR_KEY, FIVE_HOUR_LABEL),
        ("seven_day", WEEK_KEY, WEEK_LABEL),
    ] {
        if windows.iter().any(|known| known.key == key) {
            continue;
        }
        if let Some(window) = parse_window(root.get(field), key, label) {
            windows.push(window);
        }
    }
    if windows.is_empty() {
        return Err(UsageError::Unreadable(format!(
            "{ENDPOINT_LABEL}: the response carries no plan window: {}",
            parse::snippet(body)
        )));
    }
    Ok(UsageSnapshot {
        fetched_at: now,
        windows,
        plan: plan.map(str::to_string),
        credits: None,
        error: None,
        gcp_tos: None,
        reset_credits: None,
    })
}

/// One entry of the limits array: the kind decides the slot, the model scope
/// names the per-model buckets.
fn parse_limit(limit: &Value) -> Option<UsageWindow> {
    let kind = limit
        .get("kind")
        .and_then(Value::as_str)?
        .trim()
        .to_ascii_lowercase();
    let used_percent = used_percent(limit)?;
    let (key, label) = match kind.as_str() {
        "session" => (FIVE_HOUR_KEY.to_string(), FIVE_HOUR_LABEL.to_string()),
        "weekly_all" => (WEEK_KEY.to_string(), WEEK_LABEL.to_string()),
        "weekly_scoped" => {
            let name = parse::text_at(Some(limit), "/scope/model/display_name")
                .or_else(|| parse::text(limit.get("name")))?;
            (name.clone(), name)
        }
        _ => return None,
    };
    Some(window_of(
        key,
        label,
        used_percent,
        parse::epoch_seconds(limit.get("resets_at")),
    ))
}

/// One of the two top-level summary windows (the older deployment shape).
fn parse_window(node: Option<&Value>, key: &str, label: &str) -> Option<UsageWindow> {
    let node = node.filter(|value| value.is_object())?;
    let used_percent = used_percent(node)?;
    Some(window_of(
        key.to_string(),
        label.to_string(),
        used_percent,
        parse::epoch_seconds(node.get("resets_at")),
    ))
}

/// Consumption percentage: newer payloads say percent, older ones utilization,
/// and either may arrive string-encoded.
fn used_percent(node: &Value) -> Option<f64> {
    parse::number(node.get("percent")).or_else(|| parse::number(node.get("utilization")))
}

fn window_of(key: String, label: String, used_percent: f64, resets_at: Option<i64>) -> UsageWindow {
    UsageWindow {
        key,
        label,
        remaining_percent: parse::remaining_from_used(used_percent),
        used: None,
        total: None,
        unit: UsageUnit::Percent,
        resets_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_760_000_000;
    const JUNK: &str = "<!doctype html><html><body>Cloudflare</body></html>";
    const USAGE_LIMITS: &str =
        include_str!("../../../tests/fixtures/claude-code/usage_limits.json");
    const USAGE_TOP_LEVEL: &str =
        include_str!("../../../tests/fixtures/claude-code/usage_top_level.json");

    #[test]
    fn limits_win_over_the_top_level_windows() {
        let snapshot = parse_usage(USAGE_LIMITS, NOW, Some("MAX")).expect("fixture must parse");
        assert_eq!(snapshot.fetched_at, NOW);
        assert_eq!(snapshot.plan.as_deref(), Some("MAX"));
        assert!(snapshot.credits.is_none());
        assert_eq!(snapshot.windows.len(), 3);

        let session = &snapshot.windows[0];
        assert_eq!(session.key, "five_hour");
        assert_eq!(session.label, "5h");
        assert_eq!(
            session.remaining_percent, 88.0,
            "limits session, not top-level"
        );
        assert_eq!(session.resets_at, Some(1_767_243_600));
        assert_eq!(session.unit, UsageUnit::Percent);

        let weekly = &snapshot.windows[1];
        assert_eq!(weekly.key, "week");
        assert_eq!(weekly.label, "7d");
        assert_eq!(weekly.remaining_percent, 36.5, "string-encoded utilization");

        let scoped = &snapshot.windows[2];
        assert_eq!(scoped.key, "Opus");
        assert_eq!(scoped.label, "Opus");
        assert_eq!(scoped.remaining_percent, 10.0);
        assert_eq!(scoped.resets_at, Some(1_767_312_000));
    }

    #[test]
    fn top_level_windows_fall_back_with_mixed_reset_forms() {
        let snapshot = parse_usage(USAGE_TOP_LEVEL, NOW, None).expect("fixture must parse");
        assert_eq!(snapshot.plan, None);
        assert_eq!(snapshot.windows.len(), 2);
        assert_eq!(snapshot.windows[0].key, "five_hour");
        assert_eq!(snapshot.windows[0].remaining_percent, 75.0);
        assert_eq!(snapshot.windows[0].resets_at, Some(1_767_243_600));
        assert_eq!(snapshot.windows[1].key, "week");
        assert_eq!(snapshot.windows[1].remaining_percent, 60.0);
        assert_eq!(
            snapshot.windows[1].resets_at,
            Some(1_767_225_600),
            "millisecond reset is scaled to seconds"
        );
    }

    #[test]
    fn scoped_limits_use_the_model_name_or_drop_out() {
        let named = parse_usage(
            r#"{"limits":[{"kind":"weekly_scoped","percent":10.0,"name":"Sonnet"}]}"#,
            NOW,
            None,
        )
        .expect("name is an accepted scope label");
        assert_eq!(named.windows.len(), 1);
        assert_eq!(named.windows[0].key, "Sonnet");

        let anonymous = parse_usage(
            r#"{"limits":[{"kind":"weekly_scoped","percent":10.0}]}"#,
            NOW,
            None,
        )
        .expect_err("a scoped limit with no name cannot be keyed");
        assert!(matches!(anonymous, UsageError::Unreadable(_)));
    }

    #[test]
    fn rejected_and_busy_statuses_classify() {
        let rejected = interpret(401, "", NOW, None).expect_err("401 must fail");
        assert!(matches!(rejected, UsageError::AuthRequired(_)));

        let busy = interpret(429, "", NOW, None).expect_err("429 must fail");
        assert!(busy.is_transient());

        let down = interpret(500, "", NOW, None).expect_err("5xx must fail");
        assert!(down.is_transient());

        let gone = interpret(403, "", NOW, None).expect_err("403 must fail");
        assert!(matches!(gone, UsageError::Unreadable(_)));
    }

    #[test]
    fn html_body_is_unreadable() {
        let error = interpret(200, JUNK, NOW, None).expect_err("HTML is not quota");
        assert!(matches!(error, UsageError::Unreadable(_)));
        assert!(!error.is_transient());

        let empty =
            interpret(200, "{\"limits\":[]}", NOW, None).expect_err("no window is a shape change");
        assert!(matches!(empty, UsageError::Unreadable(_)));
    }
}
