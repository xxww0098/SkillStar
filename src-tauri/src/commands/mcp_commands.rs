//! Tauri commands for MCP (Model Context Protocol) server management.
//!
//! SkillStar owns a single unified MCP store (`~/.skillstar/config/mcp_servers.json`)
//! and projects each server into the native config of every supported agent tool
//! (Claude Code, Claude Desktop, Codex, Gemini CLI, OpenCode). The heavy lifting
//! lives in [`skillstar_models::mcp`]; this module is the thin, write-serialized
//! Tauri surface over it.
//!
//! All write operations are serialized through a tokio Mutex ([`McpWriteLock`])
//! to prevent concurrent corruption of the store and the live config files.

use std::collections::BTreeSet;

use serde::Serialize;
use tauri::State;
use tokio::sync::Mutex;
use tracing::warn;

use skillstar_models::mcp::{
    self, McpPreset, McpServerEntry, McpServerPatch, McpStore, McpSyncResult, McpToolStatus,
};

// ---------------------------------------------------------------------------
// State: write-serialization mutex
// ---------------------------------------------------------------------------

/// Tokio Mutex used to serialize all writes to `mcp_servers.json` and the
/// per-tool live config files. Managed as Tauri state so every command shares
/// the same lock.
pub struct McpWriteLock(pub Mutex<()>);

impl McpWriteLock {
    pub fn new() -> Self {
        Self(Mutex::new(()))
    }
}

impl Default for McpWriteLock {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

/// A server entry bundled with the results of projecting it to all tools.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerWithSync {
    pub server: McpServerEntry,
    pub sync_results: Vec<McpSyncResult>,
}

// ---------------------------------------------------------------------------
// Read commands
// ---------------------------------------------------------------------------

/// Return the full unified MCP store.
#[tauri::command]
pub async fn list_mcp_servers() -> Result<McpStore, String> {
    let path = mcp::mcp_store_path();
    mcp::read_mcp_store(&path).map_err(|e| e.to_string())
}

/// Probe each supported tool's MCP config target: installed? how many servers?
#[tauri::command]
pub async fn mcp_tool_statuses() -> Result<Vec<McpToolStatus>, String> {
    Ok(mcp::tool_statuses())
}

// ---------------------------------------------------------------------------
// Write commands
// ---------------------------------------------------------------------------

/// Create a new MCP server, persist it, and project it to every tool it is
/// enabled for (per its `enabled` map). Returns the created entry plus the
/// sync results.
#[tauri::command]
pub async fn create_mcp_server(
    lock: State<'_, McpWriteLock>,
    entry: McpServerEntry,
) -> Result<McpServerWithSync, String> {
    let _guard = lock.0.lock().await;
    let path = mcp::mcp_store_path();
    let mut store = mcp::read_mcp_store(&path).map_err(|e| e.to_string())?;

    let created = mcp::create_server(&mut store, entry).map_err(|e| e.to_string())?;
    mcp::write_mcp_store(&store, &path).map_err(|e| e.to_string())?;

    let sync_results = mcp::sync_server_all_tools(&created, false);
    Ok(McpServerWithSync {
        server: created,
        sync_results,
    })
}

/// Apply a partial patch to an existing server, persist it, then re-project it
/// to every enabled tool (and remove it from disabled ones).
#[tauri::command]
pub async fn update_mcp_server(
    lock: State<'_, McpWriteLock>,
    id: String,
    patch: McpServerPatch,
) -> Result<McpServerWithSync, String> {
    let _guard = lock.0.lock().await;
    let path = mcp::mcp_store_path();
    let mut store = mcp::read_mcp_store(&path).map_err(|e| e.to_string())?;

    // Capture the old name so a rename can be cleaned out of live configs.
    let old_name = store
        .servers
        .iter()
        .find(|s| s.id == id)
        .map(|s| s.name.clone());

    let updated = mcp::update_server(&mut store, &id, patch).map_err(|e| e.to_string())?;
    mcp::write_mcp_store(&store, &path).map_err(|e| e.to_string())?;

    // If the server was renamed, purge the stale key from every tool first.
    if let Some(old) = old_name
        && old != updated.name
    {
        for &tool_id in mcp::MCP_TOOL_IDS {
            let _ = mcp::remove_server_from_tool(&old, tool_id);
        }
    }

    let sync_results = mcp::sync_server_all_tools(&updated, false);
    Ok(McpServerWithSync {
        server: updated,
        sync_results,
    })
}

/// Delete a server: remove it from every tool's live config, then drop it from
/// the store.
#[tauri::command]
pub async fn delete_mcp_server(
    lock: State<'_, McpWriteLock>,
    id: String,
) -> Result<Vec<McpSyncResult>, String> {
    let _guard = lock.0.lock().await;
    let path = mcp::mcp_store_path();
    let mut store = mcp::read_mcp_store(&path).map_err(|e| e.to_string())?;

    let removed = mcp::delete_server(&mut store, &id).map_err(|e| e.to_string())?;
    mcp::write_mcp_store(&store, &path).map_err(|e| e.to_string())?;

    let results = mcp::MCP_TOOL_IDS
        .iter()
        .map(|&tool_id| mcp::remove_server_from_tool(&removed.name, tool_id))
        .collect();
    Ok(results)
}

