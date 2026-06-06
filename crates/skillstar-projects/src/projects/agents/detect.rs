//! Filesystem detection helpers: install status + synced-skill counting.

use std::path::Path;

/// Count how many managed skill entries (symlinks, junctions, or copies) exist
/// in a directory.
pub(crate) fn count_symlinks(dir: &Path) -> u32 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|e| {
            let Ok(ft) = e.file_type() else {
                return false;
            };

            // Fast paths using dirent file_type (no stat calls on Unix, fast on Windows)
            if ft.is_symlink() {
                return true;
            }

            // Fallback for Windows junction points which might not be marked as symlinks
            #[cfg(windows)]
            if skillstar_core::infra::fs_ops::is_link(&e.path()) {
                return true;
            }

            // Fallback for copied directories
            if ft.is_dir() {
                let mut p = e.path();
                p.push("SKILL.md");
                return p.exists();
            }

            false
        })
        .count() as u32
}

/// Detect installation by creating the config/skills dir if it doesn't exist.
pub(crate) fn detect_installed(id: &str, global_skills_dir: &Path) -> bool {
    if !global_skills_dir.exists()
        && let Err(e) = std::fs::create_dir_all(global_skills_dir)
    {
        tracing::warn!(
            "Failed to provision global skills directory for agent profile '{}' at {:?}: {}",
            id,
            global_skills_dir,
            e
        );
        return false;
    }
    true
}
