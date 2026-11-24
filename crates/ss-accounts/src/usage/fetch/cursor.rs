//! Cursor quota: the usage-summary endpoint and the plan tier behind it.
//!
//! GET /api/usage-summary is authenticated with a WorkOS session cookie built
//! from the access token (not with a bearer header), plus the dashboard's own
//! user agent. The payload exposes Auto + Composer and API percentages plus the
//! included allowance, which Cursor counts in US cents. The plan tier comes
//! from /auth/full_stripe_profile; a failure there must not cost the windows.

use base64::Engine;
use serde_json::Value;

use crate::usage::credentials::{Credentials, UsageOutcome};
use crate::usage::error::{UsageError, UsageResult};
use crate::usage::snapshot::{UsageSnapshot, UsageUnit, UsageWindow};

use super::parse;

/// What the account layer sees when a read of the summary fails.
const ENDPOINT_LABEL: &str = "cursor usage-summary";
/// The plan read is best-effort, so it is labelled separately.
const PROFILE_LABEL: &str = "cursor stripe profile";
const SUMMARY_URL: &str = "https://cursor.com/api/usage-summary";
const PROFILE_URL: &str = "https://api2.cursor.sh/auth/full_stripe_profile";
/// This endpoint rejects a bare reqwest user agent; it wants the dashboard's.
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)";
const FREE_PLAN: &str = "FREE";

const TOTAL_KEY: &str = "total";
const TOTAL_LABEL: &str = "Total";

/// The percent pools, in the order a card renders them: key, label, and the
/// field names Cursor has used for the same number.
const PERCENT_POOLS: [(&str, &str, &str, &str); 2] = [
    (
        "auto",
        "Auto + Composer",
        "autoPercentUsed",
        "auto_percent_used",
    ),
    ("api", "API", "apiPercentUsed", "api_percent_used"),
];

/// Read the Cursor quota for one account.
pub fn fetch(creds: &Credentials) -> UsageResult<UsageOutcome> {
    if crate::switch::cursor::is_safe_storage_ciphertext(&creds.tokens.access_token) {
        return Err(UsageError::AuthRequired(
            "Cursor 凭据是客户端加密密文，请切回该账号刷新或重新授权".to_string(),
        ));
    }
    let reading = super::tokens_for_read("cursor", creds)?;
    let access_token = reading.tokens.access_token.as_str();
    let user_id = user_id_from_access_token(access_token).ok_or_else(|| {
        UsageError::Unreadable(format!(
            "{ENDPOINT_LABEL}: the access token does not name a WorkOS user"
        ))
    })?;
    // The session cookie is <user id>::<access token>, URL-encoded as the
    // dashboard writes it.
    let cookie = format!("WorkosCursorSessionToken={user_id}%3A%3A{access_token}");
    let headers = [
        ("Accept", "application/json".to_string()),
        ("Cookie", cookie),
        ("User-Agent", USER_AGENT.to_string()),
    ];
    let answer = super::http_get(ENDPOINT_LABEL, SUMMARY_URL, &headers)?;
    let plan = plan_from_profile(access_token);
    let snapshot = interpret(
        answer.status,
        &answer.body,
        super::unix_now(),
        plan.as_deref(),
    )?;
    let identity = if creds.needs_identity {
        crate::oauth::agents::cursor::read_identity_blocking(&reading.tokens)
    } else {
        None
    };
    Ok(UsageOutcome {
        snapshot,
        tokens: reading.tokens,
        provider_state: reading.provider_state,
        identity,
    })
}

/// What one summary reply means: status and body in, snapshot out.
fn interpret(status: u16, body: &str, now: i64, plan: Option<&str>) -> UsageResult<UsageSnapshot> {
    if !(200..300).contains(&status) {
        return Err(super::classify_status(ENDPOINT_LABEL, status, body));
    }
    parse_usage(body, now, plan)
}

