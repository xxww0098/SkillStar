//! Slice 09's use case: the read-time-priced consumption summary command.
//!
//! Three sources, one assembly: the gateway ledger ([`load`], read with the
//! 24h pairing buffer), the agents' session files ([`read_calls`]), and the
//! merged [`consumption_view`] that deduplicates a through-proxied turn.
//! [`summarize`] then prices and buckets the rows — its clock is injected
//! here (`Utc::now` on the command path, an explicit `now_ms` in tests), and
//! its price table is [`effective_price`], so a changed price table
//! restates history on the next read (accepted semantics, see
//! docs/features/usage).
//!
//! The merge runs over [`Window::all`] on purpose: the buffered read already
//! covers the pairing, and the period filter belongs to [`summarize`]'s
//! UTC-day contract, not to [`Window`]'s local-day one.

use std::path::PathBuf;

use skillstar_core::infra::paths::{home_dir, tool_sync_home_override};
use skillstar_gateway::{ModelCost, Record, effective_price, load};
use skillstar_usage::sessions::{SessionCall, read_calls};

use crate::usage::consumption::{
    Dimension, SESSION_UNKNOWN_CATALOG, SummarizeInput, Summary, Window, consumption_view, groups,
    period_floor_ms, summarize,
};
use crate::usage::dto::{
    ConsumptionGroupDto, ConsumptionPeriodDto, ConsumptionSummaryDto, ConsumptionTotalsDto,
};

/// How much earlier than the period floor the sources are read: a turn
/// begun before the boundary can still consume a file call written after
/// it, so the pairing must see yesterday's records (magpie's buffer).
const PAIRING_BUFFER_MS: i64 = 86_400_000;

/// The consumption summary of `period`, assembled from the three sources at
/// the current time. Reads local files only — the ledger, the session
/// files, and the price tables — and cannot fail: every source degrades to
/// an empty read.
pub fn get_consumption_summary(period: ConsumptionPeriodDto) -> ConsumptionSummaryDto {
    read_and_assemble(period, chrono::Utc::now().timestamp_millis())
}

/// Read the three sources at `now_ms` and assemble the summary. The seams
/// the tests drive: the clock is a parameter, the sources read whatever
/// `SKILLSTAR_DATA_DIR` / the tool-sync sandbox pin, and the price table is
/// whatever the stores hold.
fn read_and_assemble(period: ConsumptionPeriodDto, now_ms: i64) -> ConsumptionSummaryDto {
    let domain = period.domain();
    // The pairing buffer: read from before the floor so cross-boundary
    // turns pair, then let `summarize`'s UTC filter draw the visible line.
    let since = period_floor_ms(domain, now_ms).map(|floor| floor - PAIRING_BUFFER_MS);
    let records = load(since.unwrap_or(0));
    let calls = read_calls(&agent_home(), since);
    let price = |catalog: &str, model: &str| effective_price(catalog, model);
    assemble(period, now_ms, &records, &calls, &price)
}

/// The agent-home sessions are discovered under: the tool-sync sandbox when
/// set, the real home otherwise (tool_paths.rs precedent).
fn agent_home() -> PathBuf {
    tool_sync_home_override().unwrap_or_else(home_dir)
}

/// Pure assembly over already-read sources (the test seam: clock, price,
/// and rows all injected). The merge itself is pure, so the summary of
/// fixed inputs is fixed.
fn assemble(
    period: ConsumptionPeriodDto,
    now_ms: i64,
    records: &[Record],
    calls: &[SessionCall],
    price: &dyn Fn(&str, &str) -> Option<ModelCost>,
) -> ConsumptionSummaryDto {
    let rows = consumption_view(records, calls, Window::all()).rows;
    let summary = summarize(&SummarizeInput { now_ms, rows: &rows, price }, period.domain());
    // Per-catalog groups are the「经网关」scope: only gateway-attributed
    // rows carry a catalog. Bypass rows read `session-unknown` (or nothing)
    // and never join a provider's group.
    let by_catalog = groups(
        rows.iter(),
        |call| {
            (!call.catalog.is_empty() && call.catalog != SESSION_UNKNOWN_CATALOG)
                .then(|| call.catalog.clone())
        },
        price,
    );
    to_dto(summary, by_catalog, period)
}

