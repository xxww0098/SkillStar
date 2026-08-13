//! The AI-search remote-seed criterion.
//!
//! The old criterion asked the size of the whole `marketplace_skill` table
//! (`snapshot_rows < 500`). It could not fire: the leaderboard SSR page alone
//! yields ~600 rows and the API top-up adds at most 200 more, so from the first
//! sync onwards the count sits at 600–800 and the entire remote-seed branch in
//! `ai_search_local` was unreachable. These tests pin the replacement — a
//! per-keyword `search_seed:<keyword>` record — and, just as importantly, pin
//! that the decision is now independent of how many rows the table holds.

use super::*;
use crate::snapshot::local_first::keywords_needing_remote_seed;
use crate::snapshot::*;
use std::collections::HashMap;

fn conn() -> Connection {
    create_connection().expect("open marketplace db")
}

fn hits(pairs: &[(&str, usize)]) -> HashMap<String, Vec<String>> {
    pairs
        .iter()
        .map(|(keyword, count)| {
            (
                (*keyword).to_string(),
                (0..*count).map(|index| format!("skill-{index}")).collect(),
            )
        })
        .collect()
}

fn keywords(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn record_seed(query: &str, degraded: bool) {
    let conn = conn();
    let tx = conn.unchecked_transaction().expect("open tx");
    let meta = crate::remote::FetchMeta {
        payload_sha256: "a".repeat(64),
        source_host: "https://skills.sh/".to_string(),
        etag: None,
        degraded,
    };
    mark_scope_success_with_meta_in_tx(&tx, &search_seed_scope(query), &meta, false)
        .expect("record seed");
    tx.commit().expect("commit seed record");
}

/// Fill the shared skill table to the size a real snapshot reaches after one
/// leaderboard sync — the size that made the old row-count criterion dead.
fn fill_snapshot(rows: usize) {
    let synced_at = now_rfc3339();
    let conn = conn();
    let tx = conn.unchecked_transaction().expect("open tx");
    for index in 0..rows {
        upsert_skill_identity_in_tx(&tx, "acme/repo", &format!("skill-{index}"), 5, &synced_at)
            .expect("upsert skill")
            .expect("skill key");
    }
    tx.commit().expect("commit skills");
}

/// A keyword the local snapshot answers well is never worth a round-trip,
/// whatever the sync state says.
#[test]
fn a_keyword_with_enough_local_hits_is_never_seeded() {
    with_temp_data_root(|_| {
        let needed = keywords_needing_remote_seed(
            &conn(),
            &keywords(&["rust"]),
            &hits(&[("rust", AI_SEARCH_REMOTE_SEED_MIN_HITS)]),
        )
        .expect("decide");
        assert!(needed.is_empty(), "a well-answered keyword needs no seed");
    });
}

/// The question the criterion has to answer is "have we ever asked the remote
/// about *this* keyword", and only the per-keyword record can answer it.
#[test]
fn a_thin_keyword_is_seeded_once_and_then_left_alone() {
    with_temp_data_root(|_| {
        let keywords = keywords(&["obscure"]);
        let thin = hits(&[("obscure", 1)]);

        assert_eq!(
            keywords_needing_remote_seed(&conn(), &keywords, &thin).expect("decide"),
            vec!["obscure".to_string()],
            "never asked → ask"
        );

        record_seed("obscure", false);

        assert!(
            keywords_needing_remote_seed(&conn(), &keywords, &thin)
                .expect("decide")
                .is_empty(),
            "already asked and answered → a thin result is the real answer, not a gap"
        );
    });
}

/// The regression this replaces. A snapshot at its normal size (600–800 rows)
/// made `snapshot_rows < 500` permanently false, so a keyword the remote had
/// never been asked about could never be seeded. Table size must not enter the
/// decision at all: it is a fact about leaderboard syncs, which know nothing
/// about any keyword.
#[test]
fn a_full_snapshot_does_not_suppress_seeding_an_unasked_keyword() {
    with_temp_data_root(|_| {
        fill_snapshot(700);

        assert_eq!(
            keywords_needing_remote_seed(
                &conn(),
                &keywords(&["obscure"]),
                &hits(&[("obscure", 0)]),
            )
            .expect("decide"),
            vec!["obscure".to_string()],
            "700 rows of leaderboard data say nothing about this keyword"
        );
    });
}

/// A degraded seed never gets a TTL, so it never counts as "already asked" —
/// otherwise one lossy answer would pin the keyword forever, the same
/// self-lock the `degraded_reason` column exists to prevent.
#[test]
fn a_degraded_seed_is_re_asked() {
    with_temp_data_root(|_| {
        record_seed("obscure", true);

        assert_eq!(
            keywords_needing_remote_seed(
                &conn(),
                &keywords(&["obscure"]),
                &hits(&[("obscure", 1)]),
            )
            .expect("decide"),
            vec!["obscure".to_string()],
            "a lossy answer is not an answer"
        );
    });
}

/// Search is case-insensitive, so two casings of one word must not each keep
/// their own seed record — they would only ever disagree about whether the
/// same question has been asked.
#[test]
fn seed_records_are_case_folded() {
    with_temp_data_root(|_| {
        record_seed("Rust", false);

        assert!(
            keywords_needing_remote_seed(&conn(), &keywords(&["rust"]), &hits(&[("rust", 1)]))
                .expect("decide")
                .is_empty(),
            "'Rust' and 'rust' are the same question"
        );
    });
}

/// Each keyword is decided on its own evidence; one well-covered keyword must
/// not vouch for an unasked one in the same query.
#[test]
fn keywords_are_decided_independently() {
    with_temp_data_root(|_| {
        let needed = keywords_needing_remote_seed(
            &conn(),
            &keywords(&["rust", "obscure"]),
            &hits(&[("rust", AI_SEARCH_REMOTE_SEED_MIN_HITS + 5), ("obscure", 0)]),
        )
        .expect("decide");

        assert_eq!(needed, vec!["obscure".to_string()]);
    });
}
