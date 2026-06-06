//! Local-first snapshot for the GitHub MCP Registry marketplace.
//!
//! Mirrors the skill marketplace's snapshot pattern (`snapshot.rs`): a SQLite
//! cache + FTS search + `marketplace_sync_state`-backed TTL/status, served via
//! `LocalFirstResult`. Proportional to the registry's size — one table + one
//! FTS index — rather than the full skill schema.
//!
//! Connection access and schema migration are reused from `snapshot::with_conn`
//! (the v8 migration calls [`create_mcp_registry_tables`] here). The
//! `&Connection` core functions are pure so they're unit-testable without the
//! process-global snapshot runtime.

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use tracing::warn;

use crate::mcp_models::{McpMarketEntry, McpMarketServerDetail, McpRegistryServer, McpServerKind};
use crate::mcp_remote::fetch_mcp_registry;
use crate::snapshot::{LocalFirstResult, SnapshotStatus, SyncStateEntry, with_conn};

/// Sync-state scope key in the shared `marketplace_sync_state` table.
const MCP_REGISTRY_SCOPE: &str = "mcp_registry";
/// How long a synced catalog stays "fresh" before a background refresh.
const MCP_REGISTRY_TTL_HOURS: i64 = 12;
const DEFAULT_SEARCH_LIMIT: u32 = 60;
const MAX_SEARCH_LIMIT: u32 = 200;

// ---------------------------------------------------------------------------
// Schema (called by snapshot::migrate_v7_to_v8 and by tests)
// ---------------------------------------------------------------------------

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
            fetched_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_mcp_registry_stars ON mcp_registry_server(stars DESC);

        CREATE VIRTUAL TABLE IF NOT EXISTS mcp_registry_server_fts USING fts5(
            id,
            name,
            namespace,
            description,
            tokenize='unicode61'
        );",
    )
    .context("Failed to create MCP registry snapshot schema")
}

// ---------------------------------------------------------------------------
// `&Connection` core (pure, testable)
// ---------------------------------------------------------------------------

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

fn truncate_error(error: &str) -> String {
    error.chars().take(500).collect()
}

/// Replace the entire cached catalog atomically (the registry is fetched as a
/// whole, so a full swap keeps the snapshot internally consistent).
fn replace_servers(conn: &Connection, servers: &[McpRegistryServer]) -> Result<()> {
    let tx = conn
        .unchecked_transaction()
        .context("Failed to open MCP registry write transaction")?;
    tx.execute("DELETE FROM mcp_registry_server", [])
        .context("Failed to clear mcp_registry_server")?;
    tx.execute("DELETE FROM mcp_registry_server_fts", [])
        .context("Failed to clear mcp_registry_server_fts")?;
    {
        let mut insert = tx
            .prepare(
                "INSERT INTO mcp_registry_server (
                    id, name, namespace, description, repo_url, stars, license, version,
                    kind, runtimes_json, readme, packages_json, remotes_json, raw_server_json,
                    updated_at, fetched_at
                ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
            )
            .context("Failed to prepare mcp_registry_server insert")?;
        let mut insert_fts = tx
            .prepare(
                "INSERT INTO mcp_registry_server_fts (id, name, namespace, description)
                 VALUES (?1,?2,?3,?4)",
            )
            .context("Failed to prepare mcp_registry_server_fts insert")?;
        let fetched_at = now_rfc3339();
        for server in servers {
            let runtimes_json = serde_json::to_string(&server.runtimes).unwrap_or_else(|_| "[]".into());
            let packages_json = serde_json::to_string(&server.packages).unwrap_or_else(|_| "[]".into());
            let remotes_json = serde_json::to_string(&server.remotes).unwrap_or_else(|_| "[]".into());
            insert
                .execute(params![
                    server.id,
                    server.name,
                    server.namespace,
                    server.description,
                    server.repo_url,
                    server.stars,
                    server.license,
                    server.version,
                    server.kind.as_db_str(),
                    runtimes_json,
                    server.readme,
                    packages_json,
                    remotes_json,
                    server.raw_server_json,
                    server.updated_at,
                    fetched_at,
                ])
                .with_context(|| format!("Failed to insert MCP server {}", server.id))?;
            insert_fts
                .execute(params![
                    server.id,
                    server.name,
                    server.namespace,
                    server.description
                ])
                .with_context(|| format!("Failed to index MCP server {}", server.id))?;
        }
    }
    tx.commit().context("Failed to commit MCP registry catalog")?;
    Ok(())
}

