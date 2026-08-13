//! Sync-state bookkeeping in the shared `marketplace_sync_state` table.
//!
//! Two scope shapes live here:
//! - `mcp_registry` — the aggregate the UI's freshness banner reads.
//! - `mcp_registry:<source_id>` — one row per remote source, holding that
//!   source's ETag and payload hash so a conditional request can short-circuit
//!   it independently of the others.
//!
//! `degraded_reason` is written on the success path (v12 added the column but
//! the MCP scope never populated it, so a truncated catalog was indistinguish-
//! able from a complete one after a restart — audit A.1-a / A.3-c).

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use rusqlite::{Connection, OptionalExtension, params};

use crate::snapshot::SyncStateEntry;

use crate::mcp_snapshot::{
    MCP_REGISTRY_SCOPE, MCP_REGISTRY_TTL_HOURS, now_rfc3339, truncate_error,
};

/// Sync-state scope for one remote source.
pub(crate) fn source_scope(source_id: &str) -> String {
    format!("{MCP_REGISTRY_SCOPE}:{source_id}")
}

const SELECT_COLUMNS: &str = "scope, last_success_at, last_attempt_at, last_error, next_refresh_at, \
     schema_version, source_host, payload_sha256, etag, degraded_reason";

fn map_state(row: &rusqlite::Row<'_>) -> rusqlite::Result<SyncStateEntry> {
    Ok(SyncStateEntry {
        scope: row.get(0)?,
        last_success_at: row.get(1)?,
        last_attempt_at: row.get(2)?,
        last_error: row.get(3)?,
        next_refresh_at: row.get(4)?,
        schema_version: row.get(5)?,
        source_host: row.get(6)?,
        payload_sha256: row.get(7)?,
        etag: row.get(8)?,
        degraded_reason: row.get(9)?,
    })
}

pub(crate) fn read_sync_state(conn: &Connection) -> Result<Option<SyncStateEntry>> {
    read_scope_state(conn, MCP_REGISTRY_SCOPE)
}

pub(crate) fn read_scope_state(conn: &Connection, scope: &str) -> Result<Option<SyncStateEntry>> {
    conn.query_row(
        &format!("SELECT {SELECT_COLUMNS} FROM marketplace_sync_state WHERE scope = ?1"),
        [scope],
        map_state,
    )
    .optional()
    .context("Failed to read MCP registry sync state")
}

/// Every per-source row, for diagnostics ("which source went stale?").
pub(crate) fn read_source_states(conn: &Connection) -> Result<Vec<SyncStateEntry>> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {SELECT_COLUMNS} FROM marketplace_sync_state
             WHERE scope LIKE ?1 ORDER BY scope ASC"
        ))
        .context("Failed to prepare MCP source sync state query")?;
    let rows = stmt
        .query_map([format!("{MCP_REGISTRY_SCOPE}:%")], map_state)
        .context("Failed to query MCP source sync states")?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.context("Failed to decode MCP source sync state")?);
    }
    Ok(out)
}

pub(crate) fn is_fresh(state: &Option<SyncStateEntry>) -> bool {
    let Some(state) = state else { return false };
    let Some(next) = state.next_refresh_at.as_deref() else {
        return false;
    };
    DateTime::parse_from_rfc3339(next)
        .map(|value| value.with_timezone(&Utc) > Utc::now())
        .unwrap_or(false)
}

pub(crate) fn mark_attempt(conn: &Connection) -> Result<()> {
    conn.execute(
        "INSERT INTO marketplace_sync_state (scope, last_attempt_at, schema_version)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(scope) DO UPDATE SET last_attempt_at = excluded.last_attempt_at, last_error = NULL",
        params![MCP_REGISTRY_SCOPE, now_rfc3339(), 1],
    )
    .context("Failed to mark MCP registry sync attempt")?;
    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn mark_success(conn: &Connection) -> Result<()> {
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

/// Record a successful sync together with the payload's content-addressing
/// metadata and, when the catalog is knowingly incomplete, why.
///
/// When `unchanged` is true the old hash/source/ETag are preserved (a
/// no-change refresh must not clobber the fingerprint the next incremental
/// comparison relies on). `degraded_reason` is always written: clearing it on
/// a complete sync is how a previously truncated catalog stops being reported
/// as truncated.
pub(crate) fn mark_scope_success(
    conn: &Connection,
    scope: &str,
    meta: &crate::remote::FetchMeta,
    unchanged: bool,
    degraded_reason: Option<&str>,
) -> Result<()> {
    let now = Utc::now();
    let next = (now + Duration::hours(MCP_REGISTRY_TTL_HOURS)).to_rfc3339();
    conn.execute(
        "INSERT INTO marketplace_sync_state
            (scope, last_success_at, last_attempt_at, last_error, next_refresh_at, schema_version,
             source_host, payload_sha256, etag, degraded_reason)
         VALUES (?1, ?2, ?2, NULL, ?3, ?4, ?5, ?6, ?7, ?9)
         ON CONFLICT(scope) DO UPDATE SET
            last_success_at = excluded.last_success_at,
            last_attempt_at = excluded.last_attempt_at,
            last_error = NULL,
            next_refresh_at = excluded.next_refresh_at,
            source_host = CASE WHEN ?8 THEN marketplace_sync_state.source_host ELSE excluded.source_host END,
            payload_sha256 = CASE WHEN ?8 THEN marketplace_sync_state.payload_sha256 ELSE excluded.payload_sha256 END,
            etag = CASE WHEN ?8 THEN marketplace_sync_state.etag ELSE excluded.etag END,
            degraded_reason = excluded.degraded_reason",
        params![
            scope,
            now.to_rfc3339(),
            next,
            1,
            meta.source_host,
            meta.payload_sha256,
            meta.etag,
            unchanged,
            degraded_reason.map(truncate_error),
        ],
    )
    .context("Failed to mark MCP registry sync success (with meta)")?;
    Ok(())
}

/// Aggregate-scope shorthand for [`mark_scope_success`].
pub(crate) fn mark_success_with_meta(
    conn: &Connection,
    meta: &crate::remote::FetchMeta,
    unchanged: bool,
    degraded_reason: Option<&str>,
) -> Result<()> {
    mark_scope_success(conn, MCP_REGISTRY_SCOPE, meta, unchanged, degraded_reason)
}

pub(crate) fn mark_error(conn: &Connection, error: &str) -> Result<()> {
    mark_scope_error(conn, MCP_REGISTRY_SCOPE, error)
}

pub(crate) fn mark_scope_error(conn: &Connection, scope: &str, error: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO marketplace_sync_state (scope, last_attempt_at, last_error, schema_version)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(scope) DO UPDATE SET
            last_attempt_at = excluded.last_attempt_at,
            last_error = excluded.last_error",
        params![scope, now_rfc3339(), truncate_error(error), 1],
    )
    .context("Failed to mark MCP registry sync error")?;
    Ok(())
}
