pub mod acp;
pub mod agents;
pub mod ai;
pub mod github;
pub mod launch;
pub mod marketplace;
pub mod models;
pub mod patrol;
pub mod projects;
pub mod updater;

use crate::core::{
    error::AppError,
    git_ops, github_mirror, installed_skill, local_skill, lockfile, project_manifest, proxy,
    repo_scanner, security_scan,
    skill::{
        Skill, SkillContent, extract_github_source_from_url, extract_skill_description,
        parse_skill_content,
    },
    skill_bundle, skill_install, sync,
};
use std::collections::HashMap;
use tracing::{error, warn};

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
pub async fn get_proxy_config() -> Result<proxy::ProxyConfig, AppError> {
    proxy::load_config().map_err(|e| AppError::Other(format!("Failed to read proxy config: {}", e)))
}

#[tauri::command]
pub async fn save_proxy_config(config: proxy::ProxyConfig) -> Result<(), AppError> {
    proxy::save_config(&config)
        .map_err(|e| AppError::Other(format!("Failed to write proxy config: {}", e)))
}

// ── GitHub Mirror ───────────────────────────────────────────────────

#[tauri::command]
pub async fn get_github_mirror_config() -> Result<github_mirror::GitHubMirrorConfig, AppError> {
    github_mirror::load_config()
        .map_err(|e| AppError::Other(format!("Failed to read mirror config: {}", e)))
}

#[tauri::command]
pub async fn save_github_mirror_config(
    config: github_mirror::GitHubMirrorConfig,
) -> Result<(), AppError> {
    github_mirror::save_config(&config)
        .map_err(|e| AppError::Other(format!("Failed to write mirror config: {}", e)))
}

#[tauri::command]
pub async fn get_github_mirror_presets() -> Result<Vec<github_mirror::MirrorPreset>, AppError> {
    Ok(github_mirror::builtin_presets())
}

#[tauri::command]
pub async fn test_github_mirror(url: String) -> Result<u64, AppError> {
    github_mirror::test_mirror(&url)
        .await
        .map_err(|e| AppError::Other(format!("Mirror test failed: {}", e)))
}

// ── Skills ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn list_skills() -> Result<Vec<Skill>, AppError> {
    installed_skill::list_installed_skills()
        .await
        .map_err(|e| AppError::Anyhow(e))
}

