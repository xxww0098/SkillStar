use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

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
    (
        "hermes",
        "Hermes",
        "agents/hermes.svg",
        &[".hermes", "skills"],
        ".hermes/skills",
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
/// Returns `Err` if no profile matches. This is the canonical
/// replacement for the `.find(...).ok_or_else(...)` pattern.
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
    crate::core::infra::paths::home_dir()
}

/// Path to the TOML configuration file storing user preferences.
fn prefs_path() -> PathBuf {
    crate::core::infra::paths::profiles_config_path()
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

struct BuiltinAgentData {
    id: String,
    display_name: String,
    icon: String,
    subdirs: &'static [&'static str],
    project_skills_rel: String,
}

fn builtin_agent_data() -> &'static [BuiltinAgentData] {
    static CACHED: OnceLock<Vec<BuiltinAgentData>> = OnceLock::new();
    CACHED.get_or_init(|| {
        BUILTIN_AGENT_DEFS
            .iter()
            .map(|(id, name, icon, subdirs, rel)| BuiltinAgentData {
                id: (*id).to_string(),
                display_name: (*name).to_string(),
                icon: (*icon).to_string(),
                subdirs: *subdirs,
                project_skills_rel: (*rel).to_string(),
            })
            .collect()
    })
}

/// Built-in agent definitions with correct directory paths.
///
/// Profile data is driven by `BUILTIN_AGENT_DEFS`; adding a new agent
/// only requires appending one tuple to the const table.
fn builtin_profiles() -> Vec<AgentProfile> {
    let home = home_dir();
    let data = builtin_agent_data();
    let mut profiles = Vec::with_capacity(data.len());

    for d in data {
        let mut dir = home.clone();
        dir.extend(d.subdirs);
        profiles.push(AgentProfile {
            id: d.id.clone(),
            display_name: d.display_name.clone(),
            icon: d.icon.clone(),
            global_skills_dir: dir,
            project_skills_rel: d.project_skills_rel.clone(),
            installed: false,
            enabled: false,
            synced_count: 0,
        });
    }

    profiles
}

/// Count how many managed skill entries (symlinks, junctions, or copies) exist in a directory.
fn count_symlinks(dir: &Path) -> u32 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|e| {
            let Ok(ft) = e.file_type() else {
                return false;
            };

            // Fast paths using dirent file_type (no stat calls on Unix, fast on Windows)
            if ft.is_symlink() {
                return true;
            }

            // Fallback for Windows junction points which might not be marked as symlinks
            #[cfg(windows)]
            if crate::core::infra::fs_ops::is_link(&e.path()) {
                return true;
            }

            // Fallback for copied directories
            if ft.is_dir() {
                let mut p = e.path();
                p.push("SKILL.md");
                return p.exists();
            }

            false
        })
        .count() as u32
}

/// Detect installation by creating the config/skills dir if it doesn't exist.
fn detect_installed(profile: &AgentProfile) -> bool {
    if !profile.global_skills_dir.exists() {
        if let Err(e) = std::fs::create_dir_all(&profile.global_skills_dir) {
            tracing::warn!(
                "Failed to provision global skills directory for agent profile '{}' at {:?}: {}",
                profile.id,
                profile.global_skills_dir,
                e
            );
            return false;
        }
    }
    true
}

fn get_base_profiles(prefs: &ProfilePrefs) -> Vec<AgentProfile> {
    let mut profiles = builtin_profiles();
    let home = home_dir();

    profiles.reserve(prefs.custom_profiles.len());

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

        profiles.push(AgentProfile {
            id: cp.id.clone(),
            display_name: cp.display_name.clone(),
            icon: cp
                .icon_data_uri
                .clone()
                .unwrap_or_else(|| "agents/openclaw.svg".into()),
            global_skills_dir: pbuf,
            project_skills_rel: cp.project_skills_rel.clone(),
            installed: false,
            enabled: false,
            synced_count: 0,
        });
    }

    profiles
}

/// List all agent profiles with detected install status and user prefs.
pub fn list_profiles() -> Vec<AgentProfile> {
    let prefs = load_prefs();
    let mut profiles = get_base_profiles(&prefs);

    for p in &mut profiles {
        p.installed = detect_installed(p);
        p.enabled = prefs.enabled.get(&p.id).copied().unwrap_or(p.installed);
        p.synced_count = count_symlinks(&p.global_skills_dir);
    }

    profiles
}

