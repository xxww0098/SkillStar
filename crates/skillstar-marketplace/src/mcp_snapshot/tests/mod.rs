//! MCP snapshot tests.
//!
//! - [`catalog`] — the pre-existing round-trip / publisher / FTS coverage.
//! - [`querying`] — the parameterized filter + pagination surface.
//! - [`migration`] — v12 → v13 on a database built the way a shipped release
//!   built it, which is the scenario that had zero coverage before.

use rusqlite::Connection;

use super::query::*;
use super::*;
use crate::mcp_models::{McpRegistryPackageSummary, McpRegistryRemoteSummary, McpServerKind};

mod catalog;
mod migration;
mod querying;

/// A connection with the current schema, as a fresh install gets it.
pub(super) fn test_conn() -> Connection {
    let conn = Connection::open_in_memory().expect("in-memory sqlite");
    // Minimal sync-state table (created by the base snapshot schema in prod).
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
            etag TEXT,
            degraded_reason TEXT
        );",
    )
    .unwrap();
    create_mcp_registry_tables(&conn).unwrap();
    conn
}

pub(super) fn sample(id: &str, name: &str, stars: u32, kind: McpServerKind) -> McpRegistryServer {
    McpRegistryServer {
        id: id.into(),
        name: name.into(),
        namespace: format!("acme/{name}"),
        description: format!("{name} server for testing"),
        repo_url: format!("https://github.com/acme/{name}"),
        stars,
        license: Some("MIT".into()),
        version: Some("1.0.0".into()),
        kind,
        runtimes: vec!["npx".into()],
        readme: Some("# readme".into()),
        packages: vec![McpRegistryPackageSummary {
            runtime: "npx".into(),
            identifier: format!("@acme/{name}"),
            version: Some("1.0.0".into()),
            required_env: vec!["TOKEN".into()],
            ..Default::default()
        }],
        remotes: vec![McpRegistryRemoteSummary {
            transport: "http".into(),
            url: "https://acme.example/mcp".into(),
            required_headers: vec![],
            ..Default::default()
        }],
        raw_server_json: format!("{{\"name\":\"acme/{name}\"}}"),
        updated_at: Some("2026-01-01T00:00:00Z".into()),
        recommended: false,
        source: None,
        ..Default::default()
    }
}
