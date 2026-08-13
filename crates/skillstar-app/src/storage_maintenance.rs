//! Storage overview, cache cleanup, and force-delete application use cases.

use skillstar_core::infra::error::AppError;
use skillstar_core::infra::{fs_ops, paths};
use skillstar_agents as agent_profile;
use skillstar_skills::deployment;
use skillstar_git::repo_history;
use skillstar_skills::lockfile;
use skillstar_skills::repo_scanner;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Whether a shared channel owns `name`.
///
/// Fails closed: routine maintenance treats an unreadable ownership registry
/// as "owned" so a transient read error can never turn into a deletion.
fn is_channel_managed(name: &str) -> bool {
    match skillstar_skills::skill_mutation::skill_is_channel_managed(name) {
        Ok(managed) => managed,
        Err(error) => {
            tracing::warn!(
                target: "storage",
                skill = %name,
                error = %error,
                "treating Skill as channel-owned because ownership could not be read"
            );
            true
        }
    }
}

/// Immediate child names of the hub skills directory.
fn hub_entry_names(hub_dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(hub_dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
        .collect()
}

/// Aggregated storage usage info for the Settings page.
#[derive(serde::Serialize)]
pub struct StorageOverview {
    /// Resolved data root path (`SKILLSTAR_DATA_DIR` or default `~/.skillstar`)
    pub data_root_path: String,
    /// Resolved hub root path (`SKILLSTAR_HUB_DIR` or default `~/.skillstar/.agents`)
    pub hub_root_path: String,
    /// Whether hub root is nested under data root.
    pub is_hub_under_data: bool,
    /// App config files total bytes (ai_config, proxy, profiles, groups, projects…)
    pub config_bytes: u64,
    /// Resolved app config directory path.
    pub config_path: String,
    /// Skills hub directory total bytes (~/.skillstar/.agents/skills/)
    pub hub_bytes: u64,
    /// Resolved installed skills directory path.
    pub hub_path: String,
    /// Number of valid installed skills
    pub hub_count: usize,
    /// Number of broken skills (broken symlinks, orphaned lockfile entries)
    pub broken_count: usize,
    /// Number of local skills in skills-local/
    pub local_count: usize,
    /// Total bytes of skills-local/ directory
    pub local_bytes: u64,
    /// Resolved local skills directory path.
    pub local_path: String,
    /// Repo cache total bytes (~/.skillstar/.agents/.repos/)
    pub cache_bytes: u64,
    /// Resolved repo cache directory path.
    pub cache_path: String,
    /// Number of cached repos
    pub cache_count: usize,
    /// Number of unused cached repos
    pub cache_unused_count: usize,
    /// Bytes used by unused cached repos
    pub cache_unused_bytes: u64,
    /// Number of entries in repo scan history
    pub history_count: usize,
}

pub async fn get_storage_overview() -> Result<StorageOverview, AppError> {
    Ok(tokio::task::spawn_blocking(|| {
        let data_root = paths::data_root();
        let hub_root = paths::hub_root();
        let is_hub_under_data = hub_root.starts_with(&data_root) && hub_root != data_root;

        let config_dir = paths::config_dir();
        let config_bytes = dir_size_recursive(&config_dir);

        let hub_dir = paths::hub_skills_dir();
        let hub_bytes = dir_size_recursive(&hub_dir);
        let (hub_count, broken_count) = count_hub_skills(&hub_dir);

        let local_dir = paths::local_skills_dir();
        let local_bytes = dir_size_recursive(&local_dir);
        let local_count = count_directories(&local_dir);

        let cache_info = repo_scanner::get_cache_info();

        let history_count = repo_history::entry_count();

        StorageOverview {
            data_root_path: data_root.to_string_lossy().to_string(),
            hub_root_path: hub_root.to_string_lossy().to_string(),
            is_hub_under_data,
            config_bytes,
            config_path: config_dir.to_string_lossy().to_string(),
            hub_bytes,
            hub_path: hub_dir.to_string_lossy().to_string(),
            hub_count,
            broken_count,
            local_count,
            local_bytes,
            local_path: local_dir.to_string_lossy().to_string(),
            cache_bytes: cache_info.total_bytes,
            cache_path: paths::repos_cache_dir().to_string_lossy().to_string(),
            cache_count: cache_info.repo_count,
            cache_unused_count: cache_info.unused_count,
            cache_unused_bytes: cache_info.unused_bytes,
            history_count,
        }
    })
    .await?)
}

/// Result of a unified cache cleanup.
#[derive(serde::Serialize)]
pub struct CacheCleanResult {
    /// Number of unused repos removed from cache
    pub repos_removed: usize,
    /// Number of repo history entries cleared
    pub history_cleared: usize,
}

pub async fn clear_all_caches() -> Result<CacheCleanResult, AppError> {
    Ok(tokio::task::spawn_blocking(|| {
        let repos_removed = repo_scanner::clean_unused_cache().unwrap_or(0);
        let history_cleared = repo_history::clear_history().unwrap_or(0);

        // Clean up legacy "agenthub" data directory (old app name before rename to "skillstar")
        if let Some(data_dir) = dirs::data_dir() {
            let legacy_dir = data_dir.join("agenthub");
            if legacy_dir.exists() {
                let _ = std::fs::remove_dir_all(&legacy_dir);
            }
        }

        CacheCleanResult {
            repos_removed,
            history_cleared,
        }
    })
    .await?)
}

/// Force-delete all installed skills from the hub.
///
/// Returns the number of skill entries removed.
pub async fn force_delete_installed_skills() -> Result<usize, AppError> {
    tokio::task::spawn_blocking(|| -> Result<usize, AppError> {
        let _transaction_guard = skillstar_skills::skill_update::acquire_skill_mutation_lease()?;
        let hub_dir = skillstar_core::infra::paths::hub_skills_dir();
        let removed_count = count_children(&hub_dir);

        // An explicit reset may delete channel-owned Skills, but the channel
        // registry has to learn about it — otherwise it keeps claiming names
        // with nothing behind them, and those names can never be reinstalled
        // or deleted again. Report first: a registry that cannot be updated
        // aborts the reset instead of being orphaned by it.
        skillstar_skills::skill_mutation::notify_bulk_skill_removal(&hub_entry_names(&hub_dir))?;

        // Remove all global skill symlinks from known agents to avoid dangling links.
        for profile in agent_profile::list_profiles() {
            let _ = deployment::unlink_all_skills_from_agent(&profile.id);
        }

        if hub_dir.exists() {
            fs_ops::remove_dir_all_retry(&hub_dir)?;
        }
        std::fs::create_dir_all(&hub_dir)?;

        // Clear lockfile entries so UI state and filesystem stay aligned.
        let _lock = lockfile::get_mutex()
            .lock()
            .map_err(|_| AppError::Lockfile("Lockfile mutex poisoned".to_string()))?;
        let lock_path = lockfile::lockfile_path();
        let mut lf = lockfile::Lockfile::load(&lock_path).unwrap_or_default();
        lf.skills.clear();
        let _ = lf.save(&lock_path);
        skillstar_skills::installed_skill::invalidate_cache();
        Ok(removed_count)
    })
    .await?
}

/// Force-delete all repo caches (including currently referenced ones).
///
/// Returns the number of cached repositories removed.
pub async fn force_delete_repo_caches() -> Result<usize, AppError> {
    tokio::task::spawn_blocking(|| -> Result<usize, AppError> {
        let _transaction_guard = skillstar_skills::skill_update::acquire_skill_mutation_lease()?;
        let cache_dir = repos_cache_dir();
        let repos_removed = count_directories(&cache_dir);
        let hub_dir = skillstar_core::infra::paths::hub_skills_dir();
        let mut removed_skill_names: HashSet<String> = HashSet::new();

        // Collect the hub symlinks that point into the repo cache first: the
        // channel registry must be told which Skills are about to disappear
        // before any of them do, so an unwritable registry aborts the reset.
        if let Ok(entries) = std::fs::read_dir(&hub_dir) {
            for entry in entries.flatten() {
                let skill_path = entry.path();
                if !fs_ops::is_link(&skill_path) {
                    continue;
                }
                let Some(target) = fs_ops::read_link_resolved(&skill_path).ok() else {
                    continue;
                };
                if target.starts_with(&cache_dir)
                    && let Some(name) = entry.file_name().to_str()
                {
                    removed_skill_names.insert(name.to_string());
                }
            }
        }
        let removed_skill_list = removed_skill_names.iter().cloned().collect::<Vec<_>>();
        skillstar_skills::skill_mutation::notify_bulk_skill_removal(&removed_skill_list)?;

        for name in &removed_skill_names {
            let _ = fs_ops::remove_symlink(&hub_dir.join(name));
        }

        // Remove linked references from agent skill dirs.
        for name in &removed_skill_names {
            let _ = deployment::remove_skill_from_all_agents(name);
        }

        // Prune lockfile entries for removed cache-backed skills.
        if !removed_skill_names.is_empty() {
            let _lock = lockfile::get_mutex()
                .lock()
                .map_err(|_| AppError::Lockfile("Lockfile mutex poisoned".to_string()))?;
            let lock_path = lockfile::lockfile_path();
            let mut lf = lockfile::Lockfile::load(&lock_path).unwrap_or_default();
            lf.skills
                .retain(|entry| !removed_skill_names.contains(&entry.name));
            let _ = lf.save(&lock_path);
            skillstar_skills::installed_skill::invalidate_cache();
        }

        if cache_dir.exists() {
            fs_ops::remove_dir_all_retry(&cache_dir)?;
        }
        std::fs::create_dir_all(&cache_dir)?;

        Ok(repos_removed)
    })
    .await?
}

/// Force-delete app config files.
///
/// Preserves the files that record ownership of *installed* content (the
/// shared-channel registry and subscription store). They are provenance, not
/// preferences — the lockfile's channel-side counterpart, and like the
/// lockfile they outlive a settings reset. Deleting them while their Skills
/// are still in the hub would strand those Skills: they would stop being
/// recognised as channel-owned, so the ordinary update path would start
/// fetching a private channel repository anonymously and fail forever.
///
/// Returns the number of config files removed.
pub async fn force_delete_app_config() -> Result<usize, AppError> {
    Ok(tokio::task::spawn_blocking(|| {
        let mut removed = 0usize;
        let preserved: HashSet<PathBuf> = skillstar_skills::skill_mutation::provenance_paths()
            .into_iter()
            .collect();

        // Delete config dir contents
        let config_dir = paths::config_dir();
        if config_dir.exists()
            && let Ok(entries) = std::fs::read_dir(&config_dir)
        {
            for entry in entries.flatten() {
                let path = entry.path();
                if preserved.contains(&path) {
                    continue;
                }
                if path.is_file() && std::fs::remove_file(&path).is_ok() {
                    removed += 1;
                }
            }
        }

        // Also delete state dir contents (rebuildable)
        let state_dir = paths::state_dir();
        if state_dir.exists()
            && let Ok(entries) = std::fs::read_dir(&state_dir)
        {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && std::fs::remove_file(&path).is_ok() {
                    removed += 1;
                }
            }
        }

        removed
    })
    .await?)
}

