//! Project-level skill configuration management.
//!
//! All config data lives in SkillStar's data directory (`skillstar/projects/`).
//! Project directories receive symlinks or explicit directory copies.
//!
//! This module is the frozen public façade. Behaviour lives in cohesive
//! submodules: `index` (project registry), `store` (skills-list persistence),
//! `sync` (deployment), `scan` (read-only inspection), `rebuild`
//! (reconstruct-from-disk), `refresh` (copy-deploy upkeep). The
//! `project_manifest::*` paths and signatures are stable for all consumers.

mod helpers;
mod index;
mod rebuild;
mod refresh;
mod scan;
mod store;
mod sync;
mod types;

pub use types::{
    AmbiguousGroup, CascadeUpdateSummary, DetectedAgent, ImportResult, ImportTarget,
    ProjectAgentDetection, ProjectDeployMode, ProjectEntry, ProjectScanResult, ScannedSkill,
    SkillsList,
};
pub use types::{deploy_skill_auto, ensure_project_root_exists, prune_deploy_modes_for_agents};

pub use index::{list_projects, register_project, remove_project, update_project_path};
pub use rebuild::rebuild_skills_list_from_disk;
pub use refresh::refresh_stale_copies;
pub use scan::{detect_project_agents, scan_project_skills};
pub use store::{load_skills_list, save_skills_list};
pub use sync::{
    add_skills_to_project, full_sync, remove_skill_from_all_projects, save_and_sync,
    save_skills_list_only,
};

// ── Cascade update: refresh copy-deployed skills across all projects ──