#[tauri::command]
pub async fn refresh_skill_updates(
    names: Option<Vec<String>>,
) -> Result<Vec<installed_skill::SkillUpdateState>, AppError> {
    installed_skill::refresh_skill_updates(names)
        .await
        .map_err(|e| AppError::Anyhow(e))
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
    // If it's a local skill, delegate to local_skill::delete
    if local_skill::is_local_skill(&name) {
        local_skill::delete(&name).map_err(|e| AppError::Anyhow(e))?;
        crate::core::installed_skill::invalidate_cache();
        security_scan::invalidate_skill_cache(&name);
        return Ok(());
    }

    // Remove symlinks from all agents first
    let _ = sync::remove_skill_from_all_agents(&name);

    let skills_dir = crate::core::paths::hub_skills_dir();
    let path = skills_dir.join(&name);

    // Use symlink_metadata() instead of exists() — exists() follows symlinks,
    // so a broken symlink (target deleted) returns false and is never cleaned up.
    // Also check is_link() which detects Windows junction points.
    if crate::core::paths::is_link(&path) {
        crate::core::paths::remove_symlink(&path)?;
    } else if path.exists() {
        crate::core::paths::remove_dir_all_retry(&path)?;
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
    let skills_dir = crate::core::paths::hub_skills_dir();
    let path = skills_dir.join(&name);

    // Check if this is a repo-cached skill (symlink into .repos/)
    let is_repo_skill = repo_scanner::is_repo_cached_skill(&path);

    // Read the lock entry WITHOUT holding the mutex — we only need a snapshot
    // for the git pull parameters. The mutex is acquired later for the
    // read→modify→write cycle only.
    let lock_entry = {
        let lock_path = lockfile::lockfile_path();
        lockfile::Lockfile::load(&lock_path)
            .ok()
            .and_then(|lf| lf.skills.into_iter().find(|s| s.name == name))
    };

    // ── Git pull (network I/O) — NO lockfile mutex held ─────────────
    let tree_hash = if is_repo_skill {
        let source_folder = lock_entry.as_ref().and_then(|e| e.source_folder.as_deref());
        repo_scanner::pull_repo_skill_update(&path, source_folder)
            .map_err(|e| AppError::Git(e.to_string()))?
    } else {
        git_ops::pull_repo(&path).map_err(|e| AppError::Git(e.to_string()))?;
        git_ops::compute_tree_hash(&path).map_err(|e| AppError::Git(e.to_string()))?
    };

    // ── Lockfile update — mutex held only for read→modify→write ─────
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
                        // Recompute sibling's subtree hash from the now-updated repo
                        let sibling_path = skills_dir.join(&sibling.name);
                        if sibling_path.exists() {
                            if let Some(ref folder) = sibling.source_folder {
                                if let Ok(repo_root) =
                                    resolve_repo_root_from_symlink(&sibling_path)
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
    } // ← mutex released here

    crate::core::installed_skill::invalidate_cache();
    crate::core::installed_skill::clear_update_state(&name);

    // Invalidate security scan cache — content changed, old results are stale
    security_scan::invalidate_skill_cache(&name);
    for sib in &sibling_names {
        crate::core::installed_skill::clear_update_state(sib);
        security_scan::invalidate_skill_cache(sib);
    }

    // Re-sync only to agents that already had this skill linked (preserve existing links)
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

/// Resolve the repo root from a symlinked skill path.
fn resolve_repo_root_from_symlink(
    skill_path: &std::path::Path,
) -> Result<std::path::PathBuf, String> {
    repo_scanner::resolve_skill_repo_root(skill_path)
        .ok_or_else(|| "Cannot find git repo root".to_string())
}

// ── Skill Groups ────────────────────────────────────────────────────

use crate::core::skill_group::{self, SkillGroup};

#[tauri::command]
pub async fn list_skill_groups() -> Result<Vec<SkillGroup>, AppError> {
    Ok(skill_group::list_groups())
}

#[tauri::command]
pub async fn create_skill_group(
    name: String,
    description: String,
    icon: String,
    skills: Vec<String>,
    skill_sources: Option<HashMap<String, String>>,
) -> Result<SkillGroup, AppError> {
    skill_group::create_group(
        name,
        description,
        icon,
        skills,
        skill_sources.unwrap_or_default(),
    )
    .map_err(|e| AppError::Anyhow(e))
}

#[tauri::command]
pub async fn update_skill_group(
    id: String,
    name: Option<String>,
    description: Option<String>,
    icon: Option<String>,
    skills: Option<Vec<String>>,
    skill_sources: Option<HashMap<String, String>>,
) -> Result<SkillGroup, AppError> {
    skill_group::update_group(id, name, description, icon, skills, skill_sources)
        .map_err(|e| AppError::Anyhow(e))
}

#[tauri::command]
pub async fn delete_skill_group(id: String) -> Result<(), AppError> {
    skill_group::delete_group(&id).map_err(|e| AppError::Anyhow(e))
}

#[tauri::command]
pub async fn duplicate_skill_group(id: String) -> Result<SkillGroup, AppError> {
    skill_group::duplicate_group(&id).map_err(|e| AppError::Anyhow(e))
}

#[tauri::command]
pub async fn deploy_skill_group(
    group_id: String,
    project_path: String,
    agent_types: Vec<String>,
) -> Result<u32, AppError> {
    let groups = skill_group::list_groups();
    let group = groups
        .iter()
        .find(|g| g.id == group_id)
        .ok_or_else(|| AppError::Other(format!("Group '{}' not found", group_id)))?;

    // Download any missing skills before syncing
    let skills_dir = crate::core::paths::hub_skills_dir();
    let mut sources = group.skill_sources.clone();

    // Collect skill names that are missing from the hub and have no known source
    let names_needing_source: Vec<String> = group
        .skills
        .iter()
        .filter(|name| !skills_dir.join(name).exists() && !sources.contains_key(*name))
        .cloned()
        .collect();

    // Resolve missing sources via marketplace search
    if !names_needing_source.is_empty() {
        warn!(
            target: "deploy_skill_group",
            "resolving {} missing skill source(s) via marketplace snapshot",
            names_needing_source.len()
        );
        match crate::core::marketplace::resolve_skill_sources_local_first(
            &names_needing_source,
            &sources,
        )
        .await
        {
            Ok(resolved) => {
                for (name, url) in resolved {
                    sources.insert(name, url);
                }
            }
            Err(err) => {
                error!(
                    target: "deploy_skill_group",
                    "failed to resolve missing skill sources: {err}"
                );
            }
        }
    }

    // Group skills by their Git URL to install efficiently
    let mut batch_by_url: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for skill_name in &group.skills {
        if !skills_dir.join(skill_name).exists() {
            if let Some(git_url) = sources.get(skill_name) {
                batch_by_url
                    .entry(git_url.clone())
                    .or_default()
                    .push(skill_name.clone());
            }
        }
    }

    let mut install_tasks = tokio::task::JoinSet::new();
    for (url, names) in batch_by_url {
        install_tasks.spawn_blocking(move || {
            // Keep best-effort behavior
            let _ = skill_install::install_skills_batch(&url, &names);
        });
    }
    while let Some(result) = install_tasks.join_next().await {
        if let Err(e) = result {
            error!(target: "deploy_skill_group", "install task join error: {e}");
        }
    }

    // Build per-agent map: all selected agents get the same skills
    let agents: HashMap<String, Vec<String>> = agent_types
        .into_iter()
        .map(|id| (id, group.skills.clone()))
        .collect();

    let (_name, count) = project_manifest::save_and_sync(&project_path, agents)
        .map_err(|e| AppError::Project(e.to_string()))?;

    Ok(count)
}

fn resolve_skill_dir(skill_dir: &std::path::Path) -> std::path::PathBuf {
    if !crate::core::paths::is_link(skill_dir) {
        return skill_dir.to_path_buf();
    }

    // Try std::fs::read_link first (works for true symlinks)
    let link_target = std::fs::read_link(skill_dir);

    // On Windows, junction points may not be readable via read_link;
    // fall back to junction::get_target.
    #[cfg(windows)]
    let link_target = link_target.or_else(|_| junction::get_target(skill_dir));

    link_target
        .map(|target| {
            if target.is_absolute() {
                target
            } else {
                skill_dir
                    .parent()
                    .unwrap_or(std::path::Path::new("."))
                    .join(target)
            }
        })
        .unwrap_or_else(|_| skill_dir.to_path_buf())
}

fn lockfile_source_folder(skill_name: &str) -> Option<String> {
    let lock_path = lockfile::lockfile_path();
    let lockfile = lockfile::Lockfile::load(&lock_path).ok()?;
    lockfile
        .skills
        .into_iter()
        .find(|entry| entry.name == skill_name)
        .and_then(|entry| entry.source_folder)
}

fn find_nested_skill_dir_by_name(
    root: &std::path::Path,
    skill_name: &str,
) -> Option<std::path::PathBuf> {
    const SKIP_DIRS: &[&str] = &[".git", "node_modules", "target", "dist", "build", ".next"];

    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let dir_name = entry.file_name().to_string_lossy().to_string();
            if SKIP_DIRS.iter().any(|skip| *skip == dir_name) {
                continue;
            }

            if dir_name == skill_name && path.join("SKILL.md").exists() {
                return Some(path);
            }

            stack.push(path);
        }
    }

    None
}

fn resolve_skill_content_dir(name: &str) -> Option<std::path::PathBuf> {
    let skills_dir = crate::core::paths::hub_skills_dir();
    let skill_dir = skills_dir.join(name);
    if !skill_dir.exists() {
        return None;
    }

    let effective_dir = resolve_skill_dir(&skill_dir);
    if effective_dir.join("SKILL.md").exists() {
        return Some(effective_dir);
    }

    if let Some(source_folder) = lockfile_source_folder(name) {
        let nested = effective_dir.join(source_folder);
        if nested.join("SKILL.md").exists() {
            return Some(nested);
        }
    }

    find_nested_skill_dir_by_name(&effective_dir, name)
}

#[tauri::command]
pub async fn read_skill_file_raw(name: String) -> Result<String, AppError> {
    let skills_dir = crate::core::paths::hub_skills_dir();
    let skill_dir = skills_dir.join(&name);
    let effective_dir =
        resolve_skill_content_dir(&name).unwrap_or_else(|| resolve_skill_dir(&skill_dir));

    let skill_md = effective_dir.join("SKILL.md");
    if !skill_md.exists() {
        return Err(AppError::SkillNotFound { name });
    }

    Ok(std::fs::read_to_string(&skill_md)?)
}

#[tauri::command]
pub async fn create_local_skill_from_content(
    name: String,
    content: String,
) -> Result<(), AppError> {
    let hub_dir = crate::core::paths::hub_skills_dir();
    let hub_path = hub_dir.join(&name);

    // Don't overwrite existing skills
    if hub_path.symlink_metadata().is_ok() {
        return Ok(()); // Silently skip if already exists
    }

    // Create in skills-local/ and symlink back to skills/
    let _ = local_skill::create(&name, Some(&content)).map_err(|e| AppError::Anyhow(e))?;
    crate::core::installed_skill::invalidate_cache();

    Ok(())
}

// ── Local Skills ────────────────────────────────────────────────────

#[tauri::command]
pub async fn create_local_skill(name: String, content: Option<String>) -> Result<Skill, AppError> {
    let skill = local_skill::create(&name, content.as_deref()).map_err(|e| AppError::Anyhow(e))?;
    crate::core::installed_skill::invalidate_cache();
    Ok(skill)
}

#[tauri::command]
pub async fn delete_local_skill(name: String) -> Result<(), AppError> {
    local_skill::delete(&name).map_err(|e| AppError::Anyhow(e))?;
    crate::core::installed_skill::invalidate_cache();
    Ok(())
}

#[tauri::command]
pub async fn migrate_local_skills() -> Result<u32, AppError> {
    tokio::task::spawn_blocking(|| local_skill::migrate_existing())
        .await?
        .map_err(|e| AppError::Anyhow(e))
}

#[tauri::command]
pub async fn list_skill_files(name: String) -> Result<Vec<String>, AppError> {
    let skills_dir = crate::core::paths::hub_skills_dir();
    let skill_dir = skills_dir.join(&name);

    if !skill_dir.exists() {
        return Err(AppError::SkillNotFound { name });
    }

    let effective_dir =
        resolve_skill_content_dir(&name).unwrap_or_else(|| resolve_skill_dir(&skill_dir));

    let mut files = Vec::new();
    collect_files_recursive(&effective_dir, &effective_dir, &mut files);
    files.sort();
    Ok(files)
}

fn collect_files_recursive(root: &std::path::Path, dir: &std::path::Path, files: &mut Vec<String>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = entry.file_name();
        let name_str = file_name.to_string_lossy();

        // Skip .git
        if name_str == ".git" {
            continue;
        }

        if path.is_dir() {
            collect_files_recursive(root, &path, files);
        } else if let Ok(rel) = path.strip_prefix(root) {
            // Normalize to forward slashes for consistent cross-platform display
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            files.push(rel_str);
        }
    }
}

