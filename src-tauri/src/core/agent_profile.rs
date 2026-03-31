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
    super::paths::home_dir()
}

/// Path to the TOML configuration file storing user preferences.
fn prefs_path() -> PathBuf {
    super::paths::data_root().join("profiles.toml")
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
            icon: "agents/opencode.svg".into(),
            global_skills_dir: home.join(".config").join("opencode").join("skills"),
            project_skills_rel: ".opencode/skills".into(),
            installed: false,
            enabled: false,
            synced_count: 0,
        },
        AgentProfile {
            id: "claude".into(),
            display_name: "Claude Code".into(),
            icon: "agents/claude.svg".into(),
            global_skills_dir: home.join(".claude").join("skills"),
            project_skills_rel: ".claude/skills".into(),
            installed: false,
            enabled: false,
            synced_count: 0,
        },
        AgentProfile {
            id: "codex".into(),
            display_name: "Codex CLI".into(),
            icon: "agents/codex.svg".into(),
            global_skills_dir: home.join(".codex").join("skills"),
            project_skills_rel: ".codex/skills".into(),
            installed: false,
            enabled: false,
            synced_count: 0,
        },
        AgentProfile {
            id: "antigravity".into(),
            display_name: "Antigravity".into(),
            icon: "agents/antigravity.svg".into(),
            global_skills_dir: home.join(".gemini").join("antigravity").join("skills"),
            project_skills_rel: ".agents/skills".into(),
            installed: false,
            enabled: false,
            synced_count: 0,
        },
        AgentProfile {
            id: "gemini".into(),
            display_name: "Gemini CLI".into(),
            icon: "agents/gemini.svg".into(),
            global_skills_dir: home.join(".gemini").join("skills"),
            project_skills_rel: ".gemini/skills".into(),
            installed: false,
            enabled: false,
            synced_count: 0,
        },
        AgentProfile {
            id: "openclaw".into(),
            display_name: "OpenClaw".into(),
            icon: "agents/openclaw.svg".into(),
            global_skills_dir: home.join(".openclaw").join("skills"),
            project_skills_rel: "".into(),
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
