//! Canonical directory and lock removal after deployments are cleared.
//!
//! A lock write failure puts the staged paths back. Every destructive removal
//! archives the skill into `data/skills/backups/` first.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Archive and drop the installed directory (plus any legacy `local/` target)
/// and its lock/library entries.
pub(crate) fn delete_files_and_lock(name: &str) -> Result<()> {
    let hub_path = ss_core::infra::paths::hub_skills_dir().join(name);
    let legacy_local_path = ss_core::infra::paths::legacy_local_skills_dir().join(name);
    let backup_source = if hub_path.exists() {
        &hub_path
    } else {
        &legacy_local_path
    };
    if backup_source.exists() {
        crate::backup::archive_skill(name, backup_source, crate::backup::BackupReason::Uninstall)?;
    }

    let mut staged = Vec::new();
    if let Some(aside) = stage_existing(&hub_path)? {
        staged.push((aside, hub_path));
    }
    if legacy_local_path.symlink_metadata().is_ok() {
        match stage_existing(&legacy_local_path) {
            Ok(Some(aside)) => staged.push((aside, legacy_local_path)),
            Ok(None) => {}
            Err(error) => {
                restore_all(&staged);
                return Err(error);
            }
        }
    }
    commit_removal(name, &staged, true)
}

/// Remove a legacy hub symlink (pointing into `data/skills/local/`) and/or drop
/// the lock entry for a local skill while keeping its directory intact.
pub(crate) fn unlink_hub_and_lock(name: &str) -> Result<()> {
    let hub_path = ss_core::infra::paths::hub_skills_dir().join(name);
    let is_link = ss_core::infra::fs_ops::is_link(&hub_path);
    if !is_link && hub_path.symlink_metadata().is_ok() {
        if crate::local_skill::is_local_skill(name) {
            return commit_removal(name, &[], false);
        }
        anyhow::bail!("Skill '{name}' is a canonical directory, not a local hub link");
    }
    let staged = stage_existing(&hub_path)?.map(|aside| vec![(aside, hub_path)]);
    commit_removal(name, staged.as_deref().unwrap_or(&[]), is_link)
}

fn stage_existing(path: &Path) -> Result<Option<PathBuf>> {
    if path.symlink_metadata().is_err() {
        return Ok(None);
    }
    let parent = path
        .parent()
        .with_context(|| format!("{} has no parent directory", path.display()))?;
    let file_name = path
        .file_name()
        .with_context(|| format!("{} has no file name", path.display()))?;
    let aside = parent.join(format!(
        "{}remove-{}-{}",
        crate::materialize::TRANSIENT_PREFIX,
        file_name.to_string_lossy(),
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::rename(path, &aside)
        .with_context(|| format!("Failed to stage '{}' for removal", path.display()))?;
    crate::materialize::note_transient_born(&aside);
    Ok(Some(aside))
}

fn restore_staged(aside: &Path, dest: &Path) -> bool {
    if std::fs::rename(aside, dest).is_err() {
        return false;
    }
    crate::materialize::forget_transient_born(aside);
    true
}

fn restore_all(staged: &[(PathBuf, PathBuf)]) {
    for (aside, dest) in staged.iter().rev() {
        let _ = restore_staged(aside, dest);
    }
}

fn commit_removal(name: &str, staged: &[(PathBuf, PathBuf)], forget_library: bool) -> Result<()> {
    if let Err(error) = crate::skill_lock::mutate(|lock| {
        lock.remove(name);
    }) {
        let mut restored = true;
        for (aside, dest) in staged.iter().rev() {
            if !restore_staged(aside, dest) {
                restored = false;
            }
        }
        let error = error.context(format!(
            "Failed to remove the install lock entry for '{name}'"
        ));
        if !restored {
            return Err(error.context(
                "restoring staged skill files after the lock write failed was incomplete",
            ));
        }
        return Err(error);
    }
    if forget_library && let Err(error) = crate::library::forget_skill(name) {
        tracing::warn!(target: "skills", skill = %name, %error, "failed to remove skill from library.json");
    }
    for (aside, _) in staged {
        crate::materialize::remove_entry(aside)
            .with_context(|| format!("Failed to delete staged skill files for '{name}'"))?;
    }
    Ok(())
}
