//! v12 → v13 migration: the "existing database gains new columns" path.
//!
//! This is the scenario the audit flagged as having zero coverage. Both MCP
//! tables are created with `CREATE TABLE IF NOT EXISTS`, so a shipped install
//! sitting at `user_version = 12` never re-runs them; a column added only to
//! the create path exists for fresh installs and for nobody else, and every
//! MCP marketplace read then dies with `no such column`.

use rusqlite::Connection;

use crate::mcp_snapshot::{MCP_SERVER_COLUMNS_V13, MCP_SERVER_TABLES};
use crate::snapshot::migrations::migrate_v12_to_v13;

use super::*;

/// The MCP tables exactly as schema v12 shipped them — no `title`, no
/// `status`, no `is_latest`, no `icons_json`.
const V12_MCP_SCHEMA: &str = "
    CREATE TABLE mcp_registry_server (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        namespace TEXT NOT NULL,
        description TEXT NOT NULL DEFAULT '',
        repo_url TEXT NOT NULL DEFAULT '',
        stars INTEGER NOT NULL DEFAULT 0,
        license TEXT,
        version TEXT,
        kind TEXT NOT NULL DEFAULT 'unknown',
        runtimes_json TEXT NOT NULL DEFAULT '[]',
        readme TEXT,
        packages_json TEXT NOT NULL DEFAULT '[]',
        remotes_json TEXT NOT NULL DEFAULT '[]',
        raw_server_json TEXT NOT NULL DEFAULT '{}',
        updated_at TEXT,
        fetched_at TEXT NOT NULL
    );
    CREATE INDEX idx_mcp_registry_stars ON mcp_registry_server(stars DESC);
    CREATE VIRTUAL TABLE mcp_registry_server_fts USING fts5(
        id, name, namespace, description, tokenize='unicode61'
    );
    CREATE TABLE mcp_curated_server (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        namespace TEXT NOT NULL,
        description TEXT NOT NULL DEFAULT '',
        repo_url TEXT NOT NULL DEFAULT '',
        stars INTEGER NOT NULL DEFAULT 0,
        license TEXT,
        version TEXT,
        kind TEXT NOT NULL DEFAULT 'unknown',
        runtimes_json TEXT NOT NULL DEFAULT '[]',
        readme TEXT,
        packages_json TEXT NOT NULL DEFAULT '[]',
        remotes_json TEXT NOT NULL DEFAULT '[]',
        raw_server_json TEXT NOT NULL DEFAULT '{}',
        updated_at TEXT,
        fetched_at TEXT NOT NULL,
        source TEXT NOT NULL DEFAULT 'skillstar-curated',
        is_recommended INTEGER NOT NULL DEFAULT 1,
        priority INTEGER NOT NULL DEFAULT 100
    );
    CREATE INDEX idx_mcp_curated_recommended_priority
        ON mcp_curated_server(is_recommended DESC, priority ASC, name ASC);
    CREATE VIRTUAL TABLE mcp_curated_server_fts USING fts5(
        id, name, namespace, description, tokenize='unicode61'
    );
    CREATE TABLE marketplace_sync_state (
        scope TEXT PRIMARY KEY,
        last_success_at TEXT,
        last_attempt_at TEXT,
        last_error TEXT,
        next_refresh_at TEXT,
        schema_version INTEGER NOT NULL DEFAULT 1,
        source_host TEXT,
        payload_sha256 TEXT,
        etag TEXT,
        degraded_reason TEXT
    );
";

/// A v12 database holding one registry row and one curated row, written with
/// the v12 column set — i.e. what a user upgrading actually has on disk.
fn v12_conn() -> Connection {
    let conn = Connection::open_in_memory().expect("in-memory sqlite");
    conn.execute_batch(V12_MCP_SCHEMA).unwrap();
    conn.execute_batch(
        "INSERT INTO mcp_registry_server (
            id, name, namespace, description, repo_url, stars, license, version, kind,
            runtimes_json, readme, packages_json, remotes_json, raw_server_json, updated_at, fetched_at
         ) VALUES (
            'legacy-registry', 'filesystem', 'acme/filesystem', 'old row', 'https://example.com',
            9, 'MIT', '1.0.0', 'stdio', '[\"npx\"]', '# readme',
            '[{\"runtime\":\"npx\",\"identifier\":\"@acme/fs\",\"requiredEnv\":[\"TOKEN\"]}]',
            '[]', '{\"name\":\"acme/filesystem\"}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'
         );
         INSERT INTO mcp_registry_server_fts (id, name, namespace, description)
            VALUES ('legacy-registry', 'filesystem', 'acme/filesystem', 'old row');
         INSERT INTO mcp_curated_server (
            id, name, namespace, description, repo_url, stars, license, version, kind,
            runtimes_json, readme, packages_json, remotes_json, raw_server_json, updated_at,
            fetched_at, source, is_recommended, priority
         ) VALUES (
            'legacy-curated', 'curated-thing', 'legacy-curated', 'old curated', '', 0, NULL, NULL,
            'remote', '[]', NULL, '[]', '[]', '{}', NULL, '2026-01-01T00:00:00Z', 'adspower', 1, 0
         );
         INSERT INTO mcp_curated_server_fts (id, name, namespace, description)
            VALUES ('legacy-curated', 'curated-thing', 'legacy-curated', 'old curated');",
    )
    .unwrap();
    conn.pragma_update(None, "user_version", 12_i64).unwrap();
    conn
}

