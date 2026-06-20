//! Unified MCP store: path, read/write IO, validation, and pure CRUD on `McpStore`.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use uuid::Uuid;

use super::*;

// ---------------------------------------------------------------------------
// Store path + IO
// ---------------------------------------------------------------------------

/// `~/.skillstar/config/mcp_servers.json`
pub fn mcp_store_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".skillstar")
        .join("config")
        .join("mcp_servers.json")
}

/// Read the store, returning an empty default on missing/malformed files.
pub fn read_mcp_store(path: &Path) -> Result<McpStore> {
    if !path.exists() {
        return Ok(McpStore::default());
    }
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!(
                "Failed to read MCP store {}: {e}. Using default.",
                path.display()
            );
            return Ok(McpStore::default());
        }
    };
    let text = text.trim_start_matches('\u{FEFF}');
    match serde_json::from_str::<McpStore>(text) {
        Ok(store) => Ok(store),
        Err(e) => {
            tracing::warn!(
                "Malformed MCP store {}: {e}. Using default.",
                path.display()
            );
            Ok(McpStore::default())
        }
    }
}

/// Write the store atomically (temp file + rename).
pub fn write_mcp_store(store: &McpStore, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(store).context("Failed to serialize McpStore")?;
    let temp_path = path.with_extension("json.tmp");
    std::fs::write(&temp_path, json.as_bytes())
        .with_context(|| format!("Failed to write temp file {}", temp_path.display()))?;
    std::fs::rename(&temp_path, path).with_context(|| {
        format!(
            "Failed to rename {} to {}",
            temp_path.display(),
            path.display()
        )
    })?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// Validate an entry's transport-specific required fields.
pub fn validate_entry(entry: &McpServerEntry) -> Result<()> {
    if entry.name.trim().is_empty() {
        bail!("MCP server name must not be empty");
    }
    match entry.transport.as_str() {
        "stdio" => {
            if entry
                .command
                .as_deref()
                .map(str::trim)
                .unwrap_or("")
                .is_empty()
            {
                bail!("stdio MCP server '{}' requires a command", entry.name);
            }
        }
        "http" | "sse" => {
            if entry.url.as_deref().map(str::trim).unwrap_or("").is_empty() {
                bail!(
                    "{} MCP server '{}' requires a url",
                    entry.transport,
                    entry.name
                );
            }
        }
        other => bail!("Unknown MCP transport '{other}' (expected stdio|http|sse)"),
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Store CRUD (pure — operate on &mut McpStore)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Store CRUD (pure — operate on &mut McpStore)
// ---------------------------------------------------------------------------

/// Create a new server: assigns a fresh UUID, timestamps, and sort index.
pub fn create_server(store: &mut McpStore, mut entry: McpServerEntry) -> Result<McpServerEntry> {
    validate_entry(&entry)?;
    if store.servers.iter().any(|s| s.name == entry.name) {
        bail!("An MCP server named '{}' already exists", entry.name);
    }
    entry.id = Uuid::new_v4().to_string();
    let now = now_ms();
    entry.created_at = Some(now);
    entry.updated_at = Some(now);
    entry.sort_index = store
        .servers
        .iter()
        .map(|s| s.sort_index)
        .max()
        .map_or(0, |m| m + 1);
    // Drop enable flags for unknown tools.
    entry.enabled.retain(|k, _| is_supported_tool(k));
    store.servers.push(entry.clone());
    Ok(entry)
}

/// Apply a partial patch to an existing server.
pub fn update_server(
    store: &mut McpStore,
    id: &str,
    patch: McpServerPatch,
) -> Result<McpServerEntry> {
    // Guard against renaming onto another server's name.
    if let Some(new_name) = patch.name.as_ref()
        && store
            .servers
            .iter()
            .any(|s| s.id != id && &s.name == new_name)
    {
        bail!("An MCP server named '{}' already exists", new_name);
    }
    let server = store
        .servers
        .iter_mut()
        .find(|s| s.id == id)
        .with_context(|| format!("MCP server '{id}' not found"))?;

    if let Some(v) = patch.name {
        server.name = v;
    }
    if let Some(v) = patch.transport {
        server.transport = v;
    }
    if let Some(v) = patch.command {
        server.command = Some(v);
    }
    if let Some(v) = patch.args {
        server.args = v;
    }
    if let Some(v) = patch.env {
        server.env = v;
    }
    if let Some(v) = patch.cwd {
        server.cwd = if v.trim().is_empty() { None } else { Some(v) };
    }
    if let Some(v) = patch.url {
        server.url = Some(v);
    }
    if let Some(v) = patch.headers {
        server.headers = v;
    }
    if let Some(v) = patch.description {
        server.description = if v.trim().is_empty() { None } else { Some(v) };
    }
    if let Some(v) = patch.homepage {
        server.homepage = if v.trim().is_empty() { None } else { Some(v) };
    }
    if let Some(v) = patch.tags {
        server.tags = v;
    }
    server.updated_at = Some(now_ms());

    let updated = server.clone();
    validate_entry(&updated)?;
    Ok(updated)
}

/// Remove a server from the store. Returns the removed entry.
pub fn delete_server(store: &mut McpStore, id: &str) -> Result<McpServerEntry> {
    let idx = store
        .servers
        .iter()
        .position(|s| s.id == id)
        .with_context(|| format!("MCP server '{id}' not found"))?;
    Ok(store.servers.remove(idx))
}

/// Set the enabled flag for a server on a tool. Returns the updated entry.
pub fn set_tool_enabled(
    store: &mut McpStore,
    id: &str,
    tool_id: &str,
    enabled: bool,
) -> Result<McpServerEntry> {
    if !is_supported_tool(tool_id) {
        bail!("Unsupported tool '{tool_id}'");
    }
    let server = store
        .servers
        .iter_mut()
        .find(|s| s.id == id)
        .with_context(|| format!("MCP server '{id}' not found"))?;
    server.enabled.insert(tool_id.to_string(), enabled);
    server.updated_at = Some(now_ms());
    Ok(server.clone())
}
