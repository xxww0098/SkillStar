use crate::core::{
    git::ops as git_ops,
    infra::error::AppError,
    installed_skill, local_skill, lockfile, project_manifest,
    projects::sync,
    repo_scanner, security_scan,
    skill::{Skill, extract_github_source_from_url, extract_skill_description},
    skill_install,
};

use super::skill_paths::resolve_skill_content_dir;

/// Result of updating a single skill. For repo-cached skills, pulling the
/// repo also advances all sibling skills from the same repository.
#[derive(serde::Serialize)]
pub struct UpdateResult {
    /// The skill that was explicitly updated.
    pub skill: Skill,
    /// Names of sibling skills from the same repo whose update state was
    /// also cleared by the pull. The frontend should set
    /// `update_available = false` for these.
    pub siblings_cleared: Vec<String>,
}

#[tauri::command]
pub async fn list_skills() -> Result<Vec<Skill>, AppError> {
    installed_skill::list_installed_skills()
        .await
        .map_err(AppError::Anyhow)
}

#[tauri::command]
pub async fn refresh_skill_updates(
    names: Option<Vec<String>>,
) -> Result<Vec<installed_skill::SkillUpdateState>, AppError> {
    installed_skill::refresh_skill_updates(names)
        .await
        .map_err(AppError::Anyhow)
}

#[tauri::command]
pub async fn install_skill(url: String, name: Option<String>) -> Result<Skill, AppError> {
    tokio::task::spawn_blocking(move || skill_install::install_skill(url, name))
        .await
        .map_err(|e| AppError::Other(format!("install task panicked: {e}")))?
        .map_err(AppError::Other)
}

#[tauri::command]
pub async fn uninstall_skill(name: String) -> Result<(), AppError> {
    tokio::task::spawn_blocking(move || uninstall_skill_sync(name))
        .await
        .map_err(|e| AppError::Other(format!("uninstall task panicked: {e}")))?
}

fn uninstall_skill_sync(name: String) -> Result<(), AppError> {
    if local_skill::is_local_skill(&name) {
        local_skill::delete(&name).map_err(AppError::Anyhow)?;
        crate::core::installed_skill::invalidate_cache();
        security_scan::invalidate_skill_cache(&name);
        return Ok(());
    }

    let _ = sync::remove_skill_from_all_agents(&name);

    let skills_dir = crate::core::infra::paths::hub_skills_dir();
    let path = skills_dir.join(&name);

    if crate::core::infra::fs_ops::is_link(&path) {
        crate::core::infra::fs_ops::remove_symlink(&path)?;
    } else if path.exists() {
        crate::core::infra::fs_ops::remove_dir_all_retry(&path)?;
    }

    let _lock = lockfile::get_mutex()
        .lock()
        .map_err(|_| AppError::Lockfile("Lockfile mutex poisoned".to_string()))?;
    let lock_path = lockfile::lockfile_path();
    let mut lf = lockfile::Lockfile::load(&lock_path).map_err(|e| {
        AppError::Lockfile(format!(
            "Failed to load lockfile '{}': {}",
            lock_path.display(),
            e
        ))
    })?;
    lf.remove(&name);
    lf.save(&lock_path).map_err(|e| {
        AppError::Lockfile(format!(
            "Failed to save lockfile '{}': {}",
            lock_path.display(),
            e
        ))
    })?;

    let _ = project_manifest::remove_skill_from_all_projects(&name);
    crate::core::installed_skill::invalidate_cache();
    security_scan::invalidate_skill_cache(&name);

    Ok(())
}

#[tauri::command]
pub async fn toggle_skill_for_agent(
    skill_name: String,
    agent_id: String,
    enable: bool,
) -> Result<(), AppError> {
    tracing::info!(
        target: "cmd",
        skill_name,
        agent_id,
        enable,
        "toggle_skill_for_agent called"
    );
    sync::toggle_skill_for_agent(&skill_name, &agent_id, enable).map_err(|e| {
        tracing::error!(target: "cmd", skill_name, agent_id, enable, error = %e, "toggle_skill_for_agent failed");
        AppError::Anyhow(e)
    })?;
    crate::core::installed_skill::invalidate_cache();
    tracing::info!(target: "cmd", skill_name, agent_id, enable, "toggle_skill_for_agent completed");
    Ok(())
}

#[tauri::command]
pub async fn update_skill(name: String) -> Result<UpdateResult, AppError> {
    tokio::task::spawn_blocking(move || update_skill_sync(name))
        .await
        .map_err(|e| AppError::Other(format!("update task panicked: {e}")))?
}

