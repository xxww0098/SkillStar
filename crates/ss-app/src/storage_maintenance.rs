//! Storage overview, cache cleanup, and force-delete application use cases.

#[path = "agent_intake.rs"]
mod agent_intake;
#[path = "storage_deletion_targets.rs"]
mod deletion_targets;

pub use agent_intake::{
    AgentIntakePreview, IntakeListItem, apply_agent_intake, preview_agent_intake,
};

use ss_core::infra::error::AppError;
use ss_core::infra::{fs_ops, paths};
use ss_git::repo_history;
use ss_skills::skill_lock;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Whether a shared channel owns `name`.
///
/// Fails closed: routine maintenance treats an unreadable ownership registry
/// as "owned" so a transient read error can never turn into a deletion.
fn is_channel_managed(name: &str) -> bool {
    match ss_skills::skill_mutation::skill_is_channel_managed(name) {
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
    /// Skills hub directory total bytes (~/.skillstar/data/skills/installed/)
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
    /// Import-cache directories no installed Skill still references.
    /// Zero when the install lock cannot be read, so a corrupt lock is not
    /// reported as "everything is unused".
    pub cache_unused_count: usize,
    /// Bytes in those unreferenced import-cache directories.
    pub cache_unused_bytes: u64,
    /// Findings from the read-only skill storage scan.
    pub health_issue_count: usize,
    /// Repair steps that have an ownership proof. The rest are reported only.
    pub health_repairable: usize,
    /// Agent skills that can be taken under management, plus conflicts and
    /// excluded directories that are only reported.
    pub intake: Vec<IntakeListItem>,
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

        let history_count = repo_history::entry_count();
        let cache_dir = paths::skill_import_cache_dir();
        let cache_bytes = dir_size_recursive(&cache_dir);
        let cache_count = count_directories(&cache_dir);
        let (cache_unused_count, cache_unused_bytes) = unused_import_cache(&cache_dir);
        let health = ss_skills::health::scan();
        let health_repairable = ss_skills::health::plan(&health).steps.len();

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
            cache_bytes,
            cache_path: cache_dir.to_string_lossy().to_string(),
            cache_count,
            cache_unused_count,
            cache_unused_bytes,
            health_issue_count: health.issues.len(),
            health_repairable,
            intake: agent_intake::intake_rows(),
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
    tokio::task::spawn_blocking(|| -> Result<CacheCleanResult, AppError> {
        let imports_removed = ss_skills::fetch::clear_import_cache()?;
        // Also sweep the legacy cache if one-time migration has not done so.
        let legacy_repos = paths::legacy_hub_root().join("repos");
        let repos_removed = if legacy_repos.exists() {
            let removed = count_directories(&legacy_repos);
            let _ = fs_ops::remove_dir_all_retry(&legacy_repos);
            removed
        } else {
            0
        };
        let history_cleared = repo_history::clear_history().unwrap_or(0);

        Ok(CacheCleanResult {
            repos_removed: repos_removed + imports_removed,
            history_cleared,
        })
    })
    .await?
}

/// What "delete installed skills" actually did.
#[derive(Debug, Default, serde::Serialize)]
pub struct ForceDeleteSkillsReport {
    /// Skills SkillStar uninstalled (lock entries it can re-fetch, plus the
    /// hub links of local Skills — their authored content stays).
    pub removed: Vec<String>,
    /// `name: error` for uninstalls that failed; the rest still ran.
    pub failed: Vec<String>,
    /// Canonical folders left in place because SkillStar has no provenance it
    /// understands for them (placed by hand or by another tool).
    pub kept: Vec<String>,
}

/// Names `force_delete_installed_skills` would remove, in display order.
///
/// Same selection as the delete: install-lock entries whose source SkillStar
/// recognizes, and hub
/// links that point at local skills. Does not take the transaction lock and
/// does not delete anything. An unreadable lock is the same error the delete
/// would abort on.
pub async fn preview_force_delete_installed_skills() -> Result<Vec<String>, AppError> {
    tokio::task::spawn_blocking(|| {
        let targets = deletion_targets::installed_skill_deletion_targets()?;
        let mut names = targets.all_names();
        names.sort();
        names.dedup();
        Ok(names)
    })
    .await?
}

/// Uninstall every Skill SkillStar installed, one by one.
///
/// Scope is the install lock (entries with a source SkillStar understands)
/// plus the hub links of local Skills. Locked names go through ordinary
/// uninstall. A local hub link drops SkillStar's deployments, the hub symlink
/// and the lock entry, and leaves the original files in the local-skills
/// directory. A deployment cleanup failure leaves that copy and its lock
/// entry in place and is reported in `failed`. An unreadable lock aborts
/// before anything is deleted.
pub async fn force_delete_installed_skills() -> Result<ForceDeleteSkillsReport, AppError> {
    tokio::task::spawn_blocking(|| -> Result<ForceDeleteSkillsReport, AppError> {
        let _transaction_guard = ss_skills::skill_update::acquire_update_transaction_lock()?;
        let targets = deletion_targets::installed_skill_deletion_targets()?;
        let locked = targets.locked;
        let local_links = targets.local_links;
        let names = locked
            .iter()
            .chain(&local_links)
            .cloned()
            .collect::<Vec<_>>();

        // An explicit reset may delete channel-owned Skills, but the channel
        // registry has to learn about it — otherwise it keeps claiming names
        // with nothing behind them, and those names can never be reinstalled
        // or deleted again. Report first: a registry that cannot be updated
        // aborts the reset instead of being orphaned by it.
        ss_skills::skill_mutation::notify_bulk_skill_removal(&names)?;

        let mut report = ForceDeleteSkillsReport::default();
        for name in &locked {
            match ss_skills::skill_install::uninstall_skill_locked_unchecked(name) {
                Ok(()) => report.removed.push(name.clone()),
                Err(error) => report.failed.push(format!("{name}: {error}")),
            }
        }
        for name in &local_links {
            match ss_skills::skill_install::release_local_hub_link(name) {
                Ok(()) => report.removed.push(name.clone()),
                Err(error) => report.failed.push(format!("{name}: {error:#}")),
            }
        }
        report.kept = ss_skills::installer::installed_names()
            .into_iter()
            .filter(|name| !report.removed.contains(name))
            .collect();
        report.kept.sort();
        ss_skills::installed_skill::invalidate_cache();
        Ok(report)
    })
    .await?
}

/// Force-delete all repo caches (including currently referenced ones).
///
/// Returns the number of cached repositories removed.
pub async fn force_delete_repo_caches() -> Result<usize, AppError> {
    tokio::task::spawn_blocking(|| -> Result<usize, AppError> {
        let _transaction_guard = ss_skills::skill_update::acquire_update_transaction_lock()?;
        let cache_dir = repos_cache_dir();
        let repos_removed = count_directories(&cache_dir);
        let hub_dir = ss_core::infra::paths::hub_skills_dir();
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
        ss_skills::skill_mutation::notify_bulk_skill_removal(&removed_skill_list)?;

        for name in &removed_skill_names {
            ss_skills::skill_install::clear_owned_deployments(name)
                .map_err(|error| AppError::Other(format!("{name}: {error:#}")))?;
        }
        for name in &removed_skill_names {
            fs_ops::remove_symlink(&hub_dir.join(name))
                .map_err(|error| AppError::Other(format!("{name}: {error:#}")))?;
        }

        // Prune lockfile entries for removed cache-backed skills.
        if !removed_skill_names.is_empty() {
            skill_lock::mutate(|lock| {
                lock.skills
                    .retain(|name, _| !removed_skill_names.contains(name));
            })
            .map_err(|error| AppError::Lockfile(format!("{error:#}")))?;
            ss_skills::installed_skill::invalidate_cache();
        }

        if cache_dir.exists() {
            fs_ops::remove_dir_all_retry(&cache_dir)?;
        }
        std::fs::create_dir_all(&cache_dir)?;

        Ok(repos_removed + ss_skills::fetch::clear_import_cache()?)
    })
    .await?
}

/// Files a config reset may delete: preferences that fall back to defaults
/// and state that SkillStar rebuilds on its own.
///
/// An allowlist, not a sweep of `config/` and `state/`: those directories
/// also hold records nothing can rebuild — Agent profiles, Skill groups,
/// registered projects, team intelligence, SSH host keys, OAuth clients and
/// shared-channel provenance — and new files must stay safe by default.
fn resettable_config_files() -> Vec<PathBuf> {
    vec![
        paths::ai_config_path(),
        paths::proxy_config_path(),
        paths::github_mirror_config_path(),
        paths::skill_updates_config_path(),
        paths::skill_auto_update_state_path(),
        paths::github_mirror_health_path(),
        paths::patrol_state_path(),
        paths::repo_history_path(),
        paths::state_dir().join("skill_update_states.json"),
    ]
}

/// Force-delete resettable app config files (see [`resettable_config_files`]).
///
/// Returns the number of files removed.
pub async fn force_delete_app_config() -> Result<usize, AppError> {
    Ok(tokio::task::spawn_blocking(|| {
        resettable_config_files()
            .into_iter()
            .filter(|path| path.is_file() && std::fs::remove_file(path).is_ok())
            .count()
    })
    .await?)
}

/// Explicit intake of skills an Agent already installed. Not the storage
/// doctor: Settings repair and `skillstar doctor --fix` use
/// [`preview_skill_repair`] and [`apply_skill_repair`], which never adopt an
/// Agent folder. This entry runs the same plan as `doctor --adopt --apply`.
///
/// A conflict is reported without overwriting either version.
pub async fn repair_skills() -> Result<ss_skills::local_skill::SkillRepairReport, AppError> {
    // Intake only. Broken-link cleanup is `clean_broken_skills`; running it
    // here pruned lock entries that were not part of the intake.
    Ok(tokio::task::spawn_blocking(ss_skills::local_skill::repair_installations).await??)
}

/// What a storage-page repair preview shows. Steps are the ones repair can
/// prove it owns; untouched findings stay on disk.
#[derive(Debug)]
pub struct SkillRepairPreview {
    pub issues: usize,
    pub steps: Vec<String>,
    pub untouched: Vec<String>,
}

/// Read-only repair preview for the storage page.
pub async fn preview_skill_repair() -> Result<SkillRepairPreview, AppError> {
    tokio::task::spawn_blocking(|| -> Result<SkillRepairPreview, AppError> {
        let report = ss_skills::health::scan();
        let planned = ss_skills::health::plan(&report);
        Ok(SkillRepairPreview {
            issues: report.issues.len(),
            steps: planned.steps.iter().map(step_line).collect(),
            untouched: planned
                .untouched
                .iter()
                .map(|item| {
                    let skill = item.issue.skill.as_deref().unwrap_or("-");
                    format!("{skill}: {}", item.reason)
                })
                .collect(),
        })
    })
    .await?
}

/// Outcome of applying the storage repair plan.
#[derive(Debug)]
pub struct SkillRepairApplyReport {
    pub applied: usize,
    pub failed: Vec<String>,
}

/// Apply the storage repair plan. Does not adopt external Agent folders.
pub async fn apply_skill_repair() -> Result<SkillRepairApplyReport, AppError> {
    tokio::task::spawn_blocking(|| -> Result<SkillRepairApplyReport, AppError> {
        let report = ss_skills::health::scan();
        let planned = ss_skills::health::plan(&report);
        let outcome = ss_skills::health::apply(&planned)?;
        Ok(SkillRepairApplyReport {
            applied: outcome.applied_count(),
            failed: outcome
                .failed()
                .map(|step| {
                    let skill = step.step.skill.as_deref().unwrap_or("-");
                    let error = match &step.status {
                        ss_skills::health::StepStatus::Failed { error } => error.as_str(),
                        _ => "",
                    };
                    format!("{skill}: {error}")
                })
                .collect(),
        })
    })
    .await?
}

fn step_line(step: &ss_skills::health::RepairStep) -> String {
    let action = serde_json::to_value(&step.action)
        .ok()
        .and_then(|value| {
            value
                .get("action")
                .and_then(|action| action.as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "repair".to_string());
    let skill = step.skill.as_deref().unwrap_or("-");
    format!("{action} {skill} — {}", step.evidence)
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
    clean_broken_skills_except(HashSet::new()).await
}

async fn clean_broken_skills_except(excluded: HashSet<String>) -> Result<usize, AppError> {
    tokio::task::spawn_blocking(move || -> Result<usize, AppError> {
        let _transaction_guard = ss_skills::skill_update::acquire_update_transaction_lock()?;
        let hub_dir = ss_core::infra::paths::hub_skills_dir();
        let mut fixed: usize = 0;

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
                    if excluded.contains(&name) || is_channel_managed(&name) {
                        continue;
                    }
                    ss_skills::skill_install::clear_owned_deployments(&name)
                        .map_err(|error| AppError::Other(format!("{name}: {error:#}")))?;
                    fs_ops::remove_symlink(&path)
                        .map_err(|error| AppError::Other(format!("{name}: {error:#}")))?;
                    fixed += 1;
                }
            }
        }

        // Prune orphaned install-lock entries. Keys are not directory
        // names — vercel records the raw frontmatter name (`My Skill` lives in
        // `my-skill`). A key that does not map, or a folder we cannot stat, stays.
        let orphans_removed =
            skill_lock::mutate(|lock| prune_missing_lock_entries(lock, &hub_dir, &excluded))
                .map_err(|error| AppError::Lockfile(format!("{error:#}")))?;
        if orphans_removed > 0 {
            ss_skills::installed_skill::invalidate_cache();
            fixed += orphans_removed;
        }

        Ok(fixed)
    })
    .await?
}

