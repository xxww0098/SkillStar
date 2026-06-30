//! `&Connection` query/write core for the MCP registry snapshot.
//!
//! Pure functions over a `rusqlite::Connection` (no process-global runtime),
//! so they're unit-testable from `mod.rs`'s test module. Split out of `mod.rs`
//! to keep the public local-first API and the SQL plumbing in separate files.

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use tracing::warn;

use crate::mcp_models::{McpMarketEntry, McpPublisherSummary, McpRegistryServer, McpServerKind};
use crate::snapshot::SyncStateEntry;

use super::{MCP_REGISTRY_SCOPE, MCP_REGISTRY_TTL_HOURS, now_rfc3339, truncate_error};

/// Replace the entire cached catalog atomically (the registry is fetched as a
/// whole, so a full swap keeps the snapshot internally consistent).
pub(crate) fn replace_servers(conn: &Connection, servers: &[McpRegistryServer]) -> Result<()> {
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
            let runtimes_json =
                serde_json::to_string(&server.runtimes).unwrap_or_else(|_| "[]".into());
            let packages_json =
                serde_json::to_string(&server.packages).unwrap_or_else(|_| "[]".into());
            let remotes_json =
                serde_json::to_string(&server.remotes).unwrap_or_else(|_| "[]".into());
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
    tx.commit()
        .context("Failed to commit MCP registry catalog")?;
    Ok(())
}

pub(crate) fn count_servers(conn: &Connection) -> Result<i64> {
    conn.query_row("SELECT COUNT(*) FROM mcp_registry_server", [], |row| {
        row.get(0)
    })
    .context("Failed to count MCP registry servers")
}

/// Aggregated official MCP publishers (curated `source` buckets + GitHub).
/// Curated rows are grouped by `source`; GitHub is one publisher backed by the
/// full `mcp_registry_server` table.
pub(crate) fn load_publishers(conn: &Connection) -> Result<Vec<McpPublisherSummary>> {
    // Known curated sources in priority order so the grid is stable regardless
    // of insertion order. Each maps to display name + landing page.
    const CURATED_ORDER: [(&str, &str, &str); 11] = [
        // (source id, display name, url)
        ("adspower", "AdsPower", "https://github.com/AdsPower/adspower-browser"),
        ("bigmodel", "BigModel", "https://docs.bigmodel.cn/cn/coding-plan/mcp/"),
        ("anthropic", "Anthropic", "https://github.com/modelcontextprotocol/servers"),
        ("microsoft", "Microsoft", "https://github.com/microsoft/playwright-mcp"),
        ("saas", "SaaS", "https://modelcontextprotocol.io"),
        ("cn-ai", "Dev Tools", "https://github.com/upstash/context7"),
        ("cloudflare", "Cloudflare", "https://github.com/cloudflare/mcp-server-cloudflare"),
        ("brave", "Brave", "https://github.com/brave/brave-search-mcp"),
        ("google", "Google", "https://developers.google.com/mcp"),
        ("supabase", "Supabase", "https://github.com/supabase/mcp-server-supabase"),
        ("x", "X", "https://docs.x.com/tools/mcp"),
    ];

    let mut curated_counts: std::collections::HashMap<String, i64> =
        std::collections::HashMap::new();
    let mut stmt = conn
        .prepare("SELECT source, COUNT(*) AS cnt FROM mcp_curated_server GROUP BY source")
        .context("Failed to prepare curated publisher count query")?;
    let rows = stmt
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))
        .context("Failed to query curated publisher counts")?;
    for row in rows {
        let (source, count) = row?;
        curated_counts.insert(source, count);
    }

    let mut out: Vec<McpPublisherSummary> = Vec::new();
    for (source, name, url) in CURATED_ORDER {
        // Only include curated publishers that actually have servers seeded.
        if let Some(count) = curated_counts.get(source) {
            out.push(McpPublisherSummary {
                id: source.to_string(),
                name: name.to_string(),
                server_count: *count as u32,
                url: url.to_string(),
            });
        }
    }

    // GitHub publisher — full registry table (deduped against curated ids).
    // A transient DB error (e.g. SQLite BUSY) shouldn't abort the whole
    // publisher list, but `unwrap_or(0)` would silently render the GitHub card
    // as "0 servers" — log so the misleading zero is traceable.
    let github_count = match conn.query_row(
        "SELECT COUNT(*) FROM mcp_registry_server",
        [],
        |row| row.get::<_, i64>(0),
    ) {
        Ok(c) => c,
        Err(e) => {
            warn!("mcp publishers: COUNT(*) on mcp_registry_server failed ({e}); GitHub card will show 0");
            0
        }
    };
    out.push(McpPublisherSummary {
        id: "github".to_string(),
        name: "GitHub".to_string(),
        server_count: github_count as u32,
        url: "https://github.com/modelcontextprotocol".to_string(),
    });

    Ok(out)
}

