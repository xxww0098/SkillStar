//! Per-project skills-list persistence (`skills-list.json`).

use anyhow::{Context, Result};
use std::path::{Component, Path};

use super::types::SkillsList;
use ss_core::infra::{fs_ops, paths as fs_paths};

pub(super) fn validate_project_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.contains(['/', '\\', '\0'])
        || name.chars().any(char::is_control)
        || !matches!(
            Path::new(name).components().collect::<Vec<_>>().as_slice(),
            [Component::Normal(_)]
        )
    {
        anyhow::bail!("invalid project name: {name:?}");
    }
    Ok(())
}

pub(super) fn validate_project_relative_path(relative: &str) -> Result<()> {
    if relative.is_empty()
        || relative.contains(['\\', '\0', ':'])
        || relative.chars().any(char::is_control)
        || relative
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || Path::new(relative)
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        anyhow::bail!("invalid project skills path: {relative:?}");
    }
    Ok(())
}

pub(super) fn validate_skills_list(list: &SkillsList) -> Result<()> {
    for name in list.agents.values().flatten() {
        crate::content::validate_skill_name(name)?;
    }
    for relative in list.deploy_modes.keys() {
        validate_project_relative_path(relative)?;
    }
    let profiles = crate::agents::list_profiles();
    for profile in profiles
        .iter()
        .filter(|profile| profile.has_project_skills() && list.agents.contains_key(&profile.id))
    {
        validate_project_relative_path(&profile.project_skills_rel)?;
    }
    Ok(())
}

/// Load a project's skill list by project name. Read-only callers keep the
/// forgiving API; writers use the strict reader to distinguish damage from absence.
pub fn load_skills_list(name: &str) -> Option<SkillsList> {
    load_skills_list_strict(name).ok().flatten()
}

pub(super) fn load_skills_list_strict(name: &str) -> Result<Option<SkillsList>> {
    validate_project_name(name)?;
    let path = fs_paths::project_detail_dir(name).join("skills-list.json");
    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(err)
            if err.kind() == std::io::ErrorKind::NotFound
                && matches!(path.symlink_metadata(), Err(error) if error.kind() == std::io::ErrorKind::NotFound) =>
        {
            return Ok(None);
        }
        Err(err) => {
            return Err(err)
                .with_context(|| format!("failed to read skills list: {}", path.display()));
        }
    };
    serde_json::from_str(&content)
        .with_context(|| format!("failed to parse skills list: {}", path.display()))
        .map(Some)
}

/// Save a project's skill list atomically. An unreadable existing manifest
/// must be repaired explicitly instead of being replaced with a default list.
pub fn save_skills_list(name: &str, list: &SkillsList) -> Result<()> {
    let _guard = super::write_lock::lock_project_write()?;
    validate_skills_list(list)?;
    let _ = load_skills_list_strict(name)?;
    let path = fs_paths::project_detail_dir(name).join("skills-list.json");
    let content = serde_json::to_vec_pretty(list).context("failed to serialize skills list")?;
    fs_ops::atomic_write(&path, &content)
        .with_context(|| format!("failed to write skills list: {}", path.display()))?;
    Ok(())
}