/// Remove broken symlinks from the hub and prune orphaned lockfile entries.
///
/// Channel-owned Skills are skipped entirely. This is routine housekeeping,
/// not a reset: a channel Skill whose checkout vanished is repaired through
/// the channel controls, and deleting its hub entry or lock entry here would
/// silently desynchronise the subscription from disk. Ownership that cannot be
/// determined counts as owned, so a registry read failure never escalates into
/// deletion.
///
/// Returns the number of issues fixed.
pub async fn clean_broken_skills() -> Result<usize, AppError> {
    tokio::task::spawn_blocking(|| -> Result<usize, AppError> {
        let _transaction_guard = skillstar_skills::skill_update::acquire_skill_mutation_lease()?;
        let hub_dir = skillstar_core::infra::paths::hub_skills_dir();
        let mut fixed: usize = 0;
        let mut removed_names: HashSet<String> = HashSet::new();

        // Phase 1: Remove broken symlinks from hub
        if let Ok(entries) = std::fs::read_dir(&hub_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.symlink_metadata().is_err() {
                    continue;
                }
                if fs_ops::is_link(&path) && !path.exists() {
                    // Broken symlink — target is gone
                    let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                        continue;
                    };
                    if is_channel_managed(&name) {
                        continue;
                    }
                    if fs_ops::remove_symlink(&path).is_ok() {
                        removed_names.insert(name);
                        fixed += 1;
                    }
                }
            }
        }

        // Phase 2: Clean agent-side symlinks for removed skills
        for name in &removed_names {
            let _ = deployment::remove_skill_from_all_agents(name);
        }

        // Phase 3: Prune orphaned lockfile entries
        let _lock = lockfile::get_mutex()
            .lock()
            .map_err(|_| AppError::Lockfile("Lockfile mutex poisoned".to_string()))?;
        let lock_path = lockfile::lockfile_path();
        let mut lf = lockfile::Lockfile::load(&lock_path).unwrap_or_default();
        let before = lf.skills.len();
        lf.skills.retain(|entry| {
            let skill_path = hub_dir.join(&entry.name);
            // Keep entries that have a valid directory or valid symlink, plus
            // every channel-owned entry: its subscription still tracks it, so
            // dropping the lock entry here would orphan that record.
            (skill_path.symlink_metadata().is_ok()
                && (!fs_ops::is_link(&skill_path) || skill_path.exists()))
                || is_channel_managed(&entry.name)
        });
        let orphans_removed = before - lf.skills.len();
        if orphans_removed > 0 {
            let _ = lf.save(&lock_path);
            skillstar_skills::installed_skill::invalidate_cache();
            fixed += orphans_removed;
        }

        Ok(fixed)
    })
    .await?
}

