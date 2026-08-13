//! Regressions for the "Marketplace request failed" outage: seed-state
//! bookkeeping, error recording, content-addressing safety and the degraded
//! snapshot contract. Schema migrations live in `part5`. See `docs/errors.md`.

use super::*;
use crate::snapshot::*;

fn sample_meta(sha: &str) -> crate::remote::FetchMeta {
    crate::remote::FetchMeta {
        payload_sha256: sha.to_string(),
        source_host: "https://skills.sh/".into(),
        etag: Some("\"v1\"".into()),
        degraded: false,
    }
}

fn degraded_meta(sha: &str) -> crate::remote::FetchMeta {
    crate::remote::FetchMeta {
        degraded: true,
        ..sample_meta(sha)
    }
}

/// Write `meta` as a successful sync for `scope`, the way every `sync_scope_*`
/// write transaction does.
fn commit_success(scope: &str, meta: &crate::remote::FetchMeta, fetched_unchanged: bool) {
    let conn = create_connection().expect("open marketplace db");
    let tx = conn.unchecked_transaction().expect("open tx");
    mark_scope_success_with_meta_in_tx(&tx, scope, meta, fetched_unchanged)
        .expect("record success");
    tx.commit().expect("commit success");
}

fn read_state(scope: &str) -> SyncStateEntry {
    let conn = create_connection().expect("open marketplace db");
    scope_sync_state(&conn, scope)
        .expect("read sync state")
        .expect("state row exists")
}

/// Put one row into `marketplace_skill` so the "all" tab has something to show.
fn seed_one_skill() {
    let synced_at = now_rfc3339();
    let conn = create_connection().expect("open marketplace db");
    let tx = conn.unchecked_transaction().expect("open tx");
    upsert_skill_identity_in_tx(&tx, "acme/repo", "demo", 5, &synced_at)
        .expect("upsert skill")
        .expect("skill key");
    tx.commit().expect("commit skill");
}

/// Put several named skills into `marketplace_skill` — what search reads.
fn seed_named_skills(names: &[&str]) {
    let synced_at = now_rfc3339();
    let conn = create_connection().expect("open marketplace db");
    let tx = conn.unchecked_transaction().expect("open tx");
    for name in names {
        upsert_skill_identity_in_tx(&tx, "acme/repo", name, 5, &synced_at)
            .expect("upsert skill")
            .expect("skill key");
    }
    tx.commit().expect("commit skills");
}

/// Put one row into the listing table for `scope`, so the scope counts as
/// "has local rows" the way a real sync leaves it.
fn seed_listing_row(scope: &str) {
    let synced_at = now_rfc3339();
    let conn = create_connection().expect("open marketplace db");
    let tx = conn.unchecked_transaction().expect("open tx");
    let skill_key = upsert_skill_identity_in_tx(&tx, "acme/repo", "demo", 5, &synced_at)
        .expect("upsert skill")
        .expect("skill key");
    tx.execute(
        "INSERT OR REPLACE INTO marketplace_listing (listing_type, skill_key, rank, updated_at)
         VALUES (?1, ?2, 1, ?3)",
        rusqlite::params![scope, skill_key, synced_at],
    )
    .expect("insert listing row");
    tx.commit().expect("commit listing row");
}

/// `mark_scope_attempt` inserts its row *before* the network call, so row
/// existence must never be read as "this scope has been seeded". It used to be,
/// which meant one failed first sync flipped the scope to `Synced` forever:
/// the local-first readers returned `Miss` without retrying and the startup
/// refresh skipped it because it had no `last_success_at`.
#[test]
fn an_attempt_alone_is_not_synced_and_stays_stale() {
    with_temp_data_root(|_| {
        let scope = leaderboard_scope("all");
        mark_scope_attempt(&scope).expect("record attempt");

        let conn = create_connection().expect("open marketplace db");
        let state = scope_sync_state(&conn, &scope)
            .expect("read sync state")
            .expect("attempt inserted a row");
        assert!(state.last_success_at.is_none());
        assert_eq!(
            sync_seed_state(&conn, &scope).expect("seed state"),
            ScopeSeedState::NeverSynced,
            "a failed attempt must leave the scope retryable"
        );
        drop(conn);

        assert!(
            is_scope_stale(&scope).expect("staleness"),
            "a never-succeeded scope must be picked up by the startup refresh"
        );

        // A real success flips both answers.
        let conn = create_connection().expect("reopen marketplace db");
        let tx = conn.unchecked_transaction().expect("open tx");
        mark_scope_success_in_tx(&tx, &scope).expect("record success");
        tx.commit().expect("commit success");
        assert_eq!(
            sync_seed_state(&conn, &scope).expect("seed state"),
            ScopeSeedState::Synced
        );
        drop(conn);
        assert!(
            !is_scope_stale(&scope).expect("staleness"),
            "a fresh scope is not stale"
        );
    });
}

