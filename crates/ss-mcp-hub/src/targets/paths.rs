use crate::error::{McpHubError, McpHubResult};
use crate::model::{McpAgentTarget, McpFormat};
use ss_core::infra::paths::tool_sync_home_override;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Platform directory roots resolved once per operation.
/// When `SKILLSTAR_TOOL_SYNC_HOME` is active (`sandboxed = true`), every resolved path
/// stays strictly inside that directory and host PATH/App probes are suppressed.
#[derive(Debug, Clone)]
pub struct McpPathRoots {
    pub sandboxed: bool,
    pub home_dir: PathBuf,
    pub xdg_config_home: PathBuf,
    pub mac_app_support: PathBuf,
    pub win_app_data: PathBuf,
    pub win_local_app_data: PathBuf,
}

impl McpPathRoots {
    pub fn from_env() -> Self {
        if let Some(sandbox) = tool_sync_home_override() {
            return Self::from_sandbox(sandbox);
        }
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let xdg = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"));
        let mac_app_support = home.join("Library").join("Application Support");
        let win_app_data = std::env::var_os("APPDATA")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData").join("Roaming"));
        let win_local_app_data = std::env::var_os("LOCALAPPDATA")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData").join("Local"));
        Self {
            sandboxed: false,
            home_dir: home,
            xdg_config_home: xdg,
            mac_app_support,
            win_app_data,
            win_local_app_data,
        }
    }

    pub fn from_sandbox(sandbox: PathBuf) -> Self {
        Self {
            sandboxed: true,
            xdg_config_home: sandbox.join(".config"),
            mac_app_support: sandbox.join("Library").join("Application Support"),
            win_app_data: sandbox.join("AppData").join("Roaming"),
            win_local_app_data: sandbox.join("AppData").join("Local"),
            home_dir: sandbox,
        }
    }

    pub(super) fn env_override(&self, key: &str) -> Option<PathBuf> {
        let val = std::env::var_os(key).filter(|v| !v.is_empty())?;
        let candidate = PathBuf::from(val);
        if !self.sandboxed || candidate.starts_with(&self.home_dir) {
            Some(candidate)
        } else {
            None
        }
    }

    pub(super) fn detect_dir_or_file(&self, path: &Path, binary: Option<&str>, mac_app: Option<&str>) -> bool {
        if path.exists() || path.parent().is_some_and(|p| p.exists()) {
            return true;
        }
        if self.sandboxed {
            return false;
        }
        if let Some(bin) = binary
            && which::which(bin).is_ok()
        {
            return true;
        }
        #[cfg(target_os = "macos")]
        if let Some(app_name) = mac_app
            && (Path::new("/Applications").join(app_name).exists()
                || self.home_dir.join("Applications").join(app_name).exists())
        {
            return true;
        }
        #[cfg(not(target_os = "macos"))]
        let _ = mac_app;
        false
    }
}

/// Full runtime description for one canonical MCP target (strictly matching magpie `targets.go`).
#[derive(Debug, Clone)]
pub struct BuiltInTargetSpec {
    pub id: String,
    pub name: String,
    pub primary_path: PathBuf,
    pub also_paths: Vec<PathBuf>,
    pub claude_settings_path: Option<PathBuf>,
    pub seed_from_path: Option<PathBuf>,
    pub format: McpFormat,
    pub no_cmd_refs: bool,
    pub installed: bool,
    pub shared_here: bool,
    pub supports_tools_off: bool,
    pub supports_startup_timeout: bool,
    pub notes: Option<String>,
}

/// Normalize external or legacy agent ID aliases into canonical MCP target IDs (`agy` -> `antigravity`).
pub fn canonical_agent_id(raw: &str) -> &str {
    let trimmed = raw.trim();
    if trimmed.eq_ignore_ascii_case("agy") {
        "antigravity"
    } else if trimmed.eq_ignore_ascii_case("claude-code") {
        "claude"
    } else if trimmed.eq_ignore_ascii_case("github-copilot") {
        "copilot"
    } else {
        trimmed
    }
}

fn first_existing_or_default(dir: &Path, candidates: &[&str], default_name: &str) -> PathBuf {
    for name in candidates {
        let p = dir.join(name);
        if p.exists() {
            return p;
        }
    }
    dir.join(default_name)
}

