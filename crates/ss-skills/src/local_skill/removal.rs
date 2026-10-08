//! Hub link, local directory and lock removal after deployments are cleared.
//!
//! A lock write failure puts the staged paths back. The local directory is
//! removed only by [`delete_files_and_lock`].

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Drop the hub link, the local directory and the lock entry.
pub(crate) fn delete_files_and_lock(name: &str) -> Result<()> {
    let hub_path = ss_core::infra::paths::hub_skills_dir().join(name);
    let local_path = ss_core::infra::paths::local_skills_dir().join(name);
    let mut staged = Vec::new();
    if let Some(aside) = stage_existing(&hub_path)? {
        staged.push((aside, hub_path));
    }
    match stage_existing(&local_path) {
        Ok(Some(aside)) => staged.push((aside, local_path)),
        Ok(None) => {}
        Err(error) => {
            restore_all(&staged);
            return Err(error);
        }
    }
    commit_removal(name, &staged)
}

/// Remove the hub symlink and the lock entry. The local directory stays.
pub(crate) fn unlink_hub_and_lock(name: &str) -> Result<()> {
    let hub_path = ss_core::infra::paths::hub_skills_dir().join(name);
    if hub_path.symlink_metadata().is_ok() && !ss_core::infra::fs_ops::is_link(&hub_path) {
        anyhow::bail!("Skill '{name}' is a canonical directory, not a local hub link");
    }
    let staged = stage_existing(&hub_path)?.map(|aside| vec![(aside, hub_path)]);
    commit_removal(name, staged.as_deref().unwrap_or(&[]))
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

fn commit_removal(name: &str, staged: &[(PathBuf, PathBuf)]) -> Result<()> {
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
    for (aside, _) in staged {
        crate::materialize::remove_entry(aside)
            .with_context(|| format!("Failed to delete staged skill files for '{name}'"))?;
    }
    Ok(())
}