fn count_servers(conn: &Connection) -> Result<i64> {
    conn.query_row("SELECT COUNT(*) FROM mcp_registry_server", [], |row| row.get(0))
        .context("Failed to count MCP registry servers")
}

fn row_to_card(row: &rusqlite::Row<'_>) -> rusqlite::Result<McpMarketEntry> {
    let runtimes_json: String = row.get("runtimes_json")?;
    let kind_str: String = row.get("kind")?;
    Ok(McpMarketEntry {
        id: row.get("id")?,
        name: row.get("name")?,
        namespace: row.get("namespace")?,
        description: row.get("description")?,
        repo_url: row.get("repo_url")?,
        stars: row.get::<_, i64>("stars")? as u32,
        license: row.get("license")?,
        version: row.get("version")?,
        kind: McpServerKind::from_db_str(&kind_str),
        runtimes: serde_json::from_str(&runtimes_json).unwrap_or_default(),
        updated_at: row.get("updated_at")?,
    })
}

const CARD_COLUMNS: &str =
    "id, name, namespace, description, repo_url, stars, license, version, kind, runtimes_json, updated_at";

fn load_cards(conn: &Connection) -> Result<Vec<McpMarketEntry>> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {CARD_COLUMNS} FROM mcp_registry_server
             ORDER BY stars DESC, name ASC"
        ))
        .context("Failed to prepare MCP registry list query")?;
    let rows = stmt
        .query_map([], row_to_card)
        .context("Failed to query MCP registry servers")?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.context("Failed to decode MCP registry row")?);
    }
    Ok(out)
}

/// Build a safe FTS5 MATCH expression: keep alphanumeric tokens, quote each and
/// add a prefix wildcard, AND them together. Returns `None` for an empty query.
fn build_fts_match(query: &str) -> Option<String> {
    let terms: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{}\"*", t.to_lowercase()))
        .collect();
    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" "))
    }
}

fn search_cards(conn: &Connection, query: &str, limit: u32) -> Result<Vec<McpMarketEntry>> {
    let Some(match_expr) = build_fts_match(query) else {
        let mut cards = load_cards(conn)?;
        cards.truncate(limit as usize);
        return Ok(cards);
    };
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {} FROM mcp_registry_server s
             JOIN mcp_registry_server_fts fts ON fts.id = s.id
             WHERE mcp_registry_server_fts MATCH ?1
             ORDER BY bm25(mcp_registry_server_fts, 0.0, 8.0, 4.0, 2.0) ASC, s.stars DESC
             LIMIT ?2",
            CARD_COLUMNS
                .split(", ")
                .map(|c| format!("s.{c}"))
                .collect::<Vec<_>>()
                .join(", ")
        ))
        .context("Failed to prepare MCP registry search query")?;
    let rows = stmt
        .query_map(params![match_expr, limit], row_to_card)
        .context("Failed to run MCP registry search")?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.context("Failed to decode MCP registry search row")?);
    }
    Ok(out)
}

fn load_full_server(conn: &Connection, id: &str) -> Result<Option<McpRegistryServer>> {
    conn.query_row(
        "SELECT id, name, namespace, description, repo_url, stars, license, version, kind,
                runtimes_json, readme, packages_json, remotes_json, raw_server_json, updated_at
         FROM mcp_registry_server WHERE id = ?1",
        [id],
        |row| {
            let kind_str: String = row.get("kind")?;
            let runtimes_json: String = row.get("runtimes_json")?;
            let packages_json: String = row.get("packages_json")?;
            let remotes_json: String = row.get("remotes_json")?;
            Ok(McpRegistryServer {
                id: row.get("id")?,
                name: row.get("name")?,
                namespace: row.get("namespace")?,
                description: row.get("description")?,
                repo_url: row.get("repo_url")?,
                stars: row.get::<_, i64>("stars")? as u32,
                license: row.get("license")?,
                version: row.get("version")?,
                kind: McpServerKind::from_db_str(&kind_str),
                runtimes: serde_json::from_str(&runtimes_json).unwrap_or_default(),
                readme: row.get("readme")?,
                packages: serde_json::from_str(&packages_json).unwrap_or_default(),
                remotes: serde_json::from_str(&remotes_json).unwrap_or_default(),
                raw_server_json: row.get("raw_server_json")?,
                updated_at: row.get("updated_at")?,
            })
        },
    )
    .optional()
    .context("Failed to load MCP registry server")
}

// ---------------------------------------------------------------------------
// Sync state (shared `marketplace_sync_state` table, scope = "mcp_registry")
// ---------------------------------------------------------------------------

