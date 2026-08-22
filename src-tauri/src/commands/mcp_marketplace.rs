//! Tauri commands for the **MCP marketplace**.
//!
//! Reads and syncs delegate to `skillstar_marketplace::mcp_snapshot`
//! (local-first, multi-source); turning a catalog row into something
//! installable — the pre-install confirmation payload — delegates to
//! `skillstar_app::mcp`, which is where the marketplace→models mapping belongs
//! (AGENTS.md; audit §C.1).
//!
//! This module owns command registration, argument/DTO shapes and error
//! mapping. It holds no domain logic.

use std::collections::BTreeMap;

use serde::Serialize;
use skillstar_core::infra::error::AppError;
use skillstar_marketplace::{
    LocalFirstResult, McpCustomSource, McpMarketEntry, McpMarketServerDetail, McpPublisherSummary,
    McpRegistryServer, McpServerPage, McpServerQuery, McpSourceDescriptor, SyncStateEntry,
    mcp_snapshot,
};
use skillstar_models::mcp;
use tauri::State;
use tracing::{debug, error};
use ts_rs::TS;

use skillstar_app::mcp::{McpInstallAnswer, McpInstallPlan, McpInstallPreview, McpInstallRejection};

use super::mcp_commands::{McpServerWithSync, McpWriteLock};

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
pub async fn add_mcp_source(
    source: McpCustomSource,
) -> Result<Vec<McpSourceDescriptor>, AppError> {
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

/// The same payload recomputed with the user's answers folded in: the entry as
/// it would be written, and the command line as it would run.
///
/// Separate from [`mcp_market_install_plan`] because it is cheap — no `PATH`
/// walk, no filesystem — so the wizard can call it as the form is filled. The
/// answers carry the user's secrets, which is why **only the row id and the
/// runtime shape are logged**, never a value, and why the result must not be
/// cached (a cache key holding a secret is a secret at rest).
#[tauri::command]
pub async fn mcp_market_install_preview(
    id: String,
    runtime_id: Option<String>,
    answers: Vec<McpInstallAnswer>,
) -> Result<McpInstallPreview, AppError> {
    debug!(
        target: "mcp_marketplace",
        id = %id, runtime = ?runtime_id, answers = answers.len(),
        "mcp_market_install_preview called"
    );
    Ok(skillstar_app::mcp::preview_install(
        &load_registry_server(&id)?,
        runtime_id.as_deref(),
        &answers,
    ))
}

/// What one install attempt produced: an entry that was written and projected,
/// or a refusal that wrote nothing.
///
/// A sum type rather than an `Err`, because [`AppError`] serializes to a bare
/// string — a refusal the UI has to tell apart from another refusal cannot
/// survive that. The `installed` arm is verbatim what `create_mcp_server`
/// returns, so the per-target success/skip/failure display is unchanged.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(tag = "status", rename_all = "camelCase")]
#[ts(export, export_to = "McpInstallOutcome.ts")]
pub enum McpInstallOutcome {
    /// Boxed only to keep the two arms a similar size (`clippy::large_enum_variant`);
    /// `Box` is transparent to serde and to ts-rs, so the wire shape is unchanged.
    #[serde(rename_all = "camelCase")]
    Installed { installed: Box<McpServerWithSync> },
    #[serde(rename_all = "camelCase")]
    Rejected { rejection: McpInstallRejection },
}

/// Install a catalog row from the answers the user gave.
///
/// The entry is derived here, from the catalog row as it stands *now* — the
/// renderer no longer assembles one. `approved_preview` is the string the user
/// confirmed, and `skillstar_app::mcp::prepare_install` refuses unless the
/// fresh derivation still renders it: the row is re-read at this moment, and a
/// registry sync can rewrite it while the preview sits on screen.
///
/// The answers and the approved command may both contain secrets, so **only
/// the row id and the runtime shape are logged** — never a value, never the
/// command line.
///
/// Deliberately not `create_mcp_server`, which stays as-is for the manual "add
/// server" form: that form submits an entry the user authored themselves, and
/// enforcing a publisher's required inputs against it would be enforcing them
/// against nothing.
#[tauri::command]
pub async fn mcp_market_install(
    lock: State<'_, McpWriteLock>,
    id: String,
    runtime_id: Option<String>,
    answers: Vec<McpInstallAnswer>,
    enabled: BTreeMap<String, bool>,
    approved_preview: String,
) -> Result<McpInstallOutcome, AppError> {
    debug!(
        target: "mcp_marketplace",
        id = %id, runtime = ?runtime_id, answers = answers.len(),
        "mcp_market_install called"
    );
    let entry = match skillstar_app::mcp::prepare_install(
        &load_registry_server(&id)?,
        runtime_id.as_deref(),
        &answers,
        enabled,
        &approved_preview,
    ) {
        Ok(entry) => entry,
        Err(rejection) => return Ok(McpInstallOutcome::Rejected { rejection }),
    };

    let _guard = lock.0.lock().await;
    let path = mcp::mcp_store_path();
    let mut store = mcp::read_mcp_store(&path)?;
    // Writes the store itself, before projecting — the order is that
    // function's contract, not this adapter's choice.
    let (server, sync_results) = mcp::create_server_and_sync(&mut store, &path, entry)?;
    Ok(McpInstallOutcome::Installed {
        installed: Box::new(McpServerWithSync {
            server,
            sync_results,
        }),
    })
}