/// After a hub skill is updated, push the new content into every project that
/// deploys it via **copy** mode. Symlink deployments already track the hub live
/// and need no action; [`refresh_stale_copies`] handles the per-project diff
/// (it is idempotent — skills whose content matches the hub are skipped).
///
/// Only projects whose `skills-list.json` references at least one of the
/// updated skills are touched. Returns the names of projects that actually had
/// a copy refreshed.
pub fn cascade_skill_update_to_projects(skills: &[String]) -> CascadeUpdateSummary {
    let mut summary = CascadeUpdateSummary::default();
    if skills.is_empty() {
        return summary;
    }

    for project in list_projects() {
        let Some(skills_list) = load_skills_list(&project.name) else {
            continue;
        };

        let touches_updated_skill = skills_list
            .agents
            .values()
            .flatten()
            .any(|deployed| skills.iter().any(|updated| updated == deployed));
        if !touches_updated_skill {
            continue;
        }

        match refresh_stale_copies(&project.path) {
            Ok(0) => {}
            Ok(_) => summary.projects_updated.push(project.name.clone()),
            Err(err) => {
                tracing::warn!(
                    target: "sync",
                    project = %project.name,
                    error = %format!("{err:#}"),
                    "Cascade refresh failed for project"
                );
            }
        }
    }

    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::{Context, Result};
    use skillstar_core::infra::paths as fs_paths;
    use std::collections::HashMap;
    use std::ffi::OsStr;
    use std::path::PathBuf;
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
    fn full_sync_removes_symlinks_for_deselected_agents() -> Result<()> {
        let _guard = env_lock();

        let temp_root = make_temp_root("project-sync-remove")?;
        let previous_home = std::env::var_os("HOME");
        set_env("HOME", temp_root.join("home"));
        #[cfg(windows)]
        let previous_userprofile = std::env::var_os("USERPROFILE");
        #[cfg(windows)]
        set_env("USERPROFILE", temp_root.join("home"));

        let result = (|| -> Result<()> {
            let hub_skill = fs_paths::hub_skills_dir().join("demo-skill");
            std::fs::create_dir_all(&hub_skill)?;
            std::fs::write(hub_skill.join("SKILL.md"), "description: test")?;

            let project_path = temp_root.join("workspace").join("demo-project");
            std::fs::create_dir_all(&project_path)?;

            let mut first_agents = HashMap::new();
            first_agents.insert("claude".to_string(), vec!["demo-skill".to_string()]);
            first_agents.insert("codex".to_string(), vec!["demo-skill".to_string()]);
            let first = SkillsList {
                agents: first_agents,
                deploy_modes: HashMap::new(),
                updated_at: chrono::Utc::now().to_rfc3339(),
            };
            full_sync(&project_path.to_string_lossy(), &first, None)?;

            let claude_link = project_path.join(".claude/skills/demo-skill");
            let codex_link = project_path.join(".codex/skills/demo-skill");
            assert!(
                claude_link.is_symlink(),
                "expected initial symlink to exist"
            );
            assert!(
                codex_link.is_symlink(),
                "expected initial codex symlink to exist"
            );

            // Second sync: deselect all agents. Pass previous agents as cleanup
            // to mirror the diff that save_and_sync computes in production.
            let cleanup = vec!["claude".to_string(), "codex".to_string()];
            let second = SkillsList {
                agents: HashMap::new(),
                deploy_modes: HashMap::new(),
                updated_at: chrono::Utc::now().to_rfc3339(),
            };
            full_sync(&project_path.to_string_lossy(), &second, Some(&cleanup))?;

            assert!(
                claude_link.symlink_metadata().is_err(),
                "expected stale symlink to be removed after deselecting agent"
            );
            assert!(
                codex_link.symlink_metadata().is_err(),
                "expected stale .codex symlink to be removed after deselecting agent"
            );
            assert!(
                !project_path.join(".claude/skills").exists(),
                "expected empty .claude/skills directory to be pruned"
            );
            assert!(
                !project_path.join(".claude").exists(),
                "expected empty .claude directory to be pruned"
            );
            assert!(
                !project_path.join(".codex/skills").exists(),
                "expected empty .codex/skills directory to be pruned"
            );
            assert!(
                !project_path.join(".codex").exists(),
                "expected empty .codex directory to be pruned"
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
    fn save_and_sync_prunes_zero_skill_agents_without_creating_empty_dirs() -> Result<()> {
        let _guard = env_lock();

        let temp_root = make_temp_root("project-sync-empty-agent")?;
        let previous_home = std::env::var_os("HOME");
        set_env("HOME", temp_root.join("home"));
        #[cfg(windows)]
        let previous_userprofile = std::env::var_os("USERPROFILE");
        #[cfg(windows)]
        set_env("USERPROFILE", temp_root.join("home"));

        let result = (|| -> Result<()> {
            let hub_skill = fs_paths::hub_skills_dir().join("demo-skill");
            std::fs::create_dir_all(&hub_skill)?;
            std::fs::write(hub_skill.join("SKILL.md"), "description: test")?;

            let project_path = temp_root.join("workspace").join("demo-project");
            std::fs::create_dir_all(&project_path)?;
            let project_path_str = project_path.to_string_lossy().to_string();

            let initial_agents =
                HashMap::from([("claude".to_string(), vec!["demo-skill".to_string()])]);
            save_and_sync(&project_path_str, initial_agents, HashMap::new())?;

            let claude_link = project_path.join(".claude/skills/demo-skill");
            assert!(
                claude_link.is_symlink(),
                "expected initial skill deployment"
            );

            let emptied_agents = HashMap::from([
                ("claude".to_string(), Vec::new()),
                ("codex".to_string(), Vec::new()),
            ]);
            let (project_name, count) =
                save_and_sync(&project_path_str, emptied_agents, HashMap::new())?;

            assert_eq!(count, 0, "expected no skills to be synced");

            let skills_list = load_skills_list(&project_name)
                .expect("expected persisted skills list after empty sync");
            assert!(
                skills_list.agents.is_empty(),
                "expected zero-skill agents to be pruned from metadata"
            );
            assert!(
                claude_link.symlink_metadata().is_err(),
                "expected prior skill deployment to be removed"
            );
            assert!(
                !project_path.join(".claude/skills").exists(),
                "expected .claude/skills to be pruned after last skill removal"
            );
            assert!(
                !project_path.join(".claude").exists(),
                "expected .claude to be pruned after last skill removal"
            );
            assert!(
                !project_path.join(".codex").exists(),
                "expected unused codex folder to never be created"
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
    fn remove_skill_from_all_projects_cleans_metadata_and_symlinks() -> Result<()> {
        let _guard = env_lock();

        let temp_root = make_temp_root("project-remove-skill")?;
        let previous_home = std::env::var_os("HOME");
        set_env("HOME", temp_root.join("home"));
        #[cfg(windows)]
        let previous_userprofile = std::env::var_os("USERPROFILE");
        #[cfg(windows)]
        set_env("USERPROFILE", temp_root.join("home"));

        let result = (|| -> Result<()> {
            let hub_skill = fs_paths::hub_skills_dir().join("demo-skill");
            std::fs::create_dir_all(&hub_skill)?;
            std::fs::write(hub_skill.join("SKILL.md"), "description: test")?;

            let project_path = temp_root.join("workspace").join("demo-project");
            std::fs::create_dir_all(&project_path)?;

            let mut agents = HashMap::new();
            agents.insert("claude".to_string(), vec!["demo-skill".to_string()]);
            save_and_sync(&project_path.to_string_lossy(), agents, HashMap::new())?;

            let project = list_projects()
                .into_iter()
                .find(|project| project.path == project_path.to_string_lossy())
                .expect("expected project to be registered");
            let link_path = project_path.join(".claude/skills/demo-skill");
            assert!(
                link_path.is_symlink(),
                "expected project skill symlink before cleanup"
            );

            let touched = remove_skill_from_all_projects("demo-skill")?;
            assert!(
                touched.iter().any(|name| name == &project.name),
                "expected cleanup to report the touched project"
            );

            let skills_list = load_skills_list(&project.name)
                .expect("expected project skills list to remain readable");
            assert!(
                skills_list.agents.is_empty(),
                "expected removed skill to be pruned from project metadata"
            );
            assert!(
                link_path.symlink_metadata().is_err(),
                "expected project skill symlink to be removed"
            );
            assert!(
                !project_path.join(".claude/skills").exists(),
                "expected empty project skill directory to be pruned"
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
    fn add_skills_to_project_rejects_invalid_explicit_agents() -> Result<()> {
        let _guard = env_lock();

        let temp_root = make_temp_root("project-add-invalid-agent")?;
        let previous_home = std::env::var_os("HOME");
        set_env("HOME", temp_root.join("home"));
        #[cfg(windows)]
        let previous_userprofile = std::env::var_os("USERPROFILE");
        #[cfg(windows)]
        set_env("USERPROFILE", temp_root.join("home"));

        let result = (|| -> Result<()> {
            let hub_skill = fs_paths::hub_skills_dir().join("demo-skill");
            std::fs::create_dir_all(&hub_skill)?;
            std::fs::write(hub_skill.join("SKILL.md"), "description: test")?;

            let project_path = temp_root.join("workspace").join("demo-project");
            std::fs::create_dir_all(&project_path)?;

            let skills = vec!["demo-skill".to_string()];
            let agents = vec!["nonexistent-agent".to_string()];
            let err = add_skills_to_project(&project_path.to_string_lossy(), &skills, &agents)
                .expect_err("expected invalid explicit agent selection to fail");
            assert!(
                err.to_string()
                    .contains("No valid project-level agents selected"),
                "unexpected error: {err}"
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
    fn add_skills_to_project_does_not_create_dirs_for_empty_or_missing_skills() -> Result<()> {
        let _guard = env_lock();

        let temp_root = make_temp_root("project-add-empty-skills")?;
        let previous_home = std::env::var_os("HOME");
        set_env("HOME", temp_root.join("home"));
        #[cfg(windows)]
        let previous_userprofile = std::env::var_os("USERPROFILE");
        #[cfg(windows)]
        set_env("USERPROFILE", temp_root.join("home"));

        let result = (|| -> Result<()> {
            let project_path = temp_root.join("workspace").join("demo-project");
            std::fs::create_dir_all(&project_path)?;
            let agents = vec!["claude".to_string(), "codex".to_string()];

            let empty_count = add_skills_to_project(&project_path.to_string_lossy(), &[], &agents)?;
            assert_eq!(empty_count, 0);

            let missing = vec!["missing-skill".to_string()];
            let missing_count =
                add_skills_to_project(&project_path.to_string_lossy(), &missing, &agents)?;
            assert_eq!(missing_count, 0);

            assert!(
                !project_path.join(".claude").exists(),
                "empty deploy must not create Claude project folders"
            );
            assert!(
                !project_path.join(".codex").exists(),
                "missing skills must not create Codex project folders"
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
    fn cascade_refreshes_copy_deployed_skill_and_reports_project() -> Result<()> {
        let _guard = env_lock();

        let temp_root = make_temp_root("project-cascade")?;
        let previous_home = std::env::var_os("HOME");
        set_env("HOME", temp_root.join("home"));
        #[cfg(windows)]
        let previous_userprofile = std::env::var_os("USERPROFILE");
        #[cfg(windows)]
        set_env("USERPROFILE", temp_root.join("home"));

        let result = (|| -> Result<()> {
            let hub_skill = fs_paths::hub_skills_dir().join("demo-skill");
            std::fs::create_dir_all(&hub_skill)?;
            std::fs::write(hub_skill.join("SKILL.md"), "description: v1")?;

            // Copy-deploy demo-skill into a project (deploy_modes => Copy).
            let project_path = temp_root.join("workspace").join("copy-project");
            std::fs::create_dir_all(&project_path)?;
            let project_path_str = project_path.to_string_lossy().to_string();

            let claude_profile = crate::projects::agents::list_profiles()
                .into_iter()
                .find(|p| p.id == "claude")
                .expect("claude profile must exist");
            let mut deploy_modes = HashMap::new();
            deploy_modes.insert(
                claude_profile.project_skills_rel.clone(),
                ProjectDeployMode::Copy,
            );
            let skills_list = SkillsList {
                agents: HashMap::from([(
                    "claude".to_string(),
                    vec!["demo-skill".to_string()],
                )]),
                deploy_modes,
                updated_at: chrono::Utc::now().to_rfc3339(),
            };
            register_project(&project_path_str)?;
            let project_name = ensure_project_root_exists(&project_path_str)
                .ok()
                .and_then(|_| {
                    list_projects()
                        .into_iter()
                        .find(|p| p.path == project_path_str)
                        .map(|p| p.name)
                })
                .expect("registered project must resolve a name");
            save_skills_list(&project_name, &skills_list)?;
            full_sync(&project_path_str, &skills_list, None)?;

            let deployed = project_path
                .join(&claude_profile.project_skills_rel)
                .join("demo-skill")
                .join("SKILL.md");
            assert!(
                deployed.is_file() && !skillstar_core::infra::fs_ops::is_link(&deployed),
                "expected a real copied file, not a symlink"
            );
            assert_eq!(std::fs::read_to_string(&deployed)?, "description: v1");

            // A second project that does NOT use the skill must be untouched.
            let other_path = temp_root.join("workspace").join("other-project");
            std::fs::create_dir_all(&other_path)?;
            register_project(&other_path.to_string_lossy())?;

            // Update the hub content, then cascade.
            std::fs::write(hub_skill.join("SKILL.md"), "description: v2-updated")?;
            let summary = cascade_skill_update_to_projects(&["demo-skill".to_string()]);

            assert_eq!(
                std::fs::read_to_string(&deployed)?,
                "description: v2-updated",
                "copy-deployed skill should be refreshed to new hub content"
            );
            assert!(
                summary
                    .projects_updated
                    .iter()
                    .any(|name| name.contains("copy-project")),
                "cascade summary should report the refreshed project, got {:?}",
                summary.projects_updated
            );
            assert!(
                !summary
                    .projects_updated
                    .iter()
                    .any(|name| name.contains("other-project")),
                "projects not using the skill must not be reported"
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

    fn make_temp_root(suffix: &str) -> Result<PathBuf> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("failed to read system time")?
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "skillstar-project-manifest-{}-{}-{}",
            suffix,
            std::process::id(),
            stamp
        ));
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create temp dir: {}", dir.display()))?;
        Ok(dir)
    }
}
