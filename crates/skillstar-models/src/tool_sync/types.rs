//! Data types and managed-field constants for tool-config sync.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Per-tool typed settings helpers
// ---------------------------------------------------------------------------

/// Typed accessor for Codex-specific settings stored in `ToolActivation.settings`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodexSettings {
    #[serde(default = "default_wire_api")]
    pub wire_api: String,
    #[serde(default = "default_auth_mode")]
    pub auth_mode: String,
}

fn default_wire_api() -> String { "responses".to_string() }
fn default_auth_mode() -> String { "api_key".to_string() }

impl CodexSettings {
    /// Parse from a generic `Value`, filling in defaults for missing fields.
    pub fn from_value(value: &serde_json::Value) -> Self {
        serde_json::from_value(value.clone()).unwrap_or_default()
    }
}

impl Default for CodexSettings {
    fn default() -> Self {
        Self {
            wire_api: default_wire_api(),
            auth_mode: default_auth_mode(),
        }
    }
}

// ---------------------------------------------------------------------------
// Config conflict detection types
// ---------------------------------------------------------------------------

/// Describes a detected configuration conflict that may affect tool sync.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfigConflict {
    /// The type of conflict detected.
    pub conflict_type: ConflictType,
    /// Human-readable description of the conflict.
    pub description: String,
    /// The file path involved in the conflict, if applicable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    /// Additional details (e.g., which env var, what value was found).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    /// The tool this conflict pertains to (set for tool-specific conflicts like
    /// external modification). `None` for global conflicts like env overrides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_id: Option<String>,
}

/// The type of configuration conflict.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConflictType {
    /// The config file was modified externally since our last sync write.
    ExternalModification,
    /// A legacy `~/.claude.json` file exists with conflicting env fields.
    LegacyConfig,
    /// A shell environment variable overrides config file settings.
    EnvVarOverride,
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Describes an external tool's config file target.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolConfigTarget {
    pub tool_id: String,
    pub display_name: String,
    pub config_path: String,
    pub exists: bool,
    pub current_provider: Option<String>,
}

/// Result of syncing a provider to a single tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSyncResult {
    pub tool_id: String,
    pub success: bool,
    pub error: Option<String>,
    pub config_path: String,
    pub backup_path: Option<String>,
}

/// Result of syncing a provider to a single tool using the flat store format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSyncResultFlat {
    pub tool_id: String,
    pub success: bool,
    pub config_path: Option<String>,
    pub error: Option<String>,
    pub backup_path: Option<String>,
}

/// A single on-disk config file belonging to an agent tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolConfigFileInfo {
    pub file_id: String,
    pub label: String,
    pub path: String,
    /// `"json"` or `"toml"`
    pub format: String,
    pub exists: bool,
    pub managed_by_skillstar: bool,
}

/// Result of writing a tool config file from the UI editor.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteToolConfigFileResult {
    pub success: bool,
    pub backup_path: Option<String>,
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------
// Constants: managed field names
// ---------------------------------------------------------------------------

/// Fields managed by SkillStar in Claude Code's `~/.claude/settings.json` env block.
pub(crate) const CLAUDE_MANAGED_ENV_KEYS: &[&str] = &[
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
];

/// The key for the model_providers section managed by SkillStar in Codex's config.toml.
pub(crate) const CODEX_MANAGED_PROVIDER_KEY: &str = "skillstar";

/// Provider block key under `opencode.json` → `provider`.
pub(crate) const OPENCODE_MANAGED_PROVIDER_KEY: &str = "skillstar";

/// Fields managed by SkillStar in Gemini CLI's `~/.gemini/.env` file.
pub(crate) const GEMINI_MANAGED_ENV_KEYS: &[&str] = &[
    "GOOGLE_GEMINI_BASE_URL",
    "GEMINI_API_KEY",
    "GEMINI_MODEL",
];
