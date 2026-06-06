//! Project index CRUD — the `projects.json` registry of known projects.

use anyhow::{Context, Result};
use std::path::Path;

use super::store::load_skills_list;
use super::sync::full_sync;
use super::types::{ensure_project_root_exists, ProjectEntry, ProjectIndex};
use skillstar_core::infra::paths as fs_paths;

pub(super) fn load_index() -> ProjectIndex {
    let path = fs_paths::projects_manifest_path();
    if !path.exists() {
        return ProjectIndex::default();
    }
    let Ok(content) = std::fs::read_to_string(&path) else {
        return ProjectIndex::default();
    };
    serde_json::from_str(&content).unwrap_or_default()
}

fn save_index(index: &ProjectIndex) -> Result<()> {
    let path = fs_paths::projects_manifest_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content =
        serde_json::to_string_pretty(index).context("failed to serialize project index")?;
    std::fs::write(&path, content).context("failed to write project index")?;
    Ok(())
}

/// Register a project in the index. Creates the per-project config folder.
/// If a project with the same path already exists, returns the existing entry.
pub fn register_project(project_path: &str) -> Result<ProjectEntry> {
    // Avoid silently creating new directories when the user passes a stale path.
    let _ = ensure_project_root_exists(project_path)?;

    let mut index = load_index();

    // Check for existing entry by path
    if let Some(existing) = index.projects.iter().find(|p| p.path == project_path) {
        return Ok(existing.clone());
    }

    let name = Path::new(project_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // Handle duplicate names by appending a suffix
    let unique_name = {
        let mut candidate = name.clone();
        let mut counter = 1u32;
        while index.projects.iter().any(|p| p.name == candidate) {
            counter += 1;
            candidate = format!("{name}-{counter}");
        }
        candidate
    };

    let entry = ProjectEntry {
        path: project_path.to_string(),
        name: unique_name.clone(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    // Create per-project config directory
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
    let mut index = load_index();
    let before = index.projects.len();
    index.projects.retain(|p| p.name != name);
    if index.projects.len() == before {
        anyhow::bail!("project '{}' not found", name);
    }
    save_index(&index)?;

    // Remove config folder
    let dir = fs_paths::project_detail_dir(name);
    if dir.exists() {
        std::fs::remove_dir_all(&dir)
            .with_context(|| format!("failed to remove project dir: {}", dir.display()))?;
    }

    Ok(())
}

/// Update a project's local path and rebuild its symlinks.
pub fn update_project_path(name: &str, new_path: &str) -> Result<u32> {
    // Validate before mutating the index so we don't persist a broken path.
    let _ = ensure_project_root_exists(new_path)?;

    let mut index = load_index();
    let Some(entry) = index.projects.iter_mut().find(|p| p.name == name) else {
        anyhow::bail!("project '{}' not found", name);
    };

    entry.path = new_path.to_string();
    save_index(&index)?;

    // Rebuild mapped paths
    let skills_list = load_skills_list(name).unwrap_or_default();

    let count = full_sync(new_path, &skills_list, None)?;
    Ok(count)
}
