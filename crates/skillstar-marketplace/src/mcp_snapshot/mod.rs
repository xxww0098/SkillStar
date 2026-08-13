//! Local-first snapshot for the MCP marketplace.
//!
//! Mirrors the skill marketplace's snapshot pattern: a SQLite cache + FTS
//! search + `marketplace_sync_state`-backed TTL/status, served via
//! `LocalFirstResult`. The catalog itself is fetched and merged by
//! [`crate::mcp_remote`] across every enabled source before it lands here, so
//! this layer only ever sees one deduplicated list.
//!
//! Connection access and schema migration are reused from `snapshot::with_conn`
//! (the v8 migration calls [`create_mcp_registry_tables`]; v13 adds the
//! `2025-12-11` columns to existing databases). The `&Connection` core
//! functions are pure so they're unit-testable without the process-global
//! snapshot runtime.
//!
//! Layout:
//! - this `mod.rs` — the public local-first API + sync orchestration.
//! - [`schema`] — table definitions and the v13 column list.
//! - [`seeding`] — curated seed upsert.
//! - [`filters`] — the parameterized card query shape.
//! - `query` — `&Connection` SQL read/write core (pure, testable).
//! - `seeds` — curated MCP server seed data.

use std::collections::HashMap;

use anyhow::Result;
use chrono::Utc;
use rusqlite::Connection;
use tracing::{debug, warn};

use crate::mcp_models::{
    McpMarketEntry, McpMarketServerDetail, McpPublisherSummary, McpRegistryServer,
};
use crate::mcp_remote::{self, McpSourceOutcome, fetch_mcp_catalog};
use crate::remote::FetchMeta;
use crate::snapshot::{LocalFirstResult, SnapshotStatus, SyncStateEntry, with_conn};

pub mod filters;
mod query;
mod schema;
mod seeding;
mod seeds;

#[cfg(test)]
mod tests;

pub use filters::{McpServerPage, McpServerQuery, McpSortKey};
pub(crate) use schema::create_mcp_registry_tables;
pub(crate) use schema::{MCP_SERVER_COLUMNS_V13, MCP_SERVER_TABLES};

use query::*;
use seeding::seed_default_curated_mcp_servers;

/// Sync-state scope key in the shared `marketplace_sync_state` table.
const MCP_REGISTRY_SCOPE: &str = "mcp_registry";
/// How long a synced catalog stays "fresh" before a background refresh.
const MCP_REGISTRY_TTL_HOURS: i64 = 12;
const DEFAULT_SEARCH_LIMIT: u32 = 60;
const MAX_SEARCH_LIMIT: u32 = 200;

// ---------------------------------------------------------------------------
// Shared helpers (used by this module + `query` + `seeding`)
// ---------------------------------------------------------------------------

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

fn truncate_error(error: &str) -> String {
    error.chars().take(500).collect()
}

// ---------------------------------------------------------------------------
// Source management (re-exported so the command layer has one entry point)
// ---------------------------------------------------------------------------

/// Every configured source, built-ins first, with user overrides applied.
pub fn list_mcp_sources() -> Vec<mcp_remote::McpSourceDescriptor> {
    mcp_remote::sources::resolve_sources()
}

/// Add (or replace) a user-supplied registry URL / local directory file.
pub fn add_mcp_source(
    source: mcp_remote::McpCustomSource,
) -> Result<Vec<mcp_remote::McpSourceDescriptor>> {
    mcp_remote::config::add_custom_source(source)?;
    Ok(list_mcp_sources())
}

/// Remove a user-supplied source.
pub fn remove_mcp_source(id: &str) -> Result<Vec<mcp_remote::McpSourceDescriptor>> {
    mcp_remote::config::remove_custom_source(id)?;
    Ok(list_mcp_sources())
}

/// Turn any source (built-in or user) on/off.
pub fn set_mcp_source_enabled(
    id: &str,
    enabled: bool,
) -> Result<Vec<mcp_remote::McpSourceDescriptor>> {
    mcp_remote::config::set_source_enabled(id, enabled)?;
    Ok(list_mcp_sources())
}

// ---------------------------------------------------------------------------
// Sync
// ---------------------------------------------------------------------------

