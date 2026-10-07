//! The read-time-priced consumption summary use case.
//!
//! One source: the agents' session files ([`read_calls`]). [`summarize`]
//! prices and buckets the rows — its clock is injected here (`Utc::now` on
//! the command path, an explicit `now_ms` in tests), and its price table is
//! [`effective_price_by_model`], so a changed price table restates history
//! on the next read (accepted semantics, see docs/features/usage).
//!
//! The gateway ledger that used to be merged in went away with the model
//! domain (D-082); the 24h pairing buffer that fed the record↔call
//! correlation went with it.

use std::path::PathBuf;

use crate::pricing::{ModelCost, effective_price_by_model};
use crate::sessions::{SessionCall, read_calls};
use ss_core::infra::paths::{home_dir, tool_sync_home_override};

use crate::accounts::consumption::{
    Dimension, Period, SummarizeInput, Summary, consumption_view, period_floor_ms, summarize,
    today_consumption,
};
use crate::accounts::dto::{
    ConsumptionGroupDto, ConsumptionPeriodDto, ConsumptionSummaryDto, ConsumptionTotalsDto,
    TodayConsumptionDto,
};

/// [`effective_price_by_model`] behind a per-command memo. One summary
/// folds a row once per bucket, so the same model is looked up many times a
/// command; the memo reads the two price files once per distinct key, not
/// once per lookup.
fn price_table() -> impl Fn(&str) -> Option<ModelCost> {
    let cache: std::cell::RefCell<std::collections::HashMap<String, Option<ModelCost>>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
    move |model| {
        *cache
            .borrow_mut()
            .entry(model.to_string())
            .or_insert_with_key(|model| effective_price_by_model(model))
    }
}

/// The consumption summary of `period`, assembled from the session files at
/// the current time. Reads local files only — the session files and the
/// price tables — and cannot fail: every source degrades to an empty read.
pub fn get_consumption_summary(period: ConsumptionPeriodDto) -> ConsumptionSummaryDto {
    read_and_assemble(period, chrono::Utc::now().timestamp_millis())
}

/// Today's consumption with session chips (slice 13): the same source as
/// [`get_consumption_summary`], projected onto the Usage page's session
/// drill-down. UTC day boundary, read-time pricing.
pub fn get_today_consumption() -> TodayConsumptionDto {
    let now_ms = chrono::Utc::now().timestamp_millis();
    let since = period_floor_ms(Period::Today, now_ms);
    let calls = read_calls(&agent_home(), since);
    let price = price_table();
    today_consumption(now_ms, &calls, &price)
}

/// The agent-home the sessions are discovered under: the tool-sync sandbox
/// when set, the real home otherwise (tool_paths.rs precedent).
fn agent_home() -> PathBuf {
    tool_sync_home_override().unwrap_or_else(home_dir)
}

/// Read the session files at `now_ms` and assemble the summary. The seams
/// the tests drive: the clock is a parameter, the source reads whatever
/// `SKILLSTAR_DATA_DIR` / the tool-sync sandbox pin, and the price table is
/// whatever the stores hold.
fn read_and_assemble(period: ConsumptionPeriodDto, now_ms: i64) -> ConsumptionSummaryDto {
    let domain = period.domain();
    let since = period_floor_ms(domain, now_ms);
    let calls = read_calls(&agent_home(), since);
    let price = price_table();
    assemble(period, now_ms, &calls, &price)
}

/// Pure assembly over already-read calls (the test seam: clock, price, and
/// rows all injected). The view itself is pure, so the summary of fixed
/// inputs is fixed.
fn assemble(
    period: ConsumptionPeriodDto,
    now_ms: i64,
    calls: &[SessionCall],
    price: &dyn Fn(&str) -> Option<ModelCost>,
) -> ConsumptionSummaryDto {
    let rows = consumption_view(calls);
    let summary = summarize(
        &SummarizeInput {
            now_ms,
            rows: &rows,
            price,
        },
        period.domain(),
    );
    to_dto(summary, period)
}

