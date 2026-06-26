//! Data types and managed-field constants for tool-config sync.

use serde::{Deserialize, Serialize};

use crate::providers::ProviderEntryFlat;

// ---------------------------------------------------------------------------
// Per-tool typed settings helpers
// ---------------------------------------------------------------------------

/// Typed accessor for Codex-specific settings stored in `ToolActivation.settings`.
///
/// `auth_mode` is a three-state value (see `CODEX_AUTH_MODE_*` constants):
/// - `"api_key"` — official OpenAI API key; written to `auth.json` as
///   `OPENAI_API_KEY`.
/// - `"oauth"` — ChatGPT OAuth login; `auth.json` is **never touched** so the
///   existing ChatGPT token survives. `requires_openai_auth = true`.
/// - `"third_party"` — a third-party OpenAI-compatible endpoint. The key is
///   delivered to Codex via `env_key` (the user exports it in their shell
///   profile); `auth.json` is **never touched** so a concurrent ChatGPT OAuth
///   login stays valid. `requires_openai_auth = false`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodexSettings {
    #[serde(default = "default_wire_api")]
    pub wire_api: String,
    #[serde(default = "default_auth_mode")]
    pub auth_mode: String,
}

/// Auth-mode sentinel: official OpenAI API key (written to `auth.json`).
pub const CODEX_AUTH_MODE_API_KEY: &str = "api_key";
/// Auth-mode sentinel: ChatGPT OAuth login (`auth.json` preserved untouched).
pub const CODEX_AUTH_MODE_OAUTH: &str = "oauth";
/// Auth-mode sentinel: third-party API via `env_key` (`auth.json` preserved).
pub const CODEX_AUTH_MODE_THIRD_PARTY: &str = "third_party";

fn default_wire_api() -> String {
    "responses".to_string()
}
fn default_auth_mode() -> String {
    CODEX_AUTH_MODE_API_KEY.to_string()
}

impl CodexSettings {
    /// Parse from a generic `Value`, filling in defaults for missing fields.
    pub fn from_value(value: &serde_json::Value) -> Self {
        serde_json::from_value(value.clone()).unwrap_or_default()
    }

    /// True when this activation should keep a ChatGPT OAuth token intact in
    /// `auth.json` (i.e. neither mode writes `OPENAI_API_KEY`).
    pub fn preserves_oauth_token(&self) -> bool {
        matches!(
            self.auth_mode.as_str(),
            CODEX_AUTH_MODE_OAUTH | CODEX_AUTH_MODE_THIRD_PARTY
        )
    }

    /// Whether the Codex provider table should carry `requires_openai_auth`.
    /// Only the two "official identity" modes do; `third_party` routes through
    /// `env_key` instead.
    pub fn requires_openai_auth(&self) -> bool {
        matches!(
            self.auth_mode.as_str(),
            CODEX_AUTH_MODE_API_KEY | CODEX_AUTH_MODE_OAUTH
        )
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
// Typed Codex `[model_providers.*]` table
// ---------------------------------------------------------------------------

/// The typed shape of a Codex `[model_providers.<id>]` table, replacing the
/// previous hand-built `toml::Table::insert` sequence. Serializing this through
/// `to_toml_table()` is the single source of truth for what gets written to
/// `~/.codex/config.toml`.
///
/// `env_key` is only populated in `third_party` auth mode; it is omitted from
/// the serialized table otherwise (Codex treats a missing `env_key` as
/// "use the official auth path").
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodexModelProvider {
    pub name: String,
    pub base_url: String,
    /// `"responses"` (Codex native) or `"chat"` (OpenAI-compatible `/v1/chat/completions`).
    pub wire_api: String,
    /// Mirrors Codex's `requires_openai_auth` flag.
    pub requires_openai_auth: bool,
    /// Environment variable name Codex reads the API key from. Only set for
    /// `third_party` mode.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub env_key: Option<String>,
}

impl CodexModelProvider {
    /// Build the provider table from an activation + resolved settings.
    pub fn from_activation(
        provider: &ProviderEntryFlat,
        settings: &CodexSettings,
    ) -> Self {
        let env_key = if settings.auth_mode == CODEX_AUTH_MODE_THIRD_PARTY {
            Some(codex_env_key_for(provider))
        } else {
            None
        };
        Self {
            name: "SkillStar".to_string(),
            base_url: provider.base_url_openai.clone(),
            wire_api: settings.wire_api.clone(),
            requires_openai_auth: settings.requires_openai_auth(),
            env_key,
        }
    }

    /// Serialize into the `toml::Table` shape written under
    /// `[model_providers.<managed_key>]`.
    pub fn to_toml_table(&self) -> toml::Table {
        let mut table = toml::Table::new();
        table.insert("name".to_string(), toml::Value::String(self.name.clone()));
        table.insert(
            "base_url".to_string(),
            toml::Value::String(self.base_url.clone()),
        );
        table.insert(
            "wire_api".to_string(),
            toml::Value::String(self.wire_api.clone()),
        );
        table.insert(
            "requires_openai_auth".to_string(),
            toml::Value::Boolean(self.requires_openai_auth),
        );
        if let Some(env_key) = &self.env_key {
            table.insert("env_key".to_string(), toml::Value::String(env_key.clone()));
        }
        table
    }
}

/// Derive a stable, collision-resistant env var name for a provider's API key.
///
/// Rule: `SKILLSTAR_<UPPER_PREFIX>_KEY` where `<prefix>` is the first 8 chars
/// of the provider id, uppercased and reduced to `[A-Z0-9_]`. Two providers
/// therefore never share an env var (UUIDv4 prefix collision is negligible),
/// and the name is filesystem/shell-safe.
pub fn codex_env_key_for(provider: &ProviderEntryFlat) -> String {
    let raw_prefix = provider.id.chars().take(8).collect::<String>();
    let safe: String = raw_prefix
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect();
    let safe = if safe.is_empty() { "PROVIDER".to_string() } else { safe };
    format!("SKILLSTAR_{safe}_KEY")
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
pub(crate) const GEMINI_MANAGED_ENV_KEYS: &[&str] =
    &["GOOGLE_GEMINI_BASE_URL", "GEMINI_API_KEY", "GEMINI_MODEL"];
