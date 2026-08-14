use super::*;

fn json(raw: &str) -> Value {
    serde_json::from_str(raw).expect("fixture is valid JSON")
}

// ── credential store ───────────────────────────────────────────────

#[test]
fn expires_at_is_read_as_milliseconds_not_seconds() {
    let oauth = parse_credentials(
        r#"{"claudeAiOauth":{"accessToken":"at","expiresAt":1786000000000,
            "refreshToken":"rt","subscriptionType":"max"}}"#,
    )
    .expect("credential blob parses");
    // 1_786_000_000_000 ms == 1_786_000_000 s. Reading the raw field would
    // put the expiry ~54000 years out and disable every staleness check.
    assert_eq!(oauth.expires_at_seconds(), Some(1_786_000_000));
    assert_eq!(oauth.plan_name().as_deref(), Some("MAX"));
    assert_eq!(oauth.access_token(), Some("at"));
}

#[test]
fn credential_blob_without_a_usable_token_is_rejected() {
    assert_eq!(parse_credentials("{}"), None);
    assert_eq!(parse_credentials(r#"{"claudeAiOauth":{}}"#), None);
    assert_eq!(
        parse_credentials(r#"{"claudeAiOauth":{"accessToken":"   "}}"#),
        None
    );
    assert_eq!(parse_credentials("not json"), None);
    // An mcpOAuth-only blob must not read as a Claude login.
    assert_eq!(parse_credentials(r#"{"mcpOAuth":{"srv":{"a":1}}}"#), None);
}

#[test]
fn keychain_account_falls_back_to_the_cli_literal() {
    // Whatever the runner's environment is, the label is never blank —
    // Claude Code itself falls back to this exact string.
    let account = keychain_account();
    assert!(!account.trim().is_empty());
    if std::env::var("USER").is_err() && std::env::var("LOGNAME").is_err() {
        assert_eq!(account, KEYCHAIN_ACCOUNT_FALLBACK);
    }
}

// ── usage payload: schema compatibility ────────────────────────────

#[test]
fn legacy_payload_with_only_top_level_windows_is_read() {
    let usage = parse_usage(&json(
        r#"{
            "five_hour": {"utilization": 42.4, "resets_at": "2026-08-14T18:00:00Z"},
            "seven_day": {"utilization": 61.6, "resets_at": "2026-08-20T00:00:00Z"}
        }"#,
    ));
    let hourly = usage.hourly.expect("five_hour becomes the 5h bar");
    assert_eq!(hourly.label, "5h");
    assert_eq!(hourly.percent, Some(42));
    assert_eq!(hourly.reset_at, Some(1_786_730_400));
    let weekly = usage.weekly.expect("seven_day becomes the 7d bar");
    assert_eq!(weekly.label, "7d");
    assert_eq!(weekly.percent, Some(62));
}

#[test]
fn modern_payload_with_only_limits_is_read() {
    let usage = parse_usage(&json(
        r#"{"limits":[
            {"kind":"session","percent":12,"resets_at":"2026-08-14T18:00:00Z"},
            {"kind":"weekly_all","percent":80,"resets_at":"2026-08-20T00:00:00Z"}
        ]}"#,
    ));
    assert_eq!(usage.hourly.as_ref().map(|w| w.percent), Some(Some(12)));
    assert_eq!(usage.weekly.as_ref().map(|w| w.percent), Some(Some(80)));
    assert_eq!(usage.hourly.expect("5h").label, "5h");
}

#[test]
fn limits_win_over_the_legacy_top_level_fields() {
    let usage = parse_usage(&json(
        r#"{
            "five_hour": {"utilization": 1, "resets_at": "2020-01-01T00:00:00Z"},
            "seven_day": {"utilization": 2, "resets_at": "2020-01-01T00:00:00Z"},
            "limits":[
                {"kind":"session","percent":55,"resets_at":"2026-08-14T18:00:00Z"},
                {"kind":"weekly_all","percent":66,"resets_at":"2026-08-20T00:00:00Z"}
            ]
        }"#,
    ));
    assert_eq!(usage.hourly.expect("5h").percent, Some(55));
    assert_eq!(usage.weekly.expect("7d").percent, Some(66));
}

#[test]
fn a_partial_limits_array_still_falls_back_per_window() {
    // Only the session limit migrated; the weekly bar must not vanish.
    let usage = parse_usage(&json(
        r#"{
            "seven_day": {"utilization": 33, "resets_at": "2026-08-20T00:00:00Z"},
            "limits":[{"kind":"session","percent":7}]
        }"#,
    ));
    assert_eq!(usage.hourly.expect("5h").percent, Some(7));
    assert_eq!(usage.weekly.expect("7d").percent, Some(33));
}