// ── SKILL.md Content ─────────────────────────────────────────────────

#[tauri::command]
pub async fn read_skill_content(name: String) -> Result<SkillContent, AppError> {
    let skills_dir = crate::core::paths::hub_skills_dir();
    let skill_dir = skills_dir.join(&name);
    if !skill_dir.exists() {
        return Err(AppError::SkillNotFound { name });
    }
    let _ = git_ops::ensure_worktree_checked_out(&skill_dir);
    let effective_dir = resolve_skill_content_dir(&name).unwrap_or(skill_dir);
    let skill_path = effective_dir.join("SKILL.md");

    if !skill_path.exists() {
        return Err(AppError::SkillNotFound { name });
    }

    let full_content = std::fs::read_to_string(&skill_path)?;
    Ok(parse_skill_content(name, full_content))
}

#[tauri::command]
pub async fn update_skill_content(name: String, content: String) -> Result<(), AppError> {
    let skills_dir = crate::core::paths::hub_skills_dir();
    let skill_dir = skills_dir.join(&name);
    if !skill_dir.exists() {
        return Err(AppError::SkillNotFound { name });
    }
    let effective_dir = resolve_skill_content_dir(&name).unwrap_or(skill_dir);
    let skill_path = effective_dir.join("SKILL.md");

    if !skill_path.exists() {
        return Err(AppError::SkillNotFound { name });
    }

    // Atomic write: write to temp file then rename
    let temp_path = skill_path.with_extension("tmp");
    std::fs::write(&temp_path, &content)?;
    std::fs::rename(&temp_path, &skill_path)?;

    security_scan::invalidate_skill_cache(&name);

    Ok(())
}

