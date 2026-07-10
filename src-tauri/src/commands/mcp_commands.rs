//! Tauri commands for MCP (Model Context Protocol) server management.
//!
//! SkillStar owns a single unified MCP store (`~/.skillstar/config/mcp_servers.json`)
//! and projects each server into the native config of every supported agent tool
//! (Claude Code, Codex, Gemini CLI, OpenCode). The heavy lifting
//! lives in [`skillstar_models::mcp`]; this module is the thin, write-serialized
//! Tauri surface over it.
//!
//! All write operations are serialized through a tokio Mutex ([`McpWriteLock`])
//! to prevent concurrent corruption of the store and the live config files.

use std::collections::BTreeSet;

use serde::Serialize;
use skillstar_core::infra::error::AppError;
use tauri::State;
use tokio::sync::Mutex;
use tracing::warn;
use ts_rs::TS;

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
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "McpServerWithSync.ts")]
pub struct McpServerWithSync {
    pub server: McpServerEntry,
    pub sync_results: Vec<McpSyncResult>,
}

// ---------------------------------------------------------------------------
// Read commands
// ---------------------------------------------------------------------------

/// Return the full unified MCP store.
#[tauri::command]
pub async fn list_mcp_servers() -> Result<McpStore, AppError> {
    let path = mcp::mcp_store_path();
    Ok(mcp::read_mcp_store(&path)?)
}

/// Probe each supported tool's MCP config target: installed? how many servers?
#[tauri::command]
pub async fn mcp_tool_statuses() -> Result<Vec<McpToolStatus>, AppError> {
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
) -> Result<McpServerWithSync, AppError> {
    let _guard = lock.0.lock().await;
    let path = mcp::mcp_store_path();
    let mut store = mcp::read_mcp_store(&path)?;

    let created = mcp::create_server(&mut store, entry)?;
    mcp::write_mcp_store(&store, &path)?;

    let sync_results = mcp::sync_server_public_tools(&created, false);
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
) -> Result<McpServerWithSync, AppError> {
    let _guard = lock.0.lock().await;
    let path = mcp::mcp_store_path();
    let mut store = mcp::read_mcp_store(&path)?;

    let (updated, sync_results) = mcp::update_server_and_sync(&mut store, &id, patch, false)?;
    mcp::write_mcp_store(&store, &path)?;
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
) -> Result<Vec<McpSyncResult>, AppError> {
    let _guard = lock.0.lock().await;
    let path = mcp::mcp_store_path();
    let mut store = mcp::read_mcp_store(&path)?;

    let (_removed, results) = mcp::delete_server_and_sync(&mut store, &id)?;
    mcp::write_mcp_store(&store, &path)?;
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
) -> Result<McpSyncResult, AppError> {
    let _guard = lock.0.lock().await;
    let path = mcp::mcp_store_path();
    let mut store = mcp::read_mcp_store(&path)?;

    let result =
        mcp::set_tool_enabled_and_sync(&mut store, &id, &tool_id, enabled, false)?;
    mcp::write_mcp_store(&store, &path)?;
    Ok(result)
}

/// Re-project a single server to all its enabled tools (manual re-sync).
#[tauri::command]
pub async fn sync_mcp_server(
    lock: State<'_, McpWriteLock>,
    id: String,
    force: bool,
) -> Result<Vec<McpSyncResult>, AppError> {
    let _guard = lock.0.lock().await;
    let path = mcp::mcp_store_path();
    let mut store = mcp::read_mcp_store(&path)?;

    let results = mcp::sync_server_by_id(&mut store, &id, force)?;
    mcp::write_mcp_store(&store, &path)?;
    Ok(results)
}

/// Re-project every server to every tool (full reconciliation).
#[tauri::command]
pub async fn sync_all_mcp(
    lock: State<'_, McpWriteLock>,
    force: bool,
) -> Result<Vec<McpSyncResult>, AppError> {
    let _guard = lock.0.lock().await;
    let path = mcp::mcp_store_path();
    let mut store = mcp::read_mcp_store(&path)?;
    let results = mcp::sync_all(&mut store, force);
    mcp::write_mcp_store(&store, &path)?;
    Ok(results)
}

/// Import servers found in a tool's live config into the unified store.
/// Returns the number of newly imported servers.
#[tauri::command]
pub async fn import_mcp_from_tool(
    lock: State<'_, McpWriteLock>,
    tool_id: String,
) -> Result<usize, AppError> {
    let _guard = lock.0.lock().await;
    let path = mcp::mcp_store_path();
    let mut store = mcp::read_mcp_store(&path)?;

    let count = mcp::import_from_tool(&mut store, &tool_id)?;
    if count > 0 {
        mcp::write_mcp_store(&store, &path)?;
    }
    Ok(count)
}

/// Reorder servers by assigning new `sort_index` values from the given ID list.
/// Each ID gets `sort_index = position` (0-based); unlisted servers keep theirs.
#[tauri::command]
pub async fn reorder_mcp_servers(
    lock: State<'_, McpWriteLock>,
    ordered_ids: Vec<String>,
) -> Result<(), AppError> {
    let _guard = lock.0.lock().await;
    let path = mcp::mcp_store_path();
    let mut store = mcp::read_mcp_store(&path)?;

    for (pos, id) in ordered_ids.iter().enumerate() {
        if let Some(s) = store.servers.iter_mut().find(|s| &s.id == id) {
            s.sort_index = pos as u32;
        }
    }
    store.servers.sort_by_key(|s| s.sort_index);
    mcp::write_mcp_store(&store, &path)?;
    Ok(())
}

/// Returns the built-in / recommended MCP presets (read-only, no lock needed).
#[tauri::command]
pub async fn get_mcp_presets() -> Result<Vec<McpPreset>, AppError> {
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
    let draft = super::mcp_marketplace::registry_to_entry(server);
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
    if let Some(source) = &server.source
        && !tags.iter().any(|tag| tag == source)
    {
        tags.push(source.clone());
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
