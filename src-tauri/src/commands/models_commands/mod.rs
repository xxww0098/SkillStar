//! Tauri commands for Models mode — Provider CRUD, activation, and tool sync.
//!
//! All write operations are serialized through a tokio Mutex to prevent
//! concurrent corruption of `model_providers.json`.
//!
//! ## Architecture
//!
//! Commands operate on the v4 store (`ProvidersStoreV4`: a provider list plus a
//! `bindings` map). The v1/v2/v3 formats survive only as migration sources
//! inside `skillstar_models::providers`.
//!
//! The **wire** shape is still v3 — see [`compat`] for what that means and when
//! it goes away. Nothing below this module's boundary speaks v3.

use serde::{Deserialize, Serialize};
use skillstar_core::infra::error::AppError;
use tauri::State;
use tokio::sync::Mutex;

use skillstar_models::AiProviderRef;
use skillstar_models::ai_provider;
use skillstar_models::diagnostics::ConnectionTestResult;
use skillstar_models::latency::{self, EndpointLatencyResult, LatencyResult};
use skillstar_models::providers::ProviderPresetFlat;
use skillstar_models::providers::{
    self, ModelCatalogFetchResult, ProviderEntryFlat, ProviderPatchFlat, ProvidersStoreV4,
    ToolBinding,
};
use skillstar_models::tool_sync::{self, ToolConfigTarget, ToolSyncResultFlat};

// ---------------------------------------------------------------------------
// Submodules (mechanical split — commands re-exported so `models_commands::NAME`
// resolves exactly as before).
// ---------------------------------------------------------------------------

mod compat;
mod diagnostics;
mod provider_cmds;
mod tools;

pub use diagnostics::*;
pub use provider_cmds::*;
pub use tools::*;

/// Load the v4 store, turning a store error into an `AppError` the renderer
/// can show.
///
/// Deliberately *not* the repair path: that runs once, from
/// `get_providers_flat`, because it rewrites agent config files. Every other
/// command wants the store as it is.
fn load_store() -> Result<ProvidersStoreV4, AppError> {
    providers::load_store()
        .map(|loaded| loaded.store)
        .map_err(|e| AppError::Other(e.to_string()))
}

// ---------------------------------------------------------------------------
// State: write-serialization mutex
// ---------------------------------------------------------------------------

/// Tokio Mutex used to serialize all writes to `model_providers.json`.
/// Managed as Tauri state so all commands share the same lock.
pub struct ProvidersWriteLock(pub Mutex<()>);

impl ProvidersWriteLock {
    pub fn new() -> Self {
        Self(Mutex::new(()))
    }
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

/// Response for `get_providers_flat` — returns the full flat store contents.
///
/// Field names are intentionally snake_case (no `rename_all = "camelCase"`)
/// because the frontend `FlatProvidersResponse` type and every consumer reads
/// `tool_activations` with an underscore. An earlier `rename_all` here
/// serialized that field as `toolActivations`, so the frontend always saw
/// `undefined` and every agent card reported "未接入" (inactive) even after a
/// successful activation + disk sync. `version` / `providers` are single words,
/// so dropping the attribute leaves them unchanged.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlatProvidersResponse {
    pub version: u32,
    pub providers: Vec<ProviderEntryFlat>,
    pub tool_activations: std::collections::HashMap<String, ToolBinding>,
}

/// Result of updating a flat provider, including tool re-sync outcomes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUpdateFlatResult {
    pub provider: ProviderEntryFlat,
    pub tool_sync_results: Vec<ToolSyncResultFlat>,
}
