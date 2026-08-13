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
// OMP model roles
// ---------------------------------------------------------------------------

/// OMP's built-in model roles, in the order its own selector presents them.
///
/// Mirrors `MODEL_ROLE_IDS` in `@oh-my-pi/pi-coding-agent`
/// (`src/config/model-roles.ts`). OMP routes each request to the model bound to
/// the role that fits the task: `default` for normal turns, `smol` for cheap
/// sub-agent fan-out, `slow` for deep reasoning, `plan` for planning mode, and
/// so on. `modelRoles` on disk is an open string map, so users may add their own
/// roles too — this list only pins the ones we surface in the UI.
pub const OMP_MODEL_ROLES: &[&str] = &[
    "default", "smol", "slow", "plan", "vision", "designer", "commit", "tiny", "task", "advisor",
];

/// Thinking levels OMP accepts as the `:suffix` on a role value
/// (`provider/model:xhigh`), matching its `--thinking` flag and
/// `ThinkingLevel` in `@oh-my-pi/pi-agent-core`.
///
/// `inherit` defers to the global `defaultThinkingLevel`, which is also what
/// omitting the suffix entirely does; `auto` lets OMP pick per turn. Note that
/// OMP resolves a literal model id ending in `:max` *before* reading `:max` as a
/// thinking level, so a suffix never shadows a real model.
pub const OMP_THINKING_LEVELS: &[&str] = &[
    "inherit", "off", "minimal", "low", "medium", "high", "xhigh", "max", "auto",
];

/// One role → provider+model assignment.
///
/// `provider_id` is a SkillStar provider id, *not* the on-disk `skillstar_*`
/// key — the key is derived at write time so it always tracks
/// `skillstar_managed_key`. A role may point at any bound provider, which is why
/// this lives on [`ToolBinding::settings`] rather than a single entry's.
///
/// [`ToolBinding::settings`]: crate::providers::ToolBinding::settings
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct OmpRoleTarget {
    pub provider_id: String,
    pub model: String,
    /// Optional thinking level appended as `:level`. Must be one of
    /// [`OMP_THINKING_LEVELS`]; anything else is dropped at write time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
}

impl OmpRoleTarget {
    /// Render the on-disk `modelRoles` value: `<managed_key>/<model>[:thinking]`.
    ///
    /// Returns `None` when the target is incomplete (no model) — an incomplete
    /// role must not overwrite whatever the user already has on disk.
    pub fn to_role_value(&self, managed_key: &str) -> Option<String> {
        let model = self.model.trim();
        if model.is_empty() {
            return None;
        }
        let mut value = format!("{managed_key}/{model}");
        if let Some(level) = self.thinking.as_deref().map(str::trim)
            && OMP_THINKING_LEVELS.contains(&level)
        {
            value.push(':');
            value.push_str(level);
        }
        Some(value)
    }
}

/// Typed accessor for OMP-specific tool-level settings stored in
/// `ToolBinding.settings`.
///
/// Only `roles` lives here today. Roles the user has not assigned are simply
/// absent from the map, and absent roles are never written to `config.yml` —
/// OMP falls back to `default` on its own, and untouched roles stay under the
/// user's control.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct OmpSettings {
    #[serde(default)]
    pub roles: std::collections::BTreeMap<String, OmpRoleTarget>,
}

impl OmpSettings {
    /// Parse from a generic `Value`, falling back to empty on any mismatch
    /// (mirrors [`CodexSettings::from_value`]).
    pub fn from_value(value: &serde_json::Value) -> Self {
        serde_json::from_value(value.clone()).unwrap_or_default()
    }

    /// Read the roles out of a binding's tool-level settings bag.
    pub fn from_binding(binding: &crate::providers::ToolBinding) -> Self {
        binding
            .settings
            .as_ref()
            .map(Self::from_value)
            .unwrap_or_default()
    }
}

/// Whether a role name is safe to write into `modelRoles`.
///
/// OMP's schema is an open string map, so custom roles are allowed, but a name
/// containing `/` or whitespace would corrupt the `provider/model` grammar that
/// role *values* use, and an `@`-prefixed name collides with OMP's role-alias
/// syntax (`@smol`).
pub fn is_valid_omp_role_name(role: &str) -> bool {
    !role.is_empty()
        && role.len() <= 64
        && !role.starts_with('@')
        && role
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
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
    pub fn from_activation(provider: &ProviderEntryFlat, settings: &CodexSettings) -> Self {
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
    let safe = if safe.is_empty() {
        "PROVIDER".to_string()
    } else {
        safe
    };
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

/// Result of syncing a provider to a single tool using the flat store format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSyncResultFlat {
    pub tool_id: String,
    pub success: bool,
    pub config_path: Option<String>,
    pub error: Option<String>,
    pub backup_path: Option<String>,
}

impl ToolSyncResultFlat {
    /// Fold a writer outcome into the flat result shape shared by every
    /// agent: success carries the backup path, failure the error string;
    /// both carry the resolved config path.
    pub(crate) fn from_write_outcome(
        tool_id: &str,
        config_path: &std::path::Path,
        outcome: anyhow::Result<Option<std::path::PathBuf>>,
    ) -> Self {
        let config_path = Some(config_path.to_string_lossy().to_string());
        match outcome {
            Ok(backup_path) => Self {
                tool_id: tool_id.to_string(),
                success: true,
                config_path,
                error: None,
                backup_path: backup_path.map(|p| p.to_string_lossy().to_string()),
            },
            Err(e) => Self {
                tool_id: tool_id.to_string(),
                success: false,
                config_path,
                error: Some(e.to_string()),
                backup_path: None,
            },
        }
    }

    /// Failure before a config path could be resolved.
    pub(crate) fn failed_without_path(tool_id: &str, error: impl std::fmt::Display) -> Self {
        Self {
            tool_id: tool_id.to_string(),
            success: false,
            config_path: None,
            error: Some(error.to_string()),
            backup_path: None,
        }
    }
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
