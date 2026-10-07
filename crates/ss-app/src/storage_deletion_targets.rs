//! Names a "delete installed skills" reset is allowed to remove.
//!
//! Shared by the settings confirmation preview and the reset itself, so the
//! list the user sees is the set that will be uninstalled. The install lock
//! is the only index.

use ss_core::infra::error::AppError;
use ss_core::infra::{fs_ops, paths};
use ss_skills::skill_lock;

use super::hub_entry_names;

pub(super) struct InstalledSkillDeletionTargets {
    /// Lock entries with a source SkillStar can re-fetch, excluding hub
    /// links of local skills (those are removed as links, not uninstalls).
    pub(super) locked: Vec<String>,
    /// Hub links whose target is under the local-skills root.
    pub(super) local_links: Vec<String>,
}

impl InstalledSkillDeletionTargets {
    pub(super) fn all_names(&self) -> Vec<String> {
        self.locked
            .iter()
            .chain(&self.local_links)
            .cloned()
            .collect()
    }
}

/// The install-lock and local-link names a hub reset is allowed to remove.
pub(super) fn installed_skill_deletion_targets() -> Result<InstalledSkillDeletionTargets, AppError>
{
    let lock_path = skill_lock::lock_path();
    let lock = match skill_lock::SkillLock::read_state(&lock_path) {
        skill_lock::LockFileState::Ready(lock) => lock,
        skill_lock::LockFileState::Missing | skill_lock::LockFileState::Outdated(_) => {
            skill_lock::SkillLock::default()
        }
        skill_lock::LockFileState::TooNew(version) => {
            return Err(AppError::Lockfile(format!(
                "Install lock version {version} is newer than this SkillStar; nothing was deleted"
            )));
        }
        skill_lock::LockFileState::Corrupt(reason) => {
            return Err(AppError::Lockfile(format!(
                "Install lock is unreadable ({reason}); nothing was deleted"
            )));
        }
    };

    let hub_dir = paths::hub_skills_dir();
    let local_root = fs_ops::canonicalize_existing_prefix(&paths::local_skills_dir());
    let local_links: Vec<String> = hub_entry_names(&hub_dir)
        .into_iter()
        .filter(|name| {
            let path = hub_dir.join(name);
            fs_ops::is_link(&path)
                && fs_ops::read_link_resolved(&path).is_ok_and(|target| {
                    fs_ops::canonicalize_existing_prefix(&target).starts_with(&local_root)
                })
        })
        .collect();
    let locked: Vec<String> = lock
        .skills
        .iter()
        .filter(|(name, entry)| {
            if entry.source_type == skill_lock::SourceType::Unknown {
                return false;
            }
            let folder = skill_lock::folder_for_key(name);
            !local_links
                .iter()
                .any(|link| link == *name || folder.as_ref() == Some(link))
        })
        .map(|(name, _)| name.clone())
        .collect();
    Ok(InstalledSkillDeletionTargets {
        locked,
        local_links,
    })
}
