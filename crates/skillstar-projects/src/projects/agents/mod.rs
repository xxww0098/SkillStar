//! Agent profile management.
//!
//! Built-in and custom agents both implement [`spec::AgentSpec`]; the
//! [`registry::AgentRegistry`] turns specs + persisted [`profile_storage`]
//! prefs into the enriched [`AgentProfile`] list. This module is the frozen
//! public façade — `skillstar_projects::projects::agents::{list_profiles,
//! find_profile, toggle_profile, add_custom_profile, remove_custom_profile,
//! AgentProfile, CustomProfileDef}` must keep their paths/signatures stable.

mod builtin;
mod custom;
mod detect;
mod profile_storage;
mod registry;
mod spec;
mod validation;

use anyhow::Result;

pub use custom::CustomProfileDef;
pub use registry::{AgentProfile, find_profile};

use profile_storage::TomlPrefsStore;

/// List all agent profiles with detected install status and user prefs.
pub fn list_profiles() -> Vec<AgentProfile> {
    registry::AgentRegistry::load(&TomlPrefsStore).into_profiles()
}

/// Add (or replace by id) a custom agent profile.
pub fn add_custom_profile(def: CustomProfileDef) -> Result<()> {
    custom::add(def, &TomlPrefsStore)
}

/// Remove a custom agent profile by id.
pub fn remove_custom_profile(id: &str) -> Result<()> {
    custom::remove(id, &TomlPrefsStore)
}

/// Toggle an agent's enabled state; returns the new state.
pub fn toggle_profile(id: &str) -> Result<bool> {
    registry::toggle(id, &TomlPrefsStore)
}

/// Resolve the CLI binary name for a built-in agent id, if it has one.
///
/// Exposes the builtin table's `binary` column to other crates/modules in this
/// workspace (e.g. `terminal::registry` derives its CLI list from this) without
/// leaking the private `builtin` module. Returns `None` for IDE/global-only
/// agents and for unknown ids.
pub(crate) fn binary_name_for_builtin(id: &str) -> Option<&'static str> {
    builtin::builtin_agent_data()
        .iter()
        .find(|d| d.id == id)
        .and_then(|d| d.binary)
}