/// Import-cache directories whose name is not the cache key of an updatable
/// lock entry. An unreadable lock yields zero: "unused" must not mean "all".
fn unused_import_cache(root: &Path) -> (usize, u64) {
    let referenced = match skill_lock::SkillLock::read_state(&skill_lock::lock_path()) {
        skill_lock::LockFileState::Ready(lock) => lock
            .skills
            .values()
            .filter(|entry| entry.source_type.is_updatable() && !entry.source_url.is_empty())
            .map(|entry| {
                ss_skills::fetch::import_cache_key(&entry.source_url, entry.git_ref.as_deref())
            })
            .collect::<HashSet<_>>(),
        skill_lock::LockFileState::Missing => HashSet::new(),
        skill_lock::LockFileState::Outdated(_)
        | skill_lock::LockFileState::TooNew(_)
        | skill_lock::LockFileState::Corrupt(_) => return (0, 0),
    };
    unused_cache_in(root, &referenced)
}

fn unused_cache_in(root: &Path, referenced: &HashSet<String>) -> (usize, u64) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return (0, 0);
    };
    let mut count = 0usize;
    let mut bytes = 0u64;
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        if name.starts_with('.') || referenced.contains(name) {
            continue;
        }
        let path = entry.path();
        if fs_ops::is_link(&path) || !path.is_dir() {
            continue;
        }
        count += 1;
        bytes += dir_size_recursive(&path);
    }
    (count, bytes)
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