fn read_sync_state(conn: &Connection) -> Result<Option<SyncStateEntry>> {
    conn.query_row(
        "SELECT scope, last_success_at, last_attempt_at, last_error, next_refresh_at, schema_version
         FROM marketplace_sync_state WHERE scope = ?1",
        [MCP_REGISTRY_SCOPE],
        |row| {
            Ok(SyncStateEntry {
                scope: row.get(0)?,
                last_success_at: row.get(1)?,
                last_attempt_at: row.get(2)?,
                last_error: row.get(3)?,
                next_refresh_at: row.get(4)?,
                schema_version: row.get(5)?,
            })
        },
    )
    .optional()
    .context("Failed to read MCP registry sync state")
}

fn is_fresh(state: &Option<SyncStateEntry>) -> bool {
    let Some(state) = state else { return false };
    let Some(next) = state.next_refresh_at.as_deref() else {
        return false;
    };
    DateTime::parse_from_rfc3339(next)
        .map(|value| value.with_timezone(&Utc) > Utc::now())
        .unwrap_or(false)
}

fn mark_attempt(conn: &Connection) -> Result<()> {
    conn.execute(
        "INSERT INTO marketplace_sync_state (scope, last_attempt_at, schema_version)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(scope) DO UPDATE SET last_attempt_at = excluded.last_attempt_at, last_error = NULL",
        params![MCP_REGISTRY_SCOPE, now_rfc3339(), 1],
    )
    .context("Failed to mark MCP registry sync attempt")?;
    Ok(())
}

fn mark_success(conn: &Connection) -> Result<()> {
    let now = Utc::now();
    let next = (now + Duration::hours(MCP_REGISTRY_TTL_HOURS)).to_rfc3339();
    conn.execute(
        "INSERT INTO marketplace_sync_state
            (scope, last_success_at, last_attempt_at, last_error, next_refresh_at, schema_version)
         VALUES (?1, ?2, ?2, NULL, ?3, ?4)
         ON CONFLICT(scope) DO UPDATE SET
            last_success_at = excluded.last_success_at,
            last_attempt_at = excluded.last_attempt_at,
            last_error = NULL,
            next_refresh_at = excluded.next_refresh_at",
        params![MCP_REGISTRY_SCOPE, now.to_rfc3339(), next, 1],
    )
    .context("Failed to mark MCP registry sync success")?;
    Ok(())
}

fn mark_error(conn: &Connection, error: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO marketplace_sync_state (scope, last_attempt_at, last_error, schema_version)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(scope) DO UPDATE SET
            last_attempt_at = excluded.last_attempt_at,
            last_error = excluded.last_error",
        params![MCP_REGISTRY_SCOPE, now_rfc3339(), truncate_error(error), 1],
    )
    .context("Failed to mark MCP registry sync error")?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Public API (mirrors snapshot.rs local-first functions)
// ---------------------------------------------------------------------------

/// Fetch the full registry and replace the local catalog, recording sync state.
pub async fn sync_mcp_registry_scope() -> Result<()> {
    let _ = with_conn(|conn| mark_attempt(conn));
    match fetch_mcp_registry().await {
        Ok(servers) => with_conn(|conn| {
            replace_servers(conn, &servers)?;
            mark_success(conn)?;
            Ok(())
        }),
        Err(err) => {
            let message = err.to_string();
            let _ = with_conn(|conn| mark_error(conn, &message));
            Err(err)
        }
    }
}

/// Local-first list of all registry servers (seeds on first use).
pub async fn list_mcp_servers_local() -> Result<LocalFirstResult<Vec<McpMarketEntry>>> {
    let local = with_conn(|conn| {
        let cards = load_cards(conn)?;
        let state = read_sync_state(conn)?;
        Ok((cards, state.is_some(), is_fresh(&state), state.and_then(|s| s.last_success_at)))
    });

    match local {
        Ok((cards, _, fresh, updated_at)) if !cards.is_empty() => Ok(LocalFirstResult {
            data: cards,
            snapshot_status: if fresh {
                SnapshotStatus::Fresh
            } else {
                SnapshotStatus::Stale
            },
            snapshot_updated_at: updated_at,
        }),
        Ok((_, true, _, updated_at)) => Ok(LocalFirstResult {
            data: Vec::new(),
            snapshot_status: SnapshotStatus::Miss,
            snapshot_updated_at: updated_at,
        }),
        Ok((_, false, _, _)) => {
            // Never synced — seed once, then return what we have.
            if sync_mcp_registry_scope().await.is_ok() {
                let reseeded = with_conn(|conn| {
                    let cards = load_cards(conn)?;
                    let updated_at = read_sync_state(conn)?.and_then(|s| s.last_success_at);
                    Ok((cards, updated_at))
                })?;
                return Ok(LocalFirstResult {
                    data: reseeded.0,
                    snapshot_status: SnapshotStatus::Seeding,
                    snapshot_updated_at: reseeded.1,
                });
            }
            Ok(LocalFirstResult {
                data: Vec::new(),
                snapshot_status: SnapshotStatus::RemoteError,
                snapshot_updated_at: None,
            })
        }
        Err(err) => {
            warn!(target: "mcp_marketplace", error = %err, "MCP registry local list failed");
            Ok(LocalFirstResult {
                data: Vec::new(),
                snapshot_status: SnapshotStatus::RemoteError,
                snapshot_updated_at: None,
            })
        }
    }
}

