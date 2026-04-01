pub mod agents;
pub mod ai;
pub mod github;
pub mod marketplace;
pub mod patrol;
pub mod projects;

use crate::core::{
    git_ops, installed_skill, local_skill, lockfile, marketplace_snapshot, project_manifest, proxy,
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
pub async fn get_proxy_config() -> Result<proxy::ProxyConfig, String> {
    proxy::load_config().map_err(|e| format!("Failed to read proxy config: {}", e))
}

#[tauri::command]
pub async fn save_proxy_config(config: proxy::ProxyConfig) -> Result<(), String> {
    proxy::save_config(&config).map_err(|e| format!("Failed to write proxy config: {}", e))
}

// ── Skills ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn list_skills() -> Result<Vec<Skill>, String> {
    installed_skill::list_installed_skills()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn refresh_skill_updates(
    names: Option<Vec<String>>,
) -> Result<Vec<installed_skill::SkillUpdateState>, String> {
    installed_skill::refresh_skill_updates(names)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn install_skill(url: String, name: Option<String>) -> Result<Skill, String> {
    skill_install::install_skill(url, name).await
}

#[tauri::command]
pub async fn uninstall_skill(name: String) -> Result<(), String> {
    // If it's a local skill, delegate to local_skill::delete
    if local_skill::is_local_skill(&name) {
        local_skill::delete(&name).map_err(|e| e.to_string())?;
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
    if let Ok(meta) = path.symlink_metadata() {
        if meta.is_symlink() {
            std::fs::remove_file(&path).map_err(|e| format!("Failed to remove symlink: {}", e))?;
        } else {
            std::fs::remove_dir_all(&path)
                .map_err(|e| format!("Failed to delete folder: {}", e))?;
        }
    }

    let _lock = lockfile::get_mutex()
        .lock()
        .map_err(|_| "Lockfile mutex poisoned".to_string())?;
    let lock_path = lockfile::lockfile_path();
    let mut lf = lockfile::Lockfile::load(&lock_path)
        .map_err(|e| format!("Failed to load lockfile '{}': {}", lock_path.display(), e))?;
    lf.remove(&name);
    lf.save(&lock_path)
        .map_err(|e| format!("Failed to save lockfile '{}': {}", lock_path.display(), e))?;

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
) -> Result<(), String> {
    sync::toggle_skill_for_agent(&skill_name, &agent_id, enable).map_err(|e| e.to_string())?;
    crate::core::installed_skill::invalidate_cache();
    Ok(())
}

#[tauri::command]
pub async fn update_skill(name: String) -> Result<UpdateResult, String> {
    let skills_dir = crate::core::paths::hub_skills_dir();
    let path = skills_dir.join(&name);

    // Check if this is a repo-cached skill (symlink into .repos/)
    let is_repo_skill = repo_scanner::is_repo_cached_skill(&path);

    let _lock = lockfile::get_mutex()
        .lock()
        .map_err(|_| "Lockfile mutex poisoned".to_string())?;
    let lock_path = lockfile::lockfile_path();
    let mut lf = lockfile::Lockfile::load(&lock_path)
        .map_err(|e| format!("Failed to load lockfile '{}': {}", lock_path.display(), e))?;
    let lock_entry = lf.skills.iter().find(|s| s.name == name).cloned();

    let tree_hash = if is_repo_skill {
        // Pull via the repo cache
        let source_folder = lock_entry.as_ref().and_then(|e| e.source_folder.as_deref());
        repo_scanner::pull_repo_skill_update(&path, source_folder).map_err(|e| e.to_string())?
    } else {
        git_ops::pull_repo(&path).map_err(|e| e.to_string())?;
        git_ops::compute_tree_hash(&path).map_err(|e| e.to_string())?
    };

    // For repo-cached skills, updating one skill pulls the entire repo.
    // Find all sibling skills from the same git_url and update their
    // lockfile tree_hash too, so they don't stay stale.
    let mut sibling_names: Vec<String> = Vec::new();

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
                            if let Ok(repo_root) = resolve_repo_root_from_symlink(&sibling_path) {
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

    lf.save(&lock_path)
        .map_err(|e| format!("Failed to save lockfile '{}': {}", lock_path.display(), e))?;
    crate::core::installed_skill::invalidate_cache();

    // Invalidate security scan cache — content changed, old results are stale
    security_scan::invalidate_skill_cache(&name);
    for sib in &sibling_names {
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
    let real_path = std::fs::read_link(skill_path).map_err(|e| e.to_string())?;
    let absolute_path = if real_path.is_absolute() {
        real_path
    } else {
        skill_path
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .join(real_path)
    };
    // Walk up to find .git
    let mut current = absolute_path;
    loop {
        if current.join(".git").exists() {
            return Ok(current);
        }
        if !current.pop() {
            return Err("Cannot find git repo root".to_string());
        }
    }
}

// ── Skill Groups ────────────────────────────────────────────────────

use crate::core::skill_group::{self, SkillGroup};

#[tauri::command]
pub async fn list_skill_groups() -> Result<Vec<SkillGroup>, String> {
    Ok(skill_group::list_groups())
}

#[tauri::command]
pub async fn create_skill_group(
    name: String,
    description: String,
    icon: String,
    skills: Vec<String>,
    skill_sources: Option<HashMap<String, String>>,
) -> Result<SkillGroup, String> {
    skill_group::create_group(
        name,
        description,
        icon,
        skills,
        skill_sources.unwrap_or_default(),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_skill_group(
    id: String,
    name: Option<String>,
    description: Option<String>,
    icon: Option<String>,
    skills: Option<Vec<String>>,
    skill_sources: Option<HashMap<String, String>>,
) -> Result<SkillGroup, String> {
    skill_group::update_group(id, name, description, icon, skills, skill_sources)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_skill_group(id: String) -> Result<(), String> {
    skill_group::delete_group(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn duplicate_skill_group(id: String) -> Result<SkillGroup, String> {
    skill_group::duplicate_group(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn deploy_skill_group(
    group_id: String,
    project_path: String,
    agent_types: Vec<String>,
) -> Result<u32, String> {
    let groups = skill_group::list_groups();
    let group = groups
        .iter()
        .find(|g| g.id == group_id)
        .ok_or_else(|| format!("Group '{}' not found", group_id))?;

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
        match marketplace_snapshot::resolve_skill_sources_local_first(
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

    let mut install_tasks = tokio::task::JoinSet::new();
    for skill_name in &group.skills {
        if !skills_dir.join(skill_name).exists() {
            if let Some(git_url) = sources.get(skill_name) {
                let git_url = git_url.clone();
                let skill_name = skill_name.clone();
                install_tasks.spawn(async move {
                    // Keep best-effort behavior: deployment continues even if a single install fails.
                    let _ = install_skill(git_url, Some(skill_name)).await;
                });
            }
        }
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

    let (_name, count) =
        project_manifest::save_and_sync(&project_path, agents).map_err(|e| e.to_string())?;

    Ok(count)
}

fn resolve_skill_dir(skill_dir: &std::path::Path) -> std::path::PathBuf {
    if !skill_dir.is_symlink() {
        return skill_dir.to_path_buf();
    }

    std::fs::read_link(skill_dir)
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
pub async fn read_skill_file_raw(name: String) -> Result<String, String> {
    let skills_dir = crate::core::paths::hub_skills_dir();
    let skill_dir = skills_dir.join(&name);
    let effective_dir =
        resolve_skill_content_dir(&name).unwrap_or_else(|| resolve_skill_dir(&skill_dir));

    let skill_md = effective_dir.join("SKILL.md");
    if !skill_md.exists() {
        return Err(format!("SKILL.md not found for '{}'", name));
    }

    std::fs::read_to_string(&skill_md).map_err(|e| format!("Failed to read SKILL.md: {}", e))
}

#[tauri::command]
pub async fn create_local_skill_from_content(name: String, content: String) -> Result<(), String> {
    let hub_dir = crate::core::paths::hub_skills_dir();
    let hub_path = hub_dir.join(&name);

    // Don't overwrite existing skills
    if hub_path.symlink_metadata().is_ok() {
        return Ok(()); // Silently skip if already exists
    }

    // Create in skills-local/ and symlink back to skills/
    let _ = local_skill::create(&name, Some(&content)).map_err(|e| e.to_string())?;
    crate::core::installed_skill::invalidate_cache();

    Ok(())
}

// ── Local Skills ────────────────────────────────────────────────────

#[tauri::command]
pub async fn create_local_skill(name: String, content: Option<String>) -> Result<Skill, String> {
    let skill = local_skill::create(&name, content.as_deref()).map_err(|e| e.to_string())?;
    crate::core::installed_skill::invalidate_cache();
    Ok(skill)
}

#[tauri::command]
pub async fn delete_local_skill(name: String) -> Result<(), String> {
    local_skill::delete(&name).map_err(|e| e.to_string())?;
    crate::core::installed_skill::invalidate_cache();
    Ok(())
}

#[tauri::command]
pub async fn migrate_local_skills() -> Result<u32, String> {
    tokio::task::spawn_blocking(|| local_skill::migrate_existing())
        .await
        .map_err(|e| format!("Task join error: {}", e))?
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_skill_files(name: String) -> Result<Vec<String>, String> {
    let skills_dir = crate::core::paths::hub_skills_dir();
    let skill_dir = skills_dir.join(&name);

    if !skill_dir.exists() {
        return Err(format!("Skill '{}' not found", name));
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
            files.push(rel.to_string_lossy().to_string());
        }
    }
}

// ── SKILL.md Content ─────────────────────────────────────────────────

#[tauri::command]
pub async fn read_skill_content(name: String) -> Result<SkillContent, String> {
    let skills_dir = crate::core::paths::hub_skills_dir();
    let skill_dir = skills_dir.join(&name);
    if !skill_dir.exists() {
        return Err(format!("Skill '{}' not found", name));
    }
    let _ = git_ops::ensure_worktree_checked_out(&skill_dir);
    let effective_dir = resolve_skill_content_dir(&name).unwrap_or(skill_dir);
    let skill_path = effective_dir.join("SKILL.md");

    if !skill_path.exists() {
        return Err(format!("Skill '{}' not found", name));
    }

    let full_content =
        std::fs::read_to_string(&skill_path).map_err(|e| format!("Failed to read file: {}", e))?;

    Ok(parse_skill_content(name, full_content))
}

#[tauri::command]
pub async fn update_skill_content(name: String, content: String) -> Result<(), String> {
    let skills_dir = crate::core::paths::hub_skills_dir();
    let skill_dir = skills_dir.join(&name);
    if !skill_dir.exists() {
        return Err(format!("Skill '{}' not found", name));
    }
    let effective_dir = resolve_skill_content_dir(&name).unwrap_or(skill_dir);
    let skill_path = effective_dir.join("SKILL.md");

    if !skill_path.exists() {
        return Err(format!("Skill '{}' not found", name));
    }

    // Atomic write: write to temp file then rename
    let temp_path = skill_path.with_extension("tmp");
    std::fs::write(&temp_path, &content)
        .map_err(|e| format!("Failed to write temp file: {}", e))?;

    std::fs::rename(&temp_path, &skill_path).map_err(|e| format!("Failed to save file: {}", e))?;

    security_scan::invalidate_skill_cache(&name);

    Ok(())
}

// ── Skill Bundles (.ags) ─────────────────────────────────────

#[tauri::command]
pub async fn export_skill_bundle(
    name: String,
    output_path: Option<String>,
) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        skill_bundle::export_bundle(&name, output_path.as_deref())
            .map(|path| path.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn preview_skill_bundle(
    file_path: String,
) -> Result<skill_bundle::BundleManifest, String> {
    tokio::task::spawn_blocking(move || skill_bundle::preview_bundle(&file_path))
        .await
        .map_err(|e| format!("Task join error: {}", e))?
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn import_skill_bundle(
    file_path: String,
    force: bool,
) -> Result<skill_bundle::ImportBundleResult, String> {
    tokio::task::spawn_blocking(move || skill_bundle::import_bundle(&file_path, force))
        .await
        .map_err(|e| format!("Task join error: {}", e))?
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn export_multi_skill_bundle(
    names: Vec<String>,
    output_path: String,
) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        skill_bundle::export_multi_bundle(&names, &output_path)
            .map(|path| path.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn preview_multi_skill_bundle(
    file_path: String,
) -> Result<skill_bundle::MultiManifest, String> {
    tokio::task::spawn_blocking(move || skill_bundle::preview_multi_bundle(&file_path))
        .await
        .map_err(|e| format!("Task join error: {}", e))?
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn import_multi_skill_bundle(
    file_path: String,
    force: bool,
) -> Result<skill_bundle::ImportMultiBundleResult, String> {
    tokio::task::spawn_blocking(move || skill_bundle::import_multi_bundle(&file_path, force))
        .await
        .map_err(|e| format!("Task join error: {}", e))?
        .map_err(|e| e.to_string())
}

// ── Text File I/O (share code files) ────────────────────────────────

#[tauri::command]
pub async fn write_text_file(path: String, content: String) -> Result<(), String> {
    std::fs::write(&path, &content).map_err(|e| format!("Failed to write file: {}", e))
}

#[tauri::command]
pub async fn read_text_file(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| format!("Failed to read file: {}", e))
}

#[tauri::command]
pub async fn open_folder(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    std::process::Command::new("open")
        .arg(&path)
        .spawn()
        .map_err(|e| e.to_string())?;

    #[cfg(target_os = "windows")]
    std::process::Command::new("explorer")
        .arg(&path)
        .spawn()
        .map_err(|e| e.to_string())?;

    #[cfg(target_os = "linux")]
    std::process::Command::new("xdg-open")
        .arg(&path)
        .spawn()
        .map_err(|e| e.to_string())?;

    Ok(())
}
