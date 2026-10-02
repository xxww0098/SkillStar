//! Dimension filters and newest-first paging over the ledger.
//!
//! The read side of [`super`]: the dimension semantics — which field an
//! `agent`, `session`, or `catalog` filter compares against — stay next to
//! the file that spells those fields, so no caller re-derives them. The
//! results are raw [`Record`]s: display names and subscription labels are
//! joined above this crate, never here.

use super::record::Record;

/// One ledger read: dimension filters plus newest-first paging.
///
/// A dimension set to `None` or the empty string matches every record.
/// `skip` walks from the newest end and `limit` caps the page; a `limit` of
/// `0` keeps everything past the skip, which only whole-history callers
/// want. `since` (Unix milliseconds, inclusive) is pushed down to the file
/// read, so time-bounded callers never hold the whole ledger in memory;
/// `None` reads everything the retention keeps.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LedgerQuery {
    pub agent: Option<String>,
    pub session: Option<String>,
    pub catalog: Option<String>,
    pub limit: usize,
    pub skip: usize,
    pub since: Option<i64>,
}

impl LedgerQuery {
    /// The newest `limit` records with no dimension pinned.
    pub fn tail(limit: usize) -> Self {
        Self {
            agent: None,
            session: None,
            catalog: None,
            limit,
            skip: 0,
            since: None,
        }
    }

    /// Whether any dimension is pinned. Callers that can only attribute
    /// unfiltered rows — the in-memory ring carries no session or catalog —
    /// use this to stay out of a filtered read.
    pub fn filtered(&self) -> bool {
        [self.agent.as_deref(), self.session.as_deref(), self.catalog.as_deref()]
            .into_iter()
            .any(|dimension| dimension.is_some_and(|value| !value.is_empty()))
    }

    /// Run over everything the ledger currently holds and return the page,
    /// newest first. A missing or unreadable ledger reads as empty.
    pub fn run(&self) -> Vec<Record> {
        page(super::load(self.since.unwrap_or(i64::MIN)), self)
    }
}

/// Filter in load order (oldest first), then cut the page from the newest
/// end. Pure, so the paging semantics are pinned without touching a data
/// directory.
fn page(records: Vec<Record>, query: &LedgerQuery) -> Vec<Record> {
    let mut matched: Vec<Record> = records
        .into_iter()
        .filter(|record| wants(query, record))
        .collect();
    matched.reverse();
    let mut page = matched.into_iter().skip(query.skip).collect::<Vec<_>>();
    if query.limit > 0 {
        page.truncate(query.limit);
    }
    page
}

fn wants(query: &LedgerQuery, record: &Record) -> bool {
    dimension_matches(query.agent.as_deref(), &record.agent)
        && dimension_matches(query.session.as_deref(), &record.session)
        && dimension_matches(query.catalog.as_deref(), &record.catalog)
}

