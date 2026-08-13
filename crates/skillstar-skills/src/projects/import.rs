//! Project import use case: adopt scanned project skills into the hub.
//!
//! Strategy A: Adopt + Replace with Symlink.
//! - Missing hub entry → adopt into skills-local and hub symlink
//! - Existing hub entry → replace unmanaged project copy with hub link
//! - Merge imported skills into skills-list.json

use anyhow::{Context, Result};
use skillstar_core::infra::{fs_ops, paths as fs_paths};
use std::collections::HashMap;
use std::path::Path;
use tracing::warn;

use super::{ImportResult, ImportTarget, load_skills_list, register_project, save_skills_list};
use skillstar_agents as agents;
use crate::local_skill;

/// Import discovered skills into local storage and update the project's skills-list.
pub fn import_scanned_skills(
    project_path: &str,
    project_name: &str,
    targets: &[ImportTarget],
) -> Result<ImportResult> {
    let _transaction_guard = crate::skill_update::acquire_update_transaction_lock()?;
    for target in targets {
        crate::skill_mutation::policy().ensure_skill_mutation_allowed(&target.name)?;
    }
    let entry = register_project(project_path)?;
    let canonical_project_name = entry.name;

    let hub_dir = fs_paths::hub_skills_dir();
    local_skill::reconcile_hub_symlinks();
    let profiles = agents::list_profiles();
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

        if fs_ops::is_link(&source_dir) {
            continue;
        }

        if !source_dir.join("SKILL.md").exists() {
            continue;
        }

        // Frontmatter quality gate (shared with the repo-install path): a
        // project skill without a usable description is not adoptable.
        if let Err(reason) = crate::validation::ensure_installable(&source_dir) {
            warn!(
                target: "projects_import",
                skill = %target.name,
                error = %reason,
                "skipping project skill rejected by frontmatter gate"
            );
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

        if !hub_skill_dir.exists() {
            local_skill::adopt_existing_dir_locked(&target.name, &source_dir).with_context(
                || {
                    format!(
                        "failed to adopt discovered project skill '{}' into skills-local",
                        target.name
                    )
                },
            )?;
            imported_to_hub.push(target.name.clone());
        } else {
            std::fs::remove_dir_all(&source_dir)
                .with_context(|| format!("failed to remove real dir: {}", source_dir.display()))?;
        }

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
    use crate::projects::{
        SkillsList, list_projects, load_skills_list, register_project, save_skills_list,
    };
    use std::ffi::OsStr;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn set_env<K: AsRef<OsStr>, V: AsRef<OsStr>>(key: K, value: V) {
        unsafe { std::env::set_var(key, value) }
    }
    fn remove_env<K: AsRef<OsStr>>(key: K) {
        unsafe { std::env::remove_var(key) }
    }

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("skillstar-import-{label}-{nanos}"))
    }

    fn write_skill(dir: &Path, name: &str) {
        let skill_dir = dir.join(name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {name} project skill\n---\n# {name}\n"),
        )
        .unwrap();
    }

    #[test]
    fn import_scanned_skills_registers_project_when_missing() -> Result<()> {
        let _guard = crate::lock_test_env();
        let data = unique_temp_dir("data");
        let project = unique_temp_dir("project");
        set_env("SKILLSTAR_DATA_DIR", &data);

        let result = (|| {
            let claude_skills = project.join(".claude").join("skills");
            std::fs::create_dir_all(&claude_skills)?;
            write_skill(&claude_skills, "demo-skill");

            let targets = vec![ImportTarget {
                name: "demo-skill".into(),
                agent_id: "claude".into(),
            }];
            let project_path_str = project.to_string_lossy().to_string();
            let out = import_scanned_skills(&project_path_str, "demo-import-project", &targets)?;
            assert!(out.skills_list_updated);
            assert!(list_projects().iter().any(|p| p.path == project_path_str));
            Ok(())
        })();

        remove_env("SKILLSTAR_DATA_DIR");
        let _ = std::fs::remove_dir_all(&data);
        let _ = std::fs::remove_dir_all(&project);
        result
    }

    #[test]
    fn import_scanned_skills_skips_non_skill_directories() -> Result<()> {
        let _guard = crate::lock_test_env();
        let data = unique_temp_dir("data2");
        let project = unique_temp_dir("project2");
        set_env("SKILLSTAR_DATA_DIR", &data);

        let result = (|| {
            let claude_skills = project.join(".claude").join("skills");
            std::fs::create_dir_all(claude_skills.join("not-a-skill"))?;
            // no SKILL.md
            let targets = vec![ImportTarget {
                name: "not-a-skill".into(),
                agent_id: "claude".into(),
            }];
            let project_path_str = project.to_string_lossy().to_string();
            let out = import_scanned_skills(&project_path_str, "demo-import-project", &targets)?;
            assert_eq!(out.symlink_count, 0);
            assert!(out.imported_to_hub.is_empty());
            Ok(())
        })();

        remove_env("SKILLSTAR_DATA_DIR");
        let _ = std::fs::remove_dir_all(&data);
        let _ = std::fs::remove_dir_all(&project);
        result
    }

    #[test]
    fn import_scanned_skills_preserves_shared_path_owner() -> Result<()> {
        let _guard = crate::lock_test_env();
        let data = unique_temp_dir("data3");
        let project = unique_temp_dir("project3");
        set_env("SKILLSTAR_DATA_DIR", &data);

        let result = (|| {
            std::fs::create_dir_all(&project)?;
            let entry = register_project(&project.to_string_lossy())?;
            let mut list = SkillsList::default();
            list.agents.insert("antigravity".into(), Vec::new());
            save_skills_list(&entry.name, &list)?;

            // Seed an owner for the universal path; importing through another
            // compatible Agent must preserve that single owner.
            let loaded = load_skills_list(&entry.name).unwrap();
            assert!(loaded.agents.contains_key("antigravity"));

            let universal_skills = project.join(".agents").join("skills");
            std::fs::create_dir_all(&universal_skills)?;
            write_skill(&universal_skills, "new-one");

            let targets = vec![ImportTarget {
                name: "new-one".into(),
                agent_id: "codex".into(),
            }];
            let project_path_str = project.to_string_lossy().to_string();
            let out = import_scanned_skills(&project_path_str, &entry.name, &targets)?;
            assert_eq!(out.symlink_count, 1);
            let updated = load_skills_list(&entry.name).unwrap();
            assert!(
                updated
                    .agents
                    .get("antigravity")
                    .is_some_and(|skills| skills.iter().any(|skill| skill == "new-one"))
            );
            assert!(!updated.agents.contains_key("codex"));
            Ok(())
        })();

        remove_env("SKILLSTAR_DATA_DIR");
        let _ = std::fs::remove_dir_all(&data);
        let _ = std::fs::remove_dir_all(&project);
        result
    }
}