/// All terminal-launchable built-in agents as `(id, display_name, binary)`.
///
/// Derived from the builtin table — any built-in agent whose `binary` is set is
/// included. This keeps the builtin table the single source of truth for binary
/// names (shared with install detection in `detect.rs`).
pub(crate) fn builtin_cli_entries() -> Vec<(&'static str, &'static str, &'static str)> {
    builtin::builtin_agent_data()
        .iter()
        .filter_map(|d| d.binary.map(|b| (d.id.as_str(), d.display_name.as_str(), b)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::builtin::{BuiltinSpec, builtin_agent_data};
    use super::profile_storage::{MemPrefsStore, PrefsStore, TomlPrefsStore};
    use super::spec::AgentSpec;
    use super::validation::validate_project_skills_rel;
    use super::*;
    use anyhow::Result;
    use std::ffi::OsStr;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        crate::projects::lock_test_env()
    }

    fn set_env<K: AsRef<OsStr>, V: AsRef<OsStr>>(key: K, value: V) {
        unsafe { std::env::set_var(key, value) }
    }

    fn remove_env<K: AsRef<OsStr>>(key: K) {
        unsafe { std::env::remove_var(key) }
    }

    #[test]
    fn openclaw_has_no_project_level_skills_directory() {
        let openclaw = builtin_agent_data()
            .iter()
            .find(|d| d.id == "openclaw")
            .expect("openclaw builtin should exist");
        assert!(
            BuiltinSpec(openclaw).project_skills_rel().is_none(),
            "OpenClaw should be global-only and excluded from project-level detection"
        );
    }

    #[test]
    fn antigravity_and_gemini_have_distinct_project_dirs() {
        // Antigravity shares the ~/.gemini home root with Gemini but owns a
        // distinct project-level path so they never collide — expressed purely
        // as data in the builtin table, with no code special-case.
        let data = builtin_agent_data();
        let ag = data.iter().find(|d| d.id == "antigravity").unwrap();
        let gm = data.iter().find(|d| d.id == "gemini").unwrap();
        assert_eq!(BuiltinSpec(ag).project_skills_rel(), Some(".agent/skills"));
        assert_eq!(BuiltinSpec(gm).project_skills_rel(), Some(".gemini/skills"));
    }

    #[test]
    fn binary_column_matches_expected_cli_agents() {
        // The `binary` column drives install detection (PATH probe vs directory
        // fallback) and the terminal CLI list. Lock the expected set so a typo
        // or accidental removal is caught here, not at runtime.
        let cli_agents: std::collections::HashSet<&str> = builtin_agent_data()
            .iter()
            .filter_map(|d| d.binary.map(|_| d.id.as_str()))
            .collect();
        let expected: std::collections::HashSet<&str> =
            ["opencode", "claude", "codex", "gemini", "zcode"].into_iter().collect();
        assert_eq!(
            cli_agents, expected,
            "builtin agents with a binary name drifted from the expected set"
        );
    }

    #[test]
    fn shared_home_root_agents_have_distinct_binary_strategy() {
        // Antigravity (IDE, no binary) and Gemini (CLI, has binary) share
        // ~/.gemini — the only way install detection can tell them apart is
        // that gemini probes PATH while antigravity falls back to its own
        // subdirectory. Guard that asymmetry explicitly.
        let data = builtin_agent_data();
        let ag = data.iter().find(|d| d.id == "antigravity").unwrap();
        let gm = data.iter().find(|d| d.id == "gemini").unwrap();
        assert_eq!(BuiltinSpec(ag).binary_name(), None, "Antigravity must be directory-detected");
        assert_eq!(
            BuiltinSpec(gm).binary_name(),
            Some("gemini"),
            "Gemini must be PATH-detected so a stray ~/.gemini (e.g. from Antigravity) can't false-positive it"
        );
    }

    #[test]
    fn validate_project_skills_rel_rules() {
        // normalizes backslashes, accepts the canonical form
        assert_eq!(
            validate_project_skills_rel(".ollama\\skills").unwrap(),
            ".ollama/skills"
        );
        // empty == global-only, allowed
        assert_eq!(validate_project_skills_rel("").unwrap(), "");
        // rejected: no leading dot / not ending in /skills / empty middle / bad chars
        assert!(validate_project_skills_rel("ollama/skills").is_err());
        assert!(validate_project_skills_rel(".ollama/rules").is_err());
        assert!(validate_project_skills_rel("./skills").is_err());
        assert!(validate_project_skills_rel(".foo bar/skills").is_err());
    }

    #[test]
    fn add_custom_profile_normalizes_path_via_mem_store() {
        let store = MemPrefsStore::new();
        custom::add(
            CustomProfileDef {
                id: "custom_ollama".into(),
                display_name: "Ollama".into(),
                global_skills_dir: "D:\\ollama\\skills".into(),
                project_skills_rel: ".ollama\\skills".into(),
                icon_data_uri: None,
            },
            &store,
        )
        .unwrap();

        let prefs = store.load();
        let saved = prefs
            .custom_profiles
            .iter()
            .find(|p| p.id == "custom_ollama")
            .expect("custom profile should be persisted");
        assert_eq!(saved.project_skills_rel, ".ollama/skills");
        assert_eq!(prefs.enabled.get("custom_ollama"), Some(&true));
    }

    #[test]
    fn toggle_known_builtin_defaults_to_detected_install_state() -> Result<()> {
        let _guard = env_lock();

        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let temp_root =
            std::env::temp_dir().join(format!("skillstar-agent-profile-toggle-mem-{}", stamp));
        let temp_home = temp_root.join("home");
        std::fs::create_dir_all(&temp_home)?;
        let previous_home = std::env::var_os("HOME");
        set_env("HOME", &temp_home);
        #[cfg(windows)]
        let previous_userprofile = std::env::var_os("USERPROFILE");
        #[cfg(windows)]
        set_env("USERPROFILE", &temp_home);

        let result = (|| -> Result<()> {
            let store = MemPrefsStore::new();
            // Antigravity is an IDE agent (no binary), so install detection is
            // directory-based and deterministic under an isolated temp HOME:
            // ~/.gemini/antigravity-cli/skills doesn't exist there → not
            // detected → default enabled=false → first toggle flips to true.
            // (We can't use `claude` here anymore because its detection now
            // probes PATH, which leaks the dev machine's real install.)
            assert!(
                registry::toggle("antigravity", &store)?,
                "first toggle should enable when the agent is not detected"
            );
            assert_eq!(store.load().enabled.get("antigravity"), Some(&true));
            assert!(
                !registry::toggle("antigravity", &store)?,
                "second toggle should disable the explicit enabled state"
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

    #[test]
    fn list_profiles_detection_does_not_create_skills_dirs() -> Result<()> {
        let _guard = env_lock();

        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let temp_root =
            std::env::temp_dir().join(format!("skillstar-agent-profile-detect-{}", stamp));
        let temp_home = temp_root.join("home");
        std::fs::create_dir_all(temp_home.join(".claude"))?;
        let previous_home = std::env::var_os("HOME");
        let previous_data_dir = std::env::var_os("SKILLSTAR_DATA_DIR");
        set_env("HOME", &temp_home);
        set_env("SKILLSTAR_DATA_DIR", temp_home.join(".skillstar"));
        #[cfg(windows)]
        let previous_userprofile = std::env::var_os("USERPROFILE");
        #[cfg(windows)]
        set_env("USERPROFILE", &temp_home);

        let result = (|| -> Result<()> {
            let profiles = list_profiles();
            let claude = profiles
                .into_iter()
                .find(|profile| profile.id == "claude")
                .expect("claude profile should exist");
            assert!(
                claude.installed,
                "expected config root to count as installed"
            );
            assert!(
                !temp_home.join(".claude/skills").exists(),
                "read-only profile detection must not create the skills directory"
            );
            assert!(
                !temp_home.join(".codex").exists(),
                "read-only profile detection must not create unrelated agent roots"
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

    /// Integration guard for the real on-disk path: `list_profiles()`
    /// enrichment is read-only, and `toggle_profile` follows that default.
    #[test]
    fn toggle_profile_matches_default_enabled_state() -> Result<()> {
        let _guard = env_lock();

        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let temp_root =
            std::env::temp_dir().join(format!("skillstar-agent-profile-toggle-{}", stamp));
        let temp_home = temp_root.join("home");
        // Use Antigravity (IDE agent, directory-detected) so the test is
        // deterministic and doesn't depend on whether `claude` is on PATH on
        // the dev/CI machine. Creating its config root makes install detection
        // report true → default enabled=true.
        std::fs::create_dir_all(temp_home.join(".gemini").join("antigravity-cli"))?;
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
                .find(|profile| profile.id == "antigravity")
                .expect("antigravity profile should exist");
            assert!(
                initial.enabled,
                "expected default profile state to start enabled when config root exists"
            );

            let toggled = toggle_profile("antigravity")?;
            assert!(
                !toggled,
                "expected first toggle to disable the profile from its implicit enabled state"
            );

            let updated = list_profiles()
                .into_iter()
                .find(|profile| profile.id == "antigravity")
                .expect("antigravity profile should exist after toggle");
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

    /// Integration guard for the real on-disk custom-profile round-trip via the
    /// `add_custom_profile` façade + `TomlPrefsStore`.
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

            let saved = TomlPrefsStore
                .load()
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
}
