//! What the v3 → v4 migration did, in a form the UI can show the user.
//!
//! Migration changes a user's configuration. Two of those changes are things a
//! user would reasonably object to if they happened silently:
//!
//! - **Backfilling an endpoint** the user never typed (§ the three-condition
//!   rule in [`super::v3_to_v4`]).
//! - **Dropping bindings** that can no longer work — Claude Desktop, and any
//!   Codex binding to a host that does not implement the Responses API.
//!
//! So the migration is required to report both, the UI is required to show the
//! report in a modal rather than a toast, and the permanent `model_providers.v3.json`
//! backup is what makes the "undo" button on that modal honest.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Everything the v3 → v4 migration changed that a user might want to contest.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, TS)]
#[ts(export, export_to = "MigrationReport.ts")]
pub struct MigrationReport {
    /// Providers whose `anthropic_messages` endpoint was filled in from their
    /// preset because all three backfill conditions held.
    #[serde(default)]
    pub backfilled_anthropic: Vec<BackfilledEndpoint>,
    /// Providers whose `models_list` endpoint was filled in the same way.
    #[serde(default)]
    pub backfilled_models_list: Vec<BackfilledEndpoint>,
    /// Bindings dropped wholesale, with the reason.
    #[serde(default)]
    pub dropped_bindings: Vec<DroppedBinding>,
    /// Codex entries that cannot be written because the host does not offer a
    /// Responses endpoint. These are *recorded, not deleted*: probing may show
    /// the host supports it after all, and the user can restore from here.
    #[serde(default)]
    pub codex_dropped: Vec<DroppedBinding>,
    /// Providers whose catalog moved out of the store into the cache
    /// directory, with how many rows each carried.
    #[serde(default)]
    pub externalized_catalogs: Vec<ExternalizedCatalog>,
    /// v3 `meta` keys that had no v4 home and were parked in `Provider.ext`.
    /// Listed so an unexpected key shows up in review instead of vanishing.
    #[serde(default)]
    pub preserved_ext_keys: Vec<String>,
    /// Non-fatal problems. A warning never aborts the migration.
    #[serde(default)]
    pub warnings: Vec<String>,
}

impl MigrationReport {
    /// Whether anything happened that is worth interrupting the user for.
    ///
    /// Catalog externalisation and preserved ext keys deliberately do not
    /// count: the first is invisible to the user and the second is a
    /// no-data-lost note for developers.
    pub fn needs_user_attention(&self) -> bool {
        !self.backfilled_anthropic.is_empty()
            || !self.backfilled_models_list.is_empty()
            || !self.dropped_bindings.is_empty()
            || !self.codex_dropped.is_empty()
    }
}

/// One endpoint that migration filled in on the user's behalf.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export, export_to = "BackfilledEndpoint.ts")]
pub struct BackfilledEndpoint {
    pub provider_id: String,
    pub provider_name: String,
    pub preset_id: String,
    /// The value written in. Shown so the user can see exactly what changed.
    pub url: String,
}

/// One binding migration could not carry forward.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export, export_to = "DroppedBinding.ts")]
pub struct DroppedBinding {
    pub agent_id: String,
    pub provider_id: String,
    pub provider_name: String,
    pub model: String,
    pub reason: DropReason,
}

/// Why a binding was dropped. An enum rather than a message string because the
/// user-facing wording is the frontend's job — the backend does not produce
/// display copy.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "DropReason.ts")]
pub enum DropReason {
    /// Claude Desktop moved to the planned-but-unimplemented list. Its previous
    /// binding never produced an effect on disk, but the user did click it, so
    /// it gets named rather than deleted in silence.
    AgentPlannedNotImplemented,
    /// The host offers no `/v1/responses` endpoint, and Codex ≥0.95 speaks
    /// nothing else.
    CodexRequiresResponsesApi,
    /// The binding referenced a provider id that is not in the store.
    ProviderMissing,
}

/// One provider's catalog, lifted out of the store file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export, export_to = "ExternalizedCatalog.ts")]
pub struct ExternalizedCatalog {
    pub provider_id: String,
    #[ts(type = "number")]
    pub entry_count: u64,
}
