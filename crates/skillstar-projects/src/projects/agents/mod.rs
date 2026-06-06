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
pub use registry::{find_profile, AgentProfile};

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

#[cfg(test)]
mod tests {
    use super::builtin::{builtin_agent_data, BuiltinSpec};
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
        assert_eq!(BuiltinSpec(ag).project_skills_rel(), Some(".agents/skills"));
        assert_eq!(BuiltinSpec(gm).project_skills_rel(), Some(".gemini/skills"));
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
    fn toggle_known_builtin_defaults_to_enabled_via_mem_store() {
        let store = MemPrefsStore::new();
        // claude is a known builtin → implicit enabled → first toggle disables.
        assert!(
            !registry::toggle("claude", &store).unwrap(),
            "first toggle of a known builtin should disable it"
        );
        assert_eq!(store.load().enabled.get("claude"), Some(&false));
        // toggling again re-enables.
        assert!(registry::toggle("claude", &store).unwrap());
    }

    /// Integration guard for the real on-disk path: `list_profiles()` enrichment
    /// (`detect_installed` provisions the dir → `installed` → `enabled` defaults
    /// to `installed`) plus the `TomlPrefsStore` round-trip via `toggle_profile`.
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
