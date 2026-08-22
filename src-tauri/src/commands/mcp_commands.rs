//! Tauri commands for MCP (Model Context Protocol) server management.
//!
//! SkillStar owns a single unified MCP store (`~/.skillstar/config/mcp_servers.json`)
//! and projects each server into the native config of every supported agent tool
//! (Claude Code, Codex, OpenCode, and others). The heavy lifting
//! lives in [`skillstar_models::mcp`]; this module is the thin, write-serialized
//! Tauri surface over it.
//!
//! All write operations are serialized through a tokio Mutex ([`McpWriteLock`])
//! to prevent concurrent corruption of the store and the live config files.

use serde::Serialize;
use skillstar_core::infra::error::AppError;
use tauri::State;
use tokio::sync::Mutex;
use tracing::warn;
use ts_rs::TS;

use skillstar_models::mcp::{
    self, McpPreset, McpProbeReport, McpServerEntry, McpServerPatch, McpStore, McpSyncResult,
    McpToolStatus,
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

/// Health-check one installed server: reach it, work out which MCP revision it
/// speaks, and list its tools.
///
/// Never fails for a server that is merely unhealthy — the outcome is in
/// [`McpProbeReport::status`]. In particular `authorizationRequired` (a remote
/// server answering `401` with a `WWW-Authenticate` challenge) is a *correct*
/// response asking for OAuth, not an error, and must not be rendered as one.
#[tauri::command]
pub async fn probe_mcp_server(id: String) -> Result<McpProbeReport, AppError> {
    let path = mcp::mcp_store_path();
    let store = mcp::read_mcp_store(&path)?;
    let entry = store
        .servers
        .iter()
        .find(|server| server.id == id)
        .ok_or_else(|| AppError::Other(format!("MCP server '{id}' not found")))?;
    Ok(mcp::probe_server(entry).await)
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

    let (server, sync_results) = mcp::create_server_and_sync(&mut store, &path, entry)?;
    Ok(McpServerWithSync {
        server,
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

    let result = mcp::set_tool_enabled_and_sync(&mut store, &id, &tool_id, enabled, false)?;
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
///
/// The curated marketplace rows and the built-in catalog are merged, not chosen
/// between. Treating them as alternatives is what made this command return a
/// single chip: the curated list is never empty, so it always won the branch,
/// and only one curated row is flagged `recommended` — which silently retired
/// the whole built-in catalog.
#[tauri::command]
pub async fn get_mcp_presets() -> Result<Vec<McpPreset>, AppError> {
    Ok(mcp::merge_mcp_presets(
        curated_recommended_presets(),
        mcp::get_mcp_presets(),
    ))
}

/// Curated marketplace rows explicitly flagged `recommended`.
///
/// Best-effort by design: a missing or unreadable snapshot DB must degrade to
/// "no curated additions", never to "no presets at all". The full curated
/// catalog stays behind the marketplace browser; only promoted rows join the
/// preset chips.
fn curated_recommended_presets() -> Vec<McpPreset> {
    if let Err(err) = crate::core::marketplace_snapshot::initialize() {
        warn!(target: "mcp", error = %err, "failed to initialize marketplace snapshot for MCP presets");
        return Vec::new();
    }
    match skillstar_marketplace::mcp_snapshot::list_curated_mcp_servers() {
        Ok(servers) => servers
            .iter()
            .filter(|server| server.recommended)
            .map(skillstar_app::mcp::curated_server_to_preset)
            .collect(),
        Err(err) => {
            warn!(target: "mcp", error = %err, "failed to load curated MCP presets from marketplace DB");
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use skillstar_app::mcp::curated_server_to_preset;
    use skillstar_marketplace::{McpRegistryPackageSummary, McpRegistryServer, McpServerKind};

    fn curated(id: &str, name: &str, recommended: bool) -> McpRegistryServer {
        McpRegistryServer {
            id: id.into(),
            name: name.into(),
            namespace: format!("acme/{name}"),
            description: "curated row".into(),
            repo_url: "https://github.com/acme/x".into(),
            kind: McpServerKind::Stdio,
            runtimes: vec!["npx".into()],
            packages: vec![McpRegistryPackageSummary {
                runtime: "npx".into(),
                identifier: "@acme/x".into(),
                registry_type: Some("npm".into()),
                ..Default::default()
            }],
            recommended,
            source: Some("acme".into()),
            ..Default::default()
        }
    }

    /// Pins the A.3-f regression: the command used to return *either* the
    /// curated rows *or* the built-in catalog, and since only one curated row
    /// carries `recommended: true`, the UI ended up with a single preset chip.
    #[test]
    fn recommended_curated_rows_join_the_builtin_catalog_instead_of_replacing_it() {
        let builtin = mcp::get_mcp_presets();
        let rows = [
            curated("acme-promoted", "acme-promoted", true),
            curated("acme-ordinary", "acme-ordinary", false),
        ];

        let promoted: Vec<McpPreset> = rows
            .iter()
            .filter(|server| server.recommended)
            .map(curated_server_to_preset)
            .collect();
        let merged = mcp::merge_mcp_presets(promoted, builtin.clone());

        assert_eq!(merged.len(), builtin.len() + 1);
        assert!(merged.iter().any(|p| p.id == "acme-promoted"));
        assert!(
            !merged.iter().any(|p| p.id == "acme-ordinary"),
            "only promoted curated rows belong in the preset chips"
        );
        for preset in &builtin {
            assert!(
                merged.iter().any(|p| p.id == preset.id),
                "built-in preset '{}' must still reach the UI",
                preset.id
            );
        }
    }
}