fn columns(conn: &Connection, table: &str) -> Vec<String> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .unwrap();
    let rows = stmt.query_map([], |row| row.get::<_, String>(1)).unwrap();
    rows.map(|r| r.unwrap()).collect()
}

#[test]
fn v12_database_gains_every_new_column() {
    let conn = v12_conn();
    for table in MCP_SERVER_TABLES {
        let before = columns(&conn, table);
        for (column, _) in MCP_SERVER_COLUMNS_V13 {
            assert!(
                !before.contains(&column.to_string()),
                "{table}.{column} must not exist before the migration"
            );
        }
    }

    migrate_v12_to_v13(&conn).expect("v13 migration");

    for table in MCP_SERVER_TABLES {
        let after = columns(&conn, table);
        for (column, _) in MCP_SERVER_COLUMNS_V13 {
            assert!(
                after.contains(&column.to_string()),
                "{table}.{column} missing after the migration"
            );
        }
    }
}

/// Existing rows must come out of the migration usable, with the defaults the
/// model expects — in particular `is_latest = 1`, because defaulting it to 0
/// would flag every pre-existing server as outdated.
#[test]
fn existing_rows_get_sane_defaults_and_still_read() {
    let conn = v12_conn();
    migrate_v12_to_v13(&conn).expect("v13 migration");

    let full = load_full_server(&conn, "legacy-registry").unwrap().unwrap();
    assert_eq!(full.status, crate::mcp_models::McpServerStatus::Active);
    assert!(full.is_latest);
    assert!(full.icons.is_empty());
    assert!(full.contributing_sources.is_empty());
    assert!(full.title.is_none());
    // The legacy `packages_json` shape still deserializes into the new model.
    assert_eq!(full.packages[0].identifier, "@acme/fs");
    assert_eq!(full.packages[0].required_env, vec!["TOKEN".to_string()]);

    // Every card read path works against the migrated tables.
    let cards = load_cards(&conn).unwrap();
    assert_eq!(cards.len(), 2);
    assert!(cards.iter().all(|c| c.is_latest));
    assert_eq!(load_cards_by_publisher(&conn, "github").unwrap().len(), 1);
    assert_eq!(load_cards_by_publisher(&conn, "adspower").unwrap().len(), 1);
    assert_eq!(search_cards(&conn, "filesystem", 10).unwrap().len(), 1);

    // …including the new filters, which read the freshly added columns.
    let page = query_cards(
        &conn,
        &crate::mcp_snapshot::filters::McpServerQuery {
            statuses: vec![crate::mcp_models::McpServerStatus::Active],
            latest_only: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(page.total, 2);
}

/// `user_version` is bumped only after the whole chain succeeds, so a crash
/// between two migrations replays this one. A bare `ALTER` would then fail
/// with `duplicate column name` and brick every later startup.
#[test]
fn migration_is_replayable() {
    let conn = v12_conn();
    migrate_v12_to_v13(&conn).expect("first run");
    migrate_v12_to_v13(&conn).expect("replay must be a no-op, not a failure");
    migrate_v12_to_v13(&conn).expect("and again");
    assert_eq!(columns(&conn, "mcp_registry_server").len(), 16 + 8);
}

/// A database old enough to predate the MCP tables entirely (pre-v8) reaches
/// v13 through the create path, so v13 must skip rather than fail.
#[test]
fn migration_skips_databases_without_mcp_tables() {
    let conn = Connection::open_in_memory().unwrap();
    migrate_v12_to_v13(&conn).expect("no MCP tables is not an error");
}

/// The `CREATE TABLE` half and the `ALTER TABLE` half must produce identical
/// tables. If they drift, fresh installs and upgraded installs run different
/// schemas — and only one of them is exercised by the rest of the suite.
#[test]
fn migrated_schema_matches_a_fresh_install() {
    let migrated = v12_conn();
    migrate_v12_to_v13(&migrated).unwrap();
    let fresh = test_conn();

    for table in MCP_SERVER_TABLES {
        let mut a = columns(&migrated, table);
        let mut b = columns(&fresh, table);
        a.sort();
        b.sort();
        assert_eq!(a, b, "{table} schema drifted between create and migrate");
    }
}
