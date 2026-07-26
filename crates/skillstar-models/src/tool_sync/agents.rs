//! Agent registry — the single source of truth for per-agent facts.
//!
//! One [`AgentSpec`] row per model-workbench agent (mirroring the frontend
//! `agentRegistry.ts`): identity, install probes, config-file inventory,
//! binding kind, and the per-agent dispatch columns (`sync_binding` /
//! `unsync` / `detect_provider`). The tool-sync dispatch sites
//! ([`sync_tool_binding`], [`unsync_tool`], [`resync_active_tools`],
//! [`get_tool_config_targets`], path/file resolution) all consult this table;
//! adding an agent means adding one row here plus its writer functions. The
//! consistency tests below reconcile every row against the store-layer kind
//! decision and the frontend-pinned literals.

use super::*;
use crate::providers::ToolBinding;

/// Whether the agent's config format natively holds several providers.
///
/// Mirrors `providers::crud::agent_supports_multiple_providers` (the store
/// layer's kind decision point) and the frontend descriptor `kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentKind {
    /// One global credential block; activating replaces it wholesale.
    Single,
    /// Config natively lists providers + an active pointer (`skillstar_*` blocks).
    Multi,
}

/// Which provider URL field the agent requires at activation time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequiredUrl {
    /// `base_url_anthropic` must be non-empty (Claude Code).
    Anthropic,
    /// `base_url_openai` must be non-empty (everything else).
    Openai,
}

/// One on-disk config file belonging to an agent.
pub struct AgentConfigFileSpec {
    /// Stable id used by the config-file editor commands (`file_id`).
    pub file_id: &'static str,
    /// Human label shown by the editor UI (usually the file name).
    pub label: &'static str,
    /// `"json"`, `"toml"` or `"env"`.
    pub format: &'static str,
    /// Sandbox-aware path resolver (funnels through `sync_home_dir`).
    pub resolve: fn() -> Result<PathBuf>,
    /// Skeleton content served when the file does not exist yet.
    pub default_content: &'static str,
}

/// Registry row: every per-agent fact the tool-sync layer needs.
pub struct AgentSpec {
    /// Canonical tool id (`claude-code`, `codex`, …). Shared with the frontend.
    pub id: &'static str,
    /// Display name for config-target listings.
    pub display_name: &'static str,
    /// CLI binary probed on the enriched PATH by installation detection.
    pub binary_name: &'static str,
    /// Home-relative directories whose existence signals prior configuration
    /// (joined with `/`; an agent may have several historical locations).
    pub config_dir_probes: &'static [&'static str],
    pub kind: AgentKind,
    pub required_url: RequiredUrl,
    /// Config-file inventory. The first entry is the agent's primary config
    /// file (the one `resolve_tool_config_path` returns).
    pub files: &'static [AgentConfigFileSpec],
    /// Project a whole [`ToolBinding`] onto disk. Single-provider agents
    /// resolve the active entry; multi-provider agents write every entry.
    pub sync_binding: fn(&ToolBinding, &[ProviderEntryFlat]) -> Result<ToolSyncResultFlat>,
    /// Remove every SkillStar-managed field/entry from the agent's configs.
    pub unsync: fn() -> Result<()>,
    /// Read an *existing* primary config file and report a human-readable
    /// hint (base URL / name) for the currently wired provider.
    pub detect_provider: fn(&Path) -> Result<Option<String>>,
}

/// `~/.claude/settings.json` — named resolver for the registry (the legacy
/// `resolve_tool_config_path` match inlines the same joins; the consistency
/// tests reconcile the two).
fn resolve_claude_settings_path() -> Result<PathBuf> {
    let home = sync_home_dir()?;
    Ok(home.join(".claude").join("settings.json"))
}

