use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A single Agent profile describing where its skills directory lives.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProfile {
    /// Internal identifier, e.g. "claude", "gemini"
    pub id: String,
    /// Human-readable name shown in UI
    pub display_name: String,
    /// SVG filename in /public, e.g. "claude.svg"
    pub icon: String,
    /// Global skills directory (absolute path)
    pub global_skills_dir: PathBuf,
    /// Project-level skills path relative to project root, e.g. ".claude/skills"
    pub project_skills_rel: String,
    /// Whether the agent is detected as installed on this machine
    pub installed: bool,
    /// Whether the user has enabled syncing for this agent
    pub enabled: bool,
    /// Number of skills currently symlinked to this agent
    pub synced_count: u32,
}

/// Persisted user preferences (just the enable/disable state per agent).
#[derive(Debug, Serialize, Deserialize, Default)]
struct ProfilePrefs {
    /// Map of agent id → enabled.
    enabled: std::collections::HashMap<String, bool>,
}

fn home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("~"))
}

/// Path to the TOML configuration file storing user preferences.
fn prefs_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| home_dir().join(".local").join("share"))
        .join("skillstar")
        .join("profiles.toml")
}

/// Load persisted user preferences from disk.
fn load_prefs() -> ProfilePrefs {
    let path = prefs_path();
    if !path.exists() {
        return ProfilePrefs::default();
    }
    let Ok(content) = std::fs::read_to_string(&path) else {
        return ProfilePrefs::default();
    };
    toml::from_str(&content).unwrap_or_default()
}

/// Save persisted user preferences to disk.
fn save_prefs(prefs: &ProfilePrefs) -> Result<()> {
    let path = prefs_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content =
        toml::to_string_pretty(prefs).context("Failed to serialize profile preferences")?;
    std::fs::write(&path, content).context("Failed to write profile preferences")?;
    Ok(())
}

/// Built-in agent definitions with correct directory paths.
fn builtin_profiles() -> Vec<AgentProfile> {
    let home = home_dir();
    vec![
        AgentProfile {
            id: "opencode".into(),
            display_name: "OpenCode".into(),
            icon: "opencode.svg".into(),
            global_skills_dir: home.join(".config").join("opencode").join("skills"),
            project_skills_rel: ".opencode/skills".into(),
            installed: false,
            enabled: false,
            synced_count: 0,
        },
        AgentProfile {
            id: "claude".into(),
            display_name: "Claude Code".into(),
            icon: "claude.svg".into(),
            global_skills_dir: home.join(".claude").join("skills"),
            project_skills_rel: ".claude/skills".into(),
            installed: false,
            enabled: false,
            synced_count: 0,
        },
        AgentProfile {
            id: "codex".into(),
            display_name: "Codex CLI".into(),
            icon: "codex.svg".into(),
            global_skills_dir: home.join(".codex").join("skills"),
            project_skills_rel: ".agents/skills".into(),
            installed: false,
            enabled: false,
            synced_count: 0,
        },
        AgentProfile {
            id: "gemini".into(),
            display_name: "Gemini CLI".into(),
            icon: "gemini.svg".into(),
            global_skills_dir: home.join(".gemini").join("skills"),
            project_skills_rel: ".gemini/skills".into(),
            installed: false,
            enabled: false,
            synced_count: 0,
        },
        AgentProfile {
            id: "openclaw".into(),
            display_name: "OpenClaw".into(),
            icon: "openclaw.svg".into(),
            global_skills_dir: home.join(".openclaw").join("skills"),
            project_skills_rel: ".agents/skills".into(),
            installed: false,
            enabled: false,
            synced_count: 0,
        },
    ]
}

/// Count how many skill symlinks exist in a directory.
fn count_symlinks(dir: &Path) -> u32 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries.flatten().filter(|e| e.path().is_symlink()).count() as u32
}

/// Detect installation by creating the config/skills dir if it doesn't exist.
fn detect_installed(profile: &AgentProfile) -> bool {
    // The user requested to unconditionally create the directory and treat it as installed.
    if !profile.global_skills_dir.exists() {
        let _ = std::fs::create_dir_all(&profile.global_skills_dir);
    }
    true
}

/// List all agent profiles with detected install status and user prefs.
pub fn list_profiles() -> Vec<AgentProfile> {
    let prefs = load_prefs();
    let mut profiles = builtin_profiles();

    for p in &mut profiles {
        p.installed = detect_installed(p);
        p.enabled = prefs.enabled.get(&p.id).copied().unwrap_or(p.installed);
        p.synced_count = count_symlinks(&p.global_skills_dir);
    }

    profiles
}

/// Toggle the enabled state of a single agent profile.
pub fn toggle_profile(id: &str) -> Result<bool> {
    let mut prefs = load_prefs();
    let current = prefs.enabled.get(id).copied().unwrap_or(false);
    let new_state = !current;
    prefs.enabled.insert(id.to_string(), new_state);
    save_prefs(&prefs)?;
    Ok(new_state)
}