/// A remote fetch failure has to land in `marketplace_sync_state.last_error`.
/// It never did (only DB write failures were recorded) while
/// `mark_scope_attempt` cleared the column on every retry — so the diagnostics
/// panel reported "no error" for a scope that had never once succeeded.
///
/// Hermetic: a dead-end proxy makes every request fail without leaving the box.
// The env lock is a plain mutex shared with the sync tests; this test runs on a
// current-thread runtime and must hold it across the await to keep
// `SKILLSTAR_DATA_DIR` stable for the whole sync.
#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn failed_remote_fetch_records_last_error() {
    let _guard = test_env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let temp = tempfile::tempdir().expect("create temp dir");
    let temp_root = temp.path().to_path_buf();
    configure_runtime(SnapshotRuntimeConfig::new(
        temp_root.join("marketplace.db"),
        temp_root.clone(),
        HashSet::new,
        || -> InstalledSkillsFuture { Box::pin(async { Ok(Vec::new()) }) },
    ));
    unsafe {
        std::env::set_var("SKILLSTAR_DATA_DIR", &temp_root);
    }
    skillstar_core::config::proxy::save_config(&skillstar_core::config::proxy::ProxyConfig {
        enabled: true,
        host: "127.0.0.1".into(),
        port: 1,
        ..Default::default()
    })
    .expect("write dead-end proxy config");

    let scope = leaderboard_scope("all");
    let result = sync_scope_leaderboard("all").await;
    assert!(result.is_err(), "every host is unreachable");

    let conn = create_connection().expect("open marketplace db");
    let state = scope_sync_state(&conn, &scope)
        .expect("read sync state")
        .expect("attempt inserted a row");
    assert!(
        state.last_error.is_some(),
        "the remote failure must be recorded for diagnostics"
    );
    assert!(state.last_success_at.is_none());
    assert_eq!(
        sync_seed_state(&conn, &scope).expect("seed state"),
        ScopeSeedState::NeverSynced,
        "a failed sync must stay retryable"
    );
    drop(conn);

    unsafe {
        std::env::remove_var("SKILLSTAR_DATA_DIR");
    }
}

