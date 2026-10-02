//! Slice 07 tests: the merge's golden classes, the ±2s boundary, the
//! empty-success and failure rules, the request-id stage (through the
//! projection seam, since gateway `Record` cannot carry an id yet), the
//! window floors with an injected clock, and the model-id normalization
//! goldens.
//!
//! Slice 09 adds the summarize section: the UTC period floors, the
//! auto-bucketed series, the four dimensions, unpriced counting, and the
//! injected price table's arithmetic.

use std::path::PathBuf;

use super::*;
use chrono::{TimeZone, Utc};
use skillstar_usage::sessions::SessionTokens;

// ── fixtures ──────────────────────────────────────────────────────────

const TOKENS: [u64; 4] = [100, 20, 5, 0];

fn counts(tokens: [u64; 4]) -> TokenCounts {
    TokenCounts {
        input: tokens[0],
        output: tokens[1],
        cache_read: tokens[2],
        cache_write: tokens[3],
        reasoning: 0,
    }
}

/// A succeeded gateway record: `at + LATENCY` is its end time.
const LATENCY: u64 = 1_000;

fn record(at: i64, session: &str, tokens: [u64; 4]) -> Record {
    Record {
        at,
        agent: "claude-code".to_string(),
        session: session.to_string(),
        model_asked: "gpt-5".to_string(),
        model_answered: "gpt-5".to_string(),
        catalog: String::new(),
        account: "key:0011aabb".to_string(),
        tokens: counts(tokens),
        status: 200,
        latency_ms: LATENCY,
        error_kind: None,
        endpoint: "/v1/messages".to_string(),
    }
}

fn failed_record(at: i64, session: &str) -> Record {
    Record {
        status: 500,
        error_kind: Some(ErrorKind::Upstream),
        tokens: counts([0, 0, 0, 0]),
        ..record(at, session, [0, 0, 0, 0])
    }
}

fn call(at: i64, session: &str, tokens: [u64; 4]) -> SessionCall {
    SessionCall {
        at,
        agent: "claude-code".to_string(),
        session: session.to_string(),
        model_asked: "gpt-5".to_string(),
        model_answered: "gpt-5".to_string(),
        tokens: SessionTokens {
            input: tokens[0],
            output: tokens[1],
            cache_read: tokens[2],
            cache_write: tokens[3],
        },
        effort: None,
        request_id: None,
        error_kind: None,
        latency_ms: Some(LATENCY),
        file: PathBuf::from("/sandbox/home/.claude/projects/p/one.jsonl"),
        from: 0,
        to: 10,
    }
}

fn failed_call(at: i64, session: &str) -> SessionCall {
    let mut failed = call(at, session, [0, 0, 0, 0]);
    failed.error_kind = Some("api_error".to_string());
    failed
}

/// A record and the file call of the very same turn: `call_at` sits inside
/// the record's end ±2s window.
fn same_turn(at: i64, session: &str) -> (Record, SessionCall) {
    let record = record(at, session, TOKENS);
    let call = call(at + i64::try_from(LATENCY).expect("latency fits i64"), session, TOKENS);
    (record, call)
}

// ── golden classes ────────────────────────────────────────────────────

#[test]
fn pure_gateway_call_shows_as_gateway() {
    let record = record(1_000_000, "s1", TOKENS);
    let view = consumption_view(std::slice::from_ref(&record), &[], Window::all());
    assert_eq!(view.rows.len(), 1);
    assert_eq!(view.rows[0].source, CallSource::Gateway);
    let unified = &view.rows[0].call;
    assert_eq!(unified.at, record.at);
    assert_eq!(unified.agent, "claude-code");
    assert_eq!(unified.status, Some(200));
    assert_eq!(unified.endpoint, "/v1/messages");
    assert_eq!(unified.account, "key:0011aabb");
    assert_eq!(unified.catalog, "");
    assert_eq!(unified.request_id, None);
    assert_eq!(unified.file, None);
}