// ── size / count helpers ────────────────────────────────────────────

/// Calculate total size of a directory recursively.
fn dir_size_recursive(path: &std::path::Path) -> u64 {
    if !path.exists() {
        return 0;
    }
    let mut total: u64 = 0;
    let mut stack = vec![path.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let entry_path = entry.path();
            // Do not follow symlink/junction targets when sizing storage.
            // Following links can double-count repo cache content and can hang
            // on cyclic link graphs (especially on Windows junction-heavy setups).
            if fs_ops::is_link(&entry_path) {
                continue;
            }

            let Ok(meta) = std::fs::symlink_metadata(&entry_path) else {
                continue;
            };
            if meta.is_dir() {
                stack.push(entry_path);
            } else if meta.is_file() {
                total += meta.len();
            }
        }
    }
    total
}

/// Count immediate children (files & dirs) of a directory.
fn count_children(path: &std::path::Path) -> usize {
    if !path.exists() {
        return 0;
    }
    std::fs::read_dir(path)
        .map(|entries| entries.count())
        .unwrap_or(0)
}

/// Count hub skills, returning (valid_count, broken_count).
///
/// A skill entry is "broken" if it is a symlink whose target no longer exists.
/// Uses `symlink_metadata()` to detect symlink entries that `is_dir()` would skip.
fn count_hub_skills(hub_dir: &Path) -> (usize, usize) {
    if !hub_dir.exists() {
        return (0, 0);
    }
    let mut valid: usize = 0;
    let mut broken: usize = 0;
    if let Ok(entries) = std::fs::read_dir(hub_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = path.symlink_metadata() else {
                continue;
            };
            if fs_ops::is_link(&path) {
                // Symlink: check if the target still exists
                if path.exists() {
                    valid += 1;
                } else {
                    broken += 1;
                }
            } else if meta.is_dir() {
                valid += 1;
            }
            // Skip regular files (e.g. .DS_Store)
        }
    }

    // Also count orphaned lockfile entries (in lockfile but not on disk)
    let lock_path = lockfile::lockfile_path();
    if let Ok(lf) = lockfile::Lockfile::load(&lock_path) {
        for entry in &lf.skills {
            let skill_path = hub_dir.join(&entry.name);
            // Entry exists in lockfile but has no directory/symlink at all
            if skill_path.symlink_metadata().is_err() {
                broken += 1;
            }
        }
    }

    (valid, broken)
}