/// Project the domain summary onto the wire.
fn to_dto(summary: Summary, period: ConsumptionPeriodDto) -> ConsumptionSummaryDto {
    let Summary { totals, series, by } = summary;
    let groups_of = |dimension: Dimension| {
        by.get(&dimension)
            .map(|groups| {
                groups
                    .iter()
                    .cloned()
                    .map(ConsumptionGroupDto::from)
                    .collect()
            })
            .unwrap_or_default()
    };
    ConsumptionSummaryDto {
        period,
        totals: ConsumptionTotalsDto::from(totals),
        series: series.into_iter().map(Into::into).collect(),
        by_agent: groups_of(Dimension::Agent),
        by_model: groups_of(Dimension::Model),
        by_session: groups_of(Dimension::Session),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pricing::TokenCounts;
    use crate::sessions::SessionTokens;
    use crate::test_support::EnvGuard;

    fn utc_day(now_ms: i64) -> i64 {
        now_ms.div_euclid(86_400_000) * 86_400_000
    }

    fn session_call(at: i64, model: &str, tokens: [u64; 4]) -> SessionCall {
        SessionCall {
            at,
            agent: "claude-code".to_string(),
            session: "s1".to_string(),
            model_asked: model.to_string(),
            model_answered: String::new(),
            tokens: SessionTokens {
                input: tokens[0],
                output: tokens[1],
                cache_read: tokens[2],
                cache_write: tokens[3],
            },
            effort: None,
            request_id: None,
            error_kind: None,
            latency_ms: Some(1_000),
            file: std::path::PathBuf::from("/tmp/nowhere.jsonl"),
            from: 0,
            to: 10,
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
        let price = |model: &str| {
            (model == "deepseek-chat").then_some(ModelCost {
                input: 0.27,
                output: 1.10,
                cache_read: 0.07,
                cache_write: 0.0,
            })
        };
        let now = 1_790_000_000_000;
        let calls = [session_call(now, "deepseek-chat", [500_000, 500_000, 0, 0])];
        let dto = assemble(ConsumptionPeriodDto::Today, now, &calls, &price);
        assert!(
            (dto.totals.cost_usd - 0.685).abs() < 1e-9,
            "got {}",
            dto.totals.cost_usd
        );
        assert_eq!(dto.totals.unpriced, 0);
        // Same tokens under a model the table does not know: unpriced,
        // not free-with-confidence.
        let stranger = [session_call(now, "other-model", [500_000, 500_000, 0, 0])];
        let dto = assemble(ConsumptionPeriodDto::Today, now, &stranger, &price);
        assert_eq!(dto.totals.cost_usd, 0.0);
        assert_eq!(dto.totals.unpriced, 1);
    }

    #[test]
    fn the_clock_decides_the_utc_today_window() {
        let price = |_: &str| None;
        let now = 1_790_000_000_000;
        let day = utc_day(now);
        let calls = [
            session_call(day, "deepseek-chat", [10, 1, 0, 0]),
            session_call(day - 1, "deepseek-chat", [20, 2, 0, 0]),
            session_call(day - 8 * 86_400_000, "deepseek-chat", [40, 4, 0, 0]),
        ];
        let today = assemble(ConsumptionPeriodDto::Today, now, &calls, &price);
        assert_eq!(
            today.totals.calls, 1,
            "UTC midnight cuts the day: {today:?}"
        );
        assert_eq!(today.totals.input, 10);
        // Same rows, a clock one day later: yesterday's calls left today.
        let tomorrow = assemble(
            ConsumptionPeriodDto::Today,
            now + 86_400_000,
            &calls,
            &price,
        );
        assert_eq!(tomorrow.totals.calls, 0);
        // Week keeps the trailing seven days, All keeps everything.
        let week = assemble(ConsumptionPeriodDto::Week, now, &calls, &price);
        assert_eq!(week.totals.calls, 2, "the 8-day-old call is out: {week:?}");
        let all = assemble(ConsumptionPeriodDto::All, now, &calls, &price);
        assert_eq!(all.totals.calls, 3);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn reads_the_session_files_through_the_assembly() {
        let (_temp, _env) = isolated("summary-sessions");
        let now = 1_790_000_000_000;
        // No price table in the isolated root: every call reads unpriced.
        let dto = read_and_assemble(ConsumptionPeriodDto::Today, now);
        assert_eq!(dto.period, ConsumptionPeriodDto::Today);
        assert_eq!(
            dto.totals.calls, 0,
            "no session files in the isolated root: {dto:?}"
        );
        assert_eq!(dto.totals.unpriced, 0);
    }

    /// The wire keeps the token vocabulary even though sessions report no
    /// reasoning count (folded into output).
    #[test]
    fn tokens_project_onto_the_wire_vocabulary() {
        let tokens = TokenCounts {
            input: 1,
            output: 2,
            cache_read: 3,
            cache_write: 4,
            reasoning: 0,
        };
        let dto = crate::accounts::dto::ConsumptionTokensDto::from(tokens);
        assert_eq!(
            (
                dto.input,
                dto.output,
                dto.cache_read,
                dto.cache_write,
                dto.reasoning
            ),
            (1, 2, 3, 4, 0)
        );
    }
}
