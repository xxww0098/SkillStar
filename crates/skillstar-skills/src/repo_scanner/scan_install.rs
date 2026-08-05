use crate::git::ops as git_ops;
use crate::{content, local_skill, lockfile, source_resolver};
use anyhow::{Context, Result, anyhow};
use skillstar_core::infra::{fs_ops, paths};
use std::path::Path;
use tracing::warn;

use super::SkillInstallTarget;

struct PreparedRepoInstall {
    name: String,
    source: std::path::PathBuf,
    destination: std::path::PathBuf,
    staging: std::path::PathBuf,
    previous_target: Option<std::path::PathBuf>,
    lock_entry: crate::lockfile::LockEntry,
}

/// Backward-compatible scan/install facade for callers that identify the
/// default-branch cache by source. Ref-pinned installs use
/// [`install_from_repo_at`] so they retain their isolated cache and lock data.
pub fn install_from_repo(
    source: &str,
    repo_url: &str,
    targets: &[SkillInstallTarget],
) -> Result<Vec<String>> {
    install_from_repo_in_session(
        source,
        repo_url,
        targets,
        &crate::git::transport::GitOperationSession::public(),
    )
}

pub fn install_from_repo_in_session(
    source: &str,
    repo_url: &str,
    targets: &[SkillInstallTarget],
    session: &crate::git::transport::GitOperationSession,
) -> Result<Vec<String>> {
    let repo_dir = super::clone_or_fetch_repo_in_session(repo_url, source, session)?;
    install_from_repo_at(&repo_dir, repo_url, None, targets)
}