#[test]
fn bypass_call_shows_as_session_file_with_unknown_catalog() {
    let log = call(1_000_000, "s1", TOKENS);
    let view = consumption_view(&[], std::slice::from_ref(&log), Window::all());
    assert_eq!(view.rows.len(), 1);
    assert_eq!(view.rows[0].source, CallSource::SessionFile);
    let unified = &view.rows[0].call;
    assert_eq!(unified.catalog, SESSION_UNKNOWN_CATALOG);
    assert_eq!(unified.account, "");
    assert_eq!(unified.status, None);
    assert_eq!(unified.endpoint, "");
    assert_eq!(unified.request_id, None);
    assert_eq!(unified.file, Some(log.file.clone()));
    assert_eq!(unified.from, Some(0));
    assert_eq!(unified.to, Some(10));
    assert_eq!(unified.tokens, counts(TOKENS));
}

#[test]
fn same_call_on_both_sides_is_counted_once() {
    let (record, log) = same_turn(1_000_000, "s1");
    let matches = gateway_matches(std::slice::from_ref(&record), std::slice::from_ref(&log));
    assert_eq!(matches.record_of(0), Some(0), "the record consumes the call");
    assert_eq!(matches.len(), 1);
    let view = consumption_view(&[record], &[log], Window::all());
    assert_eq!(view.rows.len(), 1, "the log line is swallowed");
    assert_eq!(view.rows[0].source, CallSource::Gateway);
}

#[test]
fn ambiguous_double_call_keeps_both_sides_visible() {
    // One record, two identical candidates: pairing would be a guess, so
    // every row stays visible (no double-swallow either).
    let (record, log) = same_turn(1_000_000, "s1");
    let twin = {
        let mut twin = log.clone();
        twin.file = PathBuf::from("/sandbox/home/.claude/projects/p/two.jsonl");
        twin
    };
    let view = consumption_view(&[record], &[log, twin], Window::all());
    assert_eq!(view.rows.len(), 3, "gateway row plus both session rows");
    assert_eq!(
        view.rows.iter().filter(|row| row.source == CallSource::SessionFile).count(),
        2
    );
    assert_eq!(
        view.rows.iter().filter(|row| row.source == CallSource::Gateway).count(),
        1
    );
}

#[test]
fn ambiguous_double_record_keeps_all_visible() {
    // Mirror ambiguity: two identical records, one file call.
    let (record, log) = same_turn(1_000_000, "s1");
    let view = consumption_view(&[record.clone(), record], &[log], Window::all());
    assert_eq!(view.rows.len(), 3, "two gateway rows plus the session row");
    assert_eq!(
        view.rows.iter().filter(|row| row.source == CallSource::Gateway).count(),
        2
    );
}

#[test]
fn two_identical_pairs_still_match_each_exactly_once() {
    // Two records and two calls with identical shapes: every candidate is
    // ambiguous, so nothing pairs (magpie: counts must be one on both
    // sides). Uniqueness, not greedy pairing.
    let (a, ca) = same_turn(1_000_000, "s1");
    let (b, cb) = same_turn(1_000_000, "s1");
    let matches = gateway_matches(&[a, b], &[ca, cb]);
    assert!(matches.is_empty(), "identical rows cannot be told apart: {matches:?}");
}

// ── the ±2s clock boundary ────────────────────────────────────────────

#[test]
fn call_at_exactly_two_seconds_around_the_end_pairs() {
    let at = 1_000_000;
    let record = record(at, "s1", TOKENS);
    let end = at + i64::try_from(LATENCY).expect("latency fits i64");
    for offset in [-2_000, 0, 2_000] {
        let log = call(end + offset, "s1", TOKENS);
        let matches = gateway_matches(std::slice::from_ref(&record), &[log]);
        assert_eq!(
            matches.record_of(0),
            Some(0),
            "offset {offset} ms sits on the inclusive boundary"
        );
    }
}

