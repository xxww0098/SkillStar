//! Agent profile management.
//!
//! Built-in and custom agents both implement [`spec::AgentSpec`]; the
//! [`registry::AgentRegistry`] turns specs + persisted [`profile_storage`]
//! prefs into the enriched [`AgentProfile`] list. This module is the
//! public façade for agent profiles; the frozen 8-field `AgentProfile` IPC
//! shape (incl. the `installed` mirror of `enabled`, see D-009) must keep its
//! paths/signatures stable.

mod builtin;
mod custom;
mod profile_storage;
mod registry;
mod spec;
mod validation;

use anyhow::Result;

pub use builtin::additional_project_skill_reads;
pub use custom::CustomProfileDef;
pub use profile_storage::AgentDeployMode;
pub use registry::{AgentProfile, compatible_profile_id, find_profile};

use profile_storage::TomlPrefsStore;

/// List all agent profiles with static capabilities and manual activation prefs.
pub fn list_profiles() -> Vec<AgentProfile> {
    registry::AgentRegistry::load(&TomlPrefsStore).into_profiles()
}

/// List all agent profiles without scanning global skills directories for
/// `synced_count` (`synced_count` is `0`). Use on paths that only inspect
/// profile capabilities, ids, or paths.
pub fn list_profile_specs() -> Vec<AgentProfile> {
    registry::AgentRegistry::load(&TomlPrefsStore).into_profile_specs()
}

/// Extra directories that must receive the same global deployments as the
/// profile's `global_skills_dir`.
///
/// Empty for every Agent with a single skills directory. Antigravity is the
/// exception: one profile fanning out to App, CLI, and shared skills directories
/// (see `builtin::GLOBAL_MIRROR_DEFS`).
pub fn global_mirror_dirs(agent_id: &str) -> Vec<std::path::PathBuf> {
    builtin::mirror_dirs(
        registry::compatible_profile_id(agent_id),
        &ss_core::infra::paths::home_dir(),
    )
}

/// Skill names temporarily suspended from one physical Global skills directory.
///
/// This recovery journal is keyed by the resolved directory, never by an Agent
/// id, because multiple profiles may legitimately share one folder.
pub fn suspended_global_skill_names(target: &std::path::Path) -> Vec<String> {
    profile_storage::suspended_global_skill_names(target, &TomlPrefsStore)
}

/// Replace the recovery-only set for one physical Global skills directory.
///
/// An empty set clears the completed journal. Writes are atomic so a pause
/// intent reaches disk before any linked skill can be removed.
pub fn replace_suspended_global_skill_names(
    target: &std::path::Path,
    names: &[String],
) -> Result<()> {
    profile_storage::replace_suspended_global_skill_names(target, names, &TomlPrefsStore)
}

/// Add (or replace by id) a custom agent profile.
pub fn add_custom_profile(def: CustomProfileDef) -> Result<()> {
    custom::add(def, &TomlPrefsStore)
}

/// Toggle an agent's enabled state; returns the new state.
pub fn toggle_profile(id: &str) -> Result<bool> {
    let enabled = registry::toggle(id, &TomlPrefsStore)?;
    crate::deployment::invalidate_profile_cache();
    if let Err(error) = crate::deployment::reconcile_shared_agents_on_profile_toggle(id) {
        tracing::warn!(
            target: "agents",
            agent = id,
            error = %error,
            "failed to reconcile shared ~/.agents/skills deployments after profile toggle"
        );
    }
    Ok(enabled)
}

/// Global default skill deployment mode (`symlink` or `copy`).
pub fn get_default_deploy_mode() -> AgentDeployMode {
    profile_storage::get_default_deploy_mode(&TomlPrefsStore)
}

/// Persist the global default skill deployment mode (`symlink` or `copy`).
pub fn set_default_deploy_mode(mode: AgentDeployMode) -> Result<()> {
    profile_storage::set_default_deploy_mode(mode, &TomlPrefsStore)
}