#[test]
fn unknown_limit_kinds_are_skipped_not_fatal() {
    let usage = parse_usage(&json(
        r#"{"limits":[
            {"kind":"opus_hourly_v3","percent":99},
            {"kind":"session","percent":10},
            {"kind":null,"percent":50},
            {"percent":50},
            {},
            "not-an-object",
            {"kind":"weekly_all","percent":20}
        ]}"#,
    ));
    assert_eq!(usage.hourly.expect("5h").percent, Some(10));
    assert_eq!(usage.weekly.expect("7d").percent, Some(20));
}

#[test]
fn weekly_scoped_limits_hang_off_the_weekly_bar() {
    let usage = parse_usage(&json(
        r#"{"limits":[
            {"kind":"weekly_all","percent":40,"resets_at":"2026-08-20T00:00:00Z"},
            {"kind":"weekly_scoped","percent":90,"resets_at":"2026-08-20T00:00:00Z",
             "scope":{"model":{"display_name":"Opus 5"}}},
            {"kind":"weekly_scoped","percent":15,"resets_at":"2026-08-20T00:00:00Z",
             "scope":{"model":{"display_name":"Sonnet 5"}}},
            {"kind":"weekly_scoped","percent":50, "scope":{"surface":"cli"}}
        ]}"#,
    ));
    let weekly = usage.weekly.expect("7d");
    assert_eq!(weekly.label, "7d");
    assert_eq!(weekly.percent, Some(40));
    let labels: Vec<_> = weekly.breakdown.iter().map(|w| w.label.as_str()).collect();
    // The unnamed scope has no honest label, so that one bar drops.
    assert_eq!(labels, ["Opus 5", "Sonnet 5"]);
    assert_eq!(weekly.breakdown[0].percent, Some(90));
    // Each breakdown row draws its own reset chip, so a scoped limit that
    // resets with its parent drops the redundant countdown.
    assert!(weekly.reset_at.is_some());
    assert!(weekly.breakdown.iter().all(|w| w.reset_at.is_none()));
}

#[test]
fn a_scoped_limit_with_its_own_reset_keeps_the_countdown() {
    let usage = parse_usage(&json(
        r#"{"limits":[
            {"kind":"weekly_all","percent":40,"resets_at":"2026-08-20T00:00:00Z"},
            {"kind":"weekly_scoped","percent":90,"resets_at":"2026-08-18T00:00:00Z",
             "scope":{"model":{"display_name":"Opus 5"}}}
        ]}"#,
    ));
    let weekly = usage.weekly.expect("7d");
    assert_eq!(weekly.breakdown[0].label, "Opus 5");
    assert!(
        weekly.breakdown[0].reset_at.is_some(),
        "a genuinely different reset is real information"
    );
    assert_ne!(weekly.breakdown[0].reset_at, weekly.reset_at);
}

#[test]
fn a_scoped_limit_alone_becomes_the_weekly_bar() {
    let usage = parse_usage(&json(
        r#"{"limits":[
            {"kind":"weekly_scoped","percent":30,"scope":{"model":{"name":"Opus"}}}
        ]}"#,
    ));
    let weekly = usage.weekly.expect("scoped limit still renders");
    assert_eq!(weekly.label, "Opus");
    assert_eq!(weekly.percent, Some(30));
    assert!(weekly.breakdown.is_empty());
}