#[test]
fn call_beyond_two_seconds_from_the_end_does_not_pair() {
    let at = 1_000_000;
    let record = record(at, "s1", TOKENS);
    let end = at + i64::try_from(LATENCY).expect("latency fits i64");
    for offset in [-2_001, 2_001, 60_000] {
        let log = call(end + offset, "s1", TOKENS);
        let matches = gateway_matches(std::slice::from_ref(&record), &[log]);
        assert!(
            matches.is_empty(),
            "offset {offset} ms is outside the window: {matches:?}"
        );
    }
}

// ── empty successes and failure agreement ─────────────────────────────

#[test]
fn empty_success_never_pairs() {
    // Zero tokens on a successful call carry too little evidence, even
    // with everything else aligned.
    let record = record(1_000_000, "s1", [0, 0, 0, 0]);
    let log = call(1_001_000, "s1", [0, 0, 0, 0]);
    assert!(gateway_matches(&[record], &[log]).is_empty());
}

#[test]
fn failed_call_pairs_with_failed_record() {
    let record = failed_record(1_000_000, "s1");
    let log = failed_call(1_001_000, "s1");
    let matches = gateway_matches(std::slice::from_ref(&record), &[log]);
    assert_eq!(matches.record_of(0), Some(0));
    let view = consumption_view(&[record], &[failed_call(9_000_000, "s2")], Window::all());
    assert_eq!(view.rows.len(), 2);
    let failed_row = view
        .rows
        .iter()
        .find(|row| row.source == CallSource::Gateway)
        .expect("the failed record is visible");
    assert_eq!(failed_row.call.error_kind.as_deref(), Some("upstream"));
}

#[test]
fn success_and_failure_never_pair() {
    let record = record(1_000_000, "s1", TOKENS);
    let log = failed_call(1_001_000, "s1");
    assert!(gateway_matches(&[record], &[log]).is_empty());

    let record = failed_record(1_000_000, "s1");
    let log = call(1_001_000, "s1", TOKENS);
    assert!(gateway_matches(&[record], &[log]).is_empty());
}

// ── the fallback quadruple ────────────────────────────────────────────

#[test]
fn token_mismatch_never_pairs() {
    let record = record(1_000_000, "s1", TOKENS);
    for bumped in [
        [101, 20, 5, 0],
        [100, 21, 5, 0],
        [100, 20, 6, 0],
        [100, 20, 5, 1],
    ] {
        let log = call(1_001_000, "s1", bumped);
        assert!(
            gateway_matches(std::slice::from_ref(&record), &[log]).is_empty(),
            "tokens {bumped:?} are another call"
        );
    }
}

#[test]
fn agent_or_session_mismatch_never_pairs() {
    let record = record(1_000_000, "s1", TOKENS);

    let mut other_agent = call(1_001_000, "s1", TOKENS);
    other_agent.agent = "codex".to_string();
    assert!(gateway_matches(std::slice::from_ref(&record), &[other_agent]).is_empty());

    let other_session = call(1_001_000, "s2", TOKENS);
    assert!(gateway_matches(std::slice::from_ref(&record), &[other_session]).is_empty());

    // A call with no session to key on never enters the fallback.
    let mut sessionless = call(1_001_000, "", TOKENS);
    sessionless.session = String::new();
    assert!(gateway_matches(&[record], &[sessionless]).is_empty());
}

#[test]
fn matching_ignores_model_ids() {
    // magpie's gatewayMatches never compares models; pin that the
    // normalization helper did not quietly leak into the pairing rule.
    let mut record = record(1_000_000, "s1", TOKENS);
    record.model_asked = "auto".to_string();
    record.model_answered = "gpt-5".to_string();
    let mut log = call(1_001_000, "s1", TOKENS);
    log.model_asked = "gemini-2.5-pro".to_string();
    log.model_answered = "gpt-5-mini".to_string();
    assert_eq!(gateway_matches(&[record], &[log]).record_of(0), Some(0));
}