/// Local-first FTS search (seeds on first use if the catalog is empty).
pub async fn search_mcp_servers_local(
    query: &str,
    limit: Option<u32>,
) -> Result<LocalFirstResult<Vec<McpMarketEntry>>> {
    let limit = limit.unwrap_or(DEFAULT_SEARCH_LIMIT).clamp(1, MAX_SEARCH_LIMIT);
    let local = with_conn(|conn| {
        let cards = search_cards(conn, query, limit)?;
        let total = count_servers(conn)?;
        let state = read_sync_state(conn)?;
        Ok((cards, total, state.is_some(), is_fresh(&state), state.and_then(|s| s.last_success_at)))
    });

    match local {
        Ok((cards, _, _, fresh, updated_at)) if !cards.is_empty() => Ok(LocalFirstResult {
            data: cards,
            snapshot_status: if fresh {
                SnapshotStatus::Fresh
            } else {
                SnapshotStatus::Stale
            },
            snapshot_updated_at: updated_at,
        }),
        Ok((_, total, _, _, updated_at)) if total > 0 => Ok(LocalFirstResult {
            data: Vec::new(),
            snapshot_status: SnapshotStatus::Miss,
            snapshot_updated_at: updated_at,
        }),
        Ok((_, _, false, _, _)) => {
            // Empty catalog, never synced — seed then re-search.
            if sync_mcp_registry_scope().await.is_ok() {
                let reseeded = with_conn(|conn| {
                    let cards = search_cards(conn, query, limit)?;
                    let updated_at = read_sync_state(conn)?.and_then(|s| s.last_success_at);
                    Ok((cards, updated_at))
                })?;
                return Ok(LocalFirstResult {
                    data: reseeded.0,
                    snapshot_status: SnapshotStatus::Seeding,
                    snapshot_updated_at: reseeded.1,
                });
            }
            Ok(LocalFirstResult {
                data: Vec::new(),
                snapshot_status: SnapshotStatus::RemoteError,
                snapshot_updated_at: None,
            })
        }
        Ok((_, _, _, _, updated_at)) => Ok(LocalFirstResult {
            data: Vec::new(),
            snapshot_status: SnapshotStatus::Miss,
            snapshot_updated_at: updated_at,
        }),
        Err(err) => {
            warn!(target: "mcp_marketplace", error = %err, "MCP registry search failed");
            Ok(LocalFirstResult {
                data: Vec::new(),
                snapshot_status: SnapshotStatus::RemoteError,
                snapshot_updated_at: None,
            })
        }
    }
}

/// Local-first detail (readme + package/remote display) for one server.
pub async fn get_mcp_server_detail_local(
    id: &str,
) -> Result<LocalFirstResult<Option<McpMarketServerDetail>>> {
    let local = with_conn(|conn| {
        let server = load_full_server(conn, id)?;
        let state = read_sync_state(conn)?;
        Ok((server, is_fresh(&state), state.and_then(|s| s.last_success_at)))
    });

    match local {
        Ok((Some(server), fresh, updated_at)) => Ok(LocalFirstResult {
            data: Some(server.to_detail()),
            snapshot_status: if fresh {
                SnapshotStatus::Fresh
            } else {
                SnapshotStatus::Stale
            },
            snapshot_updated_at: updated_at,
        }),
        Ok((None, _, updated_at)) => Ok(LocalFirstResult {
            data: None,
            snapshot_status: SnapshotStatus::Miss,
            snapshot_updated_at: updated_at,
        }),
        Err(err) => {
            warn!(target: "mcp_marketplace", error = %err, "MCP registry detail failed");
            Ok(LocalFirstResult {
                data: None,
                snapshot_status: SnapshotStatus::RemoteError,
                snapshot_updated_at: None,
            })
        }
    }
}

