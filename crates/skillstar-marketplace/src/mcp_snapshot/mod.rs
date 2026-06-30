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
//!
//! Layout:
//! - this `mod.rs` — schema + seeding + the public local-first API.
//! - [`query`] — `&Connection` SQL read/write core (pure, testable).
//! - [`seeds`] — curated MCP server seed data.

use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{Connection, params};
use tracing::warn;

use crate::mcp_models::{
    McpMarketEntry, McpMarketServerDetail, McpPublisherSummary, McpRegistryServer,
};
use crate::mcp_remote::fetch_mcp_registry;
use crate::snapshot::{LocalFirstResult, SnapshotStatus, SyncStateEntry, with_conn};

mod query;
mod seeds;

use query::*;

/// Sync-state scope key in the shared `marketplace_sync_state` table.
const MCP_REGISTRY_SCOPE: &str = "mcp_registry";
/// How long a synced catalog stays "fresh" before a background refresh.
const MCP_REGISTRY_TTL_HOURS: i64 = 12;
const DEFAULT_SEARCH_LIMIT: u32 = 60;
const MAX_SEARCH_LIMIT: u32 = 200;
const CURATED_SOURCE_ID: &str = "skillstar-curated";

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
            priority INTEGER NOT NULL DEFAULT 100
        );
        CREATE INDEX IF NOT EXISTS idx_mcp_curated_recommended_priority
            ON mcp_curated_server(is_recommended DESC, priority ASC, name ASC);

        CREATE VIRTUAL TABLE IF NOT EXISTS mcp_curated_server_fts USING fts5(
            id,
            name,
            namespace,
            description,
            tokenize='unicode61'
        );",
    )
    .context("Failed to create MCP registry snapshot schema")?;
    seed_default_curated_mcp_servers(conn)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Shared helpers (used by this module + `query`)
// ---------------------------------------------------------------------------

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

fn truncate_error(error: &str) -> String {
    error.chars().take(500).collect()
}

/// Seed/refresh the curated MCP servers (idempotent upsert). Called by schema
/// creation and defensively before each read so curated cards are always
/// present even if the registry has never synced.
fn seed_default_curated_mcp_servers(conn: &Connection) -> Result<()> {
    let seeds = seeds::default_curated_mcp_servers();
    let tx = conn
        .unchecked_transaction()
        .context("Failed to open curated MCP seed transaction")?;
    {
        let mut upsert = tx
            .prepare(
                "INSERT INTO mcp_curated_server (
                    id, name, namespace, description, repo_url, stars, license, version,
                    kind, runtimes_json, readme, packages_json, remotes_json, raw_server_json,
                    updated_at, fetched_at, source, is_recommended, priority
                ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19)
                ON CONFLICT(id) DO UPDATE SET
                    name = excluded.name,
                    namespace = excluded.namespace,
                    description = excluded.description,
                    repo_url = excluded.repo_url,
                    stars = excluded.stars,
                    license = excluded.license,
                    version = excluded.version,
                    kind = excluded.kind,
                    runtimes_json = excluded.runtimes_json,
                    readme = excluded.readme,
                    packages_json = excluded.packages_json,
                    remotes_json = excluded.remotes_json,
                    raw_server_json = excluded.raw_server_json,
                    updated_at = excluded.updated_at,
                    fetched_at = excluded.fetched_at,
                    source = excluded.source,
                    is_recommended = excluded.is_recommended,
                    priority = excluded.priority",
            )
            .context("Failed to prepare curated MCP seed upsert")?;
        let mut delete_fts = tx
            .prepare("DELETE FROM mcp_curated_server_fts WHERE id = ?1")
            .context("Failed to prepare curated MCP FTS delete")?;
        let mut insert_fts = tx
            .prepare(
                "INSERT INTO mcp_curated_server_fts (id, name, namespace, description)
                 VALUES (?1,?2,?3,?4)",
            )
            .context("Failed to prepare curated MCP FTS insert")?;
        let fetched_at = now_rfc3339();
        for seed in seeds {
            let server = seed.server;
            let runtimes_json =
                serde_json::to_string(&server.runtimes).unwrap_or_else(|_| "[]".into());
            let packages_json =
                serde_json::to_string(&server.packages).unwrap_or_else(|_| "[]".into());
            let remotes_json =
                serde_json::to_string(&server.remotes).unwrap_or_else(|_| "[]".into());
            let source = server
                .source
                .clone()
                .unwrap_or_else(|| CURATED_SOURCE_ID.to_string());
            upsert
                .execute(params![
                    &server.id,
                    &server.name,
                    &server.namespace,
                    &server.description,
                    &server.repo_url,
                    server.stars,
                    &server.license,
                    &server.version,
                    server.kind.as_db_str(),
                    runtimes_json,
                    &server.readme,
                    packages_json,
                    remotes_json,
                    &server.raw_server_json,
                    &server.updated_at,
                    &fetched_at,
                    &source,
                    if server.recommended { 1_i64 } else { 0_i64 },
                    seed.priority,
                ])
                .with_context(|| format!("Failed to seed curated MCP server {}", server.id))?;
            delete_fts
                .execute([server.id.as_str()])
                .context("Failed to delete curated MCP FTS row")?;
            insert_fts
                .execute(params![
                    server.id,
                    server.name,
                    server.namespace,
                    server.description,
                ])
                .context("Failed to index curated MCP seed")?;
        }
    }
    tx.commit()
        .context("Failed to commit curated MCP seed transaction")?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Public API (mirrors snapshot.rs local-first functions)