pub fn install_from_repo_at(
    repo_dir: &Path,
    repo_url: &str,
    git_ref: Option<&str>,
    targets: &[SkillInstallTarget],
) -> Result<Vec<String>> {
    let hub_skills_dir = paths::hub_skills_dir();
    std::fs::create_dir_all(&hub_skills_dir).context("Failed to create hub skills directory")?;

    if !repo_dir.exists() {
        return Err(anyhow!(
            "Repo cache not found. Please scan the repository first."
        ));
    }
    let repo_root = std::fs::canonicalize(repo_dir)
        .with_context(|| format!("Failed to resolve repo cache '{}'", repo_dir.display()))?;

    let _lock = lockfile::get_mutex()
        .lock()
        .map_err(|_| anyhow!("Lockfile mutex poisoned"))?;
    let lock_path = lockfile::lockfile_path();
    let mut lf = lockfile::Lockfile::load(&lock_path)?;

    let mut prepared = Vec::new();
    for (index, target) in targets.iter().enumerate() {
        content::validate_skill_name(&target.id)
            .with_context(|| format!("Invalid Skill id '{}'", target.id))?;
        let dest = hub_skills_dir.join(&target.id);
        let existing_entry = lf.skills.iter().find(|entry| entry.name == target.id);
        let source_path = if target.folder_path.is_empty() {
            repo_root.clone()
        } else {
            repo_root.join(&target.folder_path)
        };

        if !source_path.exists() {
            warn!(
                target: "repo_scanner",
                path = %source_path.display(),
                "skill folder not found"
            );
            continue;
        }
        let source_path = std::fs::canonicalize(&source_path).with_context(|| {
            format!("Failed to resolve Skill source '{}'", source_path.display())
        })?;
        if !source_path.starts_with(&repo_root) {
            anyhow::bail!(
                "Skill source escapes repo cache '{}': '{}'",
                repo_root.display(),
                source_path.display()
            );
        }
        let content_hash = content::snapshot_path(&target.id, &source_path)
            .with_context(|| format!("Failed to capture content baseline for '{}'", target.id))?
            .content_hash;

        if dest.symlink_metadata().is_ok()
            && (!fs_ops::is_link(&dest)
                || !can_replace_existing_skill(&target.id, repo_url, existing_entry))
        {
            warn!(
                target: "repo_scanner",
                skill = %target.id,
                "refusing to replace existing skill from a different source"
            );
            continue;
        }

        let relative_source = source_path
            .strip_prefix(&repo_root)
            .expect("contained source has a relative path")
            .to_string_lossy()
            .replace('\\', "/");
        let source_folder = (!relative_source.is_empty()).then_some(relative_source);
        let tree_hash = match source_folder.as_deref() {
            Some(folder) => git_ops::compute_subtree_hash(&repo_root, folder).unwrap_or_default(),
            None => git_ops::compute_tree_hash(&repo_root).unwrap_or_default(),
        };
        let previous_target = if fs_ops::is_link(&dest) {
            Some(fs_ops::read_link_resolved(&dest).with_context(|| {
                format!("Failed to record existing Skill link '{}'", dest.display())
            })?)
        } else {
            None
        };

        prepared.push(PreparedRepoInstall {
            name: target.id.clone(),
            source: source_path,
            destination: dest.clone(),
            staging: hub_skills_dir.join(format!(".skillstar-install-{index}")),
            previous_target,
            lock_entry: crate::lockfile::LockEntry {
                name: target.id.clone(),
                git_url: repo_url.to_string(),
                git_ref: git_ref.map(str::to_string),
                tree_hash,
                content_hash: Some(content_hash),
                content_hash_version: Some(content::SNAPSHOT_HASH_VERSION),
                installed_at: chrono::Utc::now().to_rfc3339(),
                source_folder,
            },
        });
    }

    for item in &prepared {
        if item.staging.symlink_metadata().is_ok()
            && let Err(error) = fs_ops::remove_link_or_copy(&item.staging)
        {
            cleanup_staging(&prepared);
            return Err(error).with_context(|| {
                format!(
                    "Failed to clear stale install staging '{}'",
                    item.staging.display()
                )
            });
        }
        if let Err(error) = fs_ops::create_symlink(&item.source, &item.staging) {
            cleanup_staging(&prepared);
            return Err(error).with_context(|| {
                format!(
                    "Failed to stage symlink {:?} → {:?}",
                    item.source, item.destination
                )
            });
        }
    }

    let mut applied = 0usize;
    for item in &prepared {
        if fs_ops::is_link(&item.destination)
            && let Err(error) = fs_ops::remove_symlink(&item.destination)
        {
            rollback_repo_installs(&prepared, applied);
            return Err(error).with_context(|| format!("Failed to replace Skill '{}'", item.name));
        }
        if let Err(error) = std::fs::rename(&item.staging, &item.destination) {
            rollback_repo_installs(&prepared, applied + 1);
            return Err(error).with_context(|| format!("Failed to install Skill '{}'", item.name));
        }
        applied += 1;
    }

    for item in &prepared {
        lf.upsert(item.lock_entry.clone());
    }
    if let Err(error) = lf
        .save(&lock_path)
        .context("Failed to save lockfile after batch install")
    {
        rollback_repo_installs(&prepared, applied);
        return Err(error);
    }

    Ok(prepared.into_iter().map(|item| item.name).collect())
}

fn cleanup_staging(prepared: &[PreparedRepoInstall]) {
    for item in prepared {
        if item.staging.symlink_metadata().is_ok() {
            let _ = fs_ops::remove_link_or_copy(&item.staging);
        }
    }
}

fn rollback_repo_installs(prepared: &[PreparedRepoInstall], applied: usize) {
    for item in prepared.iter().take(applied).rev() {
        if item.destination.symlink_metadata().is_ok() {
            let _ = fs_ops::remove_link_or_copy(&item.destination);
        }
        if let Some(previous) = &item.previous_target {
            let _ = fs_ops::create_symlink(previous, &item.destination);
        }
    }
    cleanup_staging(prepared);
}

fn can_replace_existing_skill(
    skill_name: &str,
    repo_url: &str,
    existing_entry: Option<&crate::lockfile::LockEntry>,
) -> bool {
    if local_skill::is_local_skill(skill_name) {
        return false;
    }
    existing_entry
        .map(|entry| source_resolver::same_remote_url(&entry.git_url, repo_url))
        .unwrap_or(false)
}
