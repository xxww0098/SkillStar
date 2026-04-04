use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// ── Built-in agent definitions (data table) ────────────────────────
// (id, display_name, icon, home_subdirs, project_skills_rel)
//
// Adding a new agent requires only one line here.
const BUILTIN_AGENT_DEFS: &[(&str, &str, &str, &[&str], &str)] = &[
    (
        "opencode",
        "OpenCode",
        "agents/opencode.svg",
        &[".config", "opencode", "skills"],
        ".opencode/skills",
    ),
    (
        "claude",
        "Claude Code",
        "agents/claude.svg",
        &[".claude", "skills"],
        ".claude/skills",
    ),
    (
        "codex",
        "Codex CLI",
        "agents/codex.svg",
        &[".codex", "skills"],
        ".codex/skills",
    ),
    (
        "antigravity",
        "Antigravity",
        "agents/antigravity.svg",
        &[".gemini", "antigravity", "skills"],
        ".agents/skills",
    ),
    (
        "gemini",
        "Gemini CLI",
        "agents/gemini.svg",
        &[".gemini", "skills"],
        ".gemini/skills",
    ),
    (
        "cursor",
        "Cursor",
        "agents/cursor.svg",
        &[".cursor", "skills"],
        ".cursor/skills",
    ),
    (
        "qoder",
        "Qoder",
        "agents/qoder-color.svg",
        &[".qoder", "skills"],
        ".qoder/skills",
    ),
    (
        "trae",
        "Trae",
        "agents/trae-color.svg",
        &[".trae", "skills"],
        ".trae/skills",
    ),
    (
        "openclaw",
        "OpenClaw",
        "agents/openclaw.svg",
        &[".openclaw", "skills"],
        "",
    ),
];

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

impl AgentProfile {
    /// Whether this agent supports project-level skills.
    ///
    /// Global-only agents (e.g. OpenClaw) have an empty `project_skills_rel`.
    pub fn has_project_skills(&self) -> bool {
        !self.project_skills_rel.is_empty()
    }
}

/// Find an agent profile by its ID from a slice of profiles.
///
/// Returns `Err` if no profile matches — this is the canonical
/// replacement for the `.find(...).ok_or_else(...)` pattern that was
/// previously duplicated across `sync.rs` and `project_manifest.rs`.
pub fn find_profile<'a>(
    profiles: &'a [AgentProfile],
    agent_id: &str,
) -> anyhow::Result<&'a AgentProfile> {
    profiles
        .iter()
        .find(|p| p.id == agent_id)
        .ok_or_else(|| anyhow::anyhow!("Agent profile '{}' not found", agent_id))
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CustomProfileDef {
    pub id: String,
    pub display_name: String,
    pub global_skills_dir: String,
    pub project_skills_rel: String,
    pub icon_data_uri: Option<String>,
}

/// Persisted user preferences (just the enable/disable state per agent).
#[derive(Debug, Serialize, Deserialize, Default)]
struct ProfilePrefs {
    /// Map of agent id → enabled.
    enabled: std::collections::HashMap<String, bool>,
    #[serde(default)]
    custom_profiles: Vec<CustomProfileDef>,
}

fn home_dir() -> PathBuf {
    super::paths::home_dir()
}

/// Path to the TOML configuration file storing user preferences.
fn prefs_path() -> PathBuf {
    super::paths::profiles_config_path()
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
///
/// Profile data is driven by `BUILTIN_AGENT_DEFS`; adding a new agent
/// only requires appending one tuple to the const table.
fn builtin_profiles() -> Vec<AgentProfile> {
    let home = home_dir();
    BUILTIN_AGENT_DEFS
        .iter()
        .map(|(id, name, icon, subdirs, rel)| {
            let mut dir = home.clone();
            for seg in *subdirs {
                dir = dir.join(seg);
            }
            AgentProfile {
                id: (*id).into(),
                display_name: (*name).into(),
                icon: (*icon).into(),
                global_skills_dir: dir,
                project_skills_rel: (*rel).into(),
                installed: false,
                enabled: false,
                synced_count: 0,
            }
        })
        .collect()
}

/// Count how many skill symlinks/junctions exist in a directory.
fn count_symlinks(dir: &Path) -> u32 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries.flatten().filter(|e| super::paths::is_link(&e.path())).count() as u32
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
    let home = home_dir();

    for cp in &prefs.custom_profiles {
        let path_str = &cp.global_skills_dir;
        // Handle both ~/ (Unix) and ~\ (Windows) prefixes
        let pbuf = if let Some(stripped) = path_str
            .strip_prefix("~/")
            .or_else(|| path_str.strip_prefix("~\\"))
        {
            home.join(stripped)
        } else {
            PathBuf::from(path_str)
        };

        // Normalize project_skills_rel to forward slashes for cross-platform consistency
        let normalized_rel = cp.project_skills_rel.replace('\\', "/");

        profiles.push(AgentProfile {
            id: cp.id.clone(),
            display_name: cp.display_name.clone(),
            icon: cp
                .icon_data_uri
                .clone()
                .unwrap_or_else(|| "agents/openclaw.svg".into()),
            global_skills_dir: pbuf,
            project_skills_rel: normalized_rel,
            installed: false,
            enabled: false,
            synced_count: 0,
        });
    }

    for p in &mut profiles {
        p.installed = detect_installed(p);
        p.enabled = prefs.enabled.get(&p.id).copied().unwrap_or(p.installed);
        p.synced_count = count_symlinks(&p.global_skills_dir);
    }

    profiles
}

