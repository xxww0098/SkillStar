//! Faithful materialization for user-owned local copies.
//!
//! Tutorial staging deliberately replaces symlinks with sentinel files. A
//! preserved local Skill has different semantics: the user owns the result, so
//! link entries must remain links with the same target text.

use std::path::Path;

use skillstar_core::infra::{error::AppError, fs_ops};

use crate::content::{SkillSnapshot, SnapshotFileKind};

impl SkillSnapshot {
    pub fn materialize_owned_to(&self, destination: &Path) -> Result<(), AppError> {
        if destination.symlink_metadata().is_ok() {
            return Err(AppError::Other(format!(
                "Snapshot destination already exists: {}",
                destination.display()
            )));
        }

        std::fs::create_dir_all(destination)?;
        let result = (|| -> Result<(), AppError> {
            for file in &self.files {
                let relative = safe_relative_path(&file.relative_path)?;
                let output_path = destination.join(&relative);
                if let Some(parent) = output_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }

                match file.kind {
                    SnapshotFileKind::Regular => {
                        std::fs::write(&output_path, &file.content)?;
                        set_executable(&output_path, file.executable)?;
                    }
                    SnapshotFileKind::Symlink => {
                        let target = std::str::from_utf8(&file.content).map_err(|_| {
                            AppError::Other(format!(
                                "Snapshot symlink target is not UTF-8: {}",
                                file.relative_path
                            ))
                        })?;
                        let original_link = self.root.join(&relative);
                        let target_is_dir = std::fs::metadata(&original_link)
                            .map(|metadata| metadata.is_dir())
                            .unwrap_or(false);
                        fs_ops::create_preserved_symlink(
                            Path::new(target),
                            &output_path,
                            target_is_dir,
                        )
                        .map_err(AppError::Anyhow)?;
                    }
                }
            }
            Ok(())
        })();

        if result.is_err() {
            let _ = fs_ops::remove_dir_all_retry(destination);
        }
        result
    }

    /// Restore a previously captured managed tree after a failed Git update.
    /// The checkout's `.git` directory is retained when the Skill itself is
    /// the repository root; every other captured path is restored byte-for-byte.
    pub(crate) fn restore_owned_at(&self, destination: &Path) -> Result<(), AppError> {
        std::fs::create_dir_all(destination)?;
        let link_directory_kinds = self
            .files
            .iter()
            .filter(|file| file.kind == SnapshotFileKind::Symlink)
            .map(|file| {
                (
                    file.relative_path.clone(),
                    std::fs::metadata(self.root.join(&file.relative_path))
                        .map(|metadata| metadata.is_dir())
                        .unwrap_or(false),
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        clear_managed_snapshot_entries(destination)?;

        for file in &self.files {
            let relative = safe_relative_path(&file.relative_path)?;
            let output_path = destination.join(&relative);
            if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            match file.kind {
                SnapshotFileKind::Regular => {
                    std::fs::write(&output_path, &file.content)?;
                    set_executable(&output_path, file.executable)?;
                }
                SnapshotFileKind::Symlink => {
                    let target = std::str::from_utf8(&file.content).map_err(|_| {
                        AppError::Other(format!(
                            "Snapshot symlink target is not UTF-8: {}",
                            file.relative_path
                        ))
                    })?;
                    let raw_target = Path::new(target);
                    let resolved_target = if raw_target.is_absolute() {
                        raw_target.to_path_buf()
                    } else {
                        output_path.parent().unwrap_or(destination).join(raw_target)
                    };
                    fs_ops::create_preserved_symlink(
                        raw_target,
                        &output_path,
                        link_directory_kinds
                            .get(&file.relative_path)
                            .copied()
                            .unwrap_or_else(|| resolved_target.is_dir()),
                    )
                    .map_err(AppError::Anyhow)?;
                }
            }
        }
        Ok(())
    }
}

fn clear_managed_snapshot_entries(directory: &Path) -> Result<(), AppError> {
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        if crate::content::snapshot_entry_is_ignored(&entry.file_name()) {
            continue;
        }
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() || fs_ops::is_link(&path) {
            fs_ops::remove_link_or_copy(&path).map_err(AppError::Anyhow)?;
        } else if metadata.is_dir() {
            clear_managed_snapshot_entries(&path)?;
            if std::fs::read_dir(&path)?.next().is_none() {
                fs_ops::remove_dir_retry(&path)?;
            }
        } else {
            fs_ops::remove_file_retry(&path)?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn set_executable(path: &Path, executable: bool) -> Result<(), AppError> {
    use std::os::unix::fs::PermissionsExt as _;
    let mut permissions = std::fs::metadata(path)?.permissions();
    let mode = permissions.mode();
    permissions.set_mode(if executable {
        mode | 0o111
    } else {
        mode & !0o111
    });
    std::fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_executable(_path: &Path, _executable: bool) -> Result<(), AppError> {
    Ok(())
}

fn safe_relative_path(value: &str) -> Result<std::path::PathBuf, AppError> {
    let path = Path::new(value);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(AppError::Other(format!(
            "Snapshot path is not a safe relative path: {value:?}"
        )));
    }
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    #[test]
    fn restore_owned_tree_replaces_managed_files_and_preserves_ignored_state() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("demo");
        std::fs::create_dir_all(root.join("scripts")).unwrap();
        std::fs::create_dir_all(root.join(".skillstar")).unwrap();
        std::fs::write(root.join("SKILL.md"), "original\n").unwrap();
        std::fs::write(root.join("scripts/run.sh"), "original script\n").unwrap();
        std::fs::write(root.join(".skillstar/state.json"), "before\n").unwrap();
        let snapshot = crate::content::snapshot_path("demo", &root).unwrap();

        std::fs::write(root.join("SKILL.md"), "changed\n").unwrap();
        std::fs::write(root.join("scripts/run.sh"), "changed script\n").unwrap();
        std::fs::write(root.join("scripts/extra.txt"), "remove me\n").unwrap();
        std::fs::write(root.join(".skillstar/state.json"), "runtime state\n").unwrap();
        std::fs::write(root.join("editor.tmp"), "preserve ignored\n").unwrap();

        snapshot.restore_owned_at(&root).unwrap();

        assert_eq!(
            std::fs::read_to_string(root.join("SKILL.md")).unwrap(),
            "original\n"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("scripts/run.sh")).unwrap(),
            "original script\n"
        );
        assert!(!root.join("scripts/extra.txt").exists());
        assert_eq!(
            std::fs::read_to_string(root.join(".skillstar/state.json")).unwrap(),
            "runtime state\n"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("editor.tmp")).unwrap(),
            "preserve ignored\n"
        );
    }
}