/// Content addressing skipped the rewrite whenever upstream bytes matched the
/// stored hash — even with zero local rows, which pinned a wiped store empty
/// while reporting a successful, fresh sync.
#[test]
fn unchanged_bytes_do_not_excuse_a_scope_that_lost_its_rows() {
    with_temp_data_root(|_| {
        let scope = leaderboard_scope("all");
        let meta = sample_meta(&"a".repeat(64));
        let synced_at = now_rfc3339();

        let conn = create_connection().expect("open marketplace db");
        let tx = conn.unchecked_transaction().expect("open tx");
        let skill_key = upsert_skill_identity_in_tx(&tx, "acme/repo", "demo", 5, &synced_at)
            .expect("upsert skill")
            .expect("skill key");
        tx.execute(
            "INSERT INTO marketplace_listing (listing_type, skill_key, rank, updated_at)
             VALUES (?1, ?2, 1, ?3)",
            rusqlite::params![scope, skill_key, synced_at],
        )
        .expect("insert listing row");
        mark_scope_success_with_meta_in_tx(&tx, &scope, &meta, false).expect("record success");
        tx.commit().expect("commit");

        assert_eq!(scope_local_row_count(&conn, &scope).expect("count"), 1);
        assert!(scope_has_local_rows(&scope));

        // The local rows vanish (failed write, manual wipe, empty parse) while
        // upstream keeps serving the exact same bytes.
        conn.execute("DELETE FROM marketplace_listing", [])
            .expect("wipe listing rows");
        drop(conn);

        assert!(
            payload_unchanged(&meta, &Some("a".repeat(64))),
            "the payload really is byte-identical…"
        );
        assert!(
            !scope_has_local_rows(&scope),
            "…but nothing is left locally, so the rewrite must not be skipped"
        );
        // With no rows to preserve, the ETag is withheld so the server has to
        // answer 200 with a full body instead of an unusable 304.
        assert_eq!(conditional_etag(&Some("\"v1\"".into()), false, false), None);
        assert_eq!(
            conditional_etag(&Some("\"v1\"".into()), true, false),
            Some("\"v1\"")
        );
        // A degraded scope withholds it too: a 304 carries no body, and the
        // body is what a fixed parser needs to climb back out of degraded.
        assert_eq!(conditional_etag(&Some("\"v1\"".into()), true, true), None);

        // …and the decision the sync path actually takes, not just the two
        // helpers side by side. Asserting only the helpers is why deleting the
        // `has_local_rows &&` guard from all five `sync_scope_*` bodies left
        // every test green.
        assert_eq!(
            plan_refresh(&meta, &Some("a".repeat(64)), false, false),
            RefreshPlan::Rewrite,
            "unchanged bytes with no local rows must still rewrite"
        );
        assert_eq!(
            plan_refresh(&meta, &Some("a".repeat(64)), true, false),
            RefreshPlan::SkipRewrite,
            "unchanged bytes with the rows still present may skip the rewrite"
        );
    });
}

/// The full post-fetch truth table. `plan_refresh` is the single decision every
/// `sync_scope_*` makes after a fetch, so this is where the row-count guard and
/// the degraded entry/exit rules are pinned.
#[test]
fn plan_refresh_covers_every_row_count_and_degraded_combination() {
    let sha = "a".repeat(64);
    let same = Some(sha.clone());
    let different = Some("b".repeat(64));
    let complete = sample_meta(&sha);
    let degraded = degraded_meta(&sha);
    let not_modified = sample_meta("");

    // Empty scope: always rewrite, degraded payload included — a fallback seed
    // beats an empty store, and that is the only way a first sync can recover.
    assert_eq!(
        plan_refresh(&complete, &same, false, false),
        RefreshPlan::Rewrite
    );
    assert_eq!(
        plan_refresh(&not_modified, &same, false, false),
        RefreshPlan::Rewrite
    );
    assert_eq!(
        plan_refresh(&degraded, &same, false, false),
        RefreshPlan::Rewrite
    );

    // Populated + complete payload: skip only when the bytes really match.
    assert_eq!(
        plan_refresh(&complete, &same, true, false),
        RefreshPlan::SkipRewrite
    );
    assert_eq!(
        plan_refresh(&complete, &different, true, false),
        RefreshPlan::Rewrite
    );
    assert_eq!(
        plan_refresh(&not_modified, &different, true, false),
        RefreshPlan::SkipRewrite,
        "a 304 over live rows is a no-change refresh"
    );

    // Populated + complete stored data: refuse the degraded payload.
    assert_eq!(
        plan_refresh(&degraded, &different, true, false),
        RefreshPlan::RejectDegraded
    );
    assert_eq!(
        plan_refresh(&degraded, &same, true, false),
        RefreshPlan::RejectDegraded,
        "the refusal is decided before the no-change shortcut"
    );

    // Populated but the stored data is itself degraded: an equally degraded
    // payload is allowed to replace it. Refusing here is what made the first
    // fallback seed self-locking — rows existed, so every later refresh failed
    // deterministically and forever.
    assert_eq!(
        plan_refresh(&degraded, &different, true, true),
        RefreshPlan::Rewrite
    );
    assert_eq!(
        plan_refresh(&degraded, &same, true, true),
        RefreshPlan::SkipRewrite,
        "identical fallback bytes need no rewrite, but must not error either"
    );
    // …and a complete payload always wins over degraded rows — including
    // byte-identical ones. The stored fingerprint was taken from a payload the
    // parser of the day could not read; the same bytes through a fixed parser
    // are the main exit from degraded, and content addressing would skip
    // exactly that rewrite and pin the fallback rows forever.
    assert_eq!(
        plan_refresh(&complete, &different, true, true),
        RefreshPlan::Rewrite
    );
    assert_eq!(
        plan_refresh(&complete, &same, true, true),
        RefreshPlan::Rewrite,
        "identical bytes must still be re-parsed while the stored snapshot is degraded"
    );
    // A 304 is the exception: there is no body to re-parse, so it stays a
    // no-change refresh (which preserves the degraded marker) instead of
    // rewriting the scope to nothing.
    assert_eq!(
        plan_refresh(&not_modified, &same, true, true),
        RefreshPlan::SkipRewrite,
        "a 304 has no payload to rebuild a degraded scope from"
    );
}