#[test]
fn dollar_fields_of_any_shape_never_break_a_window() {
    // `used_dollars` / `limit_dollars` have already changed shape upstream
    // and are not part of the quota model; whatever they hold, the percent
    // bars must survive.
    for dollars in [
        r#""12.50""#,
        "12.5",
        "null",
        r#"{"amount":12.5,"currency":"USD"}"#,
    ] {
        let usage = parse_usage(&json(&format!(
            r#"{{"five_hour":{{"utilization":25,"used_dollars":{dollars},
                "limit_dollars":{dollars}}},
                "limits":[{{"kind":"weekly_all","percent":75,
                "used_dollars":{dollars}}}]}}"#
        )));
        assert_eq!(
            usage.hourly.expect("5h survives").percent,
            Some(25),
            "used_dollars = {dollars}"
        );
        assert_eq!(usage.weekly.expect("7d survives").percent, Some(75));
    }
    // …and the field being absent entirely is the common case.
    let usage = parse_usage(&json(r#"{"five_hour":{"utilization":25}}"#));
    assert_eq!(usage.hourly.expect("5h").percent, Some(25));
}

#[test]
fn an_unreadable_window_is_dropped_without_failing_the_account() {
    let usage = parse_usage(&json(
        r#"{
            "five_hour": {"utilization": "not a number", "resets_at": "nonsense"},
            "limits":[{"kind":"weekly_all","percent":48,"resets_at":"nonsense"}]
        }"#,
    ));
    assert!(usage.hourly.is_none(), "unreadable percent drops that bar");
    let weekly = usage.weekly.expect("the readable bar survives");
    assert_eq!(weekly.percent, Some(48));
    assert_eq!(weekly.reset_at, None, "unparseable reset is simply unknown");
}

#[test]
fn empty_and_shapeless_payloads_yield_no_windows() {
    for raw in ["{}", r#"{"limits":[]}"#, r#"{"limits":"nope"}"#, "[]"] {
        let usage = parse_usage(&json(raw));
        assert!(usage.hourly.is_none(), "{raw}");
        assert!(usage.weekly.is_none(), "{raw}");
    }
}

// ── lenient scalars ────────────────────────────────────────────────

#[test]
fn percent_accepts_numbers_and_numeric_strings_and_clamps() {
    assert_eq!(lenient_percent(Some(&json("42"))), Some(42));
    assert_eq!(lenient_percent(Some(&json("42.6"))), Some(43));
    assert_eq!(lenient_percent(Some(&json(r#""42.6""#))), Some(43));
    assert_eq!(lenient_percent(Some(&json("140"))), Some(100));
    assert_eq!(lenient_percent(Some(&json("-5"))), Some(0));
    assert_eq!(lenient_percent(Some(&json("null"))), None);
    assert_eq!(lenient_percent(Some(&json(r#""abc""#))), None);
    assert_eq!(lenient_percent(None), None);
}

#[test]
fn reset_at_accepts_rfc3339_seconds_and_milliseconds() {
    assert_eq!(
        lenient_reset_at(Some(&json(r#""2026-08-14T18:00:00Z""#))),
        Some(1_786_730_400)
    );
    assert_eq!(
        lenient_reset_at(Some(&json("1786060800"))),
        Some(1_786_060_800)
    );
    assert_eq!(
        lenient_reset_at(Some(&json("1786060800000"))),
        Some(1_786_060_800)
    );
    assert_eq!(lenient_reset_at(Some(&json(r#""tomorrow""#))), None);
    assert_eq!(lenient_reset_at(None), None);
}

// ── quota model shape ──────────────────────────────────────────────

#[test]
fn windows_are_percent_only_so_the_shared_meter_reads_them_as_rate_limits() {
    let usage = parse_usage(&json(
        r#"{"limits":[{"kind":"session","percent":30,
            "resets_at":"2026-08-14T18:00:00Z"}]}"#,
    ));
    let hourly = usage.hourly.expect("5h");
    // `used == percent`, `total == 100`: the shape `UsageSimpleWindow`
    // renders as a rate-limit bar, identical to Codex's 5h/7d.
    assert_eq!(hourly.used, 30);
    assert_eq!(hourly.total, Some(100));
    assert_eq!(hourly.percent, Some(30));
}