fn opencode_family_dir(roots: &McpPathRoots, env_var: &str, folder: &str) -> PathBuf {
    roots
        .env_override(env_var)
        .unwrap_or_else(|| roots.xdg_config_home.join(folder))
}

fn platform_config_dir(roots: &McpPathRoots, unix_rel: &str, win_rel: &str) -> PathBuf {
    if cfg!(windows) {
        roots.win_app_data.join(win_rel)
    } else if cfg!(target_os = "macos") {
        if unix_rel == "Claude" || unix_rel == "Claude-3p" {
            roots.mac_app_support.join(unix_rel)
        } else {
            roots.xdg_config_home.join(unix_rel)
        }
    } else {
        roots.xdg_config_home.join(unix_rel)
    }
}

/// Resolve all 35 canonical built-in MCP agent specs (magpie `targets.go` excluding Gemini CLI).
pub fn builtin_targets(roots: &McpPathRoots) -> Vec<BuiltInTargetSpec> {
    let mut specs: Vec<BuiltInTargetSpec> = Vec::with_capacity(35);

    // 1. claude
    let claude_dir = roots.env_override("CLAUDE_CONFIG_DIR");
    let (claude_path, claude_settings) = match &claude_dir {
        Some(dir) => (dir.join(".claude.json"), dir.join("settings.json")),
        None => (
            roots.home_dir.join(".claude.json"),
            roots.home_dir.join(".claude").join("settings.json"),
        ),
    };
    specs.push(BuiltInTargetSpec {
        id: "claude".into(),
        name: "Claude Code".into(),
        installed: roots.detect_dir_or_file(&claude_path, Some("claude"), None)
            || claude_settings.parent().is_some_and(|p| p.exists()),
        primary_path: claude_path,
        also_paths: Vec::new(),
        claude_settings_path: Some(claude_settings),
        seed_from_path: None,
        format: McpFormat::Claude,
        no_cmd_refs: false,
        shared_here: false,
        supports_tools_off: true,
        supports_startup_timeout: false,
        notes: None,
    });

    // 2. claude-desktop
    let desktop_dir = platform_config_dir(roots, "Claude", "Claude");
    let desktop_primary = desktop_dir.join("claude_desktop_config.json");
    let mut desktop_also = Vec::new();
    let three_p_dir = if cfg!(windows) {
        roots.win_local_app_data.join("Claude-3p")
    } else if cfg!(target_os = "macos") {
        roots.mac_app_support.join("Claude-3p")
    } else {
        roots.xdg_config_home.join("Claude-3p")
    };
    if three_p_dir.is_dir() {
        desktop_also.push(three_p_dir.join("claude_desktop_config.json"));
    }
    specs.push(BuiltInTargetSpec {
        id: "claude-desktop".into(),
        name: "Claude Desktop".into(),
        installed: roots.detect_dir_or_file(&desktop_primary, None, Some("Claude.app")),
        primary_path: desktop_primary,
        also_paths: desktop_also,
        claude_settings_path: None,
        seed_from_path: None,
        format: McpFormat::Desktop,
        no_cmd_refs: false,
        shared_here: false,
        supports_tools_off: false,
        supports_startup_timeout: false,
        notes: Some("Stdio only (no remote HTTP/SSE)".into()),
    });

    // 3. codex
    let codex_dir = roots
        .env_override("CODEX_HOME")
        .unwrap_or_else(|| roots.home_dir.join(".codex"));
    let codex_path = codex_dir.join("config.toml");
    specs.push(BuiltInTargetSpec {
        id: "codex".into(),
        name: "Codex".into(),
        installed: roots.detect_dir_or_file(&codex_path, Some("codex"), Some("Codex.app")),
        primary_path: codex_path,
        also_paths: Vec::new(),
        claude_settings_path: None,
        seed_from_path: None,
        format: McpFormat::Codex,
        no_cmd_refs: false,
        shared_here: false,
        supports_tools_off: true,
        supports_startup_timeout: true,
        notes: None,
    });

    // 4. antigravity (converged alias: agy)
    let antigravity_path = roots
        .home_dir
        .join(".gemini")
        .join("config")
        .join("mcp_config.json");
    specs.push(BuiltInTargetSpec {
        id: "antigravity".into(),
        name: "Antigravity".into(),
        installed: roots.detect_dir_or_file(
            &antigravity_path,
            Some("antigravity"),
            Some("Antigravity.app"),
        ) || roots.home_dir.join(".gemini").join("antigravity").exists(),
        primary_path: antigravity_path,
        also_paths: Vec::new(),
        claude_settings_path: None,
        seed_from_path: None,
        format: McpFormat::Antigravity,
        no_cmd_refs: false,
        shared_here: false,
        supports_tools_off: false,
        supports_startup_timeout: false,
        notes: None,
    });

    // 6. cursor
    let cursor_path = roots.home_dir.join(".cursor").join("mcp.json");
    specs.push(BuiltInTargetSpec {
        id: "cursor".into(),
        name: "Cursor".into(),
        installed: roots.detect_dir_or_file(&cursor_path, Some("cursor"), Some("Cursor.app")),
        primary_path: cursor_path,
        also_paths: Vec::new(),
        claude_settings_path: None,
        seed_from_path: None,
        format: McpFormat::Cursor,
        no_cmd_refs: false,
        shared_here: false,
        supports_tools_off: false,
        supports_startup_timeout: false,
        notes: None,
    });

    // 7. opencode
    let opencode_dir = opencode_family_dir(roots, "OPENCODE_CONFIG_DIR", "opencode");
    let opencode_path = first_existing_or_default(
        &opencode_dir,
        &["opencode.jsonc", "opencode.json", "config.json"],
        "opencode.json",
    );
    specs.push(BuiltInTargetSpec {
        id: "opencode".into(),
        name: "OpenCode".into(),
        installed: roots.detect_dir_or_file(&opencode_path, Some("opencode"), None),
        primary_path: opencode_path,
        also_paths: Vec::new(),
        claude_settings_path: None,
        seed_from_path: None,
        format: McpFormat::OpenCode,
        no_cmd_refs: false,
        shared_here: false,
        supports_tools_off: true,
        supports_startup_timeout: false,
        notes: None,
    });

    // 8. deveco
    let deveco_dir = opencode_family_dir(roots, "DEVECO_CONFIG_DIR", "deveco");
    let deveco_path = first_existing_or_default(
        &deveco_dir,
        &["deveco.jsonc", "deveco.json", "config.json"],
        "deveco.json",
    );
    specs.push(BuiltInTargetSpec {
        id: "deveco".into(),
        name: "DevEco Code".into(),
        installed: roots.detect_dir_or_file(&deveco_path, Some("deveco"), None),
        primary_path: deveco_path,
        also_paths: Vec::new(),
        claude_settings_path: None,
        seed_from_path: None,
        format: McpFormat::OpenCode,
        no_cmd_refs: false,
        shared_here: false,
        supports_tools_off: true,
        supports_startup_timeout: false,
        notes: None,
    });

    // 9. mimocode
    let mimocode_dir = roots
        .env_override("MIMOCODE_HOME")
        .map(|p| p.join("config"))
        .unwrap_or_else(|| roots.xdg_config_home.join("mimocode"));
    let mimocode_path = first_existing_or_default(
        &mimocode_dir,
        &["mimocode.jsonc", "mimocode.json", "config.json"],
        "mimocode.json",
    );
    specs.push(BuiltInTargetSpec {
        id: "mimocode".into(),
        name: "MiMo Code".into(),
        installed: roots.detect_dir_or_file(&mimocode_path, Some("mimocode"), None),
        primary_path: mimocode_path,
        also_paths: Vec::new(),
        claude_settings_path: None,
        seed_from_path: None,
        format: McpFormat::OpenCode,
        no_cmd_refs: false,
        shared_here: false,
        supports_tools_off: true,
        supports_startup_timeout: false,
        notes: None,
    });

    // 10. pi
    let pi_dir = roots
        .env_override("PI_CODING_AGENT_DIR")
        .unwrap_or_else(|| roots.home_dir.join(".pi").join("agent"));
    let pi_native_path = pi_dir.join("mcp.json");
    let pi_adapter_path = pi_dir.join("mcp-adapter.json");
    let (pi_path, pi_fmt) = if !pi_native_path.exists() && pi_adapter_path.exists() {
        (pi_adapter_path, McpFormat::Pi)
    } else {
        (pi_native_path, McpFormat::PiNative)
    };
    specs.push(BuiltInTargetSpec {
        id: "pi".into(),
        name: "Pi".into(),
        installed: roots.detect_dir_or_file(&pi_path, Some("pi"), None),
        primary_path: pi_path,
        also_paths: Vec::new(),
        claude_settings_path: None,
        seed_from_path: None,
        format: pi_fmt,
        no_cmd_refs: false,
        shared_here: false,
        supports_tools_off: false,
        supports_startup_timeout: false,
        notes: None,
    });

    // 11. omp
    let omp_dir = roots
        .env_override("OMP_CODING_AGENT_DIR")
        .or_else(|| roots.env_override("PI_CONFIG_DIR").map(|d| d.join("agent")))
        .unwrap_or_else(|| roots.home_dir.join(".omp").join("agent"));
    let omp_path = omp_dir.join("mcp.json");
    specs.push(BuiltInTargetSpec {
        id: "omp".into(),
        name: "omp".into(),
        installed: roots.detect_dir_or_file(&omp_path, Some("omp"), None),
        primary_path: omp_path,
        also_paths: Vec::new(),
        claude_settings_path: None,
        seed_from_path: None,
        format: McpFormat::Omp,
        no_cmd_refs: false,
        shared_here: false,
        supports_tools_off: false,
        supports_startup_timeout: false,
        notes: None,
    });

    // 12. omo
    let omo_dir = roots
        .env_override("OMO_CODING_AGENT_DIR")
        .or_else(|| roots.env_override("SENPI_CODING_AGENT_DIR"))
        .unwrap_or_else(|| roots.home_dir.join(".omo").join("agent"));
    let omo_path = omo_dir.join("mcp.json");
    specs.push(BuiltInTargetSpec {
        id: "omo".into(),
        name: "OmO".into(),
        installed: roots.detect_dir_or_file(&omo_path, Some("omo"), None),
        primary_path: omo_path,
        also_paths: Vec::new(),
        claude_settings_path: None,
        seed_from_path: None,
        format: McpFormat::PiNative,
        no_cmd_refs: false,
        shared_here: false,
        supports_tools_off: false,
        supports_startup_timeout: false,
        notes: None,
    });

    // 13. mu
    let mu_dir = roots
        .env_override("MU_AGENT_DIR")
        .or_else(|| roots.env_override("KYRN_AGENT_DIR"))
        .or_else(|| roots.env_override("MU_CODING_AGENT_DIR"))
        .or_else(|| roots.env_override("KYRN_CODING_AGENT_DIR"))
        .unwrap_or_else(|| {
            let mu = roots.home_dir.join(".mu").join("agent");
            let kyrn = roots.home_dir.join(".kyrn").join("agent");
            if !mu.exists() && kyrn.exists() {
                kyrn
            } else {
                mu
            }
        });
    let mu_path = mu_dir.join("mcp.json");
    specs.push(BuiltInTargetSpec {
        id: "mu".into(),
        name: "mu".into(),
        installed: roots.detect_dir_or_file(&mu_path, Some("mu"), None),
        primary_path: mu_path,
        also_paths: Vec::new(),
        claude_settings_path: None,
        seed_from_path: None,
        format: McpFormat::PiNative,
        no_cmd_refs: false,
        shared_here: false,
        supports_tools_off: false,
        supports_startup_timeout: false,
        notes: None,
    });

    // 14. goose
    let goose_path = if cfg!(windows) {
        roots
            .win_app_data
            .join("Block")
            .join("goose")
            .join("config")
            .join("config.yaml")
    } else {
        roots.xdg_config_home.join("goose").join("config.yaml")
    };
    specs.push(BuiltInTargetSpec {
        id: "goose".into(),
        name: "Goose".into(),
        installed: roots.detect_dir_or_file(&goose_path, Some("goose"), Some("Goose.app")),
        primary_path: goose_path,
        also_paths: Vec::new(),
        claude_settings_path: None,
        seed_from_path: None,
        format: McpFormat::Goose,
        no_cmd_refs: false,
        shared_here: false,
        supports_tools_off: false,
        supports_startup_timeout: false,
        notes: None,
    });

    // 15. copilot
    let copilot_dir = roots
        .env_override("COPILOT_HOME")
        .unwrap_or_else(|| roots.home_dir.join(".copilot"));
    let copilot_path = copilot_dir.join("mcp-config.json");
    specs.push(BuiltInTargetSpec {
        id: "copilot".into(),
        name: "Copilot CLI".into(),
        installed: roots.detect_dir_or_file(&copilot_path, Some("copilot"), None),
        primary_path: copilot_path,
        also_paths: Vec::new(),
        claude_settings_path: None,
        seed_from_path: None,
        format: McpFormat::Copilot,
        no_cmd_refs: false,
        shared_here: false,
        supports_tools_off: false,
        supports_startup_timeout: false,
        notes: None,
    });

    // 16. crush
    let crush_path = if cfg!(windows) {
        roots.win_local_app_data.join("crush").join("crush.json")
    } else {
        roots.xdg_config_home.join("crush").join("crush.json")
    };
    specs.push(BuiltInTargetSpec {
        id: "crush".into(),
        name: "Crush".into(),
        installed: roots.detect_dir_or_file(&crush_path, Some("crush"), None),
        primary_path: crush_path,
        also_paths: Vec::new(),
        claude_settings_path: None,
        seed_from_path: None,
        format: McpFormat::Crush,
        no_cmd_refs: false,
        shared_here: false,
        supports_tools_off: true,
        supports_startup_timeout: false,
        notes: None,
    });

    // 17. zcode
    let zcode_path = roots
        .home_dir
        .join(".zcode")
        .join("cli")
        .join("config.json");
    specs.push(BuiltInTargetSpec {
        id: "zcode".into(),
        name: "ZCode".into(),
        installed: roots.detect_dir_or_file(&zcode_path, Some("zcode"), Some("ZCode.app")),
        primary_path: zcode_path,
        also_paths: Vec::new(),
        claude_settings_path: None,
        seed_from_path: None,
        format: McpFormat::ZCode,
        no_cmd_refs: false,
        shared_here: false,
        supports_tools_off: false,
        supports_startup_timeout: false,
        notes: None,
    });

    // 18. kimi
    let kimi_dir = roots
        .env_override("KIMI_CODE_HOME")
        .unwrap_or_else(|| {
            let kc = roots.home_dir.join(".kimi-code");
            if kc.exists() {
                return kc;
            }
            if let Some(share) = roots.env_override("KIMI_SHARE_DIR") {
                return share;
            }
            let legacy = roots.home_dir.join(".kimi");
            if legacy.exists() {
                return legacy;
            }
            kc
        });
    let kimi_path = kimi_dir.join("mcp.json");
    specs.push(BuiltInTargetSpec {
        id: "kimi".into(),
        name: "Kimi Code".into(),
        installed: roots.detect_dir_or_file(&kimi_path, Some("kimi"), None),
        primary_path: kimi_path,
        also_paths: Vec::new(),
        claude_settings_path: None,
        seed_from_path: None,
        format: McpFormat::Kimi,
        no_cmd_refs: false,
        shared_here: false,
        supports_tools_off: false,
        supports_startup_timeout: false,
        notes: None,
    });

    super::paths_ext::append_targets_18_to_35(roots, &mut specs);

    mark_shared_paths(&mut specs);
    specs
}