// ---------------------------------------------------------------------------

/// Fetch the full registry and replace the local catalog, recording sync state.
pub async fn sync_mcp_registry_scope() -> Result<()> {
    // Sync-state bookkeeping is best-effort: a failure here (DB locked, schema
    // not yet ready) must not abort the sync itself. But silently dropping it
    // used to leave the marketplace UI showing "never synced" even after a
    // successful fetch, so log the bookkeeping failure instead of swallowing.
    if let Err(e) = with_conn(|conn| mark_attempt(conn)) {
        warn!("mcp sync: failed to record attempt in sync_state ({e})");
    }
    match fetch_mcp_registry().await {
        Ok(servers) => with_conn(|conn| {
            replace_servers(conn, &servers)?;
            mark_success(conn)?;
            Ok(())
        }),
        Err(err) => {
            let message = err.to_string();
            if let Err(e) = with_conn(|conn| mark_error(conn, &message)) {
                warn!(
                    "mcp sync: failed to record error in sync_state ({e}); original error: {message}"
                );
            }
            Err(err)
        }
    }
}

/// Curated MCP entries maintained in the local marketplace DB. This is the
/// source for SkillStar-owned recommended MCP cards.
pub fn list_curated_mcp_servers() -> Result<Vec<McpRegistryServer>> {
    with_conn(|conn| {
        seed_default_curated_mcp_servers(conn)?;
        load_curated_servers(conn)
    })
}

