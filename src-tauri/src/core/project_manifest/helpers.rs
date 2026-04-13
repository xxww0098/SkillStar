//! Filesystem helpers for project skill directories.

use anyhow::{Context, Result};
use std::path::Path;

use crate::core::{infra::fs_ops, projects::agents as agent_profile};

pub(crate) fn clear_project_symlinks(
    project: &Path,
    profile: &agent_profile::AgentProfile,
) -> Result<()> {
    if !profile.has_project_skills() {
        return Ok(());
    }
    let target_dir = project.join(&profile.project_skills_rel);
    let entries = match std::fs::read_dir(&target_dir) {
        Ok(entries) => Some(entries),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(err) => {
            return Err(err)
                .with_context(|| format!("failed to read skill dir: {}", target_dir.display()));
        }
    };

    if let Some(entries) = entries {
        for entry in entries {
            let entry = entry.with_context(|| {
                format!(
                    "failed to inspect project skill entry in {}",
                    target_dir.display()
                )
            })?;
            let entry_path = entry.path();
            if fs_ops::is_link(&entry_path) {
                fs_ops::remove_symlink(&entry_path).with_context(|| {
                    format!("failed to remove stale symlink: {}", entry_path.display())
                })?;
            } else if entry_path.is_dir() {
                // Also remove copy-based deployments
                let _ = fs_ops::remove_link_or_copy(&entry_path);
            }
        }
    }

    prune_empty_dirs_upward(&target_dir, project)?;

    Ok(())
}

pub(crate) fn prune_empty_dirs_upward(start_dir: &Path, project_root: &Path) -> Result<()> {
    let mut current = start_dir.to_path_buf();

    while current.starts_with(project_root) && current != project_root {
        match std::fs::remove_dir(&current) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) if err.kind() == std::io::ErrorKind::DirectoryNotEmpty => break,
            Err(err) => {
                return Err(err)
                    .with_context(|| format!("failed to prune empty dir: {}", current.display()));
            }
        }

        let Some(parent) = current.parent() else {
            break;
        };
        current = parent.to_path_buf();
    }

    Ok(())
}
