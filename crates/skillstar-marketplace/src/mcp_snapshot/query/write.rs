//! Catalog write path: the full swap performed after a successful sync.

use anyhow::{Context, Result};
use rusqlite::{Connection, params};

use crate::mcp_models::McpRegistryServer;

use crate::mcp_snapshot::now_rfc3339;

/// Replace the entire cached catalog atomically (every source is fetched as a
/// whole and merged before this call, so a full swap keeps the snapshot
/// internally consistent).
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
                    updated_at, fetched_at,
                    title, website_url, icons_json, status, is_latest, published_at,
                    registry_source, contributing_sources_json
                ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,
                          ?17,?18,?19,?20,?21,?22,?23,?24)",
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
            let icons_json = serde_json::to_string(&server.icons).unwrap_or_else(|_| "[]".into());
            let contributing_sources_json =
                serde_json::to_string(&server.contributing_sources).unwrap_or_else(|_| "[]".into());
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
                    server.title,
                    server.website_url,
                    icons_json,
                    server.status.as_db_str(),
                    if server.is_latest { 1_i64 } else { 0_i64 },
                    server.published_at,
                    server.registry_source,
                    contributing_sources_json,
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