pub fn add_custom_profile(def: CustomProfileDef) -> Result<()> {
    let normalized_project_rel = def.project_skills_rel.trim().replace('\\', "/");
    let project_rel = normalized_project_rel.as_str();
    if !project_rel.is_empty() {
        if !project_rel.starts_with('.') || !project_rel.ends_with("/skills") {
            return Err(anyhow::anyhow!(
                "Project skills path must strictly follow the format '.agentname/skills'"
            ));
        }
        let middle = &project_rel[1..project_rel.len() - "/skills".len()];
        if middle.is_empty()
            || !middle
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(anyhow::anyhow!(
                "Project skills path must strictly follow the format '.agentname/skills'"
            ));
        }
    }

    let mut prefs = load_prefs();

    let mut new_def = def;
    new_def.project_skills_rel = normalized_project_rel;
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
    prefs.enabled.insert(new_def.id.clone(), true);
    prefs.custom_profiles.push(new_def);
    save_prefs(&prefs)
}

pub fn remove_custom_profile(id: &str) -> Result<()> {
    let mut prefs = load_prefs();
    prefs.custom_profiles.retain(|p| p.id != id);
    prefs.enabled.remove(id);
    save_prefs(&prefs)
}

pub fn toggle_profile(id: &str) -> Result<bool> {
    let mut prefs = load_prefs();
    let current = prefs.enabled.get(id).copied().unwrap_or_else(|| {
        // If not explicitly set, known profiles default to enabled
        builtin_agent_data().iter().any(|def| def.id == id)
            || prefs.custom_profiles.iter().any(|cp| cp.id == id)
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

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        crate::core::lock_test_env()
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
    fn add_custom_profile_normalizes_windows_project_path_before_persisting() -> Result<()> {
        let _guard = env_lock();

        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let temp_root =
            std::env::temp_dir().join(format!("skillstar-agent-profile-custom-{}", stamp));
        let temp_home = temp_root.join("home");
        std::fs::create_dir_all(&temp_home)?;
        let previous_home = std::env::var_os("HOME");
        let previous_data_dir = std::env::var_os("SKILLSTAR_DATA_DIR");
        set_env("HOME", &temp_home);
        set_env("SKILLSTAR_DATA_DIR", temp_home.join(".skillstar"));
        #[cfg(windows)]
        let previous_userprofile = std::env::var_os("USERPROFILE");
        #[cfg(windows)]
        set_env("USERPROFILE", &temp_home);

        let result = (|| -> Result<()> {
            add_custom_profile(CustomProfileDef {
                id: "custom_ollama".into(),
                display_name: "Ollama".into(),
                global_skills_dir: "D:\\ollama\\skills".into(),
                project_skills_rel: ".ollama\\skills".into(),
                icon_data_uri: None,
            })?;

            let prefs = load_prefs();
            let saved = prefs
                .custom_profiles
                .into_iter()
                .find(|profile| profile.id == "custom_ollama")
                .expect("custom profile should be persisted");

            assert_eq!(saved.project_skills_rel, ".ollama/skills");
            Ok(())
        })();

        match previous_home {
            Some(value) => set_env("HOME", value),
            None => remove_env("HOME"),
        }
        match previous_data_dir {
            Some(value) => set_env("SKILLSTAR_DATA_DIR", value),
            None => remove_env("SKILLSTAR_DATA_DIR"),
        }
        #[cfg(windows)]
        match previous_userprofile {
            Some(value) => set_env("USERPROFILE", value),
            None => remove_env("USERPROFILE"),
        }
        let _ = std::fs::remove_dir_all(&temp_root);

        result
    }

    #[test]
    fn toggle_profile_matches_default_enabled_state() -> Result<()> {
        let _guard = env_lock();

        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let temp_root =
            std::env::temp_dir().join(format!("skillstar-agent-profile-toggle-{}", stamp));
        let temp_home = temp_root.join("home");
        std::fs::create_dir_all(&temp_home)?;
        let previous_home = std::env::var_os("HOME");
        let previous_data_dir = std::env::var_os("SKILLSTAR_DATA_DIR");
        set_env("HOME", &temp_home);
        set_env("SKILLSTAR_DATA_DIR", temp_home.join(".skillstar"));
        #[cfg(windows)]
        let previous_userprofile = std::env::var_os("USERPROFILE");
        #[cfg(windows)]
        set_env("USERPROFILE", &temp_home);

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
        match previous_data_dir {
            Some(value) => set_env("SKILLSTAR_DATA_DIR", value),
            None => remove_env("SKILLSTAR_DATA_DIR"),
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