/// The wiring MX-A exposed: nothing verified that `sync_scope_*` reads the
/// *persisted* degraded marker back out. Mutating `stored_scope_state`'s
/// `degraded_reason.is_some()` to a plain `false` — which restores the original
/// self-lock exactly — left every test green, because the degraded tests
/// assembled `plan_refresh`'s arguments by hand and the truth-table test never
/// touched SQLite at all.
///
/// So this goes through the real chain: a degraded write lands, then the
/// decision is taken by `plan_scope_refresh`, which is the single seam all five
/// `sync_scope_*` bodies decide through. No network — only the post-fetch half,
/// which is where every guard in this file lives.
#[test]
fn a_persisted_degraded_marker_is_read_back_and_lets_the_next_refresh_through() {
    with_temp_data_root(|_| {
        let scope = leaderboard_scope("all");
        let stored_sha = "a".repeat(64);

        // A degraded leaderboard sync: fallback rows + the marker, written the
        // way `sync_scope_leaderboard`'s transaction writes them.
        seed_listing_row(&scope);
        commit_success(&scope, &degraded_meta(&stored_sha), false);

        assert!(scope_has_local_rows(&scope), "the fallback rows are there");
        let stored = stored_scope_state(&scope);
        assert!(
            stored.degraded,
            "the persisted marker must survive the read path — this is the self-lock"
        );
        assert_eq!(stored.payload_sha256.as_deref(), Some(stored_sha.as_str()));

        // Upstream is still broken: the next refresh is degraded too. It must
        // be accepted, not rejected — rejecting is the deterministic forever
        // failure this column exists to end.
        assert_eq!(
            plan_scope_refresh(&scope, &degraded_meta(&"b".repeat(64))),
            RefreshPlan::Rewrite,
            "a degraded refresh over degraded rows must write, not error"
        );

        // Upstream (or the parser) recovers, over the very same bytes.
        assert_eq!(
            plan_scope_refresh(&scope, &sample_meta(&stored_sha)),
            RefreshPlan::Rewrite,
            "the exit from degraded must not be skipped by content addressing"
        );
        // …and the request that fetches those bytes must not be conditional.
        assert_eq!(
            conditional_etag(&stored.etag, scope_has_local_rows(&scope), stored.degraded),
            None
        );

        // Once a complete payload lands, everything goes back to normal:
        // identical bytes skip the rewrite again.
        commit_success(&scope, &sample_meta(&stored_sha), false);
        let stored = stored_scope_state(&scope);
        assert!(!stored.degraded, "a complete payload is the exit");
        assert_eq!(
            plan_scope_refresh(&scope, &sample_meta(&stored_sha)),
            RefreshPlan::SkipRewrite
        );
        assert_eq!(
            conditional_etag(&stored.etag, scope_has_local_rows(&scope), stored.degraded),
            Some("\"v1\"")
        );
    });
}

