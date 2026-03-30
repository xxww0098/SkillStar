pub mod agents;
pub mod ai;
pub mod github;
pub mod marketplace;
pub mod patrol;
pub mod projects;

use crate::core::{
    git_ops, installed_skill, local_skill, lockfile, project_manifest, proxy, repo_scanner,
    skill::{
        extract_github_source_from_url, extract_skill_description, parse_skill_content, Skill,
        SkillContent,
    },
    skill_bundle, sync,
};
use std::collections::HashMap;

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
    let skills_dir = sync::get_hub_skills_dir();
    let name_str = name.unwrap_or_else(|| {
        url.rsplit('/')
            .next()
            .unwrap_or("skill")
            .trim_end_matches(".git")
            .to_string()
    });
    let dest = skills_dir.join(&name_str);

    if dest.exists() {
        return Err(format!("Skill '{}' is already installed", name_str));
    }

    // Reject if name exists in skills-local
    if local_skill::local_skills_dir().join(&name_str).exists() {
        return Err(format!("Skill '{}' already exists as a local skill", name_str));
    }

    git_ops::clone_repo(&url, &dest).map_err(|e| e.to_string())?;

    let tree_hash = git_ops::compute_tree_hash(&dest).map_err(|e| e.to_string())?;

    let lock_path = lockfile::lockfile_path();
    let mut lf = lockfile::Lockfile::load(&lock_path).unwrap_or_default();
    lf.upsert(lockfile::LockEntry {
        name: name_str.clone(),
        git_url: url.clone(),
        tree_hash: tree_hash.clone(),
        installed_at: chrono::Utc::now().to_rfc3339(),
        source_folder: None,
    });
    let _ = lf.save(&lock_path);

    let agent_links: Vec<String> = Vec::new();
    let description = extract_skill_description(&dest);
    let source = extract_github_source_from_url(&url);

    Ok(Skill {
        name: name_str,
        description,
        skill_type: "hub".to_string(),
        stars: 0,
        installed: true,
        update_available: false,
        last_updated: chrono::Utc::now().to_rfc3339(),
        git_url: url,
        tree_hash: Some(tree_hash),
        category: crate::core::skill::SkillCategory::None,
        author: None,
        topics: Vec::new(),
        agent_links: Some(agent_links),
        rank: None,
        source,
    })
}

#[tauri::command]
pub async fn uninstall_skill(name: String) -> Result<(), String> {
    // If it's a local skill, delegate to local_skill::delete
    if local_skill::is_local_skill(&name) {
        return local_skill::delete(&name).map_err(|e| e.to_string());
    }

    // Remove symlinks from all agents first
    let _ = sync::remove_skill_from_all_agents(&name);

    let skills_dir = sync::get_hub_skills_dir();
    let path = skills_dir.join(&name);

    // Use symlink_metadata() instead of exists() — exists() follows symlinks,
    // so a broken symlink (target deleted) returns false and is never cleaned up.
    if let Ok(meta) = path.symlink_metadata() {
        if meta.is_symlink() {
            std::fs::remove_file(&path)
                .map_err(|e| format!("Failed to remove symlink: {}", e))?;
        } else {
            std::fs::remove_dir_all(&path)
                .map_err(|e| format!("Failed to delete folder: {}", e))?;
        }
    }

    let lock_path = lockfile::lockfile_path();
    let mut lf = lockfile::Lockfile::load(&lock_path).unwrap_or_default();
    lf.remove(&name);
    let _ = lf.save(&lock_path);

    Ok(())
}

#[tauri::command]
pub async fn toggle_skill_for_agent(
    skill_name: String,
    agent_id: String,
    enable: bool,
) -> Result<(), String> {
    sync::toggle_skill_for_agent(&skill_name, &agent_id, enable).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_skill(name: String) -> Result<Skill, String> {
    let skills_dir = sync::get_hub_skills_dir();
    let path = skills_dir.join(&name);

    // Check if this is a repo-cached skill (symlink into .repos/)
    let is_repo_skill = repo_scanner::is_repo_cached_skill(&path);

    let lock_path = lockfile::lockfile_path();
    let mut lf = lockfile::Lockfile::load(&lock_path).unwrap_or_default();
    let lock_entry = lf.skills.iter().find(|s| s.name == name).cloned();

    let tree_hash = if is_repo_skill {
        // Pull via the repo cache
        let source_folder = lock_entry.as_ref().and_then(|e| e.source_folder.as_deref());
        repo_scanner::pull_repo_skill_update(&path, source_folder).map_err(|e| e.to_string())?
    } else {
        git_ops::pull_repo(&path).map_err(|e| e.to_string())?;
        git_ops::compute_tree_hash(&path).map_err(|e| e.to_string())?
    };

    if let Some(entry) = lf.skills.iter_mut().find(|s| s.name == name) {
        entry.tree_hash = tree_hash.clone();
    }
    let _ = lf.save(&lock_path);

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
        "local".to_string()
    } else {
        "hub".to_string()
    };

    Ok(Skill {
        name,
        description,
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
    })
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
    let skills_dir = sync::get_hub_skills_dir();
    let mut install_tasks = tokio::task::JoinSet::new();
    for skill_name in &group.skills {
        if !skills_dir.join(skill_name).exists() {
            if let Some(git_url) = group.skill_sources.get(skill_name) {
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
            eprintln!("[deploy_skill_group] install task join error: {}", e);
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
    let skills_dir = sync::get_hub_skills_dir();
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
    let skills_dir = sync::get_hub_skills_dir();
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
    let hub_dir = sync::get_hub_skills_dir();
    let hub_path = hub_dir.join(&name);

    // Don't overwrite existing skills
    if hub_path.symlink_metadata().is_ok() {
        return Ok(()); // Silently skip if already exists
    }

    // Create in skills-local/ and symlink back to skills/
    let _ = local_skill::create(&name, Some(&content)).map_err(|e| e.to_string())?;

    Ok(())
}

// ── Local Skills ────────────────────────────────────────────────────

#[tauri::command]
pub async fn create_local_skill(
    name: String,
    content: Option<String>,
) -> Result<Skill, String> {
    local_skill::create(&name, content.as_deref()).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_local_skill(name: String) -> Result<(), String> {
    local_skill::delete(&name).map_err(|e| e.to_string())
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
    let skills_dir = sync::get_hub_skills_dir();
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
    let skills_dir = sync::get_hub_skills_dir();
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
    let skills_dir = sync::get_hub_skills_dir();
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

    Ok(())
}

// ── Skill Bundles (.agentskill) ─────────────────────────────────────

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

// ── Text File I/O (share code files) ────────────────────────────────

#[tauri::command]
pub async fn write_text_file(path: String, content: String) -> Result<(), String> {
    std::fs::write(&path, &content).map_err(|e| format!("Failed to write file: {}", e))
}

#[tauri::command]
pub async fn read_text_file(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| format!("Failed to read file: {}", e))
}