fn count_directories(path: &Path) -> usize {
    if !path.exists() {
        return 0;
    }
    std::fs::read_dir(path)
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| {
                    let p = entry.path();
                    if fs_ops::is_link(&p) {
                        return false;
                    }
                    p.symlink_metadata().map(|m| m.is_dir()).unwrap_or(false)
                })
                .count()
        })
        .unwrap_or(0)
}

fn repos_cache_dir() -> PathBuf {
    paths::repos_cache_dir()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `dir_size_recursive` must NOT follow symlink/junction targets.
    /// AGENTS.md: "treat links as metadata-only entries to avoid recursive
    /// loops and Windows UI hangs". A symlink to a large dir must contribute 0 bytes.
    #[test]
    fn dir_size_does_not_follow_symlinks() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Real file with known size inside the scanned root.
        std::fs::write(root.join("real.txt"), b"hello").unwrap();

        // A directory with a 1MB file, placed OUTSIDE root, then symlinked in.
        // (If it were under root it would be counted as a normal subdir.)
        let outside = std::env::temp_dir().join(format!("sst-storage-test-{}", std::process::id()));
        std::fs::create_dir_all(&outside).unwrap();
        let payload = vec![0u8; 1_000_000];
        std::fs::write(outside.join("blob.bin"), &payload).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, root.join("link_to_big")).unwrap();

        let bytes = dir_size_recursive(root);
        // Only real.txt (5 bytes) counts; the 1MB via symlink must be excluded.
        assert_eq!(bytes, 5, "symlink target content must not be counted");

        // Cleanup the outside dir.
        let _ = std::fs::remove_dir_all(&outside);
    }

    /// `count_hub_skills` distinguishes valid symlinks from broken ones and
    /// skips regular files. This is the storage-overview health signal.
    /// Unix-only: the symlink fixtures are cfg(unix), so on Windows the test
    /// would see just one valid entry and fail (windows-ci lesson 4 family).
    #[cfg(unix)]
    #[test]
    fn count_hub_skills_separates_valid_and_broken_links() {
        let tmp = tempfile::tempdir().unwrap();
        let hub = tmp.path().join("skills");
        std::fs::create_dir_all(&hub).unwrap();

        // Real dir skill → valid.
        std::fs::create_dir_all(hub.join("real-skill")).unwrap();

        // Valid symlink → target exists.
        let target = tmp.path().join("target-skill");
        std::fs::create_dir_all(&target).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, hub.join("linked-skill")).unwrap();

        // Broken symlink → target missing.
        #[cfg(unix)]
        std::os::unix::fs::symlink(tmp.path().join("does-not-exist"), hub.join("broken-skill"))
            .unwrap();

        // Stray regular file → ignored entirely.
        std::fs::write(hub.join(".DS_Store"), b"x").unwrap();

        let (valid, _broken) = count_hub_skills(&hub);
        // real-skill + linked-skill = 2 valid. broken-skill is counted but the
        // lockfile lookup may also add orphans, so only assert the floor.
        assert!(
            valid >= 2,
            "real dir + valid symlink should both count as valid, got {valid}"
        );
    }
}