// ── the request-id stage ──────────────────────────────────────────────

fn rside(request_id: Option<&str>, session: &str, tokens: [u64; 4], end_ms: i64) -> RecordSide {
    RecordSide {
        request_id: request_id.map(str::to_string),
        agent: "claude-code".to_string(),
        session: session.to_string(),
        tokens,
        failed: false,
        end_ms,
    }
}

fn cside(request_id: Option<&str>, session: &str, tokens: [u64; 4], at_ms: i64) -> CallSide {
    CallSide {
        request_id: request_id.map(str::to_string),
        agent: "claude-code".to_string(),
        session: session.to_string(),
        tokens,
        failed: false,
        at_ms,
    }
}

#[test]
fn request_id_pairs_across_every_other_difference() {
    // The primary key outranks the fallback tuple entirely.
    let records = [rside(Some("req-1"), "s1", TOKENS, 1_000_000)];
    let calls = [cside(Some("req-1"), "another-session", [9, 9, 9, 9], 5_000_000)];
    assert_eq!(pair_sides(&records, &calls), vec![(0, 0)]);
}

#[test]
fn request_id_conflict_blocks_the_fallback() {
    // Both sides carry ids and they differ: stage 1 said no, so the
    // fallback must not second-guess the key.
    let records = [rside(Some("req-1"), "s1", TOKENS, 1_001_000)];
    let calls = [cside(Some("req-2"), "s1", TOKENS, 1_001_000)];
    assert!(pair_sides(&records, &calls).is_empty());
}

#[test]
fn request_id_consumes_the_first_unused_record() {
    // A retried call can leave two records under one id; the call eats the
    // first, the second record stays visible.
    let records = [
        rside(Some("req-1"), "s1", TOKENS, 1_001_000),
        rside(Some("req-1"), "s1", TOKENS, 2_001_000),
    ];
    let calls = [cside(Some("req-1"), "s1", TOKENS, 1_001_000)];
    assert_eq!(pair_sides(&records, &calls), vec![(0, 0)]);
}

#[test]
fn call_request_id_with_idless_record_still_pairs_via_fallback() {
    // The both-ids exclusion only fires when both sides have ids; today
    // every record is idless, so an id-carrying file call pairs by the
    // fallback rules.
    let record = record(1_000_000, "s1", TOKENS);
    let mut log = call(1_001_000, "s1", TOKENS);
    log.request_id = Some("req-9".to_string());
    assert_eq!(gateway_matches(&[record], &[log]).record_of(0), Some(0));
}

#[test]
fn record_request_id_stays_none_until_an_upstream_echoes_ids() {
    // The dormant stage, pinned: translated protocols never hand the
    // gateway the id an agent writes to its session file, so no record
    // enters stage 1 today.
    let record = record(1_000_000, "s1", TOKENS);
    assert_eq!(record_request_id(&record), None);
}

// ── windows ───────────────────────────────────────────────────────────

#[test]
fn window_floors_start_at_local_midnight() {
    let now = 1_759_900_000_000;
    let today = Window::today(now).floor_ms().expect("today has a floor");
    let now_local = Local.timestamp_millis_opt(now).single().expect("now");
    let floor_local = Local.timestamp_millis_opt(today).single().expect("floor");
    assert_eq!(
        floor_local.date_naive(),
        now_local.date_naive(),
        "the floor is this very local day"
    );
    assert_eq!(
        floor_local.time(),
        chrono::NaiveTime::from_hms_opt(0, 0, 0).expect("midnight"),
        "the floor is local midnight"
    );
    assert_eq!(Window::week(now).floor_ms(), Some(today - 6 * DAY_MS));
    assert_eq!(Window::month(now).floor_ms(), Some(today - 29 * DAY_MS));
    assert_eq!(Window::all().floor_ms(), None);
}

