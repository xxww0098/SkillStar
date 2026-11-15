//! Shared runtime artifact exclusions and cross-update directory carriage.
//!
//! Python virtual environments and Node dependency trees created inside an
//! installed skill after installation (`node_modules`, `.venv`, `__pycache__`,
//! etc.) are runtime artifacts rather than authored skill sources:
//! - Content snapshots, install baselines, deployment hashes, and backups skip
//!   them so running a skill's helper scripts never marks the skill dirty or
//!   bloats archives.
//! - Updating a skill carries `node_modules` / `.venv` / `venv` forward when the
//!   upstream release does not ship its own copy.

use std::ffi::OsStr;
use std::path::Path;

use anyhow::Context;
use ss_core::infra::fs_ops;

/// Directories treated as local execution/runtime artifacts inside a skill.
pub const RUNTIME_ARTIFACT_DIRS: &[&str] = &[
    "node_modules",
    ".venv",
    "venv",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
];

/// Relative paths of heavy environment directories carried across skill updates.
pub const CARRY_ARTIFACT_SUBPATHS: &[&str] = &[
    "node_modules",
    ".venv",
    "venv",
    "scripts/node_modules",
    "scripts/.venv",
    "scripts/venv",
];

/// Return `true` when `name` matches one of [`RUNTIME_ARTIFACT_DIRS`].
pub fn is_runtime_artifact_dir(name: &str) -> bool {
    RUNTIME_ARTIFACT_DIRS.contains(&name)
}

/// Return `true` when a skill tree entry should be excluded from snapshots,
/// baselines, copy-mode deployment comparisons, and backups.
pub fn should_ignore_skill_entry(name: &OsStr, is_dir: bool) -> bool {
    let Some(name) = name.to_str() else {
        return false;
    };
    if is_dir && is_runtime_artifact_dir(name) {
        return true;
    }
    matches!(
        name,
        ".git" | ".skillstar" | ".DS_Store" | "Thumbs.db" | "desktop.ini"
    ) || name.starts_with("._")
        || name.ends_with('~')
        || name.ends_with(".swp")
        || name.ends_with(".swo")
        || name.ends_with(".tmp")
}

/// Recursively copy `src` to `dst`, skipping `.git`, `.skillstar-deploy.json`,
/// OS junk, and [`RUNTIME_ARTIFACT_DIRS`].
pub fn copy_dir_excluding_runtime(src: &Path, dst: &Path, skip_names: &[&str]) -> anyhow::Result<()> {
    copy_dir_recursive(src, dst, skip_names, false)
}

/// Recursively copy `src` to `dst`, preserving everything including runtime artifacts.
pub fn copy_dir_all(src: &Path, dst: &Path) -> anyhow::Result<()> {
    copy_dir_recursive(src, dst, &[], true)
}

fn copy_dir_recursive(
    src: &Path,
    dst: &Path,
    skip_names: &[&str],
    include_runtime_artifacts: bool,
) -> anyhow::Result<()> {
    std::fs::create_dir_all(dst)
        .with_context(|| format!("Failed to create destination directory {}", dst.display()))?;
    for entry in std::fs::read_dir(src)
        .with_context(|| format!("Failed to read directory {}", src.display()))?
    {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if skip_names.iter().any(|skip| *skip == name_str.as_ref()) {
            continue;
        }
        let path = entry.path();
        let meta = std::fs::symlink_metadata(&path)?;
        let is_dir = meta.is_dir();
        if !include_runtime_artifacts && should_ignore_skill_entry(&name, is_dir) {
            continue;
        }
        let target = dst.join(&name);
        if fs_ops::is_link(&path) {
            let link_target = std::fs::read_link(&path)?;
            let target_is_dir = std::fs::metadata(&path).map(|m| m.is_dir()).unwrap_or(false);
            fs_ops::create_preserved_symlink(&link_target, &target, target_is_dir)?;
        } else if is_dir {
            copy_dir_recursive(&path, &target, skip_names, include_runtime_artifacts)?;
        } else {
            std::fs::copy(&path, &target).with_context(|| {
                format!("Failed to copy {} -> {}", path.display(), target.display())
            })?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                let _ = std::fs::set_permissions(
                    &target,
                    std::fs::Permissions::from_mode(meta.permissions().mode()),
                );
            }
        }
    }
    Ok(())
}