/// Summary JSON to snapshot. Pure: the fixtures drive it without a socket.
pub(crate) fn parse_usage(body: &str, now: i64, plan: Option<&str>) -> UsageResult<UsageSnapshot> {
    let root: Value = serde_json::from_str(body).map_err(|error| {
        UsageError::Unreadable(format!(
            "{ENDPOINT_LABEL}: cannot parse the response: {error}: {}",
            parse::snippet(body)
        ))
    })?;
    let plan = plan.map(str::to_string).or_else(|| {
        parse::text_in(&root, &["membershipType", "membership_type"])
            .map(|value| value.to_ascii_uppercase())
    });
    let windows = windows(&root);
    let is_unlimited = root.get("isUnlimited").and_then(Value::as_bool) == Some(true);
    let credits = if windows.is_empty() && is_unlimited {
        Some(crate::usage::snapshot::Credits {
            label: "Credits".to_string(),
            remaining: None,
            unlimited: true,
            unit: UsageUnit::Percent,
        })
    } else {
        None
    };
    if windows.is_empty() && credits.is_none() {
        return Err(UsageError::Unreadable(format!(
            "{ENDPOINT_LABEL}: the response carries no quota window: {}",
            parse::snippet(body)
        )));
    }
    Ok(UsageSnapshot {
        fetched_at: now,
        windows,
        plan,
        credits,
        error: None,
        gcp_tos: None,
        reset_credits: None,
    })
}

/// Resolve the plan node across the shapes Cursor ships, newest first.
fn plan_node(root: &Value) -> Option<&Value> {
    [
        "/individualUsage/plan",
        "/individual_usage/plan",
        "/teamUsage/plan",
        "/team_usage/plan",
    ]
    .into_iter()
    .find_map(|pointer| root.pointer(pointer).filter(|value| value.is_object()))
    .or_else(|| root.get("planUsage").filter(|value| value.is_object()))
    .or_else(|| root.get("plan_usage").filter(|value| value.is_object()))
}

/// The percent pools, then the total allowance (money when the payload says so).
fn windows(root: &Value) -> Vec<UsageWindow> {
    let Some(node) = plan_node(root).filter(|value| value.is_object()) else {
        return Vec::new();
    };
    let resets_at = parse::epoch_seconds(node.get("billingCycleEnd"))
        .or_else(|| parse::epoch_seconds(root.get("billingCycleEnd")))
        .or_else(|| parse::epoch_seconds(node.get("resetAt")))
        .or_else(|| parse::epoch_seconds(root.get("resetAt")))
        .or_else(|| parse::epoch_seconds(node.get("periodEnd")));
    let mut windows = Vec::new();
    for (key, label, camel, snake) in PERCENT_POOLS {
        if let Some(used_percent) = percent_in(node, camel, snake) {
            windows.push(percent_window(key, label, used_percent, resets_at));
        }
    }
    if let Some(total) = total_window(node, resets_at) {
        windows.push(total);
    }
    windows
}

