//! Tauri commands for Models mode — Provider CRUD, activation, and tool sync.
//!
//! All write operations are serialized through a tokio Mutex to prevent
//! concurrent corruption of `model_providers.json`.
//!
//! ## Architecture
//!
//! This module contains two generations of commands:
//!
//! - **Legacy (per-app)**: `get_providers_store`, `create_provider`, etc.
//!   These operate on the v1 per-app `ProvidersStore` format and are retained
//!   for backward compatibility during the transition period.
//!
//! - **Flat store (v2)**: `get_providers_flat`, `create_provider_flat`, etc.
//!   These operate on the new flat `FlatProvidersStore` format with a unified
//!   provider list and `tool_activations` map.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::State;
use tokio::sync::Mutex;

use skillstar_ai::ai_provider;
use skillstar_models::latency::{self, EndpointLatencyResult, LatencyResult};
use skillstar_models::providers::ProviderPresetFlat;
use skillstar_models::provider_ref::AiProviderRef;
use skillstar_models::providers::{
    self, AppProviders, ModelCatalogFetchResult, ProviderEntry, ProviderEntryFlat, ProviderPatch,
    ProviderPatchFlat, ProviderPreset, ProviderSettings, ProvidersStore, ToolActivation,
};
use skillstar_models::tool_sync::{self, ToolConfigTarget, ToolSyncResult, ToolSyncResultFlat};

// ---------------------------------------------------------------------------
// Submodules (mechanical split — commands re-exported so `models_commands::NAME`
// resolves exactly as before).
// ---------------------------------------------------------------------------

mod diagnostics;
mod provider_cmds;
mod tools;

pub use diagnostics::*;
pub use provider_cmds::*;
pub use tools::*;

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

/// Result of switching the active provider (includes optional tool sync results).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchResult {
    pub app_id: String,
    pub provider_id: String,
    pub provider_name: String,
    pub tools_synced: Vec<ToolSyncResult>,
}

/// Response for `get_providers_flat` — returns the full flat store contents.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlatProvidersResponse {
    pub version: u32,
    pub providers: Vec<ProviderEntryFlat>,
    pub tool_activations: std::collections::HashMap<String, Option<ToolActivation>>,
}

/// Result of updating a flat provider, including tool re-sync outcomes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUpdateFlatResult {
    pub provider: ProviderEntryFlat,
    pub tool_sync_results: Vec<ToolSyncResultFlat>,
}

// ---------------------------------------------------------------------------
// Connection test command (minimal chat completion request)
// ---------------------------------------------------------------------------

/// Result of a provider connection test using a minimal chat completion request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionTestResult {
    /// `"ok"`, `"auth_failed"`, `"timeout"`, `"network_error"`, `"model_unavailable"`
    pub status: String,
    /// Round-trip latency in milliseconds (only present when status is "ok").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    /// Error description (present for non-ok statuses).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