fn update_skill_sync(name: String) -> Result<UpdateResult, AppError> {
    let skills_dir = crate::core::infra::paths::hub_skills_dir();
    let path = skills_dir.join(&name);

    let is_repo_skill = repo_scanner::is_repo_cached_skill(&path);

    let lock_entry = {
        let lock_path = lockfile::lockfile_path();
        lockfile::Lockfile::load(&lock_path)
            .ok()
            .and_then(|lf| lf.skills.into_iter().find(|s| s.name == name))
    };

    let tree_hash = if is_repo_skill {
        let source_folder = lock_entry.as_ref().and_then(|e| e.source_folder.as_deref());
        repo_scanner::pull_repo_skill_update(&path, source_folder)
            .map_err(|e| AppError::Git(e.to_string()))?
    } else {
        git_ops::pull_repo(&path).map_err(|e| AppError::Git(e.to_string()))?;
        git_ops::compute_tree_hash(&path).map_err(|e| AppError::Git(e.to_string()))?
    };

    let mut sibling_names: Vec<String> = Vec::new();

    {
        let _lock = lockfile::get_mutex()
            .lock()
            .map_err(|_| AppError::Lockfile("Lockfile mutex poisoned".to_string()))?;
        let lock_path = lockfile::lockfile_path();
        let mut lf = lockfile::Lockfile::load(&lock_path).map_err(|e| {
            AppError::Lockfile(format!(
                "Failed to load lockfile '{}': {}",
                lock_path.display(),
                e
            ))
        })?;

        if is_repo_skill {
            if let Some(ref entry) = lock_entry {
                let git_url = &entry.git_url;
                for sibling in lf.skills.iter_mut().filter(|s| s.git_url == *git_url) {
                    if sibling.name == name {
                        sibling.tree_hash = tree_hash.clone();
                    } else {
                        let sibling_path = skills_dir.join(&sibling.name);
                        if sibling_path.exists() {
                            if let Some(ref folder) = sibling.source_folder {
                                if let Ok(repo_root) = resolve_repo_root_from_symlink(&sibling_path)
                                {
                                    if let Ok(hash) =
                                        repo_scanner::compute_subtree_hash_pub(&repo_root, folder)
                                    {
                                        sibling.tree_hash = hash;
                                    }
                                }
                            }
                            sibling_names.push(sibling.name.clone());
                        }
                    }
                }
            }
        } else if let Some(entry) = lf.skills.iter_mut().find(|s| s.name == name) {
            entry.tree_hash = tree_hash.clone();
        }

        lf.save(&lock_path).map_err(|e| {
            AppError::Lockfile(format!(
                "Failed to save lockfile '{}': {}",
                lock_path.display(),
                e
            ))
        })?;
    }

    crate::core::installed_skill::invalidate_cache();
    crate::core::installed_skill::clear_update_state(&name);

    security_scan::invalidate_skill_cache(&name);
    for sib in &sibling_names {
        crate::core::installed_skill::clear_update_state(sib);
        security_scan::invalidate_skill_cache(sib);
    }

    let agent_links = sync::resync_existing_links(&name).unwrap_or_default();

    let git_url = lock_entry
        .as_ref()
        .map(|e| e.git_url.clone())
        .unwrap_or_default();

    let description = resolve_skill_content_dir(&name)
        .map(|dir| extract_skill_description(&dir))
        .unwrap_or_else(|| extract_skill_description(&path));

    let source = lock_entry
        .as_ref()
        .and_then(|e| extract_github_source_from_url(&e.git_url));

    let skill_type = if local_skill::is_local_skill(&name) {
        crate::core::skill::SkillType::Local
    } else {
        crate::core::skill::SkillType::Hub
    };

    Ok(UpdateResult {
        skill: Skill {
            name,
            description,
            localized_description: None,
            skill_type,
            stars: 0,
            installed: true,
            update_available: false,
            last_updated: chrono::Utc::now().to_rfc3339(),
            git_url,
            tree_hash: Some(tree_hash),
            category: crate::core::skill::SkillCategory::None,
            author: None,
            topics: Vec::new(),
            agent_links: Some(agent_links),
            rank: None,
            source,
        },
        siblings_cleared: sibling_names,
    })
}

fn resolve_repo_root_from_symlink(
    skill_path: &std::path::Path,
) -> Result<std::path::PathBuf, String> {
    repo_scanner::resolve_skill_repo_root(skill_path)
        .ok_or_else(|| "Cannot find git repo root".to_string())
}
