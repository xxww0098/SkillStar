use crate::core::{
    git::ops as git_ops,
    infra::error::AppError,
    local_skill, security_scan,
    skill::{Skill, SkillContent, parse_skill_content},
};

use super::skill_paths::{resolve_skill_content_dir, resolve_skill_dir};

#[tauri::command]
pub async fn read_skill_file_raw(name: String) -> Result<String, AppError> {
    let skills_dir = crate::core::infra::paths::hub_skills_dir();
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
    let hub_dir = crate::core::infra::paths::hub_skills_dir();
    let hub_path = hub_dir.join(&name);

    if hub_path.symlink_metadata().is_ok() {
        return Ok(());
    }

    let _ = local_skill::create(&name, Some(&content)).map_err(AppError::Anyhow)?;
    crate::core::installed_skill::invalidate_cache();

    Ok(())
}

#[tauri::command]
pub async fn create_local_skill(name: String, content: Option<String>) -> Result<Skill, AppError> {
    let skill = local_skill::create(&name, content.as_deref()).map_err(AppError::Anyhow)?;
    crate::core::installed_skill::invalidate_cache();
    Ok(skill)
}

#[tauri::command]
pub async fn delete_local_skill(name: String) -> Result<(), AppError> {
    local_skill::delete(&name).map_err(AppError::Anyhow)?;
    crate::core::installed_skill::invalidate_cache();
    Ok(())
}

#[tauri::command]
pub async fn migrate_local_skills() -> Result<u32, AppError> {
    tokio::task::spawn_blocking(|| local_skill::migrate_existing())
        .await?
        .map_err(AppError::Anyhow)
}

#[tauri::command]
pub async fn list_skill_files(name: String) -> Result<Vec<String>, AppError> {
    let skills_dir = crate::core::infra::paths::hub_skills_dir();
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

        if name_str == ".git" {
            continue;
        }

        if path.is_dir() {
            collect_files_recursive(root, &path, files);
        } else if let Ok(rel) = path.strip_prefix(root) {
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            files.push(rel_str);
        }
    }
}

#[tauri::command]
pub async fn read_skill_content(name: String) -> Result<SkillContent, AppError> {
    let skills_dir = crate::core::infra::paths::hub_skills_dir();
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
    let skills_dir = crate::core::infra::paths::hub_skills_dir();
    let skill_dir = skills_dir.join(&name);
    if !skill_dir.exists() {
        return Err(AppError::SkillNotFound { name });
    }
    let effective_dir = resolve_skill_content_dir(&name).unwrap_or(skill_dir);
    let skill_path = effective_dir.join("SKILL.md");

    if !skill_path.exists() {
        return Err(AppError::SkillNotFound { name });
    }

    let temp_path = skill_path.with_extension("tmp");
    std::fs::write(&temp_path, &content)?;
    std::fs::rename(&temp_path, &skill_path)?;

    security_scan::invalidate_skill_cache(&name);

    Ok(())
}
