//! Filesystem helpers for project skill directories.

use anyhow::{Context, Result};
use std::path::Path;

pub fn prune_empty_dirs_upward(start_dir: &Path, project_root: &Path) -> Result<()> {
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