fn read_source_etags(conn: &Connection) -> Result<HashMap<String, String>> {
    let mut out = HashMap::new();
    for state in read_source_states(conn)? {
        let Some(source_id) = state
            .scope
            .strip_prefix(&format!("{MCP_REGISTRY_SCOPE}:"))
            .map(str::to_string)
        else {
            continue;
        };
        if let Some(etag) = state.etag {
            out.insert(source_id, etag);
        }
    }
    Ok(out)
}

/// Persist one `marketplace_sync_state` row per source, so a partial outage is
/// attributable ("official is down, GitHub is fine") instead of collapsing
/// into a single opaque scope.
fn record_source_states(conn: &Connection, outcomes: &[McpSourceOutcome]) -> Result<()> {
    for outcome in outcomes {
        let scope = source_scope(&outcome.source_id);
        match &outcome.error {
            Some(error) => mark_scope_error(conn, &scope, error)?,
            None => {
                let meta = FetchMeta {
                    payload_sha256: outcome.payload_sha256.clone().unwrap_or_default(),
                    source_host: outcome.source_host.clone(),
                    etag: outcome.etag.clone(),
                    degraded: outcome.degraded_reason.is_some(),
                };
                mark_scope_success(
                    conn,
                    &scope,
                    &meta,
                    outcome.unchanged,
                    outcome.degraded_reason.as_deref(),
                )?;
            }
        }
    }
    Ok(())
}

