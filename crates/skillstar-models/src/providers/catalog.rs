//! v4 model catalog types: what a model *is*, and how one host sells it.
//!
//! ## Why two layers
//!
//! models.dev splits its data the same way and the split is load-bearing: a
//! model's context window and tool-calling ability are facts about the model
//! (`gpt-4o` has them wherever you buy it), while price, max output and the
//! wire protocol are facts about the *seller*. Collapsing the two means a
//! relay's price silently becomes "the model's price".
//!
//! ## Why this is not in `model_providers.json`
//!
//! v3 cached the whole upstream `/v1/models` response — `raw` included — inside
//! `provider.meta.model_catalog`, pretty-printed. A provider with several
//! hundred models therefore added hundreds of kilobytes to the file that holds
//! the user's credentials and bindings, with no cap and no trimming. In v4 the
//! catalog is derived data and lives in a cache directory; losing it costs a
//! refetch, not a configuration.
//!
//! WP-2 owns populating these types from the three-level source chain
//! (snapshot → models.dev → provider `/v1/models`). This module defines the
//! shape they agree on.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

/// One row of a provider's catalog: "how this host sells this model".
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export, export_to = "ModelEntry.ts")]
pub struct ModelEntry {
    /// The provider-side model id. This is what gets written to disk.
    pub id: String,
    pub display_name: String,
    /// Link to the shared [`ModelFacts`] row, when one was matched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_model: Option<String>,
    /// Objective lifecycle state.
    ///
    /// Kept separate from any "enabled" flag on purpose: status is a fact
    /// about the model and enablement is the user's intent. Only with both can
    /// the UI hide deprecated models *while keeping the one the user already
    /// selected* — merging them forces a choice between lying and nagging.
    #[serde(default)]
    pub status: ModelStatus,
    pub serving: Serving,
    #[serde(default)]
    pub facts: ModelFacts,
    /// Which level of the source chain this row's metadata came from. The UI is
    /// required to surface it.
    #[serde(default)]
    pub source: CatalogSource,
}

/// Lifecycle state of a model, as the registry reports it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "ModelStatus.ts")]
pub enum ModelStatus {
    #[default]
    Active,
    Alpha,
    Beta,
    Deprecated,
}

/// Where a catalog row's metadata came from.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "CatalogSource.ts")]
pub enum CatalogSource {
    /// L0 — the compile-time snapshot.
    Snapshot,
    /// L1 — models.dev at runtime.
    Registry,
    /// L2 — the provider's own `/v1/models`, which returns ids and little else.
    #[default]
    Discovered,
    /// L3 — typed in by the user.
    UserOverride,
}

/// "How this host sells it" — pricing and limits.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, TS)]
#[ts(export, export_to = "Serving.ts")]
pub struct Serving {
    /// Required by Crush, and needed by OpenCode to compute remaining context.
    #[ts(type = "number")]
    pub context: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(type = "number | null")]
    pub max_input: Option<u64>,
    /// Required by Crush (`default_max_tokens`).
    #[ts(type = "number")]
    pub max_output: u64,
    /// Prices in **USD per 1M tokens**, matching models.dev.
    ///
    /// The unit is in this doc comment because it has to be somewhere: Aider
    /// publishes USD *per token*, a factor of 1e6 away, and a reader with no
    /// stated unit has no way to notice the discrepancy.
    #[serde(default)]
    pub cost: Cost,
    /// Which protocol to use when writing this model.
    #[serde(default = "default_wire_shape")]
    pub wire: WireShape,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(type = "unknown | null")]
    pub extra_body: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra_headers: BTreeMap<String, String>,
}

fn default_wire_shape() -> WireShape {
    WireShape::OpenaiChat
}

/// The protocol a given model is reached through.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "WireShape.ts")]
pub enum WireShape {
    #[default]
    OpenaiChat,
    OpenaiResponses,
    AnthropicMessages,
}

/// Token pricing, USD per 1M tokens.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, TS)]
#[ts(export, export_to = "Cost.ts")]
pub struct Cost {
    pub input: f64,
    pub output: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<f64>,
    /// Volume tiers. Pi spells the threshold `inputTokensAbove`; the rename to
    /// `above_input_tokens` is handled in the writer, not here.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tiers: Vec<CostTier>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export, export_to = "CostTier.ts")]
pub struct CostTier {
    #[ts(type = "number")]
    pub above_input_tokens: u64,
    pub input: f64,
    pub output: f64,
}

/// "What the model is" — independent of where you buy it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, TS)]
#[ts(export, export_to = "ModelFacts.ts")]
pub struct ModelFacts {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub knowledge_cutoff: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_date: Option<String>,
    #[serde(default)]
    pub modalities_in: Vec<Modality>,
    #[serde(default)]
    pub modalities_out: Vec<Modality>,
    #[serde(default)]
    pub tool_call: bool,
    #[serde(default)]
    pub attachment: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured_output: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<bool>,
    #[serde(default)]
    pub reasoning: Reasoning,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "Modality.ts")]
pub enum Modality {
    Text,
    Image,
    Audio,
    Video,
    Pdf,
}

/// Reasoning capability — structured, so that the UI can tell which control to
/// draw.
///
/// A boolean cannot express this and a single global enum actively misleads:
/// v3 offered all nine OMP thinking levels for every model, including models
/// with no reasoning mode at all. Anthropic-family models take a token budget,
/// OpenAI-family models take an effort enum, and many take neither. Declaring
/// the *shape* here means the picker renders a budget slider, an effort
/// selector, or nothing, driven by data instead of by a hardcoded list.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export, export_to = "Reasoning.ts")]
pub enum Reasoning {
    #[default]
    None,
    /// On or off only.
    Toggle { can_disable: bool },
    /// Discrete tiers.
    Effort {
        values: Vec<super::binding::Effort>,
        default: Option<super::binding::Effort>,
        can_disable: bool,
    },
    /// A token budget.
    BudgetTokens {
        #[ts(type = "number | null")]
        min: Option<u32>,
        #[ts(type = "number | null")]
        max: Option<u32>,
        #[ts(type = "number | null")]
        default: Option<u32>,
    },
}
