//! Per-project skills-list persistence (`skills-list.json`).

use anyhow::{Context, Result};

use super::types::SkillsList;
use skillstar_core::infra::paths as fs_paths;

/// Load a project's skill list by project name.
pub fn load_skills_list(name: &str) -> Option<SkillsList> {
    let path = fs_paths::project_detail_dir(name).join("skills-list.json");
    let content = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Save a project's skill list.
pub fn save_skills_list(name: &str, list: &SkillsList) -> Result<()> {
    let path = fs_paths::project_detail_dir(name).join("skills-list.json");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(list).context("failed to serialize skills list")?;
    std::fs::write(&path, content).context("failed to write skills list")?;
    Ok(())
}
