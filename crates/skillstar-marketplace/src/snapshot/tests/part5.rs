//! Schema migration regressions: the `marketplace_sync_state` column
//! migrations (v11 content addressing, v12 `degraded_reason`) and, above all,
//! that each one is actually reached from an existing older database. See
//! `docs/errors.md`.

use super::*;
use crate::snapshot::*;

/// Same idempotency contract as v11: `ALTER TABLE ADD COLUMN` is not repeatable
/// and `user_version` only moves once the whole chain lands.
#[test]
fn v12_migration_is_repeatable_and_adds_the_degraded_column() {
    with_temp_data_root(|_| {
        let conn = create_connection().expect("open marketplace db");
        migrate_v11_to_v12(&conn).expect("v12 must be repeatable");
        migrate_v11_to_v12(&conn).expect("v12 must stay repeatable");

        let columns: Vec<String> = conn
            .prepare("PRAGMA table_info(marketplace_sync_state)")
            .expect("pragma")
            .query_map([], |row| row.get(1))
            .expect("column names")
            .collect::<Result<_, _>>()
            .expect("collect columns");
        assert_eq!(
            columns.iter().filter(|c| *c == "degraded_reason").count(),
            1,
            "degraded_reason must exist exactly once"
        );
    });
}

/// …and the half repeatability cannot see: that v12 is actually *wired into*
/// the version chain. The test above starts from `create_connection()`, which
/// has already run the whole chain, so it only exercises
/// `migrate_v11_to_v12`'s own idempotency — changing `if version < 12` to
/// `< 11` in `migrate_schema` left it green.
///
/// The upgrade path is the only one that can break: a fresh install starts at
/// version 0 and gets `degraded_reason` from the base `CREATE TABLE` chain, so
/// no amount of local testing on a new database sees it. An existing v11
/// database instead ends up stamped 12 without the column, and then
/// `scope_sync_state`, `get_marketplace_sync_states` and the MCP sync-state
/// read all die on `no such column` — the marketplace falls back to
/// online-only and the degraded marker silently reads as `false` forever.
///
/// So: hand-seed a real v11 database and go through the startup path, exactly
/// like the v11 test below does for v10.
#[test]
fn the_v12_column_is_added_when_an_existing_v11_database_starts_up() {
    with_temp_data_root(|temp_root| {
        let path = temp_root.join("marketplace.db");
        let conn = open_raw_conn(&path);
        conn.execute_batch(
            "CREATE TABLE marketplace_sync_state (
                scope TEXT PRIMARY KEY,
                last_success_at TEXT,
                last_attempt_at TEXT,
                last_error TEXT,
                next_refresh_at TEXT,
                schema_version INTEGER NOT NULL DEFAULT 1,
                source_host TEXT,
                payload_sha256 TEXT,
                etag TEXT
            );
            INSERT INTO marketplace_sync_state (scope, last_success_at, schema_version)
                VALUES ('leaderboard_all', '2026-08-01T00:00:00Z', 11);
            PRAGMA user_version = 11;",
        )
        .expect("seed a v11 database");
        drop(conn);

        // The real startup path.
        let conn = create_connection().expect("startup migration over a v11 database");
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read user_version");
        assert_eq!(version, SNAPSHOT_SCHEMA_VERSION);

        let columns: Vec<String> = conn
            .prepare("PRAGMA table_info(marketplace_sync_state)")
            .expect("pragma")
            .query_map([], |row| row.get(1))
            .expect("column names")
            .collect::<Result<_, _>>()
            .expect("collect columns");
        assert_eq!(
            columns.iter().filter(|c| *c == "degraded_reason").count(),
            1,
            "an upgraded v11 database must gain degraded_reason, not just a fresh one"
        );

        // The symptom the missing column actually produces: every sync-state
        // read fails, so assert the reads, not only the schema.
        let state = scope_sync_state(&conn, "leaderboard_all")
            .expect("sync state must be readable after the upgrade")
            .expect("the pre-existing row survives");
        assert!(state.degraded_reason.is_none());
        assert!(state.last_success_at.is_some(), "v11 data is preserved");
        drop(conn);
        assert_eq!(
            get_marketplace_sync_states()
                .expect("diagnostics read must work after the upgrade")
                .len(),
            1
        );
    });
}

/// `ALTER TABLE ADD COLUMN` is not repeatable, and `user_version` is only
/// bumped once the whole chain succeeds. A run interrupted between two of the
/// three v11 ALTERs therefore left the columns present at version 10 — and
/// every later startup died on `duplicate column name`, permanently.
#[test]
fn v11_migration_survives_a_rerun_and_a_partial_application() {
    with_temp_data_root(|temp_root| {
        let path = temp_root.join("marketplace.db");
        let conn = open_raw_conn(&path);
        conn.execute_batch(
            "CREATE TABLE marketplace_skill (
                skill_key TEXT PRIMARY KEY,
                source TEXT NOT NULL,
                name TEXT NOT NULL,
                git_url TEXT NOT NULL DEFAULT '',
                author TEXT,
                publisher_name TEXT,
                repo_name TEXT,
                description TEXT NOT NULL DEFAULT '',
                installs INTEGER NOT NULL DEFAULT 0,
                last_seen_remote_at TEXT,
                last_list_sync_at TEXT
            );
            CREATE TABLE marketplace_sync_state (
                scope TEXT PRIMARY KEY,
                last_success_at TEXT,
                last_attempt_at TEXT,
                last_error TEXT,
                next_refresh_at TEXT,
                schema_version INTEGER NOT NULL DEFAULT 1
            );
            -- Partially applied v11: the first ALTER landed, the process died
            -- before the other two and before the user_version bump.
            ALTER TABLE marketplace_sync_state ADD COLUMN source_host TEXT;
            PRAGMA user_version = 10;",
        )
        .expect("seed a partially migrated v10 database");

        migrate_v10_to_v11(&conn).expect("partial application must still migrate");
        migrate_v10_to_v11(&conn).expect("v11 must be repeatable");

        let columns: Vec<String> = conn
            .prepare("PRAGMA table_info(marketplace_sync_state)")
            .expect("pragma")
            .query_map([], |row| row.get(1))
            .expect("column names")
            .collect::<Result<_, _>>()
            .expect("collect columns");
        for expected in ["source_host", "payload_sha256", "etag"] {
            assert_eq!(
                columns.iter().filter(|c| *c == expected).count(),
                1,
                "{expected} must exist exactly once"
            );
        }
        drop(conn);

        // The real startup path over the same database must recover too.
        let conn = create_connection().expect("startup migration over a partial v11 database");
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read user_version");
        assert_eq!(version, SNAPSHOT_SCHEMA_VERSION);
    });
}
