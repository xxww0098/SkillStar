//! Data types and managed-field constants for tool-config sync.

use crate::providers::ModelRef;
use serde::{Deserialize, Serialize};

use crate::providers::{Provider, RequiredWire};

// ---------------------------------------------------------------------------
// Per-tool typed settings helpers
// ---------------------------------------------------------------------------

/// Typed accessor for Codex-specific settings stored in `BindingEntry.settings`.
///
/// ## Why `wire_api` is gone
///
/// v3 carried a `wire_api` here (and a matching column on the provider row)
/// because Codex once accepted two protocols and SkillStar had to say which.
/// Codex ≥0.95 removed `WireApi::Chat` from its enum, so the field encodes a
/// choice that no longer exists — and worse, the value SkillStar computed for
/// every third-party host was exactly the one Codex can no longer parse. The
/// real question was never "which protocol shall I ask for" but "does this host
/// implement the one protocol Codex has left", which is a fact about the host
/// and now lives on `Provider.endpoints.openai_responses`.
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
    #[serde(default = "default_auth_mode")]
    pub auth_mode: String,
}

/// Auth-mode sentinel: official OpenAI API key (written to `auth.json`).
pub const CODEX_AUTH_MODE_API_KEY: &str = "api_key";
/// Auth-mode sentinel: ChatGPT OAuth login (`auth.json` preserved untouched).
pub const CODEX_AUTH_MODE_OAUTH: &str = "oauth";
/// Auth-mode sentinel: third-party API via `env_key` (`auth.json` preserved).
pub const CODEX_AUTH_MODE_THIRD_PARTY: &str = "third_party";

fn default_auth_mode() -> String {
    CODEX_AUTH_MODE_API_KEY.to_string()
}

/// The only `wire_api` value Codex still accepts.
///
/// Written as a constant rather than a settable field so there is no code path
/// left that can put anything else in `config.toml`.
pub const CODEX_WIRE_API: &str = "responses";

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

/// Render a role target as OMP's on-disk `modelRoles` value:
/// `<managed_key>/<model>[:thinking]`.
///
/// Returns `None` for an incomplete target — a role with no model must not
/// overwrite whatever the user already has on disk.
///
/// This replaces v3's `OmpRoleTarget::to_role_value`. The type it took was
/// OMP-private even though every other agent needed the same triple; the
/// rendering stayed here because the `provider/model:level` grammar is OMP's,
/// but the data it renders is now the shared [`ModelRef`].
pub fn omp_role_value(target: &ModelRef, managed_key: &str) -> Option<String> {
    let model = target.model.trim();
    if model.is_empty() {
        return None;
    }
    let mut value = format!("{managed_key}/{model}");
    if let Some(effort) = target.effort {
        let level = effort.as_omp_thinking();
        if OMP_THINKING_LEVELS.contains(&level) {
            value.push(':');
            value.push_str(level);
        }
    }
    Some(value)
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
    /// Always [`CODEX_WIRE_API`]. Kept as a field because it is a field in
    /// Codex's own schema, not because there is a choice to make.
    pub wire_api: String,
    /// Mirrors Codex's `requires_openai_auth` flag.
    pub requires_openai_auth: bool,
    /// Environment variable name Codex reads the API key from. Only set for
    /// `third_party` mode.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub env_key: Option<String>,
}

impl CodexModelProvider {
    /// Build the provider table from a bound provider + resolved settings.
    ///
    /// `base_url` is the **Responses** endpoint, not the chat one. Codex only
    /// calls `/v1/responses`, so pointing it at a chat base URL produces a
    /// table that parses and then fails every request.
    pub fn from_binding(provider: &Provider, settings: &CodexSettings) -> Self {
        let env_key = if settings.auth_mode == CODEX_AUTH_MODE_THIRD_PARTY {
            Some(codex_env_key_for(&provider.id))
        } else {
            None
        };
        Self {
            name: "SkillStar".to_string(),
            base_url: provider
                .endpoint_for(RequiredWire::OpenaiResponses)
                .unwrap_or_default()
                .to_string(),
            wire_api: CODEX_WIRE_API.to_string(),
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
pub fn codex_env_key_for(provider_id: &str) -> String {
    let raw_prefix = provider_id.chars().take(8).collect::<String>();
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
