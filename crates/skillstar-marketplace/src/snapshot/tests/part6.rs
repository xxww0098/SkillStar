//! Third-round leftovers from the degraded-snapshot work: the
//! `etag`/`payload_sha256`/`source_host` invariant on the rewrite path, the
//! degraded judgement for every scope that writes the shared skill table, and
//! the degraded marker on the one read path that bypasses the snapshot. See
//! `docs/errors.md`.

use super::*;
use crate::snapshot::*;

fn meta(sha: &str, host: &str, etag: Option<&str>) -> crate::remote::FetchMeta {
    crate::remote::FetchMeta {
        payload_sha256: sha.to_string(),
        source_host: host.to_string(),
        etag: etag.map(ToOwned::to_owned),
        degraded: false,
    }
}

fn degraded_meta(sha: &str) -> crate::remote::FetchMeta {
    crate::remote::FetchMeta {
        degraded: true,
        ..meta(sha, "https://skills.sh/", Some("\"v1\""))
    }
}

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

/// One row must describe one payload. A rewrite replaces the rows, so it also
/// replaces all three columns that describe them — `etag` included, and to
/// `NULL` when the response carried no validator.
///
/// Keeping the previous validator here left the row self-contradictory:
/// `payload_sha256` / `source_host` describing the body that was just written
/// and `etag` naming the one before it, possibly from another host. With a
/// mirror configured and the two hosts diverged, the next request sends that
/// foreign token, can be answered `304`, and pins the divergent body in place.
///
/// The `COALESCE` this replaces belongs to the no-change path only — see the
/// companion test in `part4`.
#[test]
fn a_full_rewrite_takes_its_etag_from_the_payload_it_wrote() {
    with_temp_data_root(|_| {
        let scope = leaderboard_scope("all");
        let first = "a".repeat(64);
        commit_success(
            &scope,
            &meta(&first, "https://skills.sh/", Some("\"v1\"")),
            false,
        );
        assert_eq!(read_state(&scope).etag.as_deref(), Some("\"v1\""));

        // A full rewrite from a mirror that sends no ETag at all.
        let second = "b".repeat(64);
        commit_success(
            &scope,
            &meta(&second, "https://mirror.example/", None),
            false,
        );

        let state = read_state(&scope);
        assert_eq!(
            state.etag, None,
            "a rewrite with no validator must clear the one describing the previous payload"
        );
        assert_eq!(state.payload_sha256.as_deref(), Some(second.as_str()));
        assert_eq!(
            state.source_host.as_deref(),
            Some("https://mirror.example/"),
            "…and the other two columns must describe the payload just written"
        );

        // The ordinary case still works: a rewrite that does carry a validator
        // adopts it.
        let third = "c".repeat(64);
        commit_success(
            &scope,
            &meta(&third, "https://skills.sh/", Some("\"v9\"")),
            false,
        );
        let state = read_state(&scope);
        assert_eq!(state.etag.as_deref(), Some("\"v9\""));
        assert_eq!(state.payload_sha256.as_deref(), Some(third.as_str()));
    });
}

/// `hot` and `trending` write their rows into the same `marketplace_skill`
/// table as `all`, through the same `upsert_skill_in_tx`. Search reads that
/// table, so a degraded `hot` sync leaves fallback rows that search returns —
/// and while the judgement asked only about `leaderboard_all`, it returned them
/// labelled `Fresh` to any user who had opened the hot tab but never the
/// default one.
// The env lock is a plain mutex; this test runs on a current-thread runtime and
// holds it across awaits to keep the data dir stable.
#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn search_reports_stale_while_any_leaderboard_scope_is_degraded() {
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

    // Three hits: enough coverage that `ai_search_local` never reaches for the
    // network (this test must stay hermetic).
    seed_named_skills(&["demo", "demo-two", "demo-three"]);

    let status = || async {
        let plain = search_local("demo", Some(10)).await.expect("search");
        let ai = ai_search_local(&["demo".to_string()], Some(10))
            .await
            .expect("ai search");
        assert_eq!(plain.data.len(), 3, "the rows are served either way");
        assert_eq!(
            plain.snapshot_status, ai.snapshot_status,
            "search and AI search answer the same question"
        );
        plain.snapshot_status
    };

    assert_eq!(
        status().await,
        SnapshotStatus::Fresh,
        "nothing degraded yet"
    );

    // Only `hot` degrades — `all` was never even synced.
    commit_success(
        &leaderboard_scope("hot"),
        &degraded_meta(&"a".repeat(64)),
        false,
    );
    assert_eq!(
        status().await,
        SnapshotStatus::Stale,
        "hot's fallback rows are in the table search reads"
    );

    // Hot recovers; nothing else is degraded.
    commit_success(
        &leaderboard_scope("hot"),
        &meta(&"b".repeat(64), "https://skills.sh/", Some("\"v1\"")),
        false,
    );
    assert_eq!(status().await, SnapshotStatus::Fresh);

    // Same for trending.
    commit_success(
        &leaderboard_scope("trending"),
        &degraded_meta(&"c".repeat(64)),
        false,
    );
    assert_eq!(status().await, SnapshotStatus::Stale);

    // And a healthy `all` does not launder a still-degraded trending.
    commit_success(
        &leaderboard_scope("all"),
        &meta(&"d".repeat(64), "https://skills.sh/", Some("\"v1\"")),
        false,
    );
    assert_eq!(
        status().await,
        SnapshotStatus::Stale,
        "one degraded scope is enough; the rows are shared"
    );
}

/// The direct-from-remote path is the only read path that never consults
/// `marketplace_sync_state`, so nothing downstream can discover that the
/// payload standing in for the unreadable snapshot was itself lossy.
/// `ErrorFallback` alone says "not from the snapshot", not "incomplete" — the
/// last hole in "knowingly lossy data never passes for complete".
#[test]
fn an_error_fallback_reports_a_degraded_remote_payload() {
    let local_err = anyhow!("database is locked");

    let complete = error_fallback(
        vec![1_u8, 2],
        &local_err,
        &meta("", "https://skills.sh/", None),
    );
    assert_eq!(complete.snapshot_status, SnapshotStatus::ErrorFallback);
    assert_eq!(
        complete.error.as_deref(),
        Some(error_detail(&local_err).as_str()),
        "a complete fallback payload reports the local cause and nothing else"
    );

    let lossy = error_fallback(vec![1_u8, 2], &local_err, &degraded_meta(&"a".repeat(64)));
    assert_eq!(lossy.snapshot_status, SnapshotStatus::ErrorFallback);
    let error = lossy.error.expect("the cause is always reported");
    assert!(
        error.contains("database is locked"),
        "the local cause must survive: {error}"
    );
    assert!(
        error.contains(DEGRADED_REMOTE_FALLBACK_NOTE),
        "…and the payload's own incompleteness must be said out loud: {error}"
    );
}
