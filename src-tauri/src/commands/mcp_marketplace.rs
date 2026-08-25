//! Tauri commands for the **MCP marketplace**.
//!
//! Reads and syncs delegate to `skillstar_marketplace::mcp_snapshot`
//! (local-first, multi-source); everything that turns a catalog row into
//! something installable — the runtime-shape picker, the prefilled draft, the
//! pre-install confirmation payload — delegates to `skillstar_app::mcp`, which
//! is where the marketplace→models mapping belongs (AGENTS.md; audit §C.1).
//!
//! This module owns command registration, argument/DTO shapes and error
//! mapping. It holds no domain logic.

use skillstar_core::infra::error::AppError;
use skillstar_marketplace::{
    LocalFirstResult, McpCustomSource, McpMarketEntry, McpMarketServerDetail, McpPublisherSummary,
    McpRegistryServer, McpServerPage, McpServerQuery, McpSourceDescriptor, SyncStateEntry,
    mcp_snapshot,
};
use skillstar_models::mcp::McpServerEntry;
use tracing::{debug, error};

use skillstar_app::mcp::{McpInstallPlan, McpRuntimeSelection};

const MCP_REGISTRY_SCOPE: &str = "mcp_registry";

// ---------------------------------------------------------------------------
// Catalog reads
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_mcp_publishers_local() -> Result<Vec<McpPublisherSummary>, AppError> {
    debug!(target: "mcp_marketplace", "list_mcp_publishers_local called");
    mcp_snapshot::list_mcp_publishers().map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn list_mcp_servers_by_publisher_local(
    publisher_id: String,
) -> Result<LocalFirstResult<Vec<McpMarketEntry>>, AppError> {
    debug!(target: "mcp_marketplace", publisher = %publisher_id, "list_mcp_servers_by_publisher_local called");
    mcp_snapshot::list_mcp_servers_by_publisher(&publisher_id)
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn list_mcp_market_servers_local()
-> Result<LocalFirstResult<Vec<McpMarketEntry>>, AppError> {
    debug!(target: "mcp_marketplace", "list_mcp_market_servers_local called");
    mcp_snapshot::list_mcp_servers_local().await.map_err(|e| {
        error!(target: "mcp_marketplace", error = %e, "list local failed");
        AppError::Other(e.to_string())
    })
}

/// Filtered, sorted, paginated card query — the read the store UI should use.
///
/// The catalog is ~21k rows across every source, so the unpaginated
/// [`list_mcp_market_servers_local`] cannot be the browse path: it would move
/// the whole registry across IPC on every keystroke and filter it in the
/// renderer's memory (audit A.3-d/A.3-e). [`McpServerPage`] carries `total`
/// alongside the page so a UI can show "60 of 21363" without a second call.
#[tauri::command]
pub async fn query_mcp_market_servers_local(
    query: McpServerQuery,
) -> Result<LocalFirstResult<McpServerPage>, AppError> {
    debug!(
        target: "mcp_marketplace",
        limit = ?query.limit, offset = ?query.offset,
        "query_mcp_market_servers_local called"
    );
    mcp_snapshot::query_mcp_servers_local(&query)
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn search_mcp_market_local(
    query: String,
    limit: Option<u32>,
) -> Result<LocalFirstResult<Vec<McpMarketEntry>>, AppError> {
    debug!(target: "mcp_marketplace", query = %query, "search_mcp_market_local called");
    mcp_snapshot::search_mcp_servers_local(&query, limit)
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn get_mcp_market_server_detail_local(
    id: String,
) -> Result<LocalFirstResult<Option<McpMarketServerDetail>>, AppError> {
    debug!(target: "mcp_marketplace", id = %id, "get_mcp_market_server_detail_local called");
    mcp_snapshot::get_mcp_server_detail_local(&id)
        .await
        .map_err(|e| AppError::Other(e.to_string()))
}

// ---------------------------------------------------------------------------
// Sync + observability
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn sync_mcp_market_scope(scope: String) -> Result<(), AppError> {
    debug!(target: "mcp_marketplace", scope = %scope, "sync_mcp_market_scope called");
    // Only the registry scope exists today; accept it (and the empty default)
    // and reject anything unexpected so typos surface instead of silently
    // syncing the wrong thing.
    if !scope.is_empty() && scope != MCP_REGISTRY_SCOPE {
        return Err(AppError::Other(format!(
            "Unknown MCP market scope: {scope}"
        )));
    }
    mcp_snapshot::sync_mcp_registry_scope().await.map_err(|e| {
        error!(target: "mcp_marketplace", error = %e, "sync failed");
        AppError::Other(e.to_string())
    })
}

/// Aggregate freshness of the whole MCP catalog.
#[tauri::command]
pub async fn get_mcp_market_sync_states() -> Result<Vec<SyncStateEntry>, AppError> {
    mcp_snapshot::mcp_market_sync_states().map_err(|e| AppError::Other(e.to_string()))
}

/// One row per source (`mcp_registry:<source_id>`), each carrying its own
/// last success/attempt/error and `degraded_reason`.
///
/// The aggregate scope alone cannot answer "why is the catalog incomplete?" —
/// a sync where one of four sources failed still reports success. These rows
/// are what lets the UI say "this sync was incomplete, because X".
#[tauri::command]
pub async fn get_mcp_source_sync_states() -> Result<Vec<SyncStateEntry>, AppError> {
    mcp_snapshot::mcp_source_sync_states().map_err(|e| AppError::Other(e.to_string()))
}

// ---------------------------------------------------------------------------
// Catalog sources (built-ins + user-added registries / local directories)
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_mcp_sources() -> Result<Vec<McpSourceDescriptor>, AppError> {
    Ok(mcp_snapshot::list_mcp_sources())
}

/// Add (or replace) a user registry URL or local JSON directory file. Returns
/// the full source list so the caller never has to re-read it.
#[tauri::command]
pub async fn add_mcp_source(source: McpCustomSource) -> Result<Vec<McpSourceDescriptor>, AppError> {
    debug!(target: "mcp_marketplace", id = %source.id, "add_mcp_source called");
    mcp_snapshot::add_mcp_source(source).map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn remove_mcp_source(id: String) -> Result<Vec<McpSourceDescriptor>, AppError> {
    debug!(target: "mcp_marketplace", id = %id, "remove_mcp_source called");
    mcp_snapshot::remove_mcp_source(&id).map_err(|e| AppError::Other(e.to_string()))
}

/// Turn any source on or off — built-in ids included.
#[tauri::command]
pub async fn set_mcp_source_enabled(
    id: String,
    enabled: bool,
) -> Result<Vec<McpSourceDescriptor>, AppError> {
    debug!(target: "mcp_marketplace", id = %id, enabled, "set_mcp_source_enabled called");
    mcp_snapshot::set_mcp_source_enabled(&id, enabled).map_err(|e| AppError::Other(e.to_string()))
}

// ---------------------------------------------------------------------------
// Install path
// ---------------------------------------------------------------------------

fn load_registry_server(id: &str) -> Result<McpRegistryServer, AppError> {
    mcp_snapshot::get_registry_server_local(id)
        .map_err(|e| AppError::Other(e.to_string()))?
        .ok_or_else(|| AppError::Other(format!("MCP server '{id}' not found in local snapshot")))
}

/// Every runtime shape this server publishes, ranked against the local
/// machine, with the recommended pick. The user may install any of them.
#[tauri::command]
pub async fn mcp_market_runtime_candidates(id: String) -> Result<McpRuntimeSelection, AppError> {
    debug!(target: "mcp_marketplace", id = %id, "mcp_market_runtime_candidates called");
    Ok(skillstar_app::mcp::select_runtime(&load_registry_server(
        &id,
    )?))
}

/// The pre-install confirmation payload: the complete resolved command, the
/// runtime alternatives, and every input the form must collect with its full
/// `server.json` semantics.
///
/// Showing the untruncated command before running it is a spec MUST and the
/// only effective mitigation for deeplink-style install attacks — see
/// `skillstar_app::mcp::install`.
#[tauri::command]
pub async fn mcp_market_install_plan(
    id: String,
    runtime_id: Option<String>,
) -> Result<McpInstallPlan, AppError> {
    debug!(target: "mcp_marketplace", id = %id, runtime = ?runtime_id, "mcp_market_install_plan called");
    Ok(skillstar_app::mcp::build_install_plan(
        &load_registry_server(&id)?,
        runtime_id.as_deref(),
    ))
}

/// Convert a marketplace server into a prefilled, ready-to-edit
/// [`McpServerEntry`] draft (id empty, secrets blank). The frontend finalizes it
/// in the MCP server form and submits via the existing `create_mcp_server`.
///
/// `runtime_id` picks a specific shape from
/// [`mcp_market_runtime_candidates`]; omitting it uses the recommendation.
#[tauri::command]
pub async fn mcp_market_entry_to_draft(
    id: String,
    runtime_id: Option<String>,
) -> Result<McpServerEntry, AppError> {
    debug!(target: "mcp_marketplace", id = %id, runtime = ?runtime_id, "mcp_market_entry_to_draft called");
    let server = load_registry_server(&id)?;
    let Some(runtime_id) = runtime_id else {
        return Ok(skillstar_app::mcp::registry_to_entry(&server));
    };
    let selection = skillstar_app::mcp::select_runtime(&server);
    Ok(skillstar_app::mcp::registry_to_entry_for(
        &server,
        selection.resolve(Some(&runtime_id)),
    ))
}