// ── Skill Bundles (.ags) ─────────────────────────────────────

#[tauri::command]
pub async fn export_skill_bundle(
    name: String,
    output_path: Option<String>,
) -> Result<String, AppError> {
    let path = tokio::task::spawn_blocking(move || {
        skill_bundle::export_bundle(&name, output_path.as_deref())
            .map(|path| path.to_string_lossy().to_string())
    })
    .await?
    .map_err(|e| AppError::Bundle(e.to_string()))?;
    Ok(path)
}

#[tauri::command]
pub async fn preview_skill_bundle(
    file_path: String,
) -> Result<skill_bundle::BundleManifest, AppError> {
    tokio::task::spawn_blocking(move || skill_bundle::preview_bundle(&file_path))
        .await?
        .map_err(|e| AppError::Bundle(e.to_string()))
}

#[tauri::command]
pub async fn import_skill_bundle(
    file_path: String,
    force: bool,
) -> Result<skill_bundle::ImportBundleResult, AppError> {
    tokio::task::spawn_blocking(move || skill_bundle::import_bundle(&file_path, force))
        .await?
        .map_err(|e| AppError::Bundle(e.to_string()))
}

#[tauri::command]
pub async fn export_multi_skill_bundle(
    names: Vec<String>,
    output_path: String,
) -> Result<String, AppError> {
    let path = tokio::task::spawn_blocking(move || {
        skill_bundle::export_multi_bundle(&names, &output_path)
            .map(|path| path.to_string_lossy().to_string())
    })
    .await?
    .map_err(|e| AppError::Bundle(e.to_string()))?;
    Ok(path)
}