/// Local-first list of all registry servers (seeds on first use).
pub async fn list_mcp_servers_local() -> Result<LocalFirstResult<Vec<McpMarketEntry>>> {
    let local = with_conn(|conn| {
        seed_default_curated_mcp_servers(conn)?;
        let cards = load_cards(conn)?;
        let state = read_sync_state(conn)?;
        Ok((
            cards,
            state.is_some(),
            is_fresh(&state),
            state.and_then(|s| s.last_success_at),
        ))
    });

    match local {
        Ok((cards, false, _, _)) => {
            // Never synced — seed once, then return what we have.
            if sync_mcp_registry_scope().await.is_ok() {
                let reseeded = with_conn(|conn| {
                    seed_default_curated_mcp_servers(conn)?;
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
                data: cards,
                snapshot_status: SnapshotStatus::RemoteError,
                snapshot_updated_at: None,
            })
        }
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
    let limit = limit
        .unwrap_or(DEFAULT_SEARCH_LIMIT)
        .clamp(1, MAX_SEARCH_LIMIT);
    let local = with_conn(|conn| {
        seed_default_curated_mcp_servers(conn)?;
        let cards = search_cards(conn, query, limit)?;
        let total = count_servers(conn)?;
        let state = read_sync_state(conn)?;
        Ok((
            cards,
            total,
            state.is_some(),
            is_fresh(&state),
            state.and_then(|s| s.last_success_at),
        ))
    });

    match local {
        Ok((cards, _, false, _, _)) => {
            // Never synced — seed then re-search. Curated hits can still be
            // returned if the remote registry is unavailable.
            if sync_mcp_registry_scope().await.is_ok() {
                let reseeded = with_conn(|conn| {
                    seed_default_curated_mcp_servers(conn)?;
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
                data: cards,
                snapshot_status: SnapshotStatus::RemoteError,
                snapshot_updated_at: None,
            })
        }
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
        seed_default_curated_mcp_servers(conn)?;
        let server = load_full_server(conn, id)?;
        let state = read_sync_state(conn)?;
        Ok((
            server,
            is_fresh(&state),
            state.and_then(|s| s.last_success_at),
        ))
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
/// build an install draft. Synchronous; keeps packaged curated rows present.
pub fn get_registry_server_local(id: &str) -> Result<Option<McpRegistryServer>> {
    with_conn(|conn| {
        seed_default_curated_mcp_servers(conn)?;
        load_full_server(conn, id)
    })
}

/// Official MCP publishers shown on the marketplace grid. Curated sources are
/// always seeded first so the grid renders instantly even before the GitHub
/// registry has synced.
pub fn list_mcp_publishers() -> Result<Vec<McpPublisherSummary>> {
    with_conn(|conn| {
        seed_default_curated_mcp_servers(conn)?;
        load_publishers(conn)
    })
}

/// Local-first list of MCP cards scoped to one official publisher. Curated
/// publishers (`adspower` / `bigmodel`) read instantly from the curated table;
/// `github` follows the same stale-refresh path as the full marketplace list.
pub async fn list_mcp_servers_by_publisher(
    publisher_id: &str,
) -> Result<LocalFirstResult<Vec<McpMarketEntry>>> {
    // Curated publishers are static — no remote sync, always fresh.
    if publisher_id != "github" {
        let cards = with_conn(|conn| {
            seed_default_curated_mcp_servers(conn)?;
            load_cards_by_publisher(conn, publisher_id)
        })?;
        return Ok(LocalFirstResult {
            data: cards,
            snapshot_status: SnapshotStatus::Fresh,
            snapshot_updated_at: None,
        });
    }

    // GitHub publisher — same local-first dance as `list_mcp_servers_local`.
    let local = with_conn(|conn| {
        seed_default_curated_mcp_servers(conn)?;
        let cards = load_cards_by_publisher(conn, "github")?;
        let state = read_sync_state(conn)?;
        Ok((
            cards,
            state.is_some(),
            is_fresh(&state),
            state.and_then(|s| s.last_success_at),
        ))
    });

    match local {
        Ok((cards, false, _, _)) => {
            if sync_mcp_registry_scope().await.is_ok() {
                let reseeded = with_conn(|conn| {
                    seed_default_curated_mcp_servers(conn)?;
                    let cards = load_cards_by_publisher(conn, "github")?;
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
                data: cards,
                snapshot_status: SnapshotStatus::RemoteError,
                snapshot_updated_at: None,
            })
        }
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
        Err(err) => {
            warn!(target: "mcp_marketplace", error = %err, "MCP publisher list failed");
            Ok(LocalFirstResult {
                data: Vec::new(),
                snapshot_status: SnapshotStatus::RemoteError,
                snapshot_updated_at: None,
            })
        }
    }
}

/// Sync-state entry for the MCP registry scope (for the status strip).
pub fn mcp_market_sync_states() -> Result<Vec<SyncStateEntry>> {
    with_conn(|conn| Ok(read_sync_state(conn)?.into_iter().collect()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp_models::{
        McpRegistryPackageSummary, McpRegistryRemoteSummary, McpServerKind,
    };

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
            recommended: false,
            source: None,
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

        // curated recommendations lead, then registry rows ordered by stars desc.
        // AdsPower is recommended → first; then the 4 BigModel curated rows
        // (priority 0..3); then registry rows by stars.
        let cards = load_cards(&conn).unwrap();
        assert_eq!(cards[0].name, "adspower-local-api");
        assert!(cards[0].recommended);
        // All curated servers sit before the registry rows: 1 adspower +
        // 4 bigmodel + 4 anthropic + 2 microsoft + 3 saas + 2 cn-ai +
        // 3 cloudflare + 1 brave + 2 google + 1 supabase + 2 x = 25.
        let registry_start = cards
            .iter()
            .position(|c| c.id == "1")
            .expect("registry filesystem card present");
        assert_eq!(registry_start, 25);
        assert_eq!(cards[registry_start].name, "filesystem");
        assert_eq!(cards[registry_start + 1].name, "postgres");
        assert_eq!(cards[registry_start].kind, McpServerKind::Stdio);

        // FTS search — "postgres" also matches the Supabase curated server
        // ("Postgres 数据库"), so filter to just the registry hit by id.
        let hits = search_cards(&conn, "postgres", 10).unwrap();
        let pg_registry = hits.iter().find(|h| h.id == "2").expect("registry postgres hit");
        assert_eq!(pg_registry.name, "postgres");

        // empty query → all, truncated
        let all = search_cards(&conn, "   ", 1).unwrap();
        assert_eq!(all.len(), 1);

        // detail + full server (raw json preserved)
        let full = load_full_server(&conn, "1").unwrap().unwrap();
        assert_eq!(full.packages[0].identifier, "@acme/filesystem");
        assert_eq!(full.raw_server_json, "{\"name\":\"acme/filesystem\"}");
        assert_eq!(full.to_detail().entry.name, "filesystem");

        let curated = load_full_server(&conn, "adspower-local-api")
            .unwrap()
            .unwrap();
        assert!(curated.recommended);
        assert_eq!(curated.packages[0].identifier, "local-api-mcp-typescript");
        // AdsPower is now its own publisher bucket.
        assert_eq!(curated.to_detail().entry.source.as_deref(), Some("adspower"));
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

    #[test]
    fn publishers_aggregate_curated_sources_and_github() {
        let conn = test_conn();
        // Curated seeds are written by `create_mcp_registry_tables`.
        let publishers = load_publishers(&conn).unwrap();

        // 11 curated publishers + GitHub (0 registry rows seeded yet) = 12.
        assert_eq!(publishers.len(), 12);
        // CURATED_ORDER dictates grid order; GitHub always last.
        assert_eq!(publishers[0].id, "adspower");
        assert_eq!(publishers[0].name, "AdsPower");
        assert_eq!(publishers[0].server_count, 1);
        assert_eq!(publishers[1].id, "bigmodel");
        assert_eq!(publishers[1].name, "BigModel");
        assert_eq!(publishers[1].server_count, 4);
        assert_eq!(publishers[2].id, "anthropic");
        assert_eq!(publishers[2].name, "Anthropic");
        assert_eq!(publishers[2].server_count, 4);
        assert_eq!(publishers[3].id, "microsoft");
        assert_eq!(publishers[3].name, "Microsoft");
        assert_eq!(publishers[3].server_count, 2);
        assert_eq!(publishers[4].id, "saas");
        assert_eq!(publishers[4].server_count, 3);
        assert_eq!(publishers[5].id, "cn-ai");
        assert_eq!(publishers[5].server_count, 2);
        assert_eq!(publishers[6].id, "cloudflare");
        assert_eq!(publishers[6].name, "Cloudflare");
        assert_eq!(publishers[6].server_count, 3);
        assert_eq!(publishers[7].id, "brave");
        assert_eq!(publishers[7].server_count, 1);
        assert_eq!(publishers[8].id, "google");
        assert_eq!(publishers[8].server_count, 2);
        assert_eq!(publishers[9].id, "supabase");
        assert_eq!(publishers[9].server_count, 1);
        assert_eq!(publishers[10].id, "x");
        assert_eq!(publishers[10].name, "X");
        assert_eq!(publishers[10].server_count, 2);
        assert_eq!(publishers[11].id, "github");
        assert_eq!(publishers[11].server_count, 0);

        // After we add registry rows, GitHub's count climbs.
        replace_servers(
            &conn,
            &[
                sample("1", "filesystem", 100, McpServerKind::Stdio),
                sample("2", "postgres", 50, McpServerKind::Both),
            ],
        )
        .unwrap();
        let publishers = load_publishers(&conn).unwrap();
        let github = publishers.iter().find(|p| p.id == "github").unwrap();
        assert_eq!(github.server_count, 2);
    }

    #[test]
    fn publisher_cards_split_curated_and_registry() {
        let conn = test_conn();
        replace_servers(
            &conn,
            &[sample("r1", "filesystem", 10, McpServerKind::Stdio)],
        )
        .unwrap();

        // Curated publisher returns only its bucket.
        let adspower = load_cards_by_publisher(&conn, "adspower").unwrap();
        assert_eq!(adspower.len(), 1);
        assert_eq!(adspower[0].id, "adspower-local-api");
        assert_eq!(adspower[0].source.as_deref(), Some("adspower"));

        let bigmodel = load_cards_by_publisher(&conn, "bigmodel").unwrap();
        assert_eq!(bigmodel.len(), 4);
        // Ordered by priority (seed order): vision, search, reader, zread.
        assert_eq!(bigmodel[0].id, "bigmodel-vision");
        assert_eq!(bigmodel[1].id, "bigmodel-search");
        assert_eq!(bigmodel[2].id, "bigmodel-reader");
        assert_eq!(bigmodel[3].id, "bigmodel-zread");
        assert_eq!(bigmodel[0].kind, McpServerKind::Stdio);
        assert_eq!(bigmodel[1].kind, McpServerKind::Remote);
        // BigModel remote servers carry their endpoint URL on the detail row.
        let vision_full = load_full_server(&conn, "bigmodel-vision")
            .unwrap()
            .unwrap();
        assert_eq!(vision_full.packages[0].identifier, "@z_ai/mcp-server");
        assert!(vision_full.packages[0]
            .required_env
            .iter()
            .any(|e| e == "Z_AI_API_KEY"));
        let search_full = load_full_server(&conn, "bigmodel-search")
            .unwrap()
            .unwrap();
        assert_eq!(search_full.remotes.len(), 1);
        assert_eq!(
            search_full.remotes[0].url,
            "https://open.bigmodel.cn/api/mcp/web_search_prime/mcp"
        );
        assert!(search_full.remotes[0]
            .required_headers
            .iter()
            .any(|h| h == "Authorization"));

        // New curated publishers are filtered by their source bucket too.
        let anthropic = load_cards_by_publisher(&conn, "anthropic").unwrap();
        assert_eq!(anthropic.len(), 4);
        assert_eq!(anthropic[0].id, "anthropic-filesystem");
        assert!(anthropic.iter().all(|c| c.source.as_deref() == Some("anthropic")));

        let microsoft = load_cards_by_publisher(&conn, "microsoft").unwrap();
        assert_eq!(microsoft.len(), 2);

        let saas = load_cards_by_publisher(&conn, "saas").unwrap();
        assert_eq!(saas.len(), 3);
        // All SaaS entries are remote streamable-http.
        assert!(saas.iter().all(|c| c.kind == McpServerKind::Remote));

        let cn_ai = load_cards_by_publisher(&conn, "cn-ai").unwrap();
        assert_eq!(cn_ai.len(), 2);
        // Firecrawl requires an API key env var.
        let fc = load_full_server(&conn, "extra-firecrawl")
            .unwrap()
            .unwrap();
        assert!(fc.packages[0]
            .required_env
            .iter()
            .any(|e| e == "FIRECRAWL_API_KEY"));

        // Second batch of curated publishers.
        let cloudflare = load_cards_by_publisher(&conn, "cloudflare").unwrap();
        assert_eq!(cloudflare.len(), 3);
        assert!(cloudflare.iter().all(|c| c.kind == McpServerKind::Remote));

        let brave = load_cards_by_publisher(&conn, "brave").unwrap();
        assert_eq!(brave.len(), 1);
        assert_eq!(brave[0].kind, McpServerKind::Stdio);

        let google = load_cards_by_publisher(&conn, "google").unwrap();
        assert_eq!(google.len(), 2);

        let supabase = load_cards_by_publisher(&conn, "supabase").unwrap();
        assert_eq!(supabase.len(), 1);

        // GitHub publisher returns registry rows, excluding curated ids.
        let github = load_cards_by_publisher(&conn, "github").unwrap();
        assert_eq!(github.len(), 1);
        assert_eq!(github[0].id, "r1");
        assert!(github[0].source.is_none());
    }
}
