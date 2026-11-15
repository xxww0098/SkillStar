//! Project index CRUD — the `projects.json` registry of known projects.

use anyhow::{Context, Result};
use std::path::Path;

use super::store::{load_skills_list_strict, validate_project_name};
use super::sync::{preflight_project_targets, sync_project_path_rebind_locked};
use super::types::{ProjectEntry, ProjectIndex, ensure_project_root_exists};
use ss_core::infra::{fs_ops, paths as fs_paths};

pub(super) fn unique_project_name(index: &ProjectIndex, name: &str) -> String {
    let mut candidate = name.to_string();
    let mut counter = 1u32;
    while index
        .projects
        .iter()
        .any(|project| project.name == candidate)
    {
        counter += 1;
        candidate = format!("{name}-{counter}");
    }
    candidate
}

pub(super) fn load_index() -> ProjectIndex {
    load_index_strict().unwrap_or_default()
}

pub(super) fn load_index_strict() -> Result<ProjectIndex> {
    let path = fs_paths::projects_manifest_path();
    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(err)
            if err.kind() == std::io::ErrorKind::NotFound
                && matches!(path.symlink_metadata(), Err(error) if error.kind() == std::io::ErrorKind::NotFound) =>
        {
            return Ok(ProjectIndex::default());
        }
        Err(err) => {
            return Err(err)
                .with_context(|| format!("failed to read project index: {}", path.display()));
        }
    };
    let index: ProjectIndex = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse project index: {}", path.display()))?;
    for project in &index.projects {
        validate_project_name(&project.name)?;
    }
    Ok(index)
}

pub(super) fn save_index(index: &ProjectIndex) -> Result<()> {
    let _guard = super::write_lock::lock_project_write()?;
    let _ = load_index_strict()?;
    for project in &index.projects {
        validate_project_name(&project.name)?;
    }
    let path = fs_paths::projects_manifest_path();
    let content = serde_json::to_vec_pretty(index).context("failed to serialize project index")?;
    fs_ops::atomic_write(&path, &content)
        .with_context(|| format!("failed to write project index: {}", path.display()))?;
    Ok(())
}

/// Register a project in the index. Creates the per-project config folder.
/// If a project with the same path already exists, returns the existing entry.
pub fn register_project(project_path: &str) -> Result<ProjectEntry> {
    let _guard = super::write_lock::lock_project_write()?;
    // Avoid silently creating new directories when the user passes a stale path.
    let _ = ensure_project_root_exists(project_path)?;
    let mut index = load_index_strict()?;

    // Keep exact-string matching for compatibility with the canonical binding API.
    if let Some(existing) = index.projects.iter().find(|p| p.path == project_path) {
        return Ok(existing.clone());
    }

    let name = Path::new(project_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let unique_name = unique_project_name(&index, &name);
    validate_project_name(&unique_name)?;

    let entry = ProjectEntry {
        path: project_path.to_string(),
        name: unique_name.clone(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    let dir = fs_paths::project_detail_dir(&unique_name);
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create project dir: {}", dir.display()))?;

    index.projects.push(entry.clone());
    save_index(&index)?;
    Ok(entry)
}

/// List all registered projects.
pub fn list_projects() -> Vec<ProjectEntry> {
    load_index().projects
}

/// Remove a project from the index and delete its config folder.
pub fn remove_project(name: &str) -> Result<()> {
    let _guard = super::write_lock::lock_project_write()?;
    validate_project_name(name)?;
    let mut index = load_index_strict()?;
    let before = index.projects.len();
    index.projects.retain(|p| p.name != name);
    if index.projects.len() == before {
        anyhow::bail!("project '{}' not found", name);
    }
    // A damaged manifest must not be silently erased together with its config.
    let _ = load_skills_list_strict(name)?;
    save_index(&index)?;

    let dir = fs_paths::project_detail_dir(name);
    if dir.exists() {
        std::fs::remove_dir_all(&dir)
            .with_context(|| format!("failed to remove project dir: {}", dir.display()))?;
    }
    Ok(())
}

/// Reconcile the configured deployments at a new root before updating the
/// registry. The old root is kept, and a partial failure at the new root can
/// be retried without losing the old registered path or the deployment mode.
pub fn update_project_path(name: &str, new_path: &str) -> Result<u32> {
    let _source = super::upkeep::lock_copy_source()?;
    let _guard = super::write_lock::lock_project_write()?;
    validate_project_name(name)?;
    let project = ensure_project_root_exists(new_path)?;
    let canonical = std::fs::canonicalize(&project)
        .with_context(|| format!("failed to resolve project root: {}", project.display()))?;
    let mut index = load_index_strict()?;
    let position = index
        .projects
        .iter()
        .position(|p| p.name == name)
        .with_context(|| format!("project '{name}' not found"))?;
    for other in index.projects.iter().filter(|p| p.name != name) {
        if std::fs::canonicalize(&other.path).ok().as_ref() == Some(&canonical) {
            anyhow::bail!(
                "project root {} is already registered as '{}'",
                canonical.display(),
                other.name
            );
        }
    }

    let skills_list = load_skills_list_strict(name)?.unwrap_or_default();
    preflight_project_targets(&project, &skills_list)?;
    let count = sync_project_path_rebind_locked(new_path, &skills_list)?;

    index.projects[position].path = new_path.to_string();
    save_index(&index)?;
    Ok(count)
}

#[cfg(test)]
#[path = "index_tests.rs"]
mod tests;
