//! Full-row reads (detail drawer + install draft input).

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension};

use crate::mcp_models::{McpRegistryServer, McpServerKind, McpServerStatus};

/// Columns shared by both tables for a full-row read. Curated reads append
/// `source, is_recommended`.
const FULL_COLUMNS: &str = "id, name, namespace, description, repo_url, stars, license, version, \
     kind, runtimes_json, readme, packages_json, remotes_json, raw_server_json, updated_at, \
     title, website_url, icons_json, status, is_latest, published_at, registry_source, \
     contributing_sources_json";

pub(crate) fn row_to_full_server(
    row: &rusqlite::Row<'_>,
    recommended: bool,
    source: Option<String>,
) -> rusqlite::Result<McpRegistryServer> {
    let kind_str: String = row.get("kind")?;
    let runtimes_json: String = row.get("runtimes_json")?;
    let packages_json: String = row.get("packages_json")?;
    let remotes_json: String = row.get("remotes_json")?;
    let icons_json: String = row
        .get::<_, Option<String>>("icons_json")?
        .unwrap_or_else(|| "[]".into());
    let contributing_sources_json: String = row
        .get::<_, Option<String>>("contributing_sources_json")?
        .unwrap_or_else(|| "[]".into());
    let status_str: String = row.get::<_, Option<String>>("status")?.unwrap_or_default();
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
        title: row.get("title")?,
        website_url: row.get("website_url")?,
        icons: serde_json::from_str(&icons_json).unwrap_or_default(),
        status: McpServerStatus::from_db_str(&status_str),
        is_latest: row.get::<_, Option<i64>>("is_latest")?.unwrap_or(1) != 0,
        published_at: row.get("published_at")?,
        registry_source: row.get("registry_source")?,
        contributing_sources: serde_json::from_str(&contributing_sources_json).unwrap_or_default(),
    })
}

pub(crate) fn load_curated_full_server(
    conn: &Connection,
    id: &str,
) -> Result<Option<McpRegistryServer>> {
    conn.query_row(
        &format!(
            "SELECT {FULL_COLUMNS}, source, is_recommended FROM mcp_curated_server WHERE id = ?1"
        ),
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
        &format!("SELECT {FULL_COLUMNS} FROM mcp_registry_server WHERE id = ?1"),
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
        .prepare(&format!(
            "SELECT {FULL_COLUMNS}, source, is_recommended
             FROM mcp_curated_server
             ORDER BY is_recommended DESC, priority ASC, name ASC"
        ))
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