fn move_or_copy_dir(src: &Path, dst: &Path) -> anyhow::Result<()> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match std::fs::rename(src, dst) {
        Ok(()) => Ok(()),
        Err(_) => {
            copy_dir_all(src, dst)?;
            fs_ops::remove_dir_all_retry(src)
                .with_context(|| format!("Failed to clean source runtime dir {}", src.display()))
        }
    }
}

/// Carry runtime environment directories (`node_modules`, `.venv`, `venv`, and
/// their `scripts/` equivalents) from `old_dir` into `target_dir` when absent
/// in `target_dir`.
///
/// Uses atomic same-volume `rename` when possible and falls back to recursive
/// copy across filesystems. Returns the number of carried directories.
pub fn carry_runtime_artifacts(old_dir: &Path, target_dir: &Path) -> anyhow::Result<usize> {
    if !old_dir.is_dir() || !target_dir.is_dir() {
        return Ok(0);
    }
    let mut carried = 0usize;
    for rel in CARRY_ARTIFACT_SUBPATHS {
        let src = old_dir.join(rel);
        let dst = target_dir.join(rel);
        let Ok(meta) = src.symlink_metadata() else {
            continue;
        };
        if !meta.is_dir() || fs_ops::is_link(&src) {
            continue;
        }
        if dst.symlink_metadata().is_ok() {
            continue;
        }
        move_or_copy_dir(&src, &dst)
            .with_context(|| format!("Failed to carry runtime directory '{rel}' across update"))?;
        carried += 1;
    }
    Ok(carried)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use tempfile::TempDir;

    #[test]
    fn runtime_dirs_are_ignored_only_as_directories() {
        for dir in RUNTIME_ARTIFACT_DIRS {
            assert!(
                should_ignore_skill_entry(OsStr::new(dir), true),
                "{dir} directory should be ignored"
            );
            assert!(
                !should_ignore_skill_entry(OsStr::new(dir), false),
                "regular file named {dir} should not be misclassified"
            );
        }
        assert!(should_ignore_skill_entry(OsStr::new(".git"), true));
        assert!(should_ignore_skill_entry(OsStr::new(".skillstar"), true));
        assert!(should_ignore_skill_entry(OsStr::new(".DS_Store"), false));
        assert!(!should_ignore_skill_entry(OsStr::new("scripts"), true));
        assert!(!should_ignore_skill_entry(OsStr::new("SKILL.md"), false));
    }

    #[test]
    fn carry_runtime_artifacts_moves_root_and_scripts_envs_without_overwriting_new_ones() {
        let temp = TempDir::new().unwrap();
        let old = temp.path().join("old");
        let new = temp.path().join("new");
        std::fs::create_dir_all(old.join("node_modules/pkg")).unwrap();
        std::fs::write(old.join("node_modules/pkg/index.js"), "module.exports = 1;").unwrap();
        std::fs::create_dir_all(old.join("scripts/.venv/bin")).unwrap();
        std::fs::write(old.join("scripts/.venv/pyvenv.cfg"), "home = /usr").unwrap();
        std::fs::create_dir_all(old.join(".venv")).unwrap();
        std::fs::write(old.join(".venv/marker"), "old").unwrap();

        std::fs::create_dir_all(new.join(".venv")).unwrap();
        std::fs::write(new.join(".venv/marker"), "fresh-from-upstream").unwrap();

        let carried = carry_runtime_artifacts(&old, &new).unwrap();
        assert_eq!(carried, 2);
        assert!(new.join("node_modules/pkg/index.js").is_file());
        assert!(new.join("scripts/.venv/pyvenv.cfg").is_file());
        assert_eq!(
            std::fs::read_to_string(new.join(".venv/marker")).unwrap(),
            "fresh-from-upstream"
        );
    }
}