/// Toggle a server on/off for a single tool. Persists the flag, then upserts
/// (enabled) or removes (disabled) the server in that tool's live config.
#[tauri::command]
pub async fn set_mcp_tool_enabled(
    lock: State<'_, McpWriteLock>,
    id: String,
    tool_id: String,
    enabled: bool,
) -> Result<McpSyncResult, String> {
    let _guard = lock.0.lock().await;
    let path = mcp::mcp_store_path();
    let mut store = mcp::read_mcp_store(&path).map_err(|e| e.to_string())?;

    let entry =
        mcp::set_tool_enabled(&mut store, &id, &tool_id, enabled).map_err(|e| e.to_string())?;
    mcp::write_mcp_store(&store, &path).map_err(|e| e.to_string())?;

    let result = if enabled {
        mcp::sync_server_to_tool(&entry, &tool_id, false)
    } else {
        mcp::remove_server_from_tool(&entry.name, &tool_id)
    };
    Ok(result)
}

/// Re-project a single server to all its enabled tools (manual re-sync).
#[tauri::command]
pub async fn sync_mcp_server(
    lock: State<'_, McpWriteLock>,
    id: String,
    force: bool,
) -> Result<Vec<McpSyncResult>, String> {
    let _guard = lock.0.lock().await;
    let path = mcp::mcp_store_path();
    let store = mcp::read_mcp_store(&path).map_err(|e| e.to_string())?;

    let entry = store
        .servers
        .iter()
        .find(|s| s.id == id)
        .ok_or_else(|| format!("MCP server '{id}' not found"))?;
    Ok(mcp::sync_server_all_tools(entry, force))
}

/// Re-project every server to every tool (full reconciliation).
#[tauri::command]
pub async fn sync_all_mcp(
    lock: State<'_, McpWriteLock>,
    force: bool,
) -> Result<Vec<McpSyncResult>, String> {
    let _guard = lock.0.lock().await;
    let path = mcp::mcp_store_path();
    let store = mcp::read_mcp_store(&path).map_err(|e| e.to_string())?;
    Ok(mcp::sync_all(&store, force))
}

/// Import servers found in a tool's live config into the unified store.
/// Returns the number of newly imported servers.
#[tauri::command]
pub async fn import_mcp_from_tool(
    lock: State<'_, McpWriteLock>,
    tool_id: String,
) -> Result<usize, String> {
    let _guard = lock.0.lock().await;
    let path = mcp::mcp_store_path();
    let mut store = mcp::read_mcp_store(&path).map_err(|e| e.to_string())?;

    let count = mcp::import_from_tool(&mut store, &tool_id).map_err(|e| e.to_string())?;
    if count > 0 {
        mcp::write_mcp_store(&store, &path).map_err(|e| e.to_string())?;
    }
    Ok(count)
}

/// Reorder servers by assigning new `sort_index` values from the given ID list.
/// Each ID gets `sort_index = position` (0-based); unlisted servers keep theirs.
#[tauri::command]
pub async fn reorder_mcp_servers(
    lock: State<'_, McpWriteLock>,
    ordered_ids: Vec<String>,
) -> Result<(), String> {
    let _guard = lock.0.lock().await;
    let path = mcp::mcp_store_path();
    let mut store = mcp::read_mcp_store(&path).map_err(|e| e.to_string())?;

    for (pos, id) in ordered_ids.iter().enumerate() {
        if let Some(s) = store.servers.iter_mut().find(|s| &s.id == id) {
            s.sort_index = pos as u32;
        }
    }
    store.servers.sort_by_key(|s| s.sort_index);
    mcp::write_mcp_store(&store, &path).map_err(|e| e.to_string())?;
    Ok(())
}

/// Returns the built-in / recommended MCP presets (read-only, no lock needed).
#[tauri::command]
pub async fn get_mcp_presets() -> Result<Vec<McpPreset>, String> {
    if let Err(err) = crate::core::marketplace::initialize_local_snapshot() {
        warn!(target: "mcp", error = %err, "failed to initialize marketplace snapshot for MCP presets");
        return Ok(mcp::get_mcp_presets());
    }

    match skillstar_marketplace::mcp_snapshot::list_curated_mcp_servers() {
        Ok(servers) if !servers.is_empty() => Ok(servers
            .iter()
            .filter(|server| server.recommended)
            .map(curated_server_to_preset)
            .collect()),
        Ok(_) => Ok(mcp::get_mcp_presets()),
        Err(err) => {
            warn!(target: "mcp", error = %err, "failed to load curated MCP presets from marketplace DB");
            Ok(mcp::get_mcp_presets())
        }
    }
}

fn curated_server_to_preset(server: &skillstar_marketplace::McpRegistryServer) -> McpPreset {
    let draft = skillstar_app::commands::mcp_marketplace::registry_to_entry(server);
    let mut required_env = BTreeSet::new();
    for package in &server.packages {
        for key in &package.required_env {
            required_env.insert(key.clone());
        }
    }
    let mut tags = draft.tags;
    if server.recommended && !tags.iter().any(|tag| tag == "recommended") {
        tags.push("recommended".to_string());
    }
    if let Some(source) = &server.source {
        if !tags.iter().any(|tag| tag == source) {
            tags.push(source.clone());
        }
    }

    McpPreset {
        id: server.id.clone(),
        name: draft.name,
        description: server.description.clone(),
        homepage: server.repo_url.clone(),
        transport: draft.transport,
        command: draft.command,
        args: draft.args,
        env: draft.env,
        url: draft.url,
        headers: draft.headers,
        tags,
        required_env: required_env.into_iter().collect(),
    }
}
