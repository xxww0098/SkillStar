use crate::core::{
    git::{ops as git_ops, repo_history, source_resolver},
    infra::{fs_ops, paths},
    local_skill, lockfile, security_scan,
};
use anyhow::{Context, Result, anyhow};
use std::path::Path;
use tracing::warn;

use super::{SkillInstallTarget, cache_dir_name};

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
            skillstar_skills::repo_scanner::compute_subtree_hash(&cache_dir, &target.folder_path)
                .unwrap_or_default()
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

pub fn compute_subtree_hash_pub(repo_dir: &Path, folder_path: &str) -> Result<String> {
    skillstar_skills::repo_scanner::compute_subtree_hash(repo_dir, folder_path)
}