/// Full cached server (incl. raw `server` JSON) — used by the app layer to
/// build an install draft. Synchronous; does not seed.
pub fn get_registry_server_local(id: &str) -> Result<Option<McpRegistryServer>> {
    with_conn(|conn| load_full_server(conn, id))
}

/// Sync-state entry for the MCP registry scope (for the status strip).
pub fn mcp_market_sync_states() -> Result<Vec<SyncStateEntry>> {
    with_conn(|conn| Ok(read_sync_state(conn)?.into_iter().collect()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp_models::{McpRegistryPackageSummary, McpRegistryRemoteSummary};

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory sqlite");
        // Minimal sync-state table (created by the base snapshot schema in prod).
        conn.execute_batch(
            "CREATE TABLE marketplace_sync_state (
                scope TEXT PRIMARY KEY,
                last_success_at TEXT,
                last_attempt_at TEXT,
                last_error TEXT,
                next_refresh_at TEXT,
                schema_version INTEGER NOT NULL DEFAULT 1
            );",
        )
        .unwrap();
        create_mcp_registry_tables(&conn).unwrap();
        conn
    }

    fn sample(id: &str, name: &str, stars: u32, kind: McpServerKind) -> McpRegistryServer {
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
            }],
            remotes: vec![McpRegistryRemoteSummary {
                transport: "http".into(),
                url: "https://acme.example/mcp".into(),
                required_headers: vec![],
            }],
            raw_server_json: format!("{{\"name\":\"acme/{name}\"}}"),
            updated_at: Some("2026-01-01T00:00:00Z".into()),
        }
    }

    #[test]
    fn replace_then_load_and_search_roundtrip() {
        let conn = test_conn();
        let servers = vec![
            sample("1", "filesystem", 100, McpServerKind::Stdio),
            sample("2", "postgres", 50, McpServerKind::Both),
        ];
        replace_servers(&conn, &servers).unwrap();
        assert_eq!(count_servers(&conn).unwrap(), 2);

        // ordered by stars desc
        let cards = load_cards(&conn).unwrap();
        assert_eq!(cards[0].name, "filesystem");
        assert_eq!(cards[1].name, "postgres");
        assert_eq!(cards[0].kind, McpServerKind::Stdio);

        // FTS search
        let hits = search_cards(&conn, "postgres", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "2");

        // empty query → all, truncated
        let all = search_cards(&conn, "   ", 1).unwrap();
        assert_eq!(all.len(), 1);

        // detail + full server (raw json preserved)
        let full = load_full_server(&conn, "1").unwrap().unwrap();
        assert_eq!(full.packages[0].identifier, "@acme/filesystem");
        assert_eq!(full.raw_server_json, "{\"name\":\"acme/filesystem\"}");
        assert_eq!(full.to_detail().entry.name, "filesystem");
    }

    #[test]
    fn replace_is_a_full_swap() {
        let conn = test_conn();
        replace_servers(&conn, &[sample("1", "old", 1, McpServerKind::Stdio)]).unwrap();
        replace_servers(&conn, &[sample("2", "new", 1, McpServerKind::Stdio)]).unwrap();
        assert_eq!(count_servers(&conn).unwrap(), 1);
        assert!(load_full_server(&conn, "1").unwrap().is_none());
        assert!(load_full_server(&conn, "2").unwrap().is_some());
        // FTS swapped too
        assert!(search_cards(&conn, "old", 10).unwrap().is_empty());
        assert_eq!(search_cards(&conn, "new", 10).unwrap().len(), 1);
    }

    #[test]
    fn sync_state_freshness_transitions() {
        let conn = test_conn();
        assert!(read_sync_state(&conn).unwrap().is_none());

        mark_success(&conn).unwrap();
        let state = read_sync_state(&conn).unwrap();
        assert!(state.is_some());
        assert!(is_fresh(&state)); // next_refresh is in the future

        mark_error(&conn, "boom").unwrap();
        let state = read_sync_state(&conn).unwrap().unwrap();
        assert_eq!(state.last_error.as_deref(), Some("boom"));
        assert!(state.last_success_at.is_some()); // success preserved on error
    }

    #[test]
    fn fts_match_builder_is_injection_safe() {
        assert!(build_fts_match("   ").is_none());
        assert_eq!(build_fts_match("github").as_deref(), Some("\"github\"*"));
        // punctuation stripped, terms ANDed
        assert_eq!(
            build_fts_match("file system!").as_deref(),
            Some("\"file\"* \"system\"*")
        );
    }
}
