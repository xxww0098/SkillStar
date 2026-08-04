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
                    SnapshotFileKind::Regular => std::fs::write(output_path, &file.content)?,
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
            let _ = std::fs::remove_dir_all(destination);
        }
        result
    }
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
