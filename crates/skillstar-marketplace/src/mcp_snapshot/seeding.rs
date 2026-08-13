//! Curated MCP seed upsert.
//!
//! Curated rows are *code as data*: `seeds::default_curated_mcp_servers()` is
//! the source of truth and this upsert makes the table match it, so a curated
//! row can be edited freely — only its `id` (primary key + publisher bucket)
//! is a contract.

use anyhow::{Context, Result};
use rusqlite::{Connection, params};

use super::{now_rfc3339, seeds};

const CURATED_SOURCE_ID: &str = "skillstar-curated";

/// Seed/refresh the curated MCP servers (idempotent upsert). Called by schema
/// creation and defensively before each read so curated cards are always
/// present even if the registry has never synced.
pub(crate) fn seed_default_curated_mcp_servers(conn: &Connection) -> Result<()> {
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
                    updated_at, fetched_at, source, is_recommended, priority,
                    title, website_url, icons_json, status, is_latest, published_at,
                    registry_source, contributing_sources_json
                ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,
                          ?20,?21,?22,?23,?24,?25,?26,?27)
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
                    priority = excluded.priority,
                    title = excluded.title,
                    website_url = excluded.website_url,
                    icons_json = excluded.icons_json,
                    status = excluded.status,
                    is_latest = excluded.is_latest,
                    published_at = excluded.published_at,
                    registry_source = excluded.registry_source,
                    contributing_sources_json = excluded.contributing_sources_json",
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
            let icons_json = serde_json::to_string(&server.icons).unwrap_or_else(|_| "[]".into());
            let contributing_sources_json =
                serde_json::to_string(&server.contributing_sources).unwrap_or_else(|_| "[]".into());
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
                    &server.title,
                    &server.website_url,
                    icons_json,
                    server.status.as_db_str(),
                    if server.is_latest { 1_i64 } else { 0_i64 },
                    &server.published_at,
                    &server.registry_source,
                    contributing_sources_json,
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