/// A degraded leaderboard fills `marketplace_skill` with capped fuzzy matches,
/// and search reads that same table. Search has no TTL of its own, but it may
/// not call those rows `Fresh` — that is the half of the freshness contract
/// that does apply to it. See `docs/features/marketplace/README.md`.
#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn search_reports_stale_while_the_shared_table_is_degraded() {
    let _guard = test_env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let temp = tempfile::tempdir().expect("create temp dir");
    let temp_root = temp.path().to_path_buf();
    configure_runtime(SnapshotRuntimeConfig::new(
        temp_root.join("marketplace.db"),
        temp_root.clone(),
        HashSet::new,
        || -> InstalledSkillsFuture { Box::pin(async { Ok(Vec::new()) }) },
    ));

    let scope = leaderboard_scope("all");
    // Three hits: enough that `ai_search_local` sees the coverage it needs and
    // does not reach for the network (this test must stay hermetic).
    seed_named_skills(&["demo", "demo-two", "demo-three"]);

    // No degraded marker: a hit is a hit.
    let result = search_local("demo", Some(10)).await.expect("search");
    assert_eq!(result.data.len(), 3);
    assert_eq!(result.snapshot_status, SnapshotStatus::Fresh);
    let result = ai_search_local(&["demo".to_string()], Some(10))
        .await
        .expect("ai search");
    assert_eq!(result.snapshot_status, SnapshotStatus::Fresh);

    // The leaderboard degrades; the rows search returns are now suspect.
    commit_success(&scope, &degraded_meta(&"a".repeat(64)), false);
    let result = search_local("demo", Some(10)).await.expect("search");
    assert_eq!(result.data.len(), 3, "the rows are still served…");
    assert_eq!(
        result.snapshot_status,
        SnapshotStatus::Stale,
        "…but never as fresh"
    );
    let result = ai_search_local(&["demo".to_string()], Some(10))
        .await
        .expect("ai search");
    assert_eq!(result.snapshot_status, SnapshotStatus::Stale);

    // A complete leaderboard sync clears it for search too.
    commit_success(&scope, &sample_meta(&"b".repeat(64)), false);
    let result = search_local("demo", Some(10)).await.expect("search");
    assert_eq!(result.snapshot_status, SnapshotStatus::Fresh);
}

/// Entering degraded: the scope has data and a success timestamp, never reports
/// fresh, and does *not* pretend to have failed. `last_success_at` +
/// `last_error` both set would read as "last sync failed" to every diagnostics
/// consumer — which is exactly what the previous post-commit
/// `mark_scope_degraded` produced.
#[test]
fn a_degraded_write_is_stale_immediately_and_records_no_error() {
    with_temp_data_root(|_| {
        let scope = leaderboard_scope("all");
        commit_success(&scope, &degraded_meta(&"a".repeat(64)), false);

        let state = read_state(&scope);
        assert!(state.last_success_at.is_some(), "there *is* data now");
        assert!(
            state.next_refresh_at.is_none(),
            "degraded rows must never sit behind a full TTL"
        );
        assert!(state.degraded_reason.is_some(), "and must say why");
        assert!(
            state.last_error.is_none(),
            "a degraded success is not a failure"
        );

        let conn = create_connection().expect("open marketplace db");
        assert!(!is_scope_fresh_conn(&conn, &scope).expect("freshness"));
        drop(conn);
        assert!(
            is_scope_stale(&scope).expect("staleness"),
            "the startup refresh must pick a degraded scope back up"
        );
    });
}

/// Leaving degraded: a complete payload clears the marker and restores the TTL.
/// Until then the stored state keeps answering "degraded", which is what lets
/// the next fallback payload replace it instead of erroring forever.
#[test]
fn a_complete_payload_clears_the_degraded_marker_and_restores_the_ttl() {
    with_temp_data_root(|_| {
        let scope = leaderboard_scope("all");
        commit_success(&scope, &degraded_meta(&"a".repeat(64)), false);
        assert!(read_state(&scope).degraded_reason.is_some());

        // A second degraded refresh is accepted (no deterministic failure).
        commit_success(&scope, &degraded_meta(&"b".repeat(64)), false);
        let state = read_state(&scope);
        assert!(state.degraded_reason.is_some());
        assert!(state.next_refresh_at.is_none());

        // Upstream recovers.
        commit_success(&scope, &sample_meta(&"c".repeat(64)), false);
        let state = read_state(&scope);
        assert!(
            state.degraded_reason.is_none(),
            "a complete payload is the exit from degraded"
        );
        assert!(state.next_refresh_at.is_some(), "and restores the TTL");

        let conn = create_connection().expect("open marketplace db");
        assert!(is_scope_fresh_conn(&conn, &scope).expect("freshness"));
    });
}