/// The registry. Order is the canonical presentation order used by
/// `get_tool_config_targets`.
static AGENT_SPECS: &[AgentSpec] = &[
    AgentSpec {
        id: "claude-code",
        display_name: "Claude Code",
        binary_name: "claude",
        config_dir_probes: &[".claude"],
        kind: AgentKind::Single,
        required_url: RequiredUrl::Anthropic,
        files: &[AgentConfigFileSpec {
            file_id: "settings",
            label: "settings.json",
            format: "json",
            resolve: resolve_claude_settings_path,
            default_content: "{\n  \"env\": {}\n}\n",
        }],
        sync_binding: sync_claude_code_binding,
        unsync: unsync_claude_code,
        detect_provider: detect_claude_code_provider,
    },
    AgentSpec {
        id: "codex",
        display_name: "Codex",
        binary_name: "codex",
        config_dir_probes: &[".codex"],
        kind: AgentKind::Multi,
        required_url: RequiredUrl::Openai,
        files: &[
            AgentConfigFileSpec {
                file_id: "config",
                label: "config.toml",
                format: "toml",
                resolve: resolve_codex_config_path,
                default_content: "model_provider = \"skillstar\"\nmodel = \"\"\n\n[model_providers.skillstar]\nname = \"SkillStar\"\nbase_url = \"\"\nwire_api = \"responses\"\nrequires_openai_auth = true\n",
            },
            AgentConfigFileSpec {
                file_id: "auth",
                label: "auth.json",
                format: "json",
                resolve: resolve_codex_auth_path,
                default_content: "{\n  \"OPENAI_API_KEY\": \"\"\n}\n",
            },
        ],
        sync_binding: sync_codex_binding,
        unsync: unsync_codex_all,
        detect_provider: detect_codex_provider,
    },
    AgentSpec {
        id: "opencode",
        display_name: "OpenCode",
        binary_name: "opencode",
        config_dir_probes: &[".config/opencode", ".opencode"],
        kind: AgentKind::Multi,
        required_url: RequiredUrl::Openai,
        files: &[AgentConfigFileSpec {
            file_id: "opencode",
            label: "opencode.json",
            format: "json",
            resolve: resolve_opencode_config_path,
            default_content: "{\n  \"$schema\": \"https://opencode.ai/config.json\",\n  \"provider\": {}\n}\n",
        }],
        sync_binding: sync_opencode_binding,
        unsync: unsync_opencode_all,
        detect_provider: detect_opencode_provider,
    },
    AgentSpec {
        id: "gemini",
        display_name: "Gemini CLI",
        binary_name: "gemini",
        config_dir_probes: &[".gemini"],
        kind: AgentKind::Single,
        required_url: RequiredUrl::Openai,
        files: &[AgentConfigFileSpec {
            file_id: "env",
            label: ".env",
            format: "env",
            resolve: resolve_gemini_env_path,
            default_content: "GOOGLE_GEMINI_BASE_URL=\nGEMINI_API_KEY=[密钥]",
        }],
        sync_binding: sync_gemini_binding,
        unsync: unsync_gemini,
        detect_provider: detect_gemini_provider,
    },
    AgentSpec {
        id: "pi",
        display_name: "Pi",
        binary_name: "pi",
        config_dir_probes: &[".pi"],
        kind: AgentKind::Multi,
        required_url: RequiredUrl::Openai,
        files: &[
            AgentConfigFileSpec {
                file_id: "models",
                label: "models.json",
                format: "json",
                resolve: resolve_pi_models_path,
                default_content: "{\n  \"providers\": {}\n}\n",
            },
            AgentConfigFileSpec {
                file_id: "settings",
                label: "settings.json",
                format: "json",
                resolve: resolve_pi_settings_path,
                default_content: "{}\n",
            },
        ],
        sync_binding: sync_pi_binding,
        unsync: unsync_pi_all,
        detect_provider: detect_pi_provider,
    },
];

/// All registered agents, in canonical presentation order.
pub fn agent_specs() -> &'static [AgentSpec] {
    AGENT_SPECS
}

/// Look up one agent's spec by tool id.
pub fn agent_spec(tool_id: &str) -> Option<&'static AgentSpec> {
    AGENT_SPECS.iter().find(|spec| spec.id == tool_id)
}

