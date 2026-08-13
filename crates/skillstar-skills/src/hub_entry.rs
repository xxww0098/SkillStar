//! Which entries in the hub skills directory are actually installed Skills.
//!
//! Every write path stages its work as a dot-prefixed sibling inside the hub
//! (`.skillstar-remove-*`, `.skillstar-install-*`, `.importing-*`) and then
//! renames it into place. If a process dies mid-transaction the staging entry
//! survives, and for repo-cached Skills it is a symlink pointing into a repo
//! cache — indistinguishable from an installed Skill to a naive `read_dir`.
//!
//! Traversals that did not filter those entries treated leftovers as installed
//! Skills with no lock entry, which made
//! `repo_scanner::cache::ensure_installed_checkout_is_clean` refuse every
//! further install, scan and update for that repository, naming a hidden path
//! the user cannot see or act on. `is_managed_hub_entry` is the single
//! predicate all hub traversals share so that can no longer happen.

use std::path::Path;

use skillstar_core::infra::fs_ops;

/// Staging prefixes used by the hub write paths.
///
/// Keep in sync with the paths built in `skill_install`, `repo_scanner::
/// scan_install`, `skill_pack` and `skill_bundle`. Every one of them is a
/// dot-prefixed name, so `is_managed_hub_entry` stays correct even if a new
/// prefix is added without updating this list; the list only drives sweeping.
pub const STAGING_PREFIXES: [&str; 3] = [
    ".skillstar-remove-",
    ".skillstar-install-",
    ".importing-",
];

/// True when a hub directory entry is a real installed Skill rather than
/// staging residue or a dotfile.
///
/// Skill names can never start with `.` — `content::validate_skill_name`
/// rejects them — so this is exact, not a heuristic.
pub fn is_managed_hub_entry(name: &str) -> bool {
    !name.is_empty() && !name.starts_with('.')
}

/// True when the entry name is leftover staging from an interrupted hub write.
pub fn is_staging_entry(name: &str) -> bool {
    STAGING_PREFIXES
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

/// Delete staging residue left behind by an interrupted hub write.
///
/// **Only call this while holding the update transaction lock.** That lock is
/// both an in-process mutex and a cross-process file lock, so under it no other
/// transaction can own a live staging entry and everything matching is stale by
/// definition. Returns the names it removed, for logging.
pub fn sweep_stale_staging(skills_dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(skills_dir) else {
        return Vec::new();
    };

    let mut swept = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !is_staging_entry(&name) {
            continue;
        }
        match remove_staging_entry(&entry.path()) {
            Ok(()) => {
                tracing::warn!(
                    target: "skills",
                    entry = %name,
                    "Removed staging residue from an interrupted hub write"
                );
                swept.push(name);
            }
            Err(error) => {
                // Best effort: a leftover we cannot delete is no longer fatal
                // because every traversal skips it via `is_managed_hub_entry`.
                tracing::warn!(
                    target: "skills",
                    entry = %name,
                    error = %error,
                    "Failed to remove staging residue"
                );
            }
        }
    }
    swept
}

/// Delete one staging path.
///
/// Deliberately does not use `fs_ops::remove_link_or_copy`: that helper refuses
/// to delete a directory without a `SKILL.md` so it can never eat an unmanaged
/// user directory. A staging path needs the opposite rule — the reserved name
/// is proof of ownership, and the residue most worth deleting is exactly the
/// half-written case that has no `SKILL.md` yet.
fn remove_staging_entry(path: &Path) -> std::io::Result<()> {
    if fs_ops::is_link(path) {
        return fs_ops::remove_symlink(path).map_err(std::io::Error::other);
    }
    let metadata = path.symlink_metadata()?;
    if metadata.is_dir() {
        fs_ops::remove_dir_all_retry(path).map_err(std::io::Error::other)
    } else {
        std::fs::remove_file(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_staging_and_dot_entries() {
        assert!(is_managed_hub_entry("writer"));
        assert!(is_managed_hub_entry("code-review"));
        assert!(!is_managed_hub_entry(""));
        assert!(!is_managed_hub_entry(".skillstar-remove-writer"));
        assert!(!is_managed_hub_entry(".skillstar-install-0"));
        assert!(!is_managed_hub_entry(".importing-pack-0"));
        assert!(!is_managed_hub_entry(".DS_Store"));
    }

    #[test]
    fn recognizes_every_staging_prefix() {
        assert!(is_staging_entry(".skillstar-remove-writer"));
        // Legacy pid-suffixed name from before the prefix was made stable.
        assert!(is_staging_entry(".skillstar-remove-writer-1234"));
        assert!(is_staging_entry(".skillstar-install-3"));
        assert!(is_staging_entry(".importing-pack-1"));
        assert!(is_staging_entry(".importing-writer"));
        assert!(!is_staging_entry("writer"));
        assert!(!is_staging_entry(".DS_Store"));
    }

    #[test]
    fn sweep_removes_only_staging_entries() {
        let dir = tempfile::tempdir().expect("tempdir");
        let hub = dir.path();
        let repo_cache = dir.path().join("repo-cache-skill");
        std::fs::create_dir_all(&repo_cache).expect("repo cache");
        std::fs::write(repo_cache.join("SKILL.md"), "# skill").expect("skill md");

        // A real installed Skill, which must survive.
        let installed = hub.join("writer");
        std::fs::create_dir_all(&installed).expect("installed");
        std::fs::write(installed.join("SKILL.md"), "# writer").expect("writer md");
        // An unrelated dotfile, which must also survive.
        std::fs::write(hub.join(".DS_Store"), "junk").expect("dotfile");

        // Residue shaped the three ways it actually occurs: a symlink into a
        // repo cache (uninstall staging), a half-written directory with no
        // SKILL.md (a clone that was killed), and a complete directory.
        fs_ops::create_symlink_or_copy(&repo_cache, &hub.join(".skillstar-remove-writer-1234"))
            .expect("staging symlink");
        std::fs::create_dir_all(hub.join(".skillstar-install-0").join("partial"))
            .expect("partial staging");
        let complete = hub.join(".importing-pack-0");
        std::fs::create_dir_all(&complete).expect("complete staging");
        std::fs::write(complete.join("SKILL.md"), "# staged").expect("staged md");

        let mut swept = sweep_stale_staging(hub);
        swept.sort();
        assert_eq!(
            swept,
            vec![
                ".importing-pack-0".to_string(),
                ".skillstar-install-0".to_string(),
                ".skillstar-remove-writer-1234".to_string(),
            ]
        );
        assert!(hub.join("writer").join("SKILL.md").is_file());
        assert!(hub.join(".DS_Store").exists());
        // Removing the staging symlink must not touch what it pointed at.
        assert!(repo_cache.join("SKILL.md").is_file());
    }

    #[test]
    fn sweep_on_missing_directory_is_a_no_op() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(sweep_stale_staging(&dir.path().join("absent")).is_empty());
    }
}
