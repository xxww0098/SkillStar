//! Table definitions for the MCP marketplace snapshot.
//!
//! `mcp_registry_server` (remote catalog, fully swapped on each sync) and
//! `mcp_curated_server` (SkillStar-owned rows, upserted from code) are
//! deliberately column-symmetric so one card/detail query can `UNION ALL` them.
//!
//! **Adding a column here is only half a change.** These statements are
//! `CREATE TABLE IF NOT EXISTS`, so they run for fresh installs only; existing
//! databases are at some `user_version` and never re-execute them. The other
//! half is an `ALTER TABLE ADD COLUMN` migration — see `migrate_v12_to_v13` in
//! `snapshot::migrations` and [`MCP_SERVER_COLUMNS_V13`], which is the single
//! list both halves read from.

use anyhow::{Context, Result};
use rusqlite::Connection;

/// Columns added to both MCP server tables by schema v13, as
/// `(name, sql_type_and_constraints)`.
///
/// The migration walks this list; the `CREATE TABLE` statements below repeat
/// it inline (SQLite has no way to splice it in). The
/// `v13_columns_match_create_table` test pins the two against each other, so
/// a column added to only one side fails in CI rather than in a user's app as
/// `no such column`.
pub(crate) const MCP_SERVER_COLUMNS_V13: [(&str, &str); 8] = [
    ("title", "TEXT"),
    ("website_url", "TEXT"),
    ("icons_json", "TEXT NOT NULL DEFAULT '[]'"),
    ("status", "TEXT NOT NULL DEFAULT 'active'"),
    ("is_latest", "INTEGER NOT NULL DEFAULT 1"),
    ("published_at", "TEXT"),
    ("registry_source", "TEXT"),
    ("contributing_sources_json", "TEXT NOT NULL DEFAULT '[]'"),
];

/// Both tables carrying the symmetric MCP server column set.
pub(crate) const MCP_SERVER_TABLES: [&str; 2] = ["mcp_registry_server", "mcp_curated_server"];

/// Create the MCP registry snapshot tables. Idempotent (`IF NOT EXISTS`).
pub(crate) fn create_mcp_registry_tables(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS mcp_registry_server (
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
            title TEXT,
            website_url TEXT,
            icons_json TEXT NOT NULL DEFAULT '[]',
            status TEXT NOT NULL DEFAULT 'active',
            is_latest INTEGER NOT NULL DEFAULT 1,
            published_at TEXT,
            registry_source TEXT,
            contributing_sources_json TEXT NOT NULL DEFAULT '[]'
        );
        CREATE INDEX IF NOT EXISTS idx_mcp_registry_stars ON mcp_registry_server(stars DESC);
        CREATE INDEX IF NOT EXISTS idx_mcp_registry_server_status
            ON mcp_registry_server(status, is_latest);

        CREATE VIRTUAL TABLE IF NOT EXISTS mcp_registry_server_fts USING fts5(
            id,
            name,
            namespace,
            description,
            tokenize='unicode61'
        );

        CREATE TABLE IF NOT EXISTS mcp_curated_server (
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
            priority INTEGER NOT NULL DEFAULT 100,
            title TEXT,
            website_url TEXT,
            icons_json TEXT NOT NULL DEFAULT '[]',
            status TEXT NOT NULL DEFAULT 'active',
            is_latest INTEGER NOT NULL DEFAULT 1,
            published_at TEXT,
            registry_source TEXT,
            contributing_sources_json TEXT NOT NULL DEFAULT '[]'
        );
        CREATE INDEX IF NOT EXISTS idx_mcp_curated_recommended_priority
            ON mcp_curated_server(is_recommended DESC, priority ASC, name ASC);
        CREATE INDEX IF NOT EXISTS idx_mcp_curated_server_status
            ON mcp_curated_server(status, is_latest);

        CREATE VIRTUAL TABLE IF NOT EXISTS mcp_curated_server_fts USING fts5(
            id,
            name,
            namespace,
            description,
            tokenize='unicode61'
        );",
    )
    .context("Failed to create MCP registry snapshot schema")?;
    super::seeding::seed_default_curated_mcp_servers(conn)?;
    Ok(())
}