/// Project the domain summary (plus the per-catalog groups) onto the wire.
fn to_dto(
    summary: Summary,
    by_catalog: Vec<crate::usage::consumption::Group>,
    period: ConsumptionPeriodDto,
) -> ConsumptionSummaryDto {
    let Summary { totals, series, by } = summary;
    let groups_of = |dimension: Dimension| {
        by.get(&dimension)
            .map(|groups| groups.iter().cloned().map(ConsumptionGroupDto::from).collect())
            .unwrap_or_default()
    };
    ConsumptionSummaryDto {
        period,
        totals: ConsumptionTotalsDto::from(totals),
        series: series.into_iter().map(Into::into).collect(),
        by_agent: groups_of(Dimension::Agent),
        by_model: groups_of(Dimension::Model),
        by_account: groups_of(Dimension::Account),
        by_session: groups_of(Dimension::Session),
        by_catalog: by_catalog.into_iter().map(Into::into).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{ENV_LOCK, EnvGuard};
    use skillstar_gateway::{TokenCounts, append};

    /// UTC midnight of `now`, so window assertions read as times. (The
    /// pairing buffer is exactly one day, so it doubles as the day span.)
    fn utc_day(now_ms: i64) -> i64 {
        now_ms.div_euclid(PAIRING_BUFFER_MS) * PAIRING_BUFFER_MS
    }

    fn record(at: i64, catalog: &str, model: &str, tokens: [u64; 4]) -> Record {
        Record {
            at,
            agent: "claude-code".to_string(),
            session: String::new(),
            model_asked: model.to_string(),
            model_answered: String::new(),
            catalog: catalog.to_string(),
            account: "key:0011aabb".to_string(),
            tokens: TokenCounts {
                input: tokens[0],
                output: tokens[1],
                cache_read: tokens[2],
                cache_write: tokens[3],
                reasoning: 0,
            },
            status: 200,
            latency_ms: 1_000,
            error_kind: None,
            endpoint: "/v1/chat/completions".to_string(),
        }
    }

    /// A temp data root with the env pinned. The guard must outlive the
    /// test body, so it travels back with the dir that holds it alive.
    fn isolated(tag: &str) -> (tempfile::TempDir, crate::test_support::EnvGuard) {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join(tag);
        std::fs::create_dir_all(&data).unwrap();
        let env = EnvGuard::set(&[
            ("SKILLSTAR_DATA_DIR", &data),
            ("SKILLSTAR_TOOL_SYNC_HOME", &data),
        ]);
        (temp, env)
    }

    #[test]
    fn prices_are_injected_and_the_deepseek_magnitude_holds() {
        // deepseek-chat's published per-million prices (input .27, output
        // 1.10, cache read .07): one million of each unit bills $1.44.
        let price = |catalog: &str, model: &str| {
            (catalog == "deepseek" && model == "deepseek-chat").then_some(ModelCost {
                input: 0.27,
                output: 1.10,
                cache_read: 0.07,
                cache_write: 0.0,
            })
        };
        let now = 1_790_000_000_000;
        let records = [record(now, "deepseek", "deepseek-chat", [500_000, 500_000, 0, 0])];
        let dto = assemble(ConsumptionPeriodDto::Today, now, &records, &[], &price);
        assert!((dto.totals.cost_usd - 0.685).abs() < 1e-9, "got {}", dto.totals.cost_usd);
        assert_eq!(dto.totals.unpriced, 0);
        // Same tokens under a catalog the table does not know: unpriced,
        // not free-with-confidence.
        let stranger = [record(now, "other", "deepseek-chat", [500_000, 500_000, 0, 0])];
        let dto = assemble(ConsumptionPeriodDto::Today, now, &stranger, &[], &price);
        assert_eq!(dto.totals.cost_usd, 0.0);
        assert_eq!(dto.totals.unpriced, 1);
    }

    #[test]
    fn by_catalog_carries_gateway_rows_only_while_totals_count_everything() {
        let now = 1_790_000_000_000;
        let priced = |_: &str, _: &str| {
            Some(ModelCost { input: 1.0, output: 1.0, cache_read: 0.0, cache_write: 0.0 })
        };
        // A gateway turn and the same turn's file line: swallowed, one row.
        let mut paired = record(now, "deepseek", "deepseek-chat", [100, 20, 5, 0]);
        paired.session = "s1".to_string();
        let mut call = SessionCall {
            at: now + 1_000,
            agent: "claude-code".to_string(),
            session: "s1".to_string(),
            model_asked: "deepseek-chat".to_string(),
            model_answered: String::new(),
            tokens: skillstar_usage::sessions::SessionTokens {
                input: 100,
                output: 20,
                cache_read: 5,
                cache_write: 0,
            },
            effort: None,
            request_id: None,
            error_kind: None,
            latency_ms: Some(1_000),
            file: std::path::PathBuf::from("/tmp/nowhere.jsonl"),
            from: 0,
            to: 10,
        };
        call.at = now + 1_000;
        // A bypass call no record stands for: session-unknown catalog.
        let mut bypass = call.clone();
        bypass.session = "s2".to_string();
        bypass.at = now + 2_000;
        let dto = assemble(
            ConsumptionPeriodDto::Today,
            now,
            &[paired],
            &[call, bypass],
            &priced,
        );
        assert_eq!(dto.totals.calls, 2, "the paired line swallowed: {dto:?}");
        assert_eq!(dto.by_catalog.len(), 1, "bypass has no catalog: {dto:?}");
        assert_eq!(dto.by_catalog[0].label, "deepseek");
        assert_eq!(dto.by_catalog[0].totals.calls, 1);
        assert_eq!(dto.by_catalog[0].totals.input, 100);
        // The bypass row is in totals and in no catalog group.
        assert_eq!(dto.totals.input, 200);
    }

    #[test]
    fn the_clock_decides_the_utc_today_window() {
        let price = |_: &str, _: &str| None;
        let now = 1_790_000_000_000;
        let day = utc_day(now);
        let records = [
            record(day, "deepseek", "deepseek-chat", [10, 1, 0, 0]),
            record(day - 1, "deepseek", "deepseek-chat", [20, 2, 0, 0]),
            record(day - 8 * PAIRING_BUFFER_MS, "deepseek", "deepseek-chat", [40, 4, 0, 0]),
        ];
        let today = assemble(ConsumptionPeriodDto::Today, now, &records, &[], &price);
        assert_eq!(today.totals.calls, 1, "UTC midnight cuts the day: {today:?}");
        assert_eq!(today.totals.input, 10);
        // Same rows, a clock one day later: yesterday's calls left today.
        let tomorrow = assemble(ConsumptionPeriodDto::Today, now + PAIRING_BUFFER_MS, &records, &[], &price);
        assert_eq!(tomorrow.totals.calls, 0);
        // Week keeps the trailing seven days, All keeps everything.
        let week = assemble(ConsumptionPeriodDto::Week, now, &records, &[], &price);
        assert_eq!(week.totals.calls, 2, "the 8-day-old call is out: {week:?}");
        let all = assemble(ConsumptionPeriodDto::All, now, &records, &[], &price);
        assert_eq!(all.totals.calls, 3);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn reads_the_persistent_ledger_through_the_assembly() {
        let _lock = ENV_LOCK.lock().await;
        let (_temp, _env) = isolated("summary-ledger");
        let now = 1_790_000_000_000;
        let day = utc_day(now);
        append(&record(day + 1_000, "deepseek", "deepseek-chat", [100, 20, 5, 0]));
        append(&record(day - 3 * PAIRING_BUFFER_MS, "deepseek", "deepseek-chat", [7, 7, 7, 7]));
        // No price table in the isolated root: every call reads unpriced.
        let dto = read_and_assemble(ConsumptionPeriodDto::Today, now);
        assert_eq!(dto.period, ConsumptionPeriodDto::Today);
        assert_eq!(dto.totals.calls, 1, "only the in-window record: {dto:?}");
        assert_eq!(dto.totals.unpriced, 1);
        assert_eq!(dto.by_catalog.len(), 1);
        assert_eq!(dto.by_catalog[0].label, "deepseek");
        assert_eq!(dto.series.len(), 1, "one hour bucket: {dto:?}");
    }
}