#[tauri::command]
pub async fn preview_multi_skill_bundle(
    file_path: String,
) -> Result<skill_bundle::MultiManifest, AppError> {
    tokio::task::spawn_blocking(move || skill_bundle::preview_multi_bundle(&file_path))
        .await?
        .map_err(|e| AppError::Bundle(e.to_string()))
}

#[tauri::command]
pub async fn import_multi_skill_bundle(
    file_path: String,
    force: bool,
) -> Result<skill_bundle::ImportMultiBundleResult, AppError> {
    tokio::task::spawn_blocking(move || skill_bundle::import_multi_bundle(&file_path, force))
        .await?
        .map_err(|e| AppError::Bundle(e.to_string()))
}

// ── Text File I/O (share code files) ────────────────────────────────

#[tauri::command]
pub async fn write_text_file(path: String, content: String) -> Result<(), AppError> {
    Ok(std::fs::write(&path, &content)?)
}

#[tauri::command]
pub async fn read_text_file(path: String) -> Result<String, AppError> {
    Ok(std::fs::read_to_string(&path)?)
}

#[tauri::command]
pub async fn open_folder(path: String) -> Result<(), AppError> {
    #[cfg(target_os = "macos")]
    std::process::Command::new("open").arg(&path).spawn()?;

    #[cfg(target_os = "windows")]
    std::process::Command::new("explorer").arg(&path).spawn()?;

    #[cfg(target_os = "linux")]
    std::process::Command::new("xdg-open").arg(&path).spawn()?;

    Ok(())
}
