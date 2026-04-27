//! Project-level skill configuration management.
//!
//! Re-exports the reusable core from `skillstar_projects::project_manifest`.
//! The app-coupled `import_scanned_skills` function is kept locally because
//! it depends on `local_skill` (adoption + symlink reconciliation).

pub use skillstar_projects::project_manifest::*;

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;

use skillstar_infra::{fs_ops, paths as fs_paths};

/// Import discovered skills into local storage and update the project's
/// skills-list.
///
/// Strategy A: Adopt + Replace with Symlink.
/// - If a skill doesn't exist in the hub, move it into `skills-local/` and
///   expose it through the hub symlink in `skills/`.
/// - Replace the original real directory in the project with a symlink to the
///   hub entry.
/// - If the skill already exists in the hub, skip adoption but still write
///   the mapping into `skills-list.json`.
/// - Finally, merge all imported skills into the project's skills-list.json.
pub fn import_scanned_skills(
    project_path: &str,
    project_name: &str,
    targets: &[ImportTarget],
) -> Result<ImportResult> {
    let entry = register_project(project_path)?;
    let canonical_project_name = entry.name;

    let hub_dir = fs_paths::hub_skills_dir();
    crate::core::skills::local_skill::reconcile_hub_symlinks();
    let profiles = crate::core::projects::agents::list_profiles();
    let project = Path::new(project_path);

    std::fs::create_dir_all(&hub_dir)
        .with_context(|| format!("failed to create hub dir: {}", hub_dir.display()))?;

    // Preserve any previously chosen owner for shared project paths.
    let mut existing = load_skills_list(&canonical_project_name)
        .or_else(|| {
            if project_name != canonical_project_name.as_str() {
                load_skills_list(project_name)
            } else {
                None
            }
        })
        .unwrap_or_default();

    let mut owner_by_path: HashMap<String, String> = HashMap::new();
    for agent_id in existing.agents.keys() {
        let Some(profile) = profiles.iter().find(|p| &p.id == agent_id) else {
            continue;
        };
        if !profile.has_project_skills() {
            continue;
        }
        owner_by_path
            .entry(profile.project_skills_rel.clone())
            .or_insert_with(|| agent_id.clone());
    }

    let mut imported_to_hub = Vec::new();
    let mut symlink_count = 0u32;

    for target in targets {
        // Find the agent profile used to locate the on-disk project folder.
        let Some(source_profile) = profiles.iter().find(|p| p.id == target.agent_id) else {
            continue;
        };
        if !source_profile.has_project_skills() {
            continue;
        }

        let source_dir = project
            .join(&source_profile.project_skills_rel)
            .join(&target.name);
        if !source_dir.exists() {
            continue;
        }

        // Skip if already a symlink (already managed)
        if fs_ops::is_link(&source_dir) {
            continue;
        }

        // Only valid skill folders are safe to import and replace.
        if !source_dir.join("SKILL.md").exists() {
            continue;
        }

        let effective_agent_id = owner_by_path
            .get(&source_profile.project_skills_rel)
            .cloned()
            .unwrap_or_else(|| target.agent_id.clone());
        owner_by_path
            .entry(source_profile.project_skills_rel.clone())
            .or_insert_with(|| effective_agent_id.clone());

        let hub_skill_dir = hub_dir.join(&target.name);

        // Step 1: Adopt into local storage if not already present in the hub.
        if !hub_skill_dir.exists() {
            crate::core::skills::local_skill::adopt_existing_dir(&target.name, &source_dir)
                .with_context(|| {
                    format!(
                        "failed to adopt discovered project skill '{}' into skills-local",
                        target.name
                    )
                })?;
            imported_to_hub.push(target.name.clone());
        } else {
            // Step 2a: Skill already exists in the hub, so replace the
            // unmanaged project copy with a symlink to the canonical hub entry.
            std::fs::remove_dir_all(&source_dir)
                .with_context(|| format!("failed to remove real dir: {}", source_dir.display()))?;
        }

        // Step 2b: Point the project entry at the hub entry, which may itself
        // be a symlink into `skills-local/`.
        fs_ops::create_symlink_or_copy(&hub_skill_dir, &source_dir)
            .with_context(|| format!("failed to create symlink for skill '{}'", target.name))?;

        symlink_count += 1;

        let agent_skills = existing.agents.entry(effective_agent_id).or_default();
        if !agent_skills.contains(&target.name) {
            agent_skills.push(target.name.clone());
        }
    }

    existing.updated_at = chrono::Utc::now().to_rfc3339();
    save_skills_list(&canonical_project_name, &existing)?;

    Ok(ImportResult {
        imported_to_hub,
        skills_list_updated: true,
        symlink_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use std::ffi::OsStr;
    use std::path::PathBuf;
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
    fn import_scanned_skills_registers_project_when_missing() -> Result<()> {
        let _guard = env_lock();

        let temp_root = make_temp_root("project-import-register")?;
        let previous_home = std::env::var_os("HOME");
        set_env("HOME", temp_root.join("home"));
        #[cfg(windows)]
        let previous_userprofile = std::env::var_os("USERPROFILE");
        #[cfg(windows)]
        set_env("USERPROFILE", temp_root.join("home"));

        let result = (|| -> Result<()> {
            let project_path = temp_root.join("workspace").join("demo-import-project");
            let project_path_str = project_path.to_string_lossy().to_string();
            let source_skill_dir = project_path.join(".claude/skills/legacy-skill");

            std::fs::create_dir_all(&source_skill_dir)?;
            std::fs::write(source_skill_dir.join("SKILL.md"), "description: legacy")?;

            let targets = vec![ImportTarget {
                name: "legacy-skill".to_string(),
                agent_id: "claude".to_string(),
            }];

            let import_result =
                import_scanned_skills(&project_path_str, "demo-import-project", &targets)?;
            assert!(import_result.skills_list_updated);
            assert!(
                import_result
                    .imported_to_hub
                    .iter()
                    .any(|name| name == "legacy-skill"),
                "expected legacy skill to be exposed through the hub during import"
            );

            let registered = list_projects()
                .into_iter()
                .find(|project| project.path == project_path_str)
                .expect("expected imported project to be auto-registered");
            let skills_list = load_skills_list(&registered.name)
                .expect("expected skills-list.json for registered project");
            let claude_skills = skills_list
                .agents
                .get("claude")
                .expect("expected imported skills under claude agent");
            assert!(
                claude_skills.iter().any(|skill| skill == "legacy-skill"),
                "expected imported skill to be present in project's skills list"
            );
            let local_skill_dir = skillstar_infra::paths::local_skills_dir().join("legacy-skill");
            let hub_skill_dir = fs_paths::hub_skills_dir().join("legacy-skill");
            assert!(
                local_skill_dir.is_dir(),
                "expected imported skill to be moved into skills-local"
            );
            assert!(
                hub_skill_dir.is_symlink(),
                "expected hub entry for imported skill to be a symlink"
            );
            assert_eq!(
                std::fs::read_link(&hub_skill_dir)?,
                local_skill_dir,
                "expected hub entry to point at skills-local storage"
            );
            assert!(
                source_skill_dir.is_symlink(),
                "expected original project skill directory to be replaced with symlink"
            );
            assert_eq!(
                std::fs::read_link(&source_skill_dir)?,
                hub_skill_dir,
                "expected project skill directory to point at the canonical hub entry"
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
    fn import_scanned_skills_skips_non_skill_directories() -> Result<()> {
        let _guard = env_lock();

        let temp_root = make_temp_root("project-import-skip-invalid")?;
        let previous_home = std::env::var_os("HOME");
        set_env("HOME", temp_root.join("home"));
        #[cfg(windows)]
        let previous_userprofile = std::env::var_os("USERPROFILE");
        #[cfg(windows)]
        set_env("USERPROFILE", temp_root.join("home"));

        let result = (|| -> Result<()> {
            let project_path = temp_root.join("workspace").join("demo-import-project");
            let project_path_str = project_path.to_string_lossy().to_string();
            let source_dir = project_path.join(".claude/skills/not-a-skill");

            std::fs::create_dir_all(&source_dir)?;
            std::fs::write(source_dir.join("README.md"), "not a skill")?;

            let targets = vec![ImportTarget {
                name: "not-a-skill".to_string(),
                agent_id: "claude".to_string(),
            }];

            let import_result =
                import_scanned_skills(&project_path_str, "demo-import-project", &targets)?;

            assert!(
                import_result.imported_to_hub.is_empty(),
                "expected invalid directories to be skipped during import"
            );
            assert_eq!(
                import_result.symlink_count, 0,
                "expected invalid directories to remain untouched"
            );
            assert!(
                source_dir.is_dir() && !source_dir.is_symlink(),
                "expected invalid source directory to remain a real directory"
            );

            let registered = list_projects()
                .into_iter()
                .find(|project| project.path == project_path_str)
                .expect("expected project registration during import");
            let skills_list = load_skills_list(&registered.name)
                .expect("expected skills-list.json for registered project");
            assert!(
                !skills_list
                    .agents
                    .values()
                    .flatten()
                    .any(|name| name == "not-a-skill"),
                "expected invalid directories to be excluded from project metadata"
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
    fn import_scanned_skills_preserves_shared_path_owner() -> Result<()> {
        let _guard = env_lock();

        let temp_root = make_temp_root("project-import-owner")?;
        let previous_home = std::env::var_os("HOME");
        set_env("HOME", temp_root.join("home"));
        #[cfg(windows)]
        let previous_userprofile = std::env::var_os("USERPROFILE");
        #[cfg(windows)]
        set_env("USERPROFILE", temp_root.join("home"));

        let result = (|| -> Result<()> {
            let project_path = temp_root.join("workspace").join("demo-import-project");
            let project_path_str = project_path.to_string_lossy().to_string();
            // With Codex now using .codex/skills (unique path), the import
            // should attribute the skill to Codex directly, not merge it into
            // antigravity's .agents/skills path.
            let source_skill_dir = project_path.join(".codex/skills/shared-skill");

            std::fs::create_dir_all(&source_skill_dir)?;
            std::fs::write(source_skill_dir.join("SKILL.md"), "description: shared")?;

            let entry = register_project(&project_path_str)?;
            let existing = SkillsList {
                agents: HashMap::from([("antigravity".to_string(), Vec::new())]),
                deploy_modes: HashMap::new(),
                updated_at: chrono::Utc::now().to_rfc3339(),
            };
            save_skills_list(&entry.name, &existing)?;

            let targets = vec![ImportTarget {
                name: "shared-skill".to_string(),
                agent_id: "codex".to_string(),
            }];

            let import_result =
                import_scanned_skills(&project_path_str, "demo-import-project", &targets)?;
            assert_eq!(import_result.symlink_count, 1);

            let skills_list = load_skills_list(&entry.name)
                .expect("expected updated skills-list.json for registered project");
            // Codex now has its own unique path (.codex/skills), so the import
            // should attribute the skill to codex, not antigravity.
            assert!(
                skills_list
                    .agents
                    .get("codex")
                    .is_some_and(|skills| skills.iter().any(|skill| skill == "shared-skill")),
                "expected codex to own the imported skill at its unique .codex/skills path"
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