#[test]
fn window_floor_is_inclusive_by_at() {
    let now = 1_759_900_000_000;
    let window = Window::today(now);
    let floor = window.floor_ms().expect("floor");
    let at_floor = record(floor, "s1", TOKENS);
    let before_floor = record(floor - 1, "s2", TOKENS);
    let log_at_floor = call(floor, "s3", TOKENS);
    let log_before = call(floor - 1, "s4", TOKENS);
    let view = consumption_view(
        &[at_floor, before_floor],
        &[log_at_floor, log_before],
        window,
    );
    let ats: Vec<i64> = view.rows.iter().map(|row| row.call.at).collect();
    assert_eq!(ats, vec![floor, floor], "exactly the two at-floor rows");
}

#[test]
fn cross_boundary_turn_pairs_but_shows_nowhere_in_the_window() {
    // A turn begun before midnight whose file call landed after it: the
    // record falls below the floor and the call is consumed, so the
    // narrower window shows nothing — the 24h read buffer exists so the
    // pairing is still computed. All time keeps the gateway row.
    let now = 1_759_900_000_000;
    let floor = Window::today(now).floor_ms().expect("floor");
    let mut record = record(floor - 2_000, "s1", TOKENS);
    record.latency_ms = 3_000;
    let log = call(floor + 1_000, "s1", TOKENS);
    assert_eq!(
        gateway_matches(std::slice::from_ref(&record), std::slice::from_ref(&log)).len(),
        1
    );

    let today = consumption_view(
        std::slice::from_ref(&record),
        std::slice::from_ref(&log),
        Window::today(now),
    );
    assert!(today.rows.is_empty(), "swallowed and out of window: {:?}", today.rows);

    let all = consumption_view(&[record], &[log], Window::all());
    assert_eq!(all.rows.len(), 1);
    assert_eq!(all.rows[0].source, CallSource::Gateway);
}

// ── shape and determinism ─────────────────────────────────────────────

#[test]
fn rows_are_newest_first_with_gateway_winning_ties() {
    let (older_record, _) = same_turn(900_000, "s1");
    let (tie_record, _) = same_turn(1_000_000, "s2");
    // A session-file call at the same instant, in a session no record has:
    // visible, and after the gateway row.
    let stranger = call(1_000_000, "s3", TOKENS);
    let view = consumption_view(&[older_record, tie_record], &[stranger], Window::all());
    let observed: Vec<(i64, CallSource)> = view
        .rows
        .iter()
        .map(|row| (row.call.at, row.source))
        .collect();
    assert_eq!(
        observed,
        vec![
            (1_000_000, CallSource::Gateway),
            (1_000_000, CallSource::SessionFile),
            (900_000, CallSource::Gateway),
        ]
    );
}

#[test]
fn matching_is_deterministic() {
    let mut records = Vec::new();
    let mut calls = Vec::new();
    for index in 0..12 {
        let (record, log) = same_turn(1_000_000 + index * 10_000, "s1");
        records.push(record);
        calls.push(log);
        calls.push(call(1_000_000 + index * 10_000 + 5, "s2", TOKENS));
    }
    let first = gateway_matches(&records, &calls);
    let second = gateway_matches(&records, &calls);
    assert_eq!(first, second);
    assert_eq!(first.len(), 12, "each record eats exactly its own call");
    assert_eq!(first.iter().collect::<Vec<_>>().len(), 12);
    let view = consumption_view(&records, &calls, Window::all());
    assert_eq!(view.rows.len(), 24, "12 gateway rows plus 12 strangers");
}

// ── model-id normalization goldens ────────────────────────────────────

