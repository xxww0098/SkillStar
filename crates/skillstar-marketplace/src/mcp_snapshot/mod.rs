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

use crate::mcp_models::{
    McpMarketEntry, McpMarketServerDetail, McpPublisherSummary, McpRegistryServer, McpServerKind,
};
use crate::mcp_remote::fetch_mcp_registry;
use crate::snapshot::{LocalFirstResult, SnapshotStatus, SyncStateEntry, with_conn};

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
// `&Connection` core (pure, testable)
// ---------------------------------------------------------------------------

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

fn truncate_error(error: &str) -> String {
    error.chars().take(500).collect()
}

mod seeds;

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

fn count_servers(conn: &Connection) -> Result<i64> {
    conn.query_row("SELECT COUNT(*) FROM mcp_registry_server", [], |row| {
        row.get(0)
    })
    .context("Failed to count MCP registry servers")
}

/// Aggregated official MCP publishers (curated `source` buckets + GitHub).
/// Curated rows are grouped by `source`; GitHub is one publisher backed by the
/// full `mcp_registry_server` table.
fn load_publishers(conn: &Connection) -> Result<Vec<McpPublisherSummary>> {
    // Known curated sources in priority order so the grid is stable regardless
    // of insertion order. Each maps to display name + landing page.
    const CURATED_ORDER: [(&str, &str, &str); 10] = [
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
fn load_cards_by_publisher(conn: &Connection, publisher_id: &str) -> Result<Vec<McpMarketEntry>> {
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

fn collect_rows<I>(rows: I) -> Result<Vec<McpMarketEntry>>
where
    I: IntoIterator<Item = rusqlite::Result<McpMarketEntry>>,
{
    let mut out = Vec::new();
    for row in rows {
        out.push(row.context("Failed to decode MCP publisher row")?);
    }
    Ok(out)
}

fn row_to_card(row: &rusqlite::Row<'_>) -> rusqlite::Result<McpMarketEntry> {
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

fn load_cards(conn: &Connection) -> Result<Vec<McpMarketEntry>> {
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

fn row_to_full_server(
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

fn load_curated_full_server(conn: &Connection, id: &str) -> Result<Option<McpRegistryServer>> {
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

fn load_registry_full_server(conn: &Connection, id: &str) -> Result<Option<McpRegistryServer>> {
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

fn load_full_server(conn: &Connection, id: &str) -> Result<Option<McpRegistryServer>> {
    if let Some(server) = load_curated_full_server(conn, id)? {
        return Ok(Some(server));
    }
    load_registry_full_server(conn, id)
}

fn load_curated_servers(conn: &Connection) -> Result<Vec<McpRegistryServer>> {
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
        // 3 cloudflare + 1 brave + 2 google + 1 supabase = 23.
        let registry_start = cards
            .iter()
            .position(|c| c.id == "1")
            .expect("registry filesystem card present");
        assert_eq!(registry_start, 23);
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

        // 10 curated publishers + GitHub (0 registry rows seeded yet) = 11.
        assert_eq!(publishers.len(), 11);
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
        assert_eq!(publishers[10].id, "github");
        assert_eq!(publishers[10].server_count, 0);

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