pub fn add_custom_profile(def: CustomProfileDef) -> Result<()> {
    let project_rel = def.project_skills_rel.trim();
    if !project_rel.is_empty() {
        if !project_rel.starts_with('.') || !project_rel.ends_with("/skills") {
            return Err(anyhow::anyhow!("Project skills path must strictly follow the format '.agentname/skills'"));
        }
        let middle = &project_rel[1..project_rel.len() - "/skills".len()];
        if middle.is_empty() || !middle.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return Err(anyhow::anyhow!("Project skills path must strictly follow the format '.agentname/skills'"));
        }
    }

    let mut prefs = load_prefs();

    let mut new_def = def.clone();
    if new_def.id.is_empty() {
        new_def.id = format!(
            "custom_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        );
    }

    prefs.custom_profiles.retain(|p| p.id != new_def.id);
    prefs.custom_profiles.push(new_def.clone());
    prefs.enabled.insert(new_def.id.clone(), true);
    save_prefs(&prefs)
}

pub fn remove_custom_profile(id: &str) -> Result<()> {
    let mut prefs = load_prefs();
    prefs.custom_profiles.retain(|p| p.id != id);
    prefs.enabled.remove(id);
    save_prefs(&prefs)
}

/// Toggle the enabled state of a single agent profile.
pub fn toggle_profile(id: &str) -> Result<bool> {
    let mut prefs = load_prefs();
    let current = prefs.enabled.get(id).copied().unwrap_or_else(|| {
        list_profiles()
            .into_iter()
            .find(|profile| profile.id == id)
            .map(|profile| profile.enabled)
            .unwrap_or(false)
    });
    let new_state = !current;
    prefs.enabled.insert(id.to_string(), new_state);
    save_prefs(&prefs)?;
    Ok(new_state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use std::ffi::OsStr;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn env_lock() -> &'static std::sync::Mutex<()> {
        crate::core::test_env_lock()
    }

    fn set_env<K: AsRef<OsStr>, V: AsRef<OsStr>>(key: K, value: V) {
        unsafe { std::env::set_var(key, value) }
    }

    fn remove_env<K: AsRef<OsStr>>(key: K) {
        unsafe { std::env::remove_var(key) }
    }

    #[test]
    fn openclaw_has_no_project_level_skills_directory() {
        let openclaw = builtin_profiles()
            .into_iter()
            .find(|profile| profile.id == "openclaw")
            .expect("openclaw profile should exist");

        assert!(
            openclaw.project_skills_rel.is_empty(),
            "OpenClaw should be global-only and excluded from project-level detection"
        );
    }

    #[test]
    fn toggle_profile_matches_default_enabled_state() -> Result<()> {
        let _guard = env_lock()
            .lock()
            .expect("environment lock should not be poisoned");

        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let temp_root =
            std::env::temp_dir().join(format!("skillstar-agent-profile-toggle-{}", stamp));
        let previous_home = std::env::var_os("HOME");
        set_env("HOME", temp_root.join("home"));
        #[cfg(windows)]
        let previous_userprofile = std::env::var_os("USERPROFILE");
        #[cfg(windows)]
        set_env("USERPROFILE", temp_root.join("home"));

        let result = (|| -> Result<()> {
            let initial = list_profiles()
                .into_iter()
                .find(|profile| profile.id == "claude")
                .expect("claude profile should exist");
            assert!(
                initial.enabled,
                "expected default profile state to start enabled"
            );

            let toggled = toggle_profile("claude")?;
            assert!(
                !toggled,
                "expected first toggle to disable the profile from its implicit enabled state"
            );

            let updated = list_profiles()
                .into_iter()
                .find(|profile| profile.id == "claude")
                .expect("claude profile should exist after toggle");
            assert!(
                !updated.enabled,
                "expected persisted profile state to be disabled after one toggle"
            );

            Ok(())
        })();

        match previous_home {
            Some(value) => set_env("HOME", value),
            None => remove_env("HOME"),
        }
        #[cfg(windows)]
        match previous_userprofile {
            Some(value) => set_env("USERPROFILE", value),
            None => remove_env("USERPROFILE"),
        }
        let _ = std::fs::remove_dir_all(&temp_root);

        result
    }
}