#[test]
fn same_model_golden_equivalences() {
    let equivalents = [
        // Dated answers to a pinned name.
        ("gpt-5", "gpt-5-2025-08-07"),
        ("claude-sonnet-4-5", "claude-sonnet-4-5-20250929"),
        ("gemini-2.5-pro", "models/gemini-2.5-pro-001"),
        // Bedrock's region and maker prefixes.
        ("claude-sonnet-4-5", "us.anthropic.claude-sonnet-4-5-v2:0"),
        ("llama-3", "meta.llama-3"),
        // Claude Code's context-size suffix.
        ("claude-opus-5", "claude-opus-5[1m]"),
        // Vendor path prefixes and case.
        ("gpt-5", "openai/GPT-5"),
        // Word and build tails.
        ("gpt-4o-mini", "gpt-4o-mini-2024-07-18"),
        ("claude-3-5-sonnet", "claude-3-5-sonnet-20241022-v2:0"),
        ("qwen-max", "qwen-max-latest"),
        ("glm-4.7", "glm-4.7-preview"),
        ("kimi-k2", "kimi-k2-exp"),
        ("deepseek-chat", "deepseek-chat_336"),
    ];
    for (a, b) in equivalents {
        assert!(same_model(a, b), "{a} should fold to {b}");
        assert!(same_model(b, a), "{b} should fold to {a}");
    }
}

#[test]
fn same_model_golden_differences() {
    let different = [
        // One or two digits are a model generation, not a version tail.
        ("deepseek-v3", "deepseek-v2"),
        ("deepseek-chat", "deepseek-chat-v2"),
        // Different family or member.
        ("gpt-5", "gpt-5-mini"),
        ("claude-opus-5", "claude-sonnet-5"),
        // A tail that is not an atom leaves the id whole.
        ("gpt-5", "gpt-5-2025-08-07-x"),
        // Case and whitespace fold, identity does not.
        ("GPT-5", "gpt-5o"),
    ];
    for (a, b) in different {
        assert!(!same_model(a, b), "{a} must not fold to {b}");
    }
}

#[test]
fn same_model_rejects_empty_ids() {
    assert!(!same_model("", ""));
    assert!(!same_model("", "gpt-5"));
    assert!(!same_model("[1m]", "[1m]"), "a bare suffix names no model");
}

// ── summarize (slice 09) ──────────────────────────────────────────────

/// A gateway-source row with hand-picked attribution, straight on
/// [`UnifiedCall`] — summarize reads the merged shape, not the sources.
fn merged_row(at: i64, catalog: &str, model: &str, tokens: [u64; 4]) -> ConsumptionRow {
    ConsumptionRow {
        call: UnifiedCall {
            at,
            agent: "claude-code".to_string(),
            session: "s1".to_string(),
            model_asked: model.to_string(),
            model_answered: String::new(),
            tokens: counts(tokens),
            effort: None,
            request_id: None,
            error_kind: None,
            latency_ms: Some(LATENCY),
            catalog: catalog.to_string(),
            account: "key:0011aabb".to_string(),
            status: Some(200),
            endpoint: "/v1/messages".to_string(),
            file: None,
            from: None,
            to: None,
        },
        source: CallSource::Gateway,
    }
}

/// A clock fixed inside a day that is *not* the local one on any UTC+8
/// machine: 2026-10-08 21:30 UTC reads as October 9 locally, so a floor
/// computed on the local calendar would sit 2.5h late and fail the asserts.
const NOW: i64 = 1_791_495_000_000;
/// UTC midnight of [`NOW`]: 2026-10-08T00:00:00Z.
const UTC_TODAY: i64 = 1_791_417_600_000;

const HOUR: i64 = 3_600_000;

fn no_price(_: &str, _: &str) -> Option<skillstar_gateway::ModelCost> {
    None
}

fn input<'a>(rows: &'a [ConsumptionRow]) -> SummarizeInput<'a> {
    SummarizeInput { now_ms: NOW, rows, price: &no_price }
}

fn labels(dimension: Dimension, summary: &Summary) -> Vec<&str> {
    summary.by[&dimension].iter().map(|group| group.label.as_str()).collect()
}