/// Cards filtered to a single publisher. Curated publishers read from
/// `mcp_curated_server WHERE source = ?`; `"github"` reads the registry table.
pub(crate) fn load_cards_by_publisher(
    conn: &Connection,
    publisher_id: &str,
) -> Result<Vec<McpMarketEntry>> {
    if publisher_id == "github" {
        // Registry-only cards (curated ids are excluded to avoid duplication).
        let mut stmt = conn
            .prepare(
                "SELECT id, name, namespace, description, repo_url, stars, license, version,
                        kind, runtimes_json, updated_at, 0 AS recommended, NULL AS source
                 FROM mcp_registry_server
                 WHERE id NOT IN (SELECT id FROM mcp_curated_server)
                 ORDER BY stars DESC, name ASC",
            )
            .context("Failed to prepare MCP registry publisher list query")?;
        return collect_rows(stmt.query_map([], row_to_card)?);
    }

    // Curated publisher — read by source bucket.
    let mut stmt = conn
        .prepare(
            "SELECT id, name, namespace, description, repo_url, stars, license, version,
                    kind, runtimes_json, updated_at,
                    is_recommended AS recommended, source
             FROM mcp_curated_server
             WHERE source = ?1
             ORDER BY priority ASC, name ASC",
        )
        .context("Failed to prepare MCP curated publisher list query")?;
    collect_rows(stmt.query_map(params![publisher_id], row_to_card)?)
}

pub(crate) fn collect_rows<I>(rows: I) -> Result<Vec<McpMarketEntry>>
where
    I: IntoIterator<Item = rusqlite::Result<McpMarketEntry>>,
{
    let mut out = Vec::new();
    for row in rows {
        out.push(row.context("Failed to decode MCP publisher row")?);
    }
    Ok(out)
}

pub(crate) fn row_to_card(row: &rusqlite::Row<'_>) -> rusqlite::Result<McpMarketEntry> {
    let runtimes_json: String = row.get("runtimes_json")?;
    let kind_str: String = row.get("kind")?;
    let recommended = row
        .get::<_, Option<i64>>("recommended")?
        .unwrap_or_default()
        != 0;
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
        recommended,
        source: row.get("source")?,
    })
}