/// `None` and `""` mean "every value"; any other wanted value must equal
/// the record's.
fn dimension_matches(wanted: Option<&str>, actual: &str) -> bool {
    match wanted {
        None | Some("") => true,
        Some(wanted) => wanted == actual,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(at: i64, agent: &str, session: &str, catalog: &str) -> Record {
        Record {
            at,
            agent: agent.to_string(),
            session: session.to_string(),
            model_asked: "m1".to_string(),
            model_answered: String::new(),
            catalog: catalog.to_string(),
            account: String::new(),
            tokens: super::super::record::TokenCounts::default(),
            status: 200,
            latency_ms: 1,
            error_kind: None,
            endpoint: "/v1/chat/completions".to_string(),
        }
    }

    fn query(
        agent: Option<&str>,
        session: Option<&str>,
        catalog: Option<&str>,
        limit: usize,
        skip: usize,
    ) -> LedgerQuery {
        LedgerQuery {
            agent: agent.map(str::to_string),
            session: session.map(str::to_string),
            catalog: catalog.map(str::to_string),
            limit,
            skip,
            since: None,
        }
    }

    /// `since` reaches the file read through `run`, so a time-bounded page
    /// never holds what the floor already excluded.
    #[test]
    fn run_pushes_since_into_the_file_read() {
        use crate::ledger::append::append;
        let _lock = crate::TEST_PATH_ENV_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let root = std::env::temp_dir().join(format!(
            "skillstar-ledger-query-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let previous = std::env::var_os("SKILLSTAR_DATA_DIR");
        // SAFETY: tests touching this var hold the lock above.
        unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", &root) };
        append(&record(1_000, "codex", "", ""));
        append(&record(2_000, "codex", "", ""));
        let mut bounded = query(None, None, None, 0, 0);
        bounded.since = Some(1_500);
        let bounded_page = bounded.run();
        let whole_page = query(None, None, None, 0, 0).run();
        // SAFETY: see above.
        unsafe {
            match previous {
                Some(value) => std::env::set_var("SKILLSTAR_DATA_DIR", value),
                None => std::env::remove_var("SKILLSTAR_DATA_DIR"),
            }
        };
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(bounded_page, vec![record(2_000, "codex", "", "")]);
        assert_eq!(whole_page.len(), 2);
    }

    #[test]
    fn pages_read_newest_first_with_skip_and_limit() {
        let records = vec![
            record(1_000, "codex", "", ""),
            record(2_000, "codex", "", ""),
            record(3_000, "omp", "", ""),
        ];
        // No filter: newest first.
        let all = page(records.clone(), &query(None, None, None, 0, 0));
        assert_eq!(
            all.iter().map(|record| record.at).collect::<Vec<_>>(),
            vec![3_000, 2_000, 1_000]
        );
        // A limit caps the page at the newest end.
        let two = page(records.clone(), &query(None, None, None, 2, 0));
        assert_eq!(
            two.iter().map(|record| record.at).collect::<Vec<_>>(),
            vec![3_000, 2_000]
        );
        // Skip walks from the newest end.
        let after_first = page(records.clone(), &query(None, None, None, 0, 1));
        assert_eq!(
            after_first.iter().map(|record| record.at).collect::<Vec<_>>(),
            vec![2_000, 1_000]
        );
        // Skip past everything reads empty.
        assert!(page(records, &query(None, None, None, 0, 9)).is_empty());
    }

    #[test]
    fn dimensions_filter_on_their_own_field() {
        let records = vec![
            record(1_000, "codex", "s1", "openai"),
            record(2_000, "codex", "s2", "openai"),
            record(3_000, "omp", "s1", "anthropic"),
        ];
        let codex = page(records.clone(), &query(Some("codex"), None, None, 0, 0));
        assert_eq!(
            codex.iter().map(|record| record.at).collect::<Vec<_>>(),
            vec![2_000, 1_000]
        );
        let session = page(records.clone(), &query(None, Some("s1"), None, 0, 0));
        assert_eq!(
            session.iter().map(|record| record.at).collect::<Vec<_>>(),
            vec![3_000, 1_000]
        );
        let catalog = page(records.clone(), &query(None, None, Some("anthropic"), 0, 0));
        assert_eq!(
            catalog.iter().map(|record| record.at).collect::<Vec<_>>(),
            vec![3_000]
        );
        // The three dimensions intersect.
        let both = page(records, &query(Some("codex"), Some("s1"), Some("openai"), 0, 0));
        assert_eq!(
            both.iter().map(|record| record.at).collect::<Vec<_>>(),
            vec![1_000]
        );
    }

    #[test]
    fn an_unset_dimension_is_not_a_filter() {
        let records = vec![record(1_000, "codex", "s1", "openai")];
        // None and the empty string both mean "match every record"; only a
        // pinned value that no record carries reads empty.
        for unset in [None, Some("")] {
            assert_eq!(
                page(records.clone(), &query(unset, unset, unset, 0, 0)).len(),
                1,
                "unset dimension {unset:?} must not filter"
            );
            assert!(!query(unset, unset, unset, 0, 0).filtered());
        }
        assert!(query(Some("codex"), None, None, 0, 0).filtered());
        assert!(query(None, Some("s1"), None, 0, 0).filtered());
        assert!(query(None, None, Some("openai"), 0, 0).filtered());
        assert!(!LedgerQuery::tail(60).filtered());
        assert_eq!(LedgerQuery::tail(3).limit, 3);
        assert_eq!(LedgerQuery::default(), LedgerQuery::tail(0));
    }
}
