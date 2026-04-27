use anyhow::{Context, Result, anyhow};
use skillstar_core_types::lockfile;
use skillstar_infra::paths;
use skillstar_skill_core::discovery as skill_discover;
use skillstar_skill_core::source_resolver;
use std::path::Path;

use super::DiscoveredSkill;

pub fn scan_skills_in_repo(
    repo_dir: &Path,
    repo_url: &str,
    full_depth: bool,
) -> Vec<DiscoveredSkill> {
    let hub_skills_dir = paths::hub_skills_dir();
    let lock_entries = lockfile::Lockfile::load(&paths::lockfile_path())
        .map(|lf| lf.skills)
        .unwrap_or_default();

    let raw_skills = skill_discover::discover_skills(repo_dir, full_depth);

    let mut discovered: Vec<DiscoveredSkill> = raw_skills;

    for skill in &mut discovered {
        let source_folder = if skill.folder_path.is_empty() {
            None
        } else {
            Some(skill.folder_path.as_str())
        };

        let legacy_name = lock_entries.iter().find_map(|entry| {
            if source_resolver::same_remote_url(&entry.git_url, repo_url)
                && option_str_eq(entry.source_folder.as_deref(), source_folder)
            {
                Some(entry.name.clone())
            } else {
                None
            }
        });

        if let Some(name) = legacy_name {
            skill.id = name;
            skill.already_installed = hub_skills_dir.join(&skill.id).exists()
                || lock_entries.iter().any(|entry| {
                    entry.name == skill.id
                        && source_resolver::same_remote_url(&entry.git_url, repo_url)
                        && option_str_eq(entry.source_folder.as_deref(), source_folder)
                });
        } else {
            skill.already_installed = hub_skills_dir.join(&skill.id).exists();
        }
    }

    skill_discover::dedupe_discovered_skills(discovered)
}

fn option_str_eq(left: Option<&str>, right: Option<&str>) -> bool {
    left == right
}

pub fn compute_subtree_hash(repo_dir: &Path, folder_path: &str) -> Result<String> {
    let output = skillstar_infra::path_env::command_with_path("git")
        .current_dir(repo_dir)
        .args(["rev-parse", &format!("HEAD:{}", folder_path)])
        .output()
        .context("Failed to execute git rev-parse for subtree")?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("git rev-parse failed: {}", err.trim()));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