/// All explicit per-agent deploy mode overrides keyed by normalized agent id.
pub fn get_agent_deploy_mode_overrides() -> std::collections::BTreeMap<String, AgentDeployMode> {
    profile_storage::get_agent_deploy_mode_overrides(&TomlPrefsStore)
}

/// Explicit deploy mode override for `agent_id`, if set (`None` = follow default).
pub fn get_agent_deploy_mode_override(agent_id: &str) -> Option<AgentDeployMode> {
    let id = compatible_profile_id(agent_id);
    profile_storage::get_agent_deploy_mode_override(id, &TomlPrefsStore)
}

/// Persist or clear (`None`) the deploy mode override for `agent_id`.
pub fn set_agent_deploy_mode_override(agent_id: &str, mode: Option<AgentDeployMode>) -> Result<()> {
    let id = compatible_profile_id(agent_id);
    if !registry::has_profile(id, &TomlPrefsStore) {
        return Err(anyhow::anyhow!("Agent profile '{}' not found", id));
    }
    profile_storage::set_agent_deploy_mode_override(id, mode, &TomlPrefsStore)
}

/// Effective deploy mode for `agent_id` (explicit override or global default).
pub fn effective_deploy_mode(agent_id: &str) -> AgentDeployMode {
    let id = compatible_profile_id(agent_id);
    profile_storage::effective_deploy_mode(id, &TomlPrefsStore)
}

/// Agents whose runtime cannot traverse Windows directory junctions (must use a
/// strict symlink or fall back to copy).
pub fn agent_disallows_junction(agent_id: &str) -> bool {
    matches!(compatible_profile_id(agent_id), "antigravity" | "crush")
}

/// Agents with their own private skills dir that also natively scan `~/.agents/skills`.
pub fn agent_reads_shared_agents_skills(agent_id: &str) -> bool {
    matches!(
        compatible_profile_id(agent_id),
        "cursor" | "gemini-cli" | "droid" | "opencode"
    )
}

/// Ecosystem shared user-level skills directory (`~/.agents/skills`), distinct
/// from SkillStar's internal canonical root (`~/.skillstar/data/skills/installed`).
pub fn shared_agents_skills_dir() -> std::path::PathBuf {
    ss_core::infra::paths::home_dir()
        .join(".agents")
        .join("skills")
}

/// Whether `profile.global_skills_dir` targets `~/.agents/skills`.
pub fn uses_shared_agents_skills_root(profile: &AgentProfile) -> bool {
    if !profile.has_global_skills() {
        return false;
    }
    ss_core::infra::fs_ops::canonicalize_existing_prefix(&profile.global_skills_dir)
        == ss_core::infra::fs_ops::canonicalize_existing_prefix(&shared_agents_skills_dir())
}