fn mark_shared_paths(specs: &mut [BuiltInTargetSpec]) {
    let mut counts: BTreeMap<PathBuf, BTreeSet<String>> = BTreeMap::new();
    for spec in specs.iter() {
        counts
            .entry(spec.primary_path.clone())
            .or_default()
            .insert(spec.id.clone());
    }
    for spec in specs.iter_mut() {
        if counts
            .get(&spec.primary_path)
            .is_some_and(|owners| owners.len() > 1)
        {
            spec.shared_here = true;
        }
    }
}

/// Convert internal specs into serializable GUI/CLI target status summaries.
pub fn detect_agent_targets(roots: &McpPathRoots) -> Vec<McpAgentTarget> {
    builtin_targets(roots)
        .into_iter()
        .map(|s| McpAgentTarget {
            id: s.id,
            name: s.name,
            installed: s.installed,
            shared_here: s.shared_here,
            config_path: s.primary_path.to_string_lossy().to_string(),
            no_remote: !s.format.supports_remote(),
            no_sse: !s.format.supports_sse(),
            format: s.format,
            supports_tools_off: s.supports_tools_off,
            supports_startup_timeout: s.supports_startup_timeout,
            notes: s.notes,
        })
        .collect()
}

/// Look up one canonical MCP target spec by id (handling legacy aliases such as `agy`).
pub fn resolve_target_spec(roots: &McpPathRoots, id: &str) -> McpHubResult<BuiltInTargetSpec> {
    let canon = canonical_agent_id(id);
    builtin_targets(roots)
        .into_iter()
        .find(|s| s.id == canon)
        .ok_or_else(|| McpHubError::UnknownAgent(id.to_string()))
}