#[test]
fn period_floors_are_utc_midnight() {
    // The floor of `now`'s day is 00:00:00 on the UTC calendar — pinned by
    // value and by spelling the instant back in UTC.
    assert_eq!(period_floor_ms(Period::Today, NOW), Some(UTC_TODAY));
    let floor = Utc.timestamp_millis_opt(UTC_TODAY).single().expect("floor instant");
    assert_eq!(floor.to_string(), "2026-10-08 00:00:00 UTC");
    assert_eq!(period_floor_ms(Period::Week, NOW), Some(UTC_TODAY - 6 * DAY_MS));
    assert_eq!(period_floor_ms(Period::Month, NOW), Some(UTC_TODAY - 29 * DAY_MS));
    assert_eq!(period_floor_ms(Period::All, NOW), None);
}

#[test]
fn the_utc_floor_is_inclusive_by_at() {
    let rows = [
        merged_row(UTC_TODAY - 1, "deepseek", "deepseek-chat", TOKENS),
        merged_row(UTC_TODAY, "deepseek", "deepseek-chat", TOKENS),
        merged_row(UTC_TODAY + HOUR, "deepseek", "deepseek-chat", TOKENS),
    ];
    let summary = summarize(&input(&rows), Period::Today);
    assert_eq!(summary.totals.calls, 2, "yesterday's tail is out: {summary:?}");
    // All keeps the pre-midnight row too.
    assert_eq!(summarize(&input(&rows), Period::All).totals.calls, 3);
}

#[test]
fn today_buckets_by_hour_aligned_to_the_utc_grid() {
    let rows = [
        merged_row(UTC_TODAY + 10 * HOUR + 5 * 60_000, "a", "m", TOKENS),
        merged_row(UTC_TODAY + 10 * HOUR + 42 * 60_000, "a", "m", TOKENS),
        merged_row(UTC_TODAY + 11 * HOUR, "a", "m", TOKENS),
    ];
    let summary = summarize(&input(&rows), Period::Today);
    let starts: Vec<i64> = summary.series.iter().map(|point| point.bucket_start_ms).collect();
    assert_eq!(starts, vec![UTC_TODAY + 10 * HOUR, UTC_TODAY + 11 * HOUR]);
    assert_eq!(summary.series[0].totals.calls, 2);
}

#[test]
fn week_and_month_bucket_by_utc_day() {
    let rows = [
        merged_row(UTC_TODAY, "a", "m", TOKENS),
        merged_row(UTC_TODAY + 90_000, "a", "m", TOKENS),
        merged_row(UTC_TODAY - DAY_MS, "a", "m", TOKENS),
    ];
    for period in [Period::Week, Period::Month] {
        let summary = summarize(&input(&rows), period);
        let starts: Vec<i64> = summary.series.iter().map(|point| point.bucket_start_ms).collect();
        assert_eq!(starts, vec![UTC_TODAY - DAY_MS, UTC_TODAY], "{period:?}");
    }
}

#[test]
fn all_buckets_by_iso_week_starting_monday_utc() {
    // UTC_TODAY is a Thursday (2026-10-08); its ISO week starts Monday
    // 2026-10-05.
    let monday = UTC_TODAY - 3 * DAY_MS;
    assert_eq!(
        Utc.timestamp_millis_opt(monday).single().expect("monday").format("%A").to_string(),
        "Monday"
    );
    let rows = [
        merged_row(monday, "a", "m", TOKENS),
        merged_row(monday + 2 * DAY_MS, "a", "m", TOKENS),
        merged_row(monday + 6 * DAY_MS + HOUR, "a", "m", TOKENS),
        merged_row(monday - 1, "a", "m", TOKENS),
    ];
    let summary = summarize(&input(&rows), Period::All);
    let starts: Vec<i64> = summary.series.iter().map(|point| point.bucket_start_ms).collect();
    assert_eq!(
        starts,
        vec![monday - 7 * DAY_MS, monday],
        "one bucket straddles the prior Monday, three share this week's"
    );
    assert_eq!(summary.series[0].totals.calls, 1);
    assert_eq!(summary.series[1].totals.calls, 3);
}