#[cfg(test)]
mod tests {
    use super::builtin::{BuiltinSpec, builtin_agent_data};
    use super::profile_storage::{self, MemPrefsStore, PrefsStore, TomlPrefsStore};
    use super::spec::AgentSpec;
    use super::validation::{validate_global_skills_dir, validate_project_skills_rel};
    use super::*;
    use anyhow::Result;
    use std::ffi::OsStr;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        crate::lock_test_env()
    }

    fn set_env<K: AsRef<OsStr>, V: AsRef<OsStr>>(key: K, value: V) {
        unsafe { std::env::set_var(key, value) }
    }

    fn remove_env<K: AsRef<OsStr>>(key: K) {
        unsafe { std::env::remove_var(key) }
    }

    #[test]
    fn suspended_global_skill_names_are_directory_scoped_and_normalized() -> Result<()> {
        let store = MemPrefsStore::new();
        let temp = tempfile::tempdir()?;
        let target = temp.path().join("shared").join("skills");
        std::fs::create_dir_all(&target)?;
        let other_target = temp.path().join("other").join("skills");
        std::fs::create_dir_all(&other_target)?;

        profile_storage::replace_suspended_global_skill_names(
            &target,
            &[
                " beta ".to_string(),
                "alpha".to_string(),
                "alpha".to_string(),
            ],
            &store,
        )?;

        assert_eq!(
            profile_storage::suspended_global_skill_names(&target, &store),
            vec!["alpha", "beta"],
            "a physical directory owns one sorted recovery set",
        );
        assert!(
            profile_storage::suspended_global_skill_names(&other_target, &store).is_empty(),
            "a different directory must not inherit a suspended set",
        );

        profile_storage::replace_suspended_global_skill_names(&target, &[], &store)?;
        assert!(profile_storage::suspended_global_skill_names(&target, &store).is_empty());
        Ok(())
    }

    #[test]
    fn openclaw_uses_its_upstream_project_skills_directory() {
        let openclaw = builtin_agent_data()
            .iter()
            .find(|d| d.id == "openclaw")
            .expect("openclaw builtin should exist");
        assert_eq!(BuiltinSpec(openclaw).project_skills_rel(), Some("skills"));
    }

    #[test]
    fn universal_agents_share_the_open_skills_project_dir() {
        let data = builtin_agent_data();
        let ids = [
            "antigravity",
            "codex",
            "cursor",
            "droid",
            "fx",
            "gemini-cli",
            "kilo",
            "pi",
        ];
        for id in ids {
            let agent = data.iter().find(|d| d.id == id).unwrap();
            assert_eq!(
                BuiltinSpec(agent).project_skills_rel(),
                Some(".agents/skills"),
                "{id} must keep the open-skills project path"
            );
        }
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
    fn validate_global_skills_dir_refuses_sweepable_roots_and_the_canonical_root() {
        let sandbox = crate::test_sandbox::Sandbox::production();
        let home = &sandbox.home();

        // Allowed: the project-only agent's empty value, a dir under home and
        // an absolute path outside home. None of these can be required to
        // look a particular way.
        assert!(validate_global_skills_dir(Path::new(""), home).is_ok());
        assert!(validate_global_skills_dir(&home.join(".dsh/skills"), home).is_ok());
        assert!(validate_global_skills_dir(Path::new("/opt/agent-skills"), home).is_ok());

        // Refused: "unlink all" would sweep the entire directory.
        assert!(validate_global_skills_dir(home, home).is_err());
        assert!(validate_global_skills_dir(Path::new("/"), home).is_err());
        // `~/.agents/skills` is an Agent directory, not the SkillStar store.
        assert!(validate_global_skills_dir(&home.join(".agents/skills"), home).is_ok());
        // Refused: the canonical root holds the installed Skills themselves.
        let canonical = ss_core::infra::paths::agents_skills_root();
        assert!(validate_global_skills_dir(&canonical, home).is_err());
        assert!(validate_global_skills_dir(&canonical.join("foo"), home).is_err());
    }

    #[test]
    fn stored_custom_agent_inside_the_canonical_root_is_skipped_and_kept() {
        let sandbox = crate::test_sandbox::Sandbox::production();
        let store = MemPrefsStore::new();
        let mut prefs = profile_storage::ProfilePrefs::default();
        prefs.custom_profiles.push(CustomProfileDef {
            id: "nested".into(),
            display_name: "Nested".into(),
            global_skills_dir: ss_core::infra::paths::agents_skills_root()
                .join("nested")
                .display()
                .to_string(),
            project_skills_rel: ".nested/skills".into(),
            icon_data_uri: None,
        });
        prefs.custom_profiles.push(CustomProfileDef {
            id: "ok_agent".into(),
            display_name: "Ok".into(),
            global_skills_dir: sandbox.home().join(".ok/skills").display().to_string(),
            project_skills_rel: ".ok/skills".into(),
            icon_data_uri: None,
        });
        prefs.enabled.insert("nested".into(), true);
        store.save(&prefs).unwrap();

        let profiles = registry::AgentRegistry::load(&store).into_profiles();
        assert!(profiles.iter().all(|profile| profile.id != "nested"));
        assert!(profiles.iter().any(|profile| profile.id == "ok_agent"));
        assert!(
            store
                .load()
                .custom_profiles
                .iter()
                .any(|profile| profile.id == "nested"),
            "a skipped agent stays in the saved preferences"
        );
    }

    #[test]
    fn add_custom_profile_refuses_a_global_dir_that_would_sweep_home() {
        let _guard = env_lock();
        let store = MemPrefsStore::new();

        let error = custom::add(
            CustomProfileDef {
                id: "custom_home".into(),
                display_name: "Home".into(),
                global_skills_dir: "~/".into(),
                project_skills_rel: String::new(),
                icon_data_uri: None,
            },
            &store,
        )
        .unwrap_err();

        assert!(
            error.to_string().contains("home directory"),
            "unexpected error: {error}"
        );
        assert!(
            store.load().custom_profiles.is_empty(),
            "a refused profile must not be persisted"
        );
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
        assert_eq!(prefs.enabled.get("custom_ollama"), Some(&false));
    }

    #[test]
    fn editing_custom_profile_preserves_explicit_activation() -> Result<()> {
        let store = MemPrefsStore::new();
        let original = CustomProfileDef {
            id: "custom_manual".into(),
            display_name: "Manual".into(),
            global_skills_dir: "~/.manual/skills".into(),
            project_skills_rel: ".manual/skills".into(),
            icon_data_uri: None,
        };
        custom::add(original.clone(), &store)?;
        assert!(registry::toggle("custom_manual", &store)?);

        custom::add(
            CustomProfileDef {
                display_name: "Manual Updated".into(),
                ..original
            },
            &store,
        )?;

        assert_eq!(store.load().enabled.get("custom_manual"), Some(&true));
        Ok(())
    }

    #[test]
    fn existing_shared_target_does_not_activate_custom_agent() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let shared_skills_dir = temp.path().join(".agents").join("skills");
        std::fs::create_dir_all(&shared_skills_dir)?;

        let store = MemPrefsStore::new();
        custom::add(
            CustomProfileDef {
                id: "custom_shared_target".into(),
                display_name: "Shared Target".into(),
                global_skills_dir: shared_skills_dir.to_string_lossy().into_owned(),
                project_skills_rel: ".agents/skills".into(),
                icon_data_uri: None,
            },
            &store,
        )?;

        let profile = registry::AgentRegistry::load(&store)
            .into_profiles()
            .into_iter()
            .find(|profile| profile.id == "custom_shared_target")
            .expect("custom profile should be listed");

        assert!(!profile.enabled, "new custom profiles must start inactive");
        assert!(
            !profile.installed,
            "the compatibility field mirrors enabled"
        );
        Ok(())
    }

    #[test]
    fn toggle_known_builtin_starts_from_inactive() -> Result<()> {
        let store = MemPrefsStore::new();
        assert!(registry::toggle("openclaw", &store)?);
        assert_eq!(store.load().enabled.get("openclaw"), Some(&true));
        assert!(!registry::toggle("openclaw", &store)?);
        assert_eq!(store.load().enabled.get("openclaw"), Some(&false));
        Ok(())
    }

    #[test]
    fn empty_preferences_keep_every_builtin_inactive() {
        let profiles = registry::AgentRegistry::load(&MemPrefsStore::new()).into_profiles();
        assert!(!profiles.is_empty());
        assert!(profiles.iter().all(|profile| !profile.enabled));
        assert!(profiles.iter().all(|profile| !profile.installed));
    }

    /// An existing Agent config root must not alter the manual default.
    #[test]
    fn existing_agent_directory_does_not_change_default_enabled_state() -> Result<()> {
        let _guard = env_lock();

        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let temp_root =
            std::env::temp_dir().join(format!("skillstar-agent-profile-toggle-{}", stamp));
        let temp_home = temp_root.join("home");
        std::fs::create_dir_all(temp_home.join(".openclaw"))?;
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
                .find(|profile| profile.id == "openclaw")
                .expect("openclaw profile should exist");
            assert!(!initial.enabled);
            assert!(!initial.installed);

            let toggled = toggle_profile("openclaw")?;
            assert!(toggled, "first explicit toggle must enable the profile");

            let updated = list_profiles()
                .into_iter()
                .find(|profile| profile.id == "openclaw")
                .expect("openclaw profile should exist after toggle");
            assert!(updated.enabled);
            assert!(updated.installed, "compatibility field mirrors enabled");

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