/// A 304 over a degraded scope must not launder it: nothing was rewritten, so
/// the stored rows are still the fallback ones. Handing them the full TTL here
/// would put the "already up to date" label back on a capped fuzzy search
/// result.
#[test]
fn a_no_change_refresh_preserves_the_degraded_marker() {
    with_temp_data_root(|_| {
        let scope = leaderboard_scope("all");
        commit_success(&scope, &degraded_meta(&"a".repeat(64)), false);

        // 304: empty sha, degraded flag unset because nothing was parsed.
        let not_modified = crate::remote::FetchMeta {
            payload_sha256: String::new(),
            source_host: "https://skills.sh/".into(),
            etag: Some("\"v1\"".into()),
            degraded: false,
        };
        commit_success(&scope, &not_modified, true);

        let state = read_state(&scope);
        assert!(
            state.degraded_reason.is_some(),
            "an unchanged fetch changes no rows, so it changes no quality"
        );
        assert!(state.next_refresh_at.is_none());
        assert!(state.last_error.is_none());
    });
}

/// A no-change refresh keeps the stored rows, the fingerprint and the source
/// host — but it must still adopt a rotated validator. A 200 whose body is
/// byte-identical can carry a brand-new ETag; keeping the old one meant every
/// later request sent a token the server no longer knew, so no 304 ever came
/// back and the full body was transferred forever.
#[test]
fn a_no_change_refresh_adopts_a_rotated_etag_but_keeps_the_fingerprint() {
    with_temp_data_root(|_| {
        let scope = leaderboard_scope("all");
        let sha = "a".repeat(64);
        commit_success(&scope, &sample_meta(&sha), false);

        // Same bytes, new validator, different host (a mirror answered).
        let rotated = crate::remote::FetchMeta {
            payload_sha256: sha.clone(),
            source_host: "https://mirror.example/".into(),
            etag: Some("\"v2\"".into()),
            degraded: false,
        };
        commit_success(&scope, &rotated, true);

        let state = read_state(&scope);
        assert_eq!(
            state.etag.as_deref(),
            Some("\"v2\""),
            "the next request must send the validator the server just issued"
        );
        assert_eq!(state.payload_sha256.as_deref(), Some(sha.as_str()));
        assert_eq!(state.source_host.as_deref(), Some("https://skills.sh/"));

        // A 304 that echoes no ETag keeps the one we have.
        let not_modified = crate::remote::FetchMeta {
            payload_sha256: String::new(),
            source_host: "https://skills.sh/".into(),
            etag: None,
            degraded: false,
        };
        commit_success(&scope, &not_modified, true);
        assert_eq!(read_state(&scope).etag.as_deref(), Some("\"v2\""));
    });
}

/// The default "all" tab used to sit outside the freshness contract entirely:
/// it reported `Fresh` whenever `marketplace_skill` had any row at all, so a
/// degraded seed, an expired TTL and a stray search seed all read as "up to
/// date" and `Stale` was unreachable — which made the UI's auto-refresh, its
/// retry budget and its retry button dead code on the most-used view.
///
/// No network: every branch here is reached with rows already present.
// The env lock is a plain mutex; this test runs on a current-thread runtime and
// holds it across awaits to keep `SKILLSTAR_DATA_DIR` stable.
#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn the_all_tab_reports_stale_exactly_when_the_leaderboard_scope_is() {
    let _guard = test_env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let temp = tempfile::tempdir().expect("create temp dir");
    let temp_root = temp.path().to_path_buf();
    configure_runtime(SnapshotRuntimeConfig::new(
        temp_root.join("marketplace.db"),
        temp_root.clone(),
        HashSet::new,
        || -> InstalledSkillsFuture { Box::pin(async { Ok(Vec::new()) }) },
    ));

    let scope = leaderboard_scope("all");
    seed_one_skill();

    // Rows exist but the leaderboard scope was never synced (a search seed put
    // them there). Not fresh → stale, and the startup refresh will pick it up.
    let result = list_skills_local().await.expect("read all skills");
    assert_eq!(result.data.len(), 1);
    assert_eq!(
        result.snapshot_status,
        SnapshotStatus::Stale,
        "rows alone never mean fresh"
    );

    // A degraded leaderboard write is the exact scenario the UI contract is
    // about: 200 fallback rows must not claim to be the leaderboard.
    commit_success(&scope, &degraded_meta(&"a".repeat(64)), false);
    let result = list_skills_local().await.expect("read all skills");
    assert_eq!(
        result.snapshot_status,
        SnapshotStatus::Stale,
        "degraded data must be marked stale, not fresh"
    );
    assert!(
        is_scope_stale(&scope).expect("staleness"),
        "and `schedule_startup_refreshes` must retry it"
    );

    // A complete sync is the only thing that reports fresh.
    commit_success(&scope, &sample_meta(&"b".repeat(64)), false);
    let result = list_skills_local().await.expect("read all skills");
    assert_eq!(result.snapshot_status, SnapshotStatus::Fresh);
    assert!(result.snapshot_updated_at.is_some());
    assert!(result.error.is_none());
}