pub(crate) fn load_cards(conn: &Connection) -> Result<Vec<McpMarketEntry>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, name, namespace, description, repo_url, stars, license, version,
                    kind, runtimes_json, updated_at, recommended, source
             FROM (
                 SELECT
                    id, name, namespace, description, repo_url, stars, license, version,
                    kind, runtimes_json, updated_at,
                    is_recommended AS recommended,
                    source,
                    priority AS sort_priority
                 FROM mcp_curated_server
                 UNION ALL
                 SELECT
                    id, name, namespace, description, repo_url, stars, license, version,
                    kind, runtimes_json, updated_at,
                    0 AS recommended,
                    NULL AS source,
                    100000 AS sort_priority
                 FROM mcp_registry_server
                 WHERE id NOT IN (SELECT id FROM mcp_curated_server)
             )
             ORDER BY recommended DESC, sort_priority ASC, stars DESC, name ASC",
        )
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
pub(crate) fn build_fts_match(query: &str) -> Option<String> {
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

pub(crate) fn search_cards(conn: &Connection, query: &str, limit: u32) -> Result<Vec<McpMarketEntry>> {
    let Some(match_expr) = build_fts_match(query) else {
        let mut cards = load_cards(conn)?;
        cards.truncate(limit as usize);
        return Ok(cards);
    };
    let mut stmt = conn
        .prepare(
            "SELECT id, name, namespace, description, repo_url, stars, license, version,
                    kind, runtimes_json, updated_at, recommended, source
             FROM (
                 SELECT
                    c.id, c.name, c.namespace, c.description, c.repo_url, c.stars, c.license, c.version,
                    c.kind, c.runtimes_json, c.updated_at,
                    c.is_recommended AS recommended,
                    c.source,
                    c.priority AS sort_priority,
                    bm25(mcp_curated_server_fts, 0.0, 8.0, 4.0, 2.0) AS search_rank
                 FROM mcp_curated_server c
                 JOIN mcp_curated_server_fts fts ON fts.id = c.id
                 WHERE mcp_curated_server_fts MATCH ?1
                 UNION ALL
                 SELECT
                    s.id, s.name, s.namespace, s.description, s.repo_url, s.stars, s.license, s.version,
                    s.kind, s.runtimes_json, s.updated_at,
                    0 AS recommended,
                    NULL AS source,
                    100000 AS sort_priority,
                    bm25(mcp_registry_server_fts, 0.0, 8.0, 4.0, 2.0) AS search_rank
                 FROM mcp_registry_server s
                 JOIN mcp_registry_server_fts fts ON fts.id = s.id
                 WHERE mcp_registry_server_fts MATCH ?1
                   AND s.id NOT IN (SELECT id FROM mcp_curated_server)
             )
             ORDER BY recommended DESC, sort_priority ASC, search_rank ASC, stars DESC, name ASC
             LIMIT ?2",
        )
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

pub(crate) fn row_to_full_server(
    row: &rusqlite::Row<'_>,
    recommended: bool,
    source: Option<String>,
) -> rusqlite::Result<McpRegistryServer> {
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
        recommended,
        source,
    })
}

pub(crate) fn load_curated_full_server(
    conn: &Connection,
    id: &str,
) -> Result<Option<McpRegistryServer>> {
    conn.query_row(
        "SELECT id, name, namespace, description, repo_url, stars, license, version, kind,
                runtimes_json, readme, packages_json, remotes_json, raw_server_json, updated_at,
                source, is_recommended
         FROM mcp_curated_server WHERE id = ?1",
        [id],
        |row| {
            let recommended = row.get::<_, i64>("is_recommended")? != 0;
            let source: String = row.get("source")?;
            row_to_full_server(row, recommended, Some(source))
        },
    )
    .optional()
    .context("Failed to load curated MCP server")
}

pub(crate) fn load_registry_full_server(
    conn: &Connection,
    id: &str,
) -> Result<Option<McpRegistryServer>> {
    conn.query_row(
        "SELECT id, name, namespace, description, repo_url, stars, license, version, kind,
                runtimes_json, readme, packages_json, remotes_json, raw_server_json, updated_at
         FROM mcp_registry_server WHERE id = ?1",
        [id],
        |row| row_to_full_server(row, false, None),
    )
    .optional()
    .context("Failed to load MCP registry server")
}

pub(crate) fn load_full_server(conn: &Connection, id: &str) -> Result<Option<McpRegistryServer>> {
    if let Some(server) = load_curated_full_server(conn, id)? {
        return Ok(Some(server));
    }
    load_registry_full_server(conn, id)
}

pub(crate) fn load_curated_servers(conn: &Connection) -> Result<Vec<McpRegistryServer>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, name, namespace, description, repo_url, stars, license, version, kind,
                    runtimes_json, readme, packages_json, remotes_json, raw_server_json, updated_at,
                    source, is_recommended
             FROM mcp_curated_server
             ORDER BY is_recommended DESC, priority ASC, name ASC",
        )
        .context("Failed to prepare curated MCP server query")?;
    let rows = stmt
        .query_map([], |row| {
            let recommended = row.get::<_, i64>("is_recommended")? != 0;
            let source: String = row.get("source")?;
            row_to_full_server(row, recommended, Some(source))
        })
        .context("Failed to query curated MCP servers")?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.context("Failed to decode curated MCP server")?);
    }
    Ok(out)
}

// ── Sync state (shared `marketplace_sync_state` table, scope = "mcp_registry") ──

pub(crate) fn read_sync_state(conn: &Connection) -> Result<Option<SyncStateEntry>> {
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

pub(crate) fn mark_error(conn: &Connection, error: &str) -> Result<()> {
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