#[test]
fn the_four_dimensions_group_their_labels() {
    let mut rows = vec![
        merged_row(UTC_TODAY, "deepseek", "deepseek-chat", TOKENS),
        merged_row(UTC_TODAY + 90_000, "openai", "gpt-5", TOKENS),
        merged_row(UTC_TODAY + 2 * 90_000, "openai", "gpt-5", TOKENS),
    ];
    rows[0].call.session = "s1".to_string();
    rows[1].call.session = "s2".to_string();
    rows[2].call.session = "s2".to_string();
    rows[2].call.account = String::new();
    // The answered model outranks the asked one for grouping and billing.
    rows[0].call.model_asked = "auto".to_string();
    rows[0].call.model_answered = "deepseek-chat".to_string();
    let summary = summarize(&input(&rows), Period::Today);

    assert_eq!(summary.totals.calls, 3);
    assert_eq!(labels(Dimension::Agent, &summary), vec!["claude-code"]);
    assert_eq!(labels(Dimension::Model, &summary), vec!["deepseek-chat", "gpt-5"]);
    // The row with no account attribution stays out of the breakdown.
    assert_eq!(summary.by[&Dimension::Account][0].label, "key:0011aabb");
    assert_eq!(summary.by[&Dimension::Account][0].totals.calls, 2);
    assert_eq!(labels(Dimension::Session, &summary), vec!["s1", "s2"]);
    assert_eq!(summary.by[&Dimension::Session][1].totals.calls, 2);
    // Rows a label declines do not vanish from totals.
    assert_eq!(summary.totals.calls, 3);
}

#[test]
fn prices_are_injected_at_read_time_and_unpriced_counts_the_rest() {
    let price = |catalog: &str, model: &str| {
        (catalog == "deepseek" && model == "deepseek-chat").then_some(skillstar_gateway::ModelCost {
            input: 0.27,
            output: 1.10,
            cache_read: 0.07,
            cache_write: 0.0,
        })
    };
    let rows = [
        merged_row(UTC_TODAY, "deepseek", "deepseek-chat", [500_000, 500_000, 0, 0]),
        merged_row(UTC_TODAY + 90_000, "stranger", "deepseek-chat", [1, 1, 0, 0]),
    ];
    let summary = summarize(
        &SummarizeInput { now_ms: NOW, rows: &rows, price: &price },
        Period::Today,
    );
    // 0.5M × ($0.27 + $1.10) per million = $0.685, the deepseek magnitude.
    assert!((summary.totals.cost_usd - 0.685).abs() < 1e-9, "{}", summary.totals.cost_usd);
    assert_eq!(summary.totals.unpriced, 1);
    // An empty price table prices nothing but counts everything.
    let bare = summarize(&input(&rows), Period::Today);
    assert_eq!(bare.totals.cost_usd, 0.0);
    assert_eq!(bare.totals.unpriced, 2);
}

#[test]
fn errors_and_mean_latency_aggregate() {
    let mut failed = merged_row(UTC_TODAY, "a", "m", TOKENS);
    failed.call.error_kind = Some("upstream".to_string());
    failed.call.latency_ms = Some(3_000);
    let mut latencyless = merged_row(UTC_TODAY + 90_000, "a", "m", TOKENS);
    latencyless.call.latency_ms = None;
    let summary = summarize(&input(&[failed, latencyless]), Period::Today);
    assert_eq!(summary.totals.calls, 2);
    assert_eq!(summary.totals.errors, 1);
    // Only the row that measured a duration entered the mean.
    assert_eq!(summary.totals.mean_latency_ms, 3_000.0);
}