/// The included allowance: money when the payload carries cents, else the
/// total percentage on its own.
fn total_window(node: &Value, resets_at: Option<i64>) -> Option<UsageWindow> {
    let percent_used = percent_in(node, "totalPercentUsed", "total_percent_used");
    let spent = parse::number_in(
        node,
        &[
            "used",
            "totalSpend",
            "total_spend",
            "includedSpend",
            "included_spend",
        ],
    )
    .filter(|value| *value >= 0.0);
    let limit = parse::number_in(node, &["limit"]).filter(|value| *value > 0.0);
    let bonus = parse::number_at(node, "/breakdown/bonus").unwrap_or(0.0);
    let bonus_covers_included = bonus > 0.0
        || matches!((spent, limit, percent_used), (Some(s), Some(l), Some(p)) if s >= l && p < 100.0);
    if bonus_covers_included
        && let Some(used_percent) = percent_used
    {
        return Some(percent_window(
            TOTAL_KEY,
            TOTAL_LABEL,
            used_percent,
            resets_at,
        ));
    }
    if let (Some(spent), Some(limit)) = (spent, limit) {
        return Some(UsageWindow {
            key: TOTAL_KEY.to_string(),
            label: TOTAL_LABEL.to_string(),
            remaining_percent: parse::remaining_from_amounts(spent, limit),
            used: Some(spent),
            total: Some(limit),
            unit: UsageUnit::UsdCents,
            resets_at,
        });
    }
    // Older payloads carry only the cap (breakdown.total), so the spend has to
    // come from the total percentage.
    let cap = parse::number_at(node, "/breakdown/total").filter(|value| *value > 0.0);
    if let (Some(used_percent), Some(cap)) = (percent_used, cap) {
        return Some(UsageWindow {
            key: TOTAL_KEY.to_string(),
            label: TOTAL_LABEL.to_string(),
            remaining_percent: parse::remaining_from_used(used_percent),
            used: Some(used_percent / 100.0 * cap),
            total: Some(cap),
            unit: UsageUnit::UsdCents,
            resets_at,
        });
    }
    percent_used.map(|used_percent| percent_window(TOTAL_KEY, TOTAL_LABEL, used_percent, resets_at))
}

/// A percentage-only window: the unit says so and the amounts stay empty.
fn percent_window(
    key: &str,
    label: &str,
    used_percent: f64,
    resets_at: Option<i64>,
) -> UsageWindow {
    UsageWindow {
        key: key.to_string(),
        label: label.to_string(),
        remaining_percent: parse::remaining_from_used(used_percent),
        used: None,
        total: None,
        unit: UsageUnit::Percent,
        resets_at,
    }
}

fn percent_in(node: &Value, camel: &str, snake: &str) -> Option<f64> {
    parse::number_in(node, &[camel, snake])
}

/// The plan tier from the Stripe profile; a failure leaves the plan unset
/// instead of failing the whole read.
fn plan_from_profile(access_token: &str) -> Option<String> {
    let headers = [
        ("Accept", "application/json".to_string()),
        ("Authorization", format!("Bearer {access_token}")),
    ];
    let answer = super::http_get(PROFILE_LABEL, PROFILE_URL, &headers).ok()?;
    if !(200..300).contains(&answer.status) {
        return None;
    }
    parse_profile(&answer.body)
}

/// Profile JSON to a plan name. Pure.
pub(crate) fn parse_profile(body: &str) -> Option<String> {
    let profile: Value = serde_json::from_str(body).ok()?;
    let bool_in = |keys: &[&str]| {
        keys.iter()
            .find_map(|key| profile.get(*key).and_then(Value::as_bool))
            .unwrap_or(false)
    };
    if bool_in(&["is_enterprise", "isEnterprise"]) {
        return Some("ENTERPRISE".to_string());
    }
    if bool_in(&["is_team_member", "isTeamMember"]) {
        return Some("TEAM".to_string());
    }
    for key in [
        "individualMembershipType",
        "membershipType",
        "individual_membership_type",
        "membership_type",
    ] {
        if let Some(name) = parse::text(profile.get(key)) {
            return Some(name.to_ascii_uppercase());
        }
    }
    Some(FREE_PLAN.to_string())
}