/// Fetch every enabled source, merge, and replace the local catalog.
pub async fn sync_mcp_registry_scope() -> Result<()> {
    // Sync-state bookkeeping is best-effort: a failure here (DB locked, schema
    // not yet ready) must not abort the sync itself. But silently dropping it
    // used to leave the marketplace UI showing "never synced" even after a
    // successful fetch, so log the bookkeeping failure instead of swallowing.
    if let Err(e) = with_conn(mark_attempt) {
        warn!("mcp sync: failed to record attempt in sync_state ({e})");
    }
    let prev_etags = with_conn(read_source_etags).unwrap_or_default();

    match fetch_mcp_catalog(&prev_etags).await {
        Ok(fetched) => {
            let degraded = mcp_remote::degraded_reason(&fetched.outcomes);

            if fetched.all_unchanged {
                // Nothing was re-read, so the previous run's completeness verdict
                // still stands — clearing it here would report a still-truncated
                // catalog as complete.
                if let Err(e) = with_conn(|conn| {
                    let previous = read_sync_state(conn)?.and_then(|s| s.degraded_reason);
                    let reason = degraded.as_deref().or(previous.as_deref());
                    mark_success_with_meta(conn, &fetched.meta, true, reason)?;
                    record_source_states(conn, &fetched.outcomes)
                }) {
                    warn!("mcp sync: failed to record unchanged sync state ({e})");
                }
                debug!(target: "mcp_marketplace", "MCP catalog unchanged across all sources; kept local rows");
                return Ok(());
            }

            // Content-addressed incremental write: when the merged payload
            // fingerprints identically to the last successful fetch, the
            // catalog is unchanged — refresh the timestamp, keep the rows.
            let unchanged = with_conn(|conn| {
                let state = read_sync_state(conn)?;
                Ok(state
                    .and_then(|s| s.payload_sha256)
                    .as_deref()
                    .is_some_and(|prev| prev == fetched.meta.payload_sha256))
            })
            .unwrap_or(false);

            if unchanged {
                if let Err(e) = with_conn(|conn| {
                    mark_success_with_meta(conn, &fetched.meta, true, degraded.as_deref())?;
                    record_source_states(conn, &fetched.outcomes)
                }) {
                    warn!("mcp sync: failed to record unchanged sync state ({e})");
                }
                debug!(target: "mcp_marketplace", hash = %fetched.meta.payload_sha256, "MCP catalog unchanged; kept local rows");
                return Ok(());
            }

            if let Some(reason) = degraded.as_deref() {
                warn!(target: "mcp_marketplace", reason, "MCP catalog stored in a degraded state");
            }
            with_conn(|conn| {
                replace_servers(conn, &fetched.servers)?;
                mark_success_with_meta(conn, &fetched.meta, false, degraded.as_deref())?;
                record_source_states(conn, &fetched.outcomes)
            })
        }
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

// ---------------------------------------------------------------------------
// Public API (mirrors snapshot.rs local-first functions)
// ---------------------------------------------------------------------------

/// Curated MCP entries maintained in the local marketplace DB. This is the
/// source for SkillStar-owned recommended MCP cards.
pub fn list_curated_mcp_servers() -> Result<Vec<McpRegistryServer>> {
    with_conn(|conn| {
        seed_default_curated_mcp_servers(conn)?;
        load_curated_servers(conn)
    })
}

/// Parameterized card query: filters, sorting and pagination in SQL, with the
/// pre-pagination total so a UI can page without a second call.
pub async fn query_mcp_servers_local(
    request: &McpServerQuery,
) -> Result<LocalFirstResult<McpServerPage>> {
    let local = with_conn(|conn| {
        seed_default_curated_mcp_servers(conn)?;
        let page = query_cards(conn, request)?;
        let state = read_sync_state(conn)?;
        Ok((
            page,
            state.is_some(),
            is_fresh(&state),
            state.and_then(|s| s.last_success_at),
        ))
    });

    match local {
        Ok((page, false, _, _)) => {
            // Never synced — seed once, then return what we have.
            if let Err(sync_err) = sync_mcp_registry_scope().await {
                return Ok(LocalFirstResult {
                    data: page,
                    snapshot_status: SnapshotStatus::RemoteError,
                    snapshot_updated_at: None,
                    error: Some(format!("{sync_err:#}")),
                });
            }
            let reseeded = with_conn(|conn| {
                seed_default_curated_mcp_servers(conn)?;
                let page = query_cards(conn, request)?;
                let updated_at = read_sync_state(conn)?.and_then(|s| s.last_success_at);
                Ok((page, updated_at))
            })?;
            Ok(LocalFirstResult {
                data: reseeded.0,
                snapshot_status: SnapshotStatus::Seeding,
                snapshot_updated_at: reseeded.1,
                error: None,
            })
        }
        Ok((page, _, fresh, updated_at)) => Ok(LocalFirstResult {
            snapshot_status: if page.items.is_empty() {
                SnapshotStatus::Miss
            } else if fresh {
                SnapshotStatus::Fresh
            } else {
                SnapshotStatus::Stale
            },
            data: page,
            snapshot_updated_at: updated_at,
            error: None,
        }),
        Err(err) => {
            warn!(target: "mcp_marketplace", error = %err, "MCP registry card query failed");
            Ok(LocalFirstResult {
                data: McpServerPage {
                    items: Vec::new(),
                    total: 0,
                    offset: request.offset.unwrap_or(0),
                    limit: request.limit,
                },
                snapshot_status: SnapshotStatus::RemoteError,
                snapshot_updated_at: None,
                error: Some(format!("{err:#}")),
            })
        }
    }
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
            if let Err(sync_err) = sync_mcp_registry_scope().await {
                return Ok(LocalFirstResult {
                    data: cards,
                    snapshot_status: SnapshotStatus::RemoteError,
                    snapshot_updated_at: None,
                    error: Some(format!("{sync_err:#}")),
                });
            }
            let reseeded = with_conn(|conn| {
                seed_default_curated_mcp_servers(conn)?;
                let cards = load_cards(conn)?;
                let updated_at = read_sync_state(conn)?.and_then(|s| s.last_success_at);
                Ok((cards, updated_at))
            })?;
            Ok(LocalFirstResult {
                data: reseeded.0,
                snapshot_status: SnapshotStatus::Seeding,
                snapshot_updated_at: reseeded.1,
                error: None,
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
            error: None,
        }),
        Ok((_, true, _, updated_at)) => Ok(LocalFirstResult {
            data: Vec::new(),
            snapshot_status: SnapshotStatus::Miss,
            snapshot_updated_at: updated_at,
            error: None,
        }),
        Err(err) => {
            warn!(target: "mcp_marketplace", error = %err, "MCP registry local list failed");
            Ok(LocalFirstResult {
                data: Vec::new(),
                snapshot_status: SnapshotStatus::RemoteError,
                snapshot_updated_at: None,
                error: Some(format!("{err:#}")),
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
            if let Err(sync_err) = sync_mcp_registry_scope().await {
                return Ok(LocalFirstResult {
                    data: cards,
                    snapshot_status: SnapshotStatus::RemoteError,
                    snapshot_updated_at: None,
                    error: Some(format!("{sync_err:#}")),
                });
            }
            let reseeded = with_conn(|conn| {
                seed_default_curated_mcp_servers(conn)?;
                let cards = search_cards(conn, query, limit)?;
                let updated_at = read_sync_state(conn)?.and_then(|s| s.last_success_at);
                Ok((cards, updated_at))
            })?;
            Ok(LocalFirstResult {
                data: reseeded.0,
                snapshot_status: SnapshotStatus::Seeding,
                snapshot_updated_at: reseeded.1,
                error: None,
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
            error: None,
        }),
        Ok((_, _, _, _, updated_at)) => Ok(LocalFirstResult {
            data: Vec::new(),
            snapshot_status: SnapshotStatus::Miss,
            snapshot_updated_at: updated_at,
            error: None,
        }),
        Err(err) => {
            warn!(target: "mcp_marketplace", error = %err, "MCP registry search failed");
            Ok(LocalFirstResult {
                data: Vec::new(),
                snapshot_status: SnapshotStatus::RemoteError,
                snapshot_updated_at: None,
                error: Some(format!("{err:#}")),
            })
        }
    }
}

/// Local-first detail (readme + package/remote specs) for one server.
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
            error: None,
        }),
        Ok((None, _, updated_at)) => Ok(LocalFirstResult {
            data: None,
            snapshot_status: SnapshotStatus::Miss,
            snapshot_updated_at: updated_at,
            error: None,
        }),
        Err(err) => {
            warn!(target: "mcp_marketplace", error = %err, "MCP registry detail failed");
            Ok(LocalFirstResult {
                data: None,
                snapshot_status: SnapshotStatus::RemoteError,
                snapshot_updated_at: None,
                error: Some(format!("{err:#}")),
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
/// always seeded first so the grid renders instantly even before the remote
/// catalog has synced.
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
            error: None,
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
            if let Err(sync_err) = sync_mcp_registry_scope().await {
                return Ok(LocalFirstResult {
                    data: cards,
                    snapshot_status: SnapshotStatus::RemoteError,
                    snapshot_updated_at: None,
                    error: Some(format!("{sync_err:#}")),
                });
            }
            let reseeded = with_conn(|conn| {
                seed_default_curated_mcp_servers(conn)?;
                let cards = load_cards_by_publisher(conn, "github")?;
                let updated_at = read_sync_state(conn)?.and_then(|s| s.last_success_at);
                Ok((cards, updated_at))
            })?;
            Ok(LocalFirstResult {
                data: reseeded.0,
                snapshot_status: SnapshotStatus::Seeding,
                snapshot_updated_at: reseeded.1,
                error: None,
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
            error: None,
        }),
        Ok((_, true, _, updated_at)) => Ok(LocalFirstResult {
            data: Vec::new(),
            snapshot_status: SnapshotStatus::Miss,
            snapshot_updated_at: updated_at,
            error: None,
        }),
        Err(err) => {
            warn!(target: "mcp_marketplace", error = %err, "MCP publisher list failed");
            Ok(LocalFirstResult {
                data: Vec::new(),
                snapshot_status: SnapshotStatus::RemoteError,
                snapshot_updated_at: None,
                error: Some(format!("{err:#}")),
            })
        }
    }
}

/// Sync-state entry for the aggregate MCP registry scope (status strip).
pub fn mcp_market_sync_states() -> Result<Vec<SyncStateEntry>> {
    with_conn(|conn| Ok(read_sync_state(conn)?.into_iter().collect()))
}

/// Per-source sync state — which source is stale, degraded or failing.
pub fn mcp_source_sync_states() -> Result<Vec<SyncStateEntry>> {
    with_conn(read_source_states)
}
