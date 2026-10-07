//! Tests for the session-file consumption view and its projections.

use crate::pricing::ModelCost;
use crate::sessions::{SessionCall, SessionTokens};

use super::crossview::today_from_rows;
use super::summarize::{Period, SummarizeInput, summarize};
use super::{Dimension, consumption_view, period_floor_ms};

const NOW: i64 = 1_790_000_000_000;
const DAY: i64 = 86_400_000;

fn call(at: i64, agent: &str, session: &str, model: &str, answered: &str) -> SessionCall {
    SessionCall {
        at,
        agent: agent.to_string(),
        session: session.to_string(),
        model_asked: model.to_string(),
        model_answered: answered.to_string(),
        tokens: SessionTokens {
            input: 30,
            output: 7,
            cache_read: 0,
            cache_write: 0,
        },
        effort: None,
        request_id: None,
        error_kind: None,
        latency_ms: Some(900),
        file: std::path::PathBuf::from("/tmp/nowhere.jsonl"),
        from: 0,
        to: 10,
    }
}

fn priced(model: &str) -> Option<ModelCost> {
    (model == "glm-4.7").then_some(ModelCost {
        input: 0.6,
        output: 2.2,
        cache_read: 0.1,
        cache_write: 0.0,
    })
}

#[test]
fn the_view_projects_and_orders_newest_first() {
    let rows = consumption_view(&[
        call(NOW, "opencode", "s1", "glm-4.7", ""),
        call(NOW + 5_000, "codex", "s2", "gpt-5", "gpt-5-2025-08-07"),
        call(NOW - 5_000, "claude-code", "s3", "claude-opus-4-5", ""),
    ]);
    assert_eq!(rows.len(), 3);
    assert!(
        rows[0].at > rows[1].at && rows[1].at > rows[2].at,
        "newest first"
    );
    // The answered model wins the served spelling; the asked one fills in.
    assert_eq!(rows[0].model_answered, "gpt-5-2025-08-07");
    assert_eq!(rows[2].model_asked, "claude-opus-4-5");
    // Session vocabulary: reasoning folded into output, reported as 0.
    assert_eq!(rows[0].tokens.reasoning, 0);
    assert_eq!(rows[0].tokens.input, 30);
}

#[test]
fn chips_group_by_session_and_price_by_served_model() {
    let rows = consumption_view(&[
        call(NOW, "opencode", "s1", "glm-4.7", ""),
        call(NOW - 1_000, "opencode", "s1", "glm-4.7", ""),
        call(NOW + 2_000, "codex", "s2", "gpt-5", ""),
    ]);
    let dto = today_from_rows(NOW, &rows, &priced);
    assert_eq!(dto.totals.calls, 3, "everything counts: {dto:?}");
    assert_eq!(dto.chips.len(), 2);
    // Newest activity first.
    assert_eq!(dto.chips[0].session, "s2", "newest first: {dto:?}");
    assert_eq!(dto.chips[0].title.as_deref(), Some("gpt-5"));
    assert_eq!(dto.chips[0].cost_usd, None, "gpt-5 is unpriced here");
    let one = dto.chips[1].clone();
    assert_eq!(one.session, "s1");
    assert_eq!(one.title.as_deref(), Some("glm-4.7"));
    assert_eq!(one.tokens.input, 60, "two calls folded: {one:?}");
    assert!(
        (one.cost_usd.unwrap() - 2.0 * (30.0 * 0.6 + 7.0 * 2.2) / 1e6).abs() < 1e-9,
        "{one:?}"
    );
    // Totals bill the priced calls and count the unpriced one.
    assert_eq!(dto.totals.unpriced, 1);
    assert!((dto.totals.cost_usd - one.cost_usd.unwrap()).abs() < 1e-12);
}

#[test]
fn chips_keep_the_utc_today_boundary_and_drop_idless_calls() {
    let day = NOW.div_euclid(DAY) * DAY;
    let mut idless = call(day + 100, "codex", "", "gpt-5", "");
    idless.session = String::new();
    let rows = consumption_view(&[
        call(day, "codex", "s9", "gpt-5", ""),
        idless,
        call(NOW - 2 * DAY, "codex", "s8", "gpt-5", ""),
    ]);
    let dto = today_from_rows(NOW, &rows, &|_| None);
    assert_eq!(dto.totals.calls, 2, "yesterday's call is out, idless is in");
    assert_eq!(dto.chips.len(), 1, "idless calls have no chip: {dto:?}");
    assert_eq!(dto.chips[0].session, "s9");
}

#[test]
fn summarize_buckets_by_period_and_skips_empty_labels() {
    let rows = consumption_view(&[
        call(NOW, "opencode", "s1", "glm-4.7", ""),
        call(NOW - DAY - 1_000, "opencode", "s1", "glm-4.7", ""),
        call(NOW, "codex", "", "gpt-5", ""),
    ]);
    let today = summarize(
        &SummarizeInput {
            now_ms: NOW,
            rows: &rows,
            price: &priced,
        },
        Period::Today,
    );
    // Today keeps the two NOW rows; yesterday's is out.
    assert_eq!(today.totals.calls, 2);
    assert_eq!(today.series.len(), 1, "one hour bucket");
    // Agent dimension keeps both; session dimension drops the idless row.
    assert_eq!(today.by[&Dimension::Agent].len(), 2);
    assert_eq!(today.by[&Dimension::Session].len(), 1);
    // The model dimension bills under the served spelling.
    let glm = today.by[&Dimension::Model]
        .iter()
        .find(|group| group.label == "glm-4.7")
        .unwrap();
    assert_eq!(glm.totals.calls, 1);
    assert_eq!(glm.totals.unpriced, 0);
    let gpt = today.by[&Dimension::Model]
        .iter()
        .find(|group| group.label == "gpt-5")
        .unwrap();
    assert_eq!(gpt.totals.unpriced, 1, "unknown is not free");

    let week = summarize(
        &SummarizeInput {
            now_ms: NOW,
            rows: &rows,
            price: &priced,
        },
        Period::Week,
    );
    assert_eq!(
        week.totals.calls, 3,
        "the 2-day-old call is inside the week"
    );
}

#[test]
fn period_floors_pin_utc_midnight() {
    let today = period_floor_ms(Period::Today, NOW).unwrap();
    assert_eq!(today, NOW.div_euclid(DAY) * DAY);
    assert_eq!(period_floor_ms(Period::Week, NOW).unwrap(), today - 6 * DAY);
    assert_eq!(
        period_floor_ms(Period::Month, NOW).unwrap(),
        today - 29 * DAY
    );
    assert_eq!(period_floor_ms(Period::All, NOW), None);
}

#[test]
fn failed_calls_count_as_errors() {
    let mut failed = call(NOW, "codex", "s1", "gpt-5", "");
    failed.error_kind = Some("rate_limited".to_string());
    let rows = consumption_view(&[failed]);
    let dto = today_from_rows(NOW, &rows, &|_| None);
    assert_eq!(dto.totals.errors, 1);
}
