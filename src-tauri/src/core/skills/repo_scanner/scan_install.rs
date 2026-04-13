//! Lockfile-aware repo scan and batch install into the hub.

use crate::core::{
    git::{ops as git_ops, repo_history, source_resolver},
    infra::{fs_ops, paths},
    local_skill,
    lockfile,
    path_env::command_with_path,
    security_scan,
    skills::discover as skill_discover,
};
use anyhow::{Context, Result, anyhow};
use std::path::Path;
use tracing::warn;

use super::cache::cache_dir_name;
use super::{DiscoveredSkill, SkillInstallTarget};

/// Scan a cloned repo for SKILL.md files and return discovered skills.
///
/// This is a **lockfile-aware** scan — it enriches each discovered skill with
/// `already_installed` by consulting the lockfile.
pub fn scan_skills_in_repo(
    repo_dir: &Path,
    repo_url: &str,
    full_depth: bool,
) -> Vec<DiscoveredSkill> {
    let hub_skills_dir = paths::hub_skills_dir();
    let lock_entries = lockfile::Lockfile::load(&lockfile::lockfile_path())
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

/// Install selected skills from a scanned repo.
pub fn install_from_repo(
    source: &str,
    repo_url: &str,
    targets: &[SkillInstallTarget],
) -> Result<Vec<String>> {
    let hub_skills_dir = paths::hub_skills_dir();
    std::fs::create_dir_all(&hub_skills_dir).context("Failed to create hub skills directory")?;

    let cache_dir = paths::repos_cache_dir().join(cache_dir_name(source));
    if !cache_dir.exists() {
        return Err(anyhow!(
            "Repo cache not found. Please scan the repository first."
        ));
    }

    let _lock = lockfile::get_mutex()
        .lock()
        .map_err(|_| anyhow!("Lockfile mutex poisoned"))?;
    let lock_path = lockfile::lockfile_path();
    let mut lf = lockfile::Lockfile::load(&lock_path).unwrap_or_default();

    let mut installed_names = Vec::new();

    for target in targets {
        let dest = hub_skills_dir.join(&target.id);
        let existing_entry = lf.skills.iter().find(|entry| entry.name == target.id);

        if dest.symlink_metadata().is_ok()
            && !can_replace_existing_skill(&target.id, repo_url, existing_entry)
        {
            warn!(
                target: "repo_scanner",
                skill = %target.id,
                "refusing to replace existing skill from a different source"
            );
            continue;
        }

        if let Ok(_meta) = dest.symlink_metadata() {
            if fs_ops::is_link(&dest) {
                let _ = fs_ops::remove_symlink(&dest);
            } else {
                let _ = std::fs::remove_dir_all(&dest);
            }
        }

        let source_path = if target.folder_path.is_empty() {
            cache_dir.clone()
        } else {
            cache_dir.join(&target.folder_path)
        };

        if !source_path.exists() {
            warn!(
                target: "repo_scanner",
                path = %source_path.display(),
                "skill folder not found"
            );
            continue;
        }

        fs_ops::create_symlink(&source_path, &dest)
            .with_context(|| format!("Failed to symlink {:?} → {:?}", source_path, dest))?;

        let tree_hash = if target.folder_path.is_empty() {
            git_ops::compute_tree_hash(&cache_dir).unwrap_or_default()
        } else {
            compute_subtree_hash(&cache_dir, &target.folder_path).unwrap_or_default()
        };

        let source_folder = if target.folder_path.is_empty() {
            None
        } else {
            Some(target.folder_path.clone())
        };

        lf.upsert(lockfile::LockEntry {
            name: target.id.clone(),
            git_url: repo_url.to_string(),
            tree_hash,
            installed_at: chrono::Utc::now().to_rfc3339(),
            source_folder,
        });

        installed_names.push(target.id.clone());
    }

    lf.save(&lock_path)
        .context("Failed to save lockfile after batch install")?;

    let _ = repo_history::upsert_entry(source, repo_url);

    for name in &installed_names {
        security_scan::invalidate_skill_cache(name);
    }

    Ok(installed_names)
}

fn can_replace_existing_skill(
    skill_name: &str,
    repo_url: &str,
    existing_entry: Option<&lockfile::LockEntry>,
) -> bool {
    if local_skill::is_local_skill(skill_name) {
        return false;
    }
    existing_entry
        .map(|entry| source_resolver::same_remote_url(&entry.git_url, repo_url))
        .unwrap_or(false)
}

pub(super) fn compute_subtree_hash(repo_dir: &Path, folder_path: &str) -> Result<String> {
    let output = command_with_path("git")
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

/// Compute the git tree hash for a specific subfolder (public wrapper).
pub fn compute_subtree_hash_pub(repo_dir: &Path, folder_path: &str) -> Result<String> {
    compute_subtree_hash(repo_dir, folder_path)
}
