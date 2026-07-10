use skillstar_core::infra::error::AppError;
use skillstar_skills::projects as pm;
use skillstar_skills::projects::{ImportResult, ImportTarget};
use std::collections::HashMap;

use anyhow::Result;

// Formerly re-exported from `skillstar_app::commands::projects` (that crate no
// longer depends on Tauri, so its command wrappers were absorbed here).
#[tauri::command]
pub async fn create_project_skills(
    project_path: String,
    selected_skills: Vec<String>,
    agent_types: Vec<String>,
) -> Result<u32, AppError> {
    skillstar_skills::deployment::create_project_skills(
        &std::path::PathBuf::from(project_path),
        &selected_skills,
        &agent_types,
    )
    .map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn register_project(project_path: String) -> Result<pm::ProjectEntry, AppError> {
    pm::register_project(&project_path).map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn list_projects() -> Result<Vec<pm::ProjectEntry>, AppError> {
    Ok(pm::list_projects())
}

#[tauri::command]
pub async fn get_project_skills(name: String) -> Result<Option<pm::SkillsList>, AppError> {
    Ok(pm::load_skills_list(&name))
}

#[tauri::command]
pub async fn save_and_sync_project(
    project_path: String,
    agents: HashMap<String, Vec<String>>,
    deploy_modes: Option<HashMap<String, pm::ProjectDeployMode>>,
) -> Result<u32, AppError> {
    let (_name, count) =
        pm::save_and_sync(&project_path, agents, deploy_modes.unwrap_or_default())
            .map_err(|e| AppError::Other(e.to_string()))?;
    Ok(count)
}

#[tauri::command]
pub async fn save_project_skills_list(
    project_path: String,
    agents: HashMap<String, Vec<String>>,
) -> Result<pm::SkillsList, AppError> {
    pm::save_skills_list_only(&project_path, agents).map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn update_project_path(name: String, new_path: String) -> Result<u32, AppError> {
    pm::update_project_path(&name, &new_path).map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn remove_project(name: String) -> Result<(), AppError> {
    pm::remove_project(&name).map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn scan_project_skills(project_path: String) -> Result<pm::ProjectScanResult, AppError> {
    pm::scan_project_skills(&project_path).map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn refresh_stale_project_copies(project_path: String) -> Result<u32, AppError> {
    pm::refresh_stale_copies(&project_path).map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn rebuild_project_skills_from_disk(
    project_path: String,
) -> Result<pm::SkillsList, AppError> {
    pm::rebuild_skills_list_from_disk(&project_path).map_err(|e| AppError::Other(e.to_string()))
}

#[tauri::command]
pub async fn detect_project_agents(
    project_path: String,
) -> Result<pm::ProjectAgentDetection, AppError> {
    Ok(pm::detect_project_agents(&project_path))
}

#[tauri::command]
pub async fn import_project_skills(
    project_path: String,
    project_name: String,
    targets: Vec<ImportTarget>,
) -> Result<ImportResult, AppError> {
    pm::import_scanned_skills(&project_path, &project_name, &targets)
        .map_err(|e| AppError::Other(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::{Context, Result};
    use pm::{list_projects, load_skills_list, register_project, save_skills_list};
    use skillstar_core::infra::paths as fs_paths;
    use skillstar_skills::projects::SkillsList;
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
        let previous_data_dir = std::env::var_os("SKILLSTAR_DATA_DIR");
        set_env("SKILLSTAR_DATA_DIR", temp_root.join("home").join(".skillstar"));
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
                pm::import_scanned_skills(&project_path_str, "demo-import-project", &targets)?;
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
            let local_skill_dir =
                skillstar_core::infra::paths::local_skills_dir().join("legacy-skill");
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
    fn import_scanned_skills_skips_non_skill_directories() -> Result<()> {
        let _guard = env_lock();

        let temp_root = make_temp_root("project-import-skip-invalid")?;
        let previous_home = std::env::var_os("HOME");
        set_env("HOME", temp_root.join("home"));
        let previous_data_dir = std::env::var_os("SKILLSTAR_DATA_DIR");
        set_env("SKILLSTAR_DATA_DIR", temp_root.join("home").join(".skillstar"));
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
                pm::import_scanned_skills(&project_path_str, "demo-import-project", &targets)?;

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
    fn import_scanned_skills_preserves_shared_path_owner() -> Result<()> {
        let _guard = env_lock();

        let temp_root = make_temp_root("project-import-owner")?;
        let previous_home = std::env::var_os("HOME");
        set_env("HOME", temp_root.join("home"));
        let previous_data_dir = std::env::var_os("SKILLSTAR_DATA_DIR");
        set_env("SKILLSTAR_DATA_DIR", temp_root.join("home").join(".skillstar"));
        #[cfg(windows)]
        let previous_userprofile = std::env::var_os("USERPROFILE");
        #[cfg(windows)]
        set_env("USERPROFILE", temp_root.join("home"));

        let result = (|| -> Result<()> {
            let project_path = temp_root.join("workspace").join("demo-import-project");
            let project_path_str = project_path.to_string_lossy().to_string();
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
                pm::import_scanned_skills(&project_path_str, "demo-import-project", &targets)?;
            assert_eq!(import_result.symlink_count, 1);

            let skills_list = load_skills_list(&entry.name)
                .expect("expected updated skills-list.json for registered project");
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