/// The WorkOS user id the session cookie needs: the access token's JWT claim
/// sub, keeping the segment after the separator (auth0|user_...).
pub(crate) fn user_id_from_access_token(access_token: &str) -> Option<String> {
    let payload = access_token.split('.').nth(1)?;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload.trim_end_matches('='))
        .ok()?;
    let claims: Value = serde_json::from_slice(&decoded).ok()?;
    let sub = claims.get("sub")?.as_str()?.trim();
    let user_id = sub.rsplit('|').next()?.trim();
    user_id.starts_with("user_").then(|| user_id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_760_000_000;
    const JUNK: &str = "<!doctype html><html><body>Just a moment...</body></html>";
    const USAGE_SUMMARY: &str = include_str!("../../../tests/fixtures/cursor/usage_summary.json");
    const USAGE_PERCENT_ONLY: &str =
        include_str!("../../../tests/fixtures/cursor/usage_summary_percent_only.json");
    const USAGE_BREAKDOWN: &str =
        include_str!("../../../tests/fixtures/cursor/usage_summary_breakdown.json");
    const STRIPE_PROFILE: &str =
        include_str!("../../../tests/fixtures/cursor/stripe_profile.json");

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.01,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn summary_keeps_percent_pools_and_cents_allowance() {
        let plan = parse_profile(STRIPE_PROFILE);
        assert_eq!(plan.as_deref(), Some("PRO"));
        let snapshot =
            parse_usage(USAGE_SUMMARY, NOW, plan.as_deref()).expect("fixture must parse");
        assert_eq!(snapshot.fetched_at, NOW);
        assert_eq!(snapshot.plan.as_deref(), Some("PRO"));
        assert_eq!(snapshot.windows.len(), 3);

        let auto = &snapshot.windows[0];
        assert_eq!(auto.key, "auto");
        assert_eq!(auto.label, "Auto + Composer");
        assert_eq!(auto.remaining_percent, 49.0);
        assert_eq!(auto.unit, UsageUnit::Percent);
        assert_eq!(auto.used, None);

        let api = &snapshot.windows[1];
        assert_eq!(api.key, "api");
        assert_eq!(api.remaining_percent, 57.0);

        let total = &snapshot.windows[2];
        assert_eq!(total.key, "total");
        assert_eq!(total.label, "Total");
        assert_eq!(total.unit, UsageUnit::UsdCents);
        assert_eq!(total.used, Some(2375.0));
        assert_eq!(total.total, Some(9495.0));
        assert_close(total.remaining_percent, 74.987);
        assert_eq!(total.resets_at, Some(1_767_225_600));
    }

    #[test]
    fn percent_only_summary_stays_on_the_percent_unit() {
        let snapshot = parse_usage(USAGE_PERCENT_ONLY, NOW, None).expect("fixture must parse");
        assert_eq!(snapshot.plan, None);
        assert_eq!(snapshot.windows.len(), 3);
        for window in &snapshot.windows {
            assert_eq!(window.unit, UsageUnit::Percent);
            assert_eq!(window.used, None);
            assert_eq!(window.total, None);
        }
        assert_eq!(snapshot.windows[0].key, "auto");
        assert_eq!(snapshot.windows[0].remaining_percent, 80.0);
        assert_eq!(snapshot.windows[1].remaining_percent, 69.5);
        assert_eq!(snapshot.windows[2].key, "total");
        assert_eq!(snapshot.windows[2].remaining_percent, 75.0);
    }

    #[test]
    fn older_breakdown_cap_becomes_a_cents_allowance() {
        let snapshot = parse_usage(USAGE_BREAKDOWN, NOW, None).expect("fixture must parse");
        assert_eq!(snapshot.windows.len(), 1);
        let total = &snapshot.windows[0];
        assert_eq!(total.key, "total");
        assert_eq!(total.unit, UsageUnit::UsdCents);
        assert_eq!(total.used, Some(800.0), "40% of a 2000 cent cap");
        assert_eq!(total.total, Some(2000.0));
        assert_eq!(total.remaining_percent, 60.0);
    }

    #[test]
    fn cookie_user_id_comes_from_the_jwt_subject() {
        let claims = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(r#"{"sub":"auth0|user_2abc","exp":1760000000}"#);
        let token = format!("header.{claims}.signature");
        assert_eq!(
            user_id_from_access_token(&token).as_deref(),
            Some("user_2abc")
        );

        let no_prefix =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"sub":"auth0|2abc"}"#);
        assert_eq!(
            user_id_from_access_token(&format!("header.{no_prefix}.signature")),
            None,
            "a subject without the user_ prefix cannot key the cookie"
        );
        assert_eq!(user_id_from_access_token("not-a-jwt"), None);

        assert_eq!(
            parse_profile(r#"{"is_enterprise":true}"#).as_deref(),
            Some("ENTERPRISE")
        );
        assert_eq!(
            parse_profile(r#"{"is_team_member":true}"#).as_deref(),
            Some("TEAM")
        );
        assert_eq!(parse_profile("{}").as_deref(), Some("FREE"));
        assert_eq!(parse_profile(JUNK), None);
    }

    #[test]
    fn statuses_and_junk_bodies_classify() {
        let rejected = interpret(401, "", NOW, None).expect_err("401 must fail");
        assert!(matches!(rejected, UsageError::AuthRequired(_)));

        let busy = interpret(429, "", NOW, None).expect_err("429 must fail");
        assert!(busy.is_transient());

        let down = interpret(502, "", NOW, None).expect_err("5xx must fail");
        assert!(down.is_transient());

        let gone = interpret(404, "not found", NOW, None).expect_err("404 must fail");
        assert!(matches!(gone, UsageError::Unreadable(_)));

        let junk = interpret(200, JUNK, NOW, None).expect_err("HTML is not quota");
        assert!(matches!(junk, UsageError::Unreadable(_)));
        assert!(!junk.is_transient());

        let empty = interpret(200, "{}", NOW, None).expect_err("no plan node is a shape change");
        assert!(matches!(empty, UsageError::Unreadable(_)));
    }

    #[test]
    fn ultra_bonus_uses_total_percent_used_and_falls_back_to_summary_membership_type() {
        let ultra = r#"{
            "billingCycleEnd": "2026-11-16T17:43:37.000Z",
            "membershipType": "ultra",
            "individualUsage": {
                "plan": {
                    "used": 40000,
                    "limit": 40000,
                    "remaining": 0,
                    "breakdown": { "included": 40000, "bonus": 225206, "total": 265206 },
                    "autoPercentUsed": 85.0,
                    "apiPercentUsed": 100.0,
                    "totalPercentUsed": 85.5
                }
            }
        }"#;
        let snapshot = parse_usage(ultra, NOW, None).expect("ultra summary");
        assert_eq!(snapshot.plan.as_deref(), Some("ULTRA"));
        assert_eq!(snapshot.windows.len(), 3);
        let total = &snapshot.windows[2];
        assert_eq!(total.key, "total");
        assert_eq!(total.unit, UsageUnit::Percent);
        assert_close(total.remaining_percent, 14.5);

        assert_eq!(
            parse_profile(r#"{"isTeamMember":true,"membershipType":"pro"}"#).as_deref(),
            Some("TEAM")
        );
        assert_eq!(
            parse_profile(r#"{"isTeamMember":false,"individualMembershipType":"ultra","membershipType":"pro"}"#).as_deref(),
            Some("ULTRA")
        );

        let unlimited = parse_usage(r#"{"isUnlimited":true,"membershipType":"enterprise"}"#, NOW, None)
            .expect("unlimited credits");
        assert_eq!(unlimited.plan.as_deref(), Some("ENTERPRISE"));
        assert!(unlimited.windows.is_empty());
        assert_eq!(unlimited.credits.as_ref().map(|c| c.unlimited), Some(true));

        let ciphertext = Credentials {
            tokens: crate::oauth::TokenSet {
                access_token: "cursor-safe-storage:v1:encrypted".to_string(),
                ..Default::default()
            },
            account_id: None,
            provider_state: None,
            needs_identity: false,
        };
        let err = fetch(&ciphertext).expect_err("ciphertext rejected");
        assert!(matches!(err, UsageError::AuthRequired(_)), "got {err:?}");
    }
}