/// Whether a canonical folder is still a usable install.
enum HubFolder {
    Present,
    /// Nothing at the path, or a symlink whose target is gone.
    Missing,
    /// Stat failed for a reason other than "not there". Never prune this.
    Uncertain,
}

fn hub_folder_state(path: &Path) -> HubFolder {
    match path.symlink_metadata() {
        Ok(_) => {
            if fs_ops::is_link(path) && !path.exists() {
                HubFolder::Missing
            } else {
                HubFolder::Present
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => HubFolder::Missing,
        Err(_) => HubFolder::Uncertain,
    }
}

/// Drop lock entries whose canonical folder is confirmed gone.
///
/// `lock.remove(folder)` deletes every key for that directory, including a
/// vercel raw-name key next to the folder name. A key `folder_for_key` cannot
/// map, a folder we cannot stat, and anything excluded or channel-owned stays.
fn prune_missing_lock_entries(
    lock: &mut skill_lock::SkillLock,
    hub_dir: &Path,
    excluded: &HashSet<String>,
) -> usize {
    let before = lock.skills.len();
    let keys: Vec<String> = lock.skills.keys().cloned().collect();
    let mut drop_folders = HashSet::new();
    let mut keep_folders = HashSet::new();
    for key in keys {
        let Some(folder) = skill_lock::folder_for_key(&key) else {
            continue;
        };
        let protected = excluded.contains(&key)
            || excluded.contains(&folder)
            || is_channel_managed(&folder)
            || (key != folder && is_channel_managed(&key));
        if protected {
            keep_folders.insert(folder);
            continue;
        }
        match hub_folder_state(&hub_dir.join(&folder)) {
            HubFolder::Present | HubFolder::Uncertain => {
                keep_folders.insert(folder);
            }
            HubFolder::Missing => {
                drop_folders.insert(folder);
            }
        }
    }
    for folder in drop_folders {
        if !keep_folders.contains(&folder) {
            lock.remove(&folder);
        }
    }
    before.saturating_sub(lock.skills.len())
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

    // Orphaned install-lock entries: recorded, but no directory. Resolve the
    // canonical folder first so a vercel key is not counted while its folder
    // is present. One missing folder counts once. Unmapped keys are uncertain
    // and are not reported as broken. Broken symlinks are already counted above.
    let lock = skill_lock::load();
    let mut seen_folders = HashSet::new();
    for name in lock.skills.keys() {
        let Some(folder) = skill_lock::folder_for_key(name) else {
            continue;
        };
        if !seen_folders.insert(folder.clone()) {
            continue;
        }
        let skill_path = hub_dir.join(&folder);
        if matches!(
            skill_path.symlink_metadata(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound
        ) {
            broken += 1;
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
    use std::collections::HashSet;

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

    #[test]
    fn unused_cache_counts_only_directories_the_lock_does_not_reference() {
        let tmp = tempfile::tempdir().unwrap();
        let keep = ss_skills::fetch::import_cache_key("https://example.com/keep.git", None);
        std::fs::create_dir_all(tmp.path().join(&keep)).unwrap();
        std::fs::write(tmp.path().join(&keep).join("a"), b"abcd").unwrap();
        std::fs::create_dir_all(tmp.path().join("orphan")).unwrap();
        std::fs::write(tmp.path().join("orphan").join("b"), b"ef").unwrap();
        std::fs::create_dir_all(tmp.path().join(".fetch-temp")).unwrap();
        let mut referenced = HashSet::new();
        referenced.insert(keep);
        let (count, bytes) = unused_cache_in(tmp.path(), &referenced);
        assert_eq!(count, 1);
        assert_eq!(bytes, 2);
    }

    /// vercel keys the lock by the raw frontmatter name. `My Skill` is the
    /// folder `my-skill` and must survive cleanup while that folder exists.
    /// A key that does not map to a folder is uncertain and is kept. Every key
    /// for a folder that is confirmed missing is removed together.
    #[test]
    fn clean_broken_keeps_vercel_raw_name_keys_for_an_existing_folder() {
        let _env_lock = futures::executor::block_on(crate::test_support::ENV_LOCK.lock());
        let temp = tempfile::tempdir().unwrap();
        let _env = MaintenanceEnv::new(temp.path());

        let hub = ss_core::infra::paths::hub_skills_dir();
        std::fs::create_dir_all(hub.join("my-skill")).unwrap();
        let lock_path = ss_skills::skill_lock::lock_path();
        std::fs::create_dir_all(lock_path.parent().unwrap()).unwrap();
        std::fs::write(
            &lock_path,
            r#"{
  "version": 3,
  "skills": {
    "My Skill": {"source":"o/r","sourceType":"github","sourceUrl":"https://github.com/o/r.git","ref":"main","skillPath":"skills/my-skill","installedAt":"2020-01-01T00:00:00Z","updatedAt":"2020-01-01T00:00:00Z","pluginName":"plug"},
    "Ghost": {"source":"o/r","sourceType":"github","sourceUrl":"https://github.com/o/r.git","ref":"main","installedAt":"t","updatedAt":"t"},
    "ghost": {"source":"o/r","sourceType":"github","sourceUrl":"https://github.com/o/r.git","ref":"main","installedAt":"t","updatedAt":"t"},
    "...": {"source":"o/r","sourceType":"github","sourceUrl":"https://github.com/o/r.git","ref":"main","installedAt":"t","updatedAt":"t"}
  }
}"#,
        )
        .unwrap();

        let (_valid, broken) = count_hub_skills(&hub);
        assert_eq!(
            broken, 1,
            "My Skill maps to my-skill, which exists; only the missing ghost folder is broken"
        );

        let fixed = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(clean_broken_skills())
            .unwrap();
        assert_eq!(fixed, 2, "Ghost and ghost are one folder, two keys");

        let written: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&lock_path).unwrap()).unwrap();
        assert_eq!(written["skills"]["My Skill"]["pluginName"], "plug");
        assert!(written["skills"].get("Ghost").is_none());
        assert!(written["skills"].get("ghost").is_none());
        assert!(
            written["skills"].get("...").is_some(),
            "a key with no folder mapping is never pruned"
        );
        assert!(hub.join("my-skill").is_dir());
    }

    struct MaintenanceEnv {
        previous: Vec<(&'static str, Option<std::ffi::OsString>)>,
    }

    impl MaintenanceEnv {
        fn new(root: &std::path::Path) -> Self {
            let home = root.join("home");
            std::fs::create_dir_all(&home).unwrap();
            let assignments = [
                ("HOME", Some(home.clone())),
                ("USERPROFILE", Some(home.clone())),
                ("SKILLSTAR_DATA_DIR", Some(root.join("data"))),
                ("SKILLSTAR_HUB_DIR", Some(root.join("hub"))),
                ("SKILLSTAR_TOOL_SYNC_HOME", Some(root.join("tool-home"))),
                ("XDG_CONFIG_HOME", Some(home.join(".config"))),
                ("XDG_STATE_HOME", None),
                ("CLAUDE_CONFIG_DIR", None),
                ("AUTOHAND_HOME", None),
                ("DSH_HOME", None),
                ("GROK_HOME", None),
                ("HERMES_HOME", None),
                ("VIBE_HOME", None),
            ];
            let previous = assignments
                .iter()
                .map(|(key, _)| (*key, std::env::var_os(key)))
                .collect();
            unsafe {
                for (key, value) in assignments {
                    match value {
                        Some(path) => std::env::set_var(key, path),
                        None => std::env::remove_var(key),
                    }
                }
            }
            Self { previous }
        }
    }

    impl Drop for MaintenanceEnv {
        fn drop(&mut self) {
            unsafe {
                for (key, previous) in self.previous.drain(..).rev() {
                    match previous {
                        Some(value) => std::env::set_var(key, value),
                        None => std::env::remove_var(key),
                    }
                }
            }
        }
    }
}
