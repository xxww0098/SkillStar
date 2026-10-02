//! The Gateway column's ledger view: the persistent usage ledger's newest
//! page, merged with the in-memory ring, mapped onto the recent-call DTO.
//!
//! The ledger is the source of truth — a restart keeps the column. The ring
//! only contributes turns the ledger never gained, because a turn is pushed
//! to the ring first and its ledger line appended right after; the one row
//! shape the ledger lacks is a turn whose append failed (the read-only-disk
//! degradation the append side warns about and swallows). One turn spelled
//! both ways is matched on agent, asked model, status, and completion
//! count; the two stamps cannot be compared (the ring stamps the finish,
//! the ledger the dispatch entry), and an unmatched ring row is exactly the
//! degradation case.
//!
//! The joins live here, not in the gateway: the gateway hands out raw ids
//! and this projection spells them with the saved model display name.

use std::collections::BTreeMap;

use skillstar_gateway::{RecentCall, Record};

use super::RecentCallDto;
// The query type the command layer binds to, re-exported the way
// `codex_save` re-exports `CodexRoute`: the app facade stays the only
// surface src-tauri names.
pub use skillstar_gateway::LedgerQuery;

/// How many rows one unfiltered page keeps. Sized to the ring's
/// `TRACE_KEEP`, so the merged tail never truncates between the two
/// sources while both are healthy.
pub const PAGE_KEEP: usize = 60;

/// The merged newest-first page behind `get_recent_calls` and
/// `get_ledger_page`. Ledger records carry the full field set; ring rows
/// join only where the ledger cannot.
pub fn load_ledger_page(query: LedgerQuery) -> Vec<RecentCallDto> {
    let records = query.run();
    // The ring has no session or catalog to filter by, and a paged-in read
    // (skip > 0) is looking at history the ring was never shaped to cover.
    let ring = if query.filtered() || query.skip > 0 {
        Vec::new()
    } else {
        ring_rows_missing_from(&records)
    };
    let names = skillstar_gateway::stored_model_names();
    let mut rows: Vec<RecentCallDto> = records
        .iter()
        .map(|record| project_record(record, &names))
        .chain(ring.into_iter().map(project_call))
        .collect();
    // Both sources spell `at` as `YYYY-MM-DD HH:MM:SS` UTC, so the string
    // order is the time order; the sort is stable, keeping the ledger's
    // own newest-first order inside one second.
    rows.sort_by(|left, right| right.at.cmp(&left.at));
    if query.limit > 0 {
        rows.truncate(query.limit);
    }
    rows
}

/// Ring rows with no counterpart among the page's records: the turns whose
/// ledger append failed, plus — on a process whose ledger is empty or
/// always failing — everything the ring holds.
fn ring_rows_missing_from(records: &[Record]) -> Vec<RecentCall> {
    skillstar_gateway::recent_calls()
        .into_iter()
        .filter(|call| !records.iter().any(|record| same_turn(record, call)))
        .collect()
}

/// One turn as the two pipelines spell it. Two turns of one agent on one
/// model with one status and one completion count collide here; the row
/// that drops is indistinguishable in the DTO apart from its stamp, so the
/// miss is cosmetic, not a lost metering fact (the ledger line stays).
fn same_turn(record: &Record, call: &RecentCall) -> bool {
    record.agent == call.agent
        && record.model_asked == call.model
        && record.status == call.status
        && record.tokens.output == call.completion_tokens.unwrap_or(0)
}

fn project_record(record: &Record, names: &BTreeMap<String, String>) -> RecentCallDto {
    let served = if record.model_answered.is_empty() {
        record.model_asked.as_str()
    } else {
        record.model_answered.as_str()
    };
    RecentCallDto {
        at: utc_stamp(record.at),
        agent: record.agent.clone(),
        model: skillstar_gateway::model_label(served, names),
        status: record.status,
        completion_tokens: count(record.tokens.output),
        in_tokens: count(record.tokens.input),
        session: record.session.clone(),
        latency: count(record.latency_ms),
    }
}

fn project_call(call: RecentCall) -> RecentCallDto {
    RecentCallDto {
        at: call.at,
        agent: call.agent,
        model: call.model,
        status: call.status,
        completion_tokens: call
            .completion_tokens
            .map(|tokens| tokens.to_string())
            .unwrap_or_default(),
        // The ring carries none of these; an absent reading stays empty.
        in_tokens: String::new(),
        session: String::new(),
        latency: String::new(),
    }
}

