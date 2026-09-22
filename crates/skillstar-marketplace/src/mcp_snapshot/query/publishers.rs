//! Publisher summary aggregation.

use anyhow::{Context, Result};
use rusqlite::Connection;
use tracing::warn;

use crate::mcp_models::McpPublisherSummary;

/// Display name and landing page for each curated `source` bucket, in the
/// order the summary lists them.
///
/// A bucket is the shelf a curated row sits on (`core` / `context` /
/// `browser` / `creative`), not the company that publishes it — the store
/// groups its cards by the same value. This is presentation only: a bucket's
/// *rows* come from the curated catalog (`mcp_snapshot::seeds`), and
/// `seed_default_curated_mcp_servers` deletes any curated row whose id left
/// that catalog — so a bucket with no catalog rows is skipped here rather
/// than hidden by it. Renaming a bucket therefore means editing the catalog
/// rows, not this table.
const CURATED_ORDER: [(&str, &str, &str); 4] = [
    // (source id, display name, url)
    (
        "core",
        "Core",
        "https://github.com/modelcontextprotocol/servers",
    ),
    ("context", "Context", "https://github.com/upstash/context7"),
    (
        "browser",
        "Browser",
        "https://github.com/microsoft/playwright-mcp",
    ),
    ("creative", "Creative", "https://www.figma.com/"),
];

/// Aggregated curated shelves (curated `source` buckets) plus GitHub, which is
/// one publisher backed by the full `mcp_registry_server` table.
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