// ---------------------------------------------------------------------------
// Table-driven consistency tests: reconcile the registry against every legacy
// match site it will eventually replace. A failure here means someone changed
// one side without the other.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::agent_supports_multiple_providers;
    use std::collections::HashSet;

    #[test]
    fn registry_covers_exactly_the_known_agents_in_order() {
        let ids: Vec<&str> = agent_specs().iter().map(|s| s.id).collect();
        // Same literal set the frontend agentRegistry test pins.
        assert_eq!(ids, ["claude-code", "codex", "opencode", "gemini", "pi"]);
    }

    #[test]
    fn registry_rows_are_well_formed() {
        let mut seen = HashSet::new();
        for spec in agent_specs() {
            assert!(seen.insert(spec.id), "duplicate agent id {}", spec.id);
            assert!(!spec.display_name.is_empty());
            assert!(!spec.binary_name.is_empty());
            assert!(!spec.config_dir_probes.is_empty(), "{}", spec.id);
            assert!(!spec.files.is_empty(), "{} has no config files", spec.id);

            let mut file_ids = HashSet::new();
            for file in spec.files {
                assert!(
                    file_ids.insert(file.file_id),
                    "duplicate file_id {}/{}",
                    spec.id,
                    file.file_id
                );
                assert!(matches!(file.format, "json" | "toml" | "env"));
                assert!(!file.default_content.is_empty());
            }
        }
    }

    #[test]
    fn registry_paths_match_legacy_resolvers() {
        for spec in agent_specs() {
            // Primary file is what resolve_tool_config_path returns.
            let primary = (spec.files[0].resolve)().unwrap();
            assert_eq!(
                primary,
                resolve_tool_config_path(spec.id).unwrap(),
                "{}: primary file drifted from resolve_tool_config_path",
                spec.id
            );

            for file in spec.files {
                assert_eq!(
                    (file.resolve)().unwrap(),
                    resolve_tool_config_file_path(spec.id, file.file_id).unwrap(),
                    "{}/{}: path drifted from resolve_tool_config_file_path",
                    spec.id,
                    file.file_id
                );
            }
        }
    }

    #[test]
    fn registry_files_match_legacy_editor_listing() {
        for spec in agent_specs() {
            let listed = list_tool_config_files(spec.id).unwrap();
            assert_eq!(listed.len(), spec.files.len(), "{}", spec.id);
            for (info, file) in listed.iter().zip(spec.files) {
                assert_eq!(info.file_id, file.file_id, "{}", spec.id);
                assert_eq!(info.label, file.label, "{}", spec.id);
                assert_eq!(info.format, file.format, "{}", spec.id);
                assert!(info.managed_by_skillstar, "{}", spec.id);
            }
        }
    }

    #[test]
    fn registry_default_content_matches_legacy_editor_defaults() {
        for spec in agent_specs() {
            for file in spec.files {
                assert_eq!(
                    file.default_content,
                    default_empty_config_content(spec.id, file.file_id),
                    "{}/{}: default content drifted",
                    spec.id,
                    file.file_id
                );
            }
        }
    }

    #[test]
    fn registry_kind_matches_store_layer_decision_point() {
        for spec in agent_specs() {
            assert_eq!(
                matches!(spec.kind, AgentKind::Multi),
                agent_supports_multiple_providers(spec.id),
                "{}: kind drifted from agent_supports_multiple_providers",
                spec.id
            );
        }
    }

    #[test]
    fn registry_display_names_match_legacy_targets() {
        // get_tool_config_targets touches the filesystem, so pin the literals
        // its tool table hardcodes instead of invoking it here.
        let expected = [
            ("claude-code", "Claude Code"),
            ("codex", "Codex"),
            ("opencode", "OpenCode"),
            ("gemini", "Gemini CLI"),
            ("pi", "Pi"),
        ];
        for (id, name) in expected {
            assert_eq!(agent_spec(id).unwrap().display_name, name);
        }
    }

    #[test]
    fn required_url_pins_activation_validation_rules() {
        // Mirrors the match in providers::crud::activate_tool.
        assert_eq!(
            agent_spec("claude-code").unwrap().required_url,
            RequiredUrl::Anthropic
        );
        for id in ["codex", "opencode", "gemini", "pi"] {
            assert_eq!(agent_spec(id).unwrap().required_url, RequiredUrl::Openai);
        }
    }

    #[test]
    fn unknown_tool_id_has_no_spec() {
        assert!(agent_spec("zcode").is_none());
        assert!(agent_spec("").is_none());
    }
}
