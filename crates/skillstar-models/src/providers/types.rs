//! Provider data types: store models, flat-store entries, and patches.

use super::*;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A single model mapping entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelMapping {
    pub source_model: String,
    pub target_model: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// Provider settings (nested inside ProviderEntry.settings_config).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderSettings {
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    pub models: Vec<ModelMapping>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<u32>,
}

/// A single named provider entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderEntry {
    pub id: String,
    pub name: String,
    pub category: String,
    pub settings_config: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub website_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort_index: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// Per-app provider collection.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppProviders {
    pub providers: HashMap<String, ProviderEntry>,
    pub current: Option<String>,
}

/// Root structure stored in model_providers.json.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProvidersStore {
    #[serde(default)]
    pub claude: AppProviders,
    #[serde(default)]
    pub codex: AppProviders,
    #[serde(default)]
    pub opencode: AppProviders,
    #[serde(default)]
    pub gemini: AppProviders,
}

/// A built-in provider preset template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderPreset {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub api_key_url: String,
    pub icon_color: String,
    pub models: Vec<String>,
}

// ---------------------------------------------------------------------------
// Flat provider store types (v2 architecture)
// ---------------------------------------------------------------------------

/// Current on-disk schema version of the flat provider store.
///
/// - v2 stored one `Option<ToolActivation>` per tool (single provider per agent).
/// - v3 stores a [`ToolBinding`] per tool — an ordered list of provider+model
///   entries with an `active_index` pointer. Single-provider agents
///   (claude-code, gemini) keep 0 or 1 entry; multi-provider agents
///   (codex, opencode) may hold several.
pub const FLAT_STORE_VERSION: u32 = 3;

/// Root structure for the flat provider store (`model_providers.json`).
///
/// Stores providers as a flat array with a separate `tool_activations` map
/// that records which providers + models each Agent tool is bound to.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlatProvidersStore {
    pub version: u32,
    pub providers: Vec<ProviderEntryFlat>,
    #[serde(default)]
    pub tool_activations: HashMap<String, ToolBinding>,
}

impl Default for FlatProvidersStore {
    fn default() -> Self {
        Self {
            version: FLAT_STORE_VERSION,
            providers: Vec::new(),
            tool_activations: HashMap::new(),
        }
    }
}

/// A single provider entry in the flat store.
///
/// Each provider has dual base URLs: one for OpenAI-compatible endpoints and
/// one for Anthropic-compatible endpoints. This allows a single provider config
/// to serve both Claude Code (Anthropic format) and Codex (OpenAI format).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderEntryFlat {
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub base_url_openai: String,
    #[serde(default)]
    pub base_url_anthropic: String,
    /// Unique "fetch available models" endpoint for this provider.
    ///
    /// All agent configurations (Claude, Codex, …) call this same URL to
    /// populate their model pickers. Typically an OpenAI-compatible
    /// `.../v1/models` endpoint — the response is parsed as
    /// `{ "data": [{ "id": "<model>" }] }`.
    #[serde(default)]
    pub models_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub default_model: String,
    #[serde(default)]
    pub sort_index: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Value>,
    /// Codex API format: `"responses"` (default) or `"chat"`.
    #[serde(default = "default_codex_wire_api")]
    pub codex_wire_api: String,
    /// Codex auth mode: `"api_key"` (default, writes `OPENAI_API_KEY`),
    /// `"oauth"` (preserves ChatGPT OAuth token), or `"third_party"` (routes
    /// the key through `env_key` so a ChatGPT OAuth login can coexist).
    #[serde(default = "default_codex_auth_mode")]
    pub codex_auth_mode: String,
}

pub(crate) fn default_codex_wire_api() -> String {
    "responses".to_string()
}
pub(crate) fn default_codex_auth_mode() -> String {
    "api_key".to_string()
}

/// Normalized model metadata cached under `ProviderEntryFlat.meta.model_catalog`.
///
/// The shape intentionally mirrors the fields OpenCode can use directly while
/// remaining tolerant of upstream registries such as CLIProxyAPI and models.dev.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ModelCatalogEntry {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_length: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_completion_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<Value>,
}

/// Result returned by the model-catalog discovery command.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ModelCatalogFetchResult {
    pub models: Vec<String>,
    pub catalog: Vec<ModelCatalogEntry>,
    #[serde(default)]
    pub metadata_sources: Vec<String>,
    pub missing_cost_count: usize,
}

/// A single provider+model binding entry for an Agent tool.
///
/// One entry = one provider the agent can use, plus the model selected for it
/// and any per-entry tool settings (e.g. Codex `wire_api` / `auth_mode`).
/// Single-provider agents (claude-code, gemini) only ever hold one of these;
/// multi-provider agents (codex, opencode) may hold several, all written to the
/// agent's config file as parallel managed entries.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolActivation {
    pub provider_id: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<serde_json::Value>,
    /// Unix seconds of the last successful disk sync for this tool. Baseline for
    /// external-modification conflict detection. `None` until the first sync.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_sync_at: Option<u64>,
}

/// All provider+model bindings for one Agent tool, plus which one is active.
///
/// `entries` is the ordered list of providers bound to this agent. `active_index`
/// points at the entry that owns the agent's "active" pointer on disk (Codex
/// `model_provider`, OpenCode top-level `model`). For single-provider agents the
/// list never exceeds one entry, so `active_index` is always 0. An empty
/// `entries` means the tool is not bound to anything (the v2 `None` state).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ToolBinding {
    #[serde(default)]
    pub entries: Vec<ToolActivation>,
    #[serde(default)]
    pub active_index: usize,
}

impl ToolBinding {
    /// A binding holding exactly one entry (the single-provider agent shape).
    pub fn single(entry: ToolActivation) -> Self {
        Self {
            entries: vec![entry],
            active_index: 0,
        }
    }

    /// The active entry, clamping a stale `active_index` to the last entry.
    pub fn active(&self) -> Option<&ToolActivation> {
        if self.entries.is_empty() {
            return None;
        }
        let idx = self.active_index.min(self.entries.len() - 1);
        self.entries.get(idx)
    }

    /// Mutable access to the active entry (clamped like [`active`]).
    pub fn active_mut(&mut self) -> Option<&mut ToolActivation> {
        if self.entries.is_empty() {
            return None;
        }
        let idx = self.active_index.min(self.entries.len() - 1);
        self.entries.get_mut(idx)
    }

    /// Whether any entry binds the given provider.
    pub fn binds_provider(&self, provider_id: &str) -> bool {
        self.entries.iter().any(|e| e.provider_id == provider_id)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Partial update patch for a flat provider entry.
///
/// All fields are optional — only non-None fields are applied during update.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ProviderPatchFlat {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url_openai: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url_anthropic: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub models_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub models: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort_index: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex_wire_api: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex_auth_mode: Option<String>,
}

/// Partial update patch for a provider.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings_config: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub website_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort_index: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}
