//! Publisher grid aggregation.

use anyhow::{Context, Result};
use rusqlite::Connection;
use tracing::warn;

use crate::mcp_models::McpPublisherSummary;

/// Known curated sources in priority order so the grid is stable regardless of
/// insertion order. Each maps to display name + landing page.
///
/// Removing an entry hides that bucket's rows from the grid without deleting
/// them (the seed keeps writing them) — a removal must ship with a
/// `DELETE FROM mcp_curated_server WHERE source = ?` migration.
const CURATED_ORDER: [(&str, &str, &str); 11] = [
    // (source id, display name, url)
    (
        "adspower",
        "AdsPower",
        "https://github.com/AdsPower/adspower-browser",
    ),
    (
        "bigmodel",
        "BigModel",
        "https://docs.bigmodel.cn/cn/coding-plan/mcp/",
    ),
    (
        "anthropic",
        "Anthropic",
        "https://github.com/modelcontextprotocol/servers",
    ),
    (
        "microsoft",
        "Microsoft",
        "https://github.com/microsoft/playwright-mcp",
    ),
    ("saas", "SaaS", "https://modelcontextprotocol.io"),
    ("cn-ai", "Dev Tools", "https://github.com/upstash/context7"),
    (
        "cloudflare",
        "Cloudflare",
        "https://github.com/cloudflare/mcp-server-cloudflare",
    ),
    (
        "brave",
        "Brave",
        "https://github.com/brave/brave-search-mcp",
    ),
    ("google", "Google", "https://developers.google.com/mcp"),
    (
        "supabase",
        "Supabase",
        "https://github.com/supabase/mcp-server-supabase",
    ),
    ("x", "X", "https://docs.x.com/tools/mcp"),
];

/// Aggregated official MCP publishers (curated `source` buckets + GitHub).
/// Curated rows are grouped by `source`; GitHub is one publisher backed by the
/// full `mcp_registry_server` table.
pub(crate) fn load_publishers(conn: &Connection) -> Result<Vec<McpPublisherSummary>> {
    let mut curated_counts: std::collections::HashMap<String, i64> =
        std::collections::HashMap::new();
    let mut stmt = conn
        .prepare("SELECT source, COUNT(*) AS cnt FROM mcp_curated_server GROUP BY source")
        .context("Failed to prepare curated publisher count query")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
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
    let github_count = match conn.query_row("SELECT COUNT(*) FROM mcp_registry_server", [], |row| {
        row.get::<_, i64>(0)
    }) {
        Ok(c) => c,
        Err(e) => {
            warn!(
                "mcp publishers: COUNT(*) on mcp_registry_server failed ({e}); GitHub card will show 0"
            );
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