/// An empty table with a successfully synced scope is a real (empty) answer,
/// not a reason to hit the network.
#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn the_all_tab_reports_miss_for_an_empty_but_synced_scope() {
    let _guard = test_env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let temp = tempfile::tempdir().expect("create temp dir");
    let temp_root = temp.path().to_path_buf();
    configure_runtime(SnapshotRuntimeConfig::new(
        temp_root.join("marketplace.db"),
        temp_root.clone(),
        HashSet::new,
        || -> InstalledSkillsFuture { Box::pin(async { Ok(Vec::new()) }) },
    ));

    commit_success(
        &leaderboard_scope("all"),
        &sample_meta(&"a".repeat(64)),
        false,
    );
    let result = list_skills_local().await.expect("read all skills");
    assert!(result.data.is_empty());
    assert_eq!(result.snapshot_status, SnapshotStatus::Miss);
}

#[test]
fn scope_row_count_covers_every_content_addressed_scope() {
    with_temp_data_root(|_| {
        let synced_at = now_rfc3339();
        let conn = create_connection().expect("open marketplace db");

        assert_eq!(
            scope_local_row_count(&conn, "official_publishers").expect("count"),
            0
        );
        conn.execute(
            "INSERT INTO marketplace_publisher (publisher_name, repo_count, skill_count, url, updated_at)
             VALUES ('acme', 1, 1, 'https://skills.sh/acme', ?1)",
            rusqlite::params![synced_at],
        )
        .expect("insert publisher");
        assert_eq!(
            scope_local_row_count(&conn, "official_publishers").expect("count"),
            1
        );

        assert_eq!(
            scope_local_row_count(&conn, "publisher_repos:acme").expect("count"),
            0
        );
        conn.execute(
            "INSERT INTO marketplace_repo (source, publisher_name, repo_name, skill_count, installs, installs_label, url, updated_at)
             VALUES ('acme/repo', 'acme', 'repo', 1, 5, '5', 'https://skills.sh/acme/repo', ?1)",
            rusqlite::params![synced_at],
        )
        .expect("insert repo");
        assert_eq!(
            scope_local_row_count(&conn, "publisher_repos:acme").expect("count"),
            1
        );

        let tx = conn.unchecked_transaction().expect("open tx");
        let skill_key = upsert_skill_identity_in_tx(&tx, "acme/repo", "demo", 5, &synced_at)
            .expect("upsert skill")
            .expect("skill key");
        tx.execute(
            "INSERT INTO marketplace_repo_skill (source, skill_key, installs, rank, updated_at)
             VALUES ('acme/repo', ?1, 5, 1, ?2)",
            rusqlite::params![skill_key, synced_at],
        )
        .expect("insert repo skill");
        tx.commit().expect("commit");
        assert_eq!(
            scope_local_row_count(&conn, "repo_skills:acme/repo").expect("count"),
            1
        );

        let detail_scope = skill_detail_scope("acme/repo", "demo").expect("detail scope");
        assert_eq!(
            scope_local_row_count(&conn, &detail_scope).expect("count"),
            0
        );
        let tx = conn.unchecked_transaction().expect("open tx");
        upsert_detail_in_tx(
            &tx,
            "acme/repo",
            "demo",
            &crate::remote::MarketplaceSkillDetails {
                summary: Some("demo".into()),
                readme: None,
                weekly_installs: None,
                github_stars: None,
                first_seen: None,
                security_audits: Vec::new(),
            },
            &synced_at,
        )
        .expect("upsert detail");
        tx.commit().expect("commit detail");
        assert_eq!(
            scope_local_row_count(&conn, &detail_scope).expect("count"),
            1
        );
    });
}