/// Dispatch-entry millis in the ring's UTC spelling, `YYYY-MM-DD HH:MM:SS`.
fn utc_stamp(millis: i64) -> String {
    chrono::DateTime::from_timestamp_millis(millis)
        .map(|at| at.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default()
}

/// Decimal string, or empty for zero: a count the response did not name is
/// not a zero reading.
fn count(value: u64) -> String {
    if value == 0 {
        String::new()
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{ENV_LOCK, EnvGuard};
    use skillstar_gateway::{
        TokenCounts, append, clear_recent_calls, note_recent_call, recent_calls,
    };

    /// Millis for `YYYY-MM-DD HH:MM:SS` stamps used below, so the merge's
    /// ordering assertions can read as times.
    const AT_1205: i64 = 1_790_000_000_000 + 5_000;
    const AT_1207: i64 = 1_790_000_000_000 + 7_000;
    const STAMP_1205: &str = "2026-09-21 14:13:25";
    const STAMP_1207: &str = "2026-09-21 14:13:27";

    fn record(at: i64, agent: &str, asked: &str, output: u64) -> Record {
        Record {
            at,
            agent: agent.to_string(),
            session: "s1".to_string(),
            model_asked: asked.to_string(),
            model_answered: String::new(),
            catalog: String::new(),
            account: String::new(),
            tokens: TokenCounts {
                input: 10,
                output,
                ..TokenCounts::default()
            },
            status: 200,
            latency_ms: 42,
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
    fn records_map_onto_the_dto_with_the_new_fields() {
        let source = record(AT_1205, "codex", "m1", 5);
        let row = project_record(&source, &BTreeMap::new());
        assert_eq!(row.at, STAMP_1205);
        assert_eq!(row.agent, "codex");
        assert_eq!(row.model, "m1");
        assert_eq!(row.status, 200);
        assert_eq!(row.completion_tokens, "5");
        assert_eq!(row.in_tokens, "10");
        assert_eq!(row.session, "s1");
        assert_eq!(row.latency, "42");
        // Zero counts read as absent, not as a printed zero.
        let bare = project_record(
            &Record {
                at: AT_1205,
                latency_ms: 0,
                tokens: TokenCounts::default(),
                ..record(AT_1205, "codex", "m1", 0)
            },
            &BTreeMap::new(),
        );
        assert_eq!(bare.completion_tokens, "");
        assert_eq!(bare.in_tokens, "");
        assert_eq!(bare.latency, "");
    }

    #[test]
    fn the_model_label_is_joined_here_not_in_the_gateway() {
        let mut names = BTreeMap::new();
        names.insert("m2".to_string(), "Renamed m2".to_string());
        // The answered model carries the label; the asked model is the
        // fallback when the ledger never learned what answered.
        let answered = Record {
            model_answered: "m2".to_string(),
            ..record(AT_1205, "codex", "m1", 5)
        };
        assert_eq!(project_record(&answered, &names).model, "Renamed m2");
        let asked_only = record(AT_1205, "codex", "m1", 5);
        assert_eq!(project_record(&asked_only, &names).model, "m1");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn the_ledger_tail_survives_a_restart() {
        let _lock = ENV_LOCK.lock().await;
        let (_temp, _data) = isolated("ledger-restart");
        clear_recent_calls();
        append(&record(AT_1205, "codex", "m1", 5));
        append(&record(AT_1207, "omp", "m2", 3));
        // The ring is empty — a restart — and the column still reads.
        assert!(recent_calls().is_empty());
        let rows = load_ledger_page(LedgerQuery::tail(PAGE_KEEP));
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].agent, "omp", "newest first: {rows:?}");
        assert_eq!(rows[0].at, STAMP_1207);
        assert_eq!(rows[1].completion_tokens, "5");
        assert_eq!(rows[1].in_tokens, "10");
        assert_eq!(rows[1].session, "s1");
        assert_eq!(rows[1].latency, "42");
        clear_recent_calls();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn the_ring_only_joins_turns_the_ledger_lacks() {
        let _lock = ENV_LOCK.lock().await;
        let (_temp, _data) = isolated("ledger-merge");
        clear_recent_calls();
        // A healthy turn: pushed to the ring, appended to the ledger. The
        // stamps differ on purpose — the ring stamps the finish, the ledger
        // the dispatch entry — so matching cannot lean on time.
        append(&record(AT_1205, "codex", "m1", 5));
        note_recent_call(RecentCall {
            at: "2026-09-21 14:13:26".to_string(),
            agent: "codex".to_string(),
            model: "m1".to_string(),
            status: 200,
            completion_tokens: Some(5),
        });
        // A degraded turn: the ring holds it, a failed append means the
        // ledger never did.
        note_recent_call(RecentCall {
            at: "2026-09-21 14:13:28".to_string(),
            agent: "omp".to_string(),
            model: "m2".to_string(),
            status: 500,
            completion_tokens: None,
        });
        let rows = load_ledger_page(LedgerQuery::tail(PAGE_KEEP));
        assert_eq!(rows.len(), 2, "no duplicate of the covered turn: {rows:?}");
        // Newest first: the degraded ring row, then the ledger row.
        assert_eq!(rows[0].agent, "omp");
        assert_eq!(rows[0].status, 500);
        assert_eq!(rows[0].completion_tokens, "");
        assert_eq!(rows[0].in_tokens, "", "the ring carries no input count");
        assert_eq!(rows[1].agent, "codex");
        assert_eq!(rows[1].completion_tokens, "5");
        assert_eq!(rows[1].in_tokens, "10");
        clear_recent_calls();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn a_filtered_or_paged_read_stays_off_the_ring() {
        let _lock = ENV_LOCK.lock().await;
        let (_temp, _data) = isolated("ledger-filtered");
        clear_recent_calls();
        append(&record(AT_1205, "codex", "m1", 5));
        append(&record(AT_1207, "omp", "m2", 3));
        note_recent_call(RecentCall {
            at: "2026-09-21 14:13:29".to_string(),
            agent: "omp".to_string(),
            model: "m3".to_string(),
            status: 200,
            completion_tokens: None,
        });
        // A dimension filter keeps the ring out: it cannot vouch for a
        // session or a catalog.
        let mut codex = LedgerQuery::tail(PAGE_KEEP);
        codex.agent = Some("codex".to_string());
        let rows = load_ledger_page(codex);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].agent, "codex");
        // A paged-in read (skip past the newest) is history the ring was
        // never shaped to cover.
        let paged = LedgerQuery {
            skip: 1,
            ..LedgerQuery::tail(PAGE_KEEP)
        };
        let rows = load_ledger_page(paged);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].agent, "codex", "the ring row stayed out: {rows:?}");
        clear_recent_calls();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn an_empty_ledger_and_ring_read_empty() {
        let _lock = ENV_LOCK.lock().await;
        let (_temp, _data) = isolated("ledger-empty");
        clear_recent_calls();
        assert!(load_ledger_page(LedgerQuery::tail(PAGE_KEEP)).is_empty());
        clear_recent_calls();
    }
}
