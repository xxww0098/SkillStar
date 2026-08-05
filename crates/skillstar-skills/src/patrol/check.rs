//! Pure patrol check helpers (no Tauri / no async runtime).
//!
//! Callers inject event sinks and cancellation; this module only decides
//! *what* to check and *whether* an update is available.

use anyhow::Result;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tracing::warn;

use crate::local_skill;
use crate::patrol::types::HubSkillEntry;
use crate::{repo_link, repo_scanner, update_checker};

/// Check a single skill for available updates without network (after prefetch).
///
/// Returns `None` when the repo fetch failed for this skill's root so callers
/// can skip emitting and preserve the previous badge.
pub fn check_skill_update_local(
    skill_name: &str,
    skill_path: &Path,
    failed_fetch_roots: &HashSet<PathBuf>,
) -> Option<bool> {
    check_skill_update_local_in_session(
        skill_name,
        skill_path,
        failed_fetch_roots,
        &crate::git::transport::GitOperationSession::public(),
    )
}

pub fn check_skill_update_local_in_session(
    skill_name: &str,
    skill_path: &Path,
    failed_fetch_roots: &HashSet<PathBuf>,
    session: &crate::git::transport::GitOperationSession,
) -> Option<bool> {
    let _guard = crate::skill_update::acquire_update_transaction_lock().ok()?;
    if !matches!(
        crate::shared_channels::generic_installed_skill_is_mutable(skill_name, skill_path),
        Ok(true)
    ) {
        return None;
    }
    if repo_link::is_repo_cached(skill_path) {
        return update_checker::check_update_local(skill_path, failed_fetch_roots);
    }

    // Fallback for non-repo-cached hub skills.
    let _ = crate::git::ops::ensure_worktree_checked_out_in_session(skill_path, session);
    match crate::git::ops::check_update_in_session(skill_path, session) {
        Ok(update_available) => Some(update_available),
        Err(err) => {
            warn!(
                target: "patrol",
                skill = %skill_name,
                error = %err,
                "check failed"
            );
            Some(false)
        }
    }
}

/// Collect installed hub (non-local) skills and their paths.
///
/// Lightweight directory scan — avoids parsing every SKILL.md each cycle.
pub fn collect_hub_skills() -> Result<Vec<HubSkillEntry>> {
    let skills_dir = skillstar_core::infra::paths::hub_skills_dir();
    let entries = match std::fs::read_dir(&skills_dir) {
        Ok(e) => e,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => {
            return Err(anyhow::anyhow!("Failed to read skills directory: {err}"));
        }
    };

    let mut skills = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.is_empty() {
            continue;
        }
        if local_skill::is_local_skill(&name) {
            continue;
        }
        match crate::shared_channels::generic_installed_skill_is_mutable(&name, &path) {
            Ok(true) => {}
            Ok(false) => continue,
            Err(error) => {
                warn!(
                    target: "patrol",
                    skill = %name,
                    error = %error,
                    "skipping update patrol because shared-channel ownership is unknown"
                );
                continue;
            }
        }
        skills.push(HubSkillEntry { name, path });
    }
    skills.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(skills)
}

/// Prefetch unique repos for a skill path batch; returns failed roots.
pub fn prefetch_failed_repos(skill_paths: &[PathBuf]) -> HashSet<PathBuf> {
    prefetch_failed_repos_in_session(
        skill_paths,
        &crate::git::transport::GitOperationSession::public(),
    )
}

pub fn prefetch_failed_repos_in_session(
    skill_paths: &[PathBuf],
    session: &crate::git::transport::GitOperationSession,
) -> HashSet<PathBuf> {
    let Ok(_guard) = crate::skill_update::acquire_update_transaction_lock() else {
        return skill_paths
            .iter()
            .filter_map(|path| repo_link::repo_root_of(path))
            .collect();
    };
    let safe_paths = skill_paths
        .iter()
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    matches!(
                        crate::shared_channels::generic_installed_skill_is_mutable(name, path),
                        Ok(true)
                    )
                })
        })
        .cloned()
        .collect::<Vec<_>>();
    update_checker::prefetch_unique_repos_in_session(&safe_paths, session)
}

/// Detect newly available skills in already-fetched repo caches.
pub fn detect_new_skills_in_cached_repos() -> Vec<crate::repo_scanner::RepoNewSkill> {
    repo_scanner::detect_new_skills_in_cached_repos()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn collect_hub_skills_skips_local_and_missing_dir() {
        let _guard = crate::lock_test_env();
        let temp = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
        }

        // Missing hub dir → empty
        let empty = collect_hub_skills().unwrap();
        assert!(empty.is_empty());

        let hub = skillstar_core::infra::paths::hub_skills_dir();
        fs::create_dir_all(&hub).unwrap();
        fs::create_dir_all(hub.join("remote-skill")).unwrap();
        // Local skill: create skills-local + hub symlink so is_local_skill is true
        let local_root = skillstar_core::infra::paths::local_skills_dir();
        fs::create_dir_all(local_root.join("local-one")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(local_root.join("local-one"), hub.join("local-one")).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(local_root.join("local-one"), hub.join("local-one"))
            .unwrap();

        let skills = collect_hub_skills().unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "remote-skill");

        unsafe {
            std::env::remove_var("SKILLSTAR_DATA_DIR");
        }
    }

    #[cfg(unix)]
    #[test]
    fn collect_hub_skills_skips_repo_checkout_without_a_trusted_baseline() {
        let _guard = crate::lock_test_env();
        let temp = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
        }
        let repository = skillstar_core::infra::paths::repos_cache_dir().join("acme--ordinary");
        fs::create_dir_all(repository.join(".git")).unwrap();
        fs::create_dir_all(repository.join("skills/ordinary")).unwrap();
        let hub = skillstar_core::infra::paths::hub_skills_dir();
        fs::create_dir_all(&hub).unwrap();
        std::os::unix::fs::symlink(repository.join("skills/ordinary"), hub.join("ordinary"))
            .unwrap();

        assert!(collect_hub_skills().unwrap().is_empty());

        unsafe {
            std::env::remove_var("SKILLSTAR_DATA_DIR");
        }
    }

    #[cfg(unix)]
    #[test]
    fn collect_hub_skills_skips_every_skill_in_a_channel_owned_checkout() {
        use crate::shared_channels::ChannelSubscriptionRegistry as _;

        let _guard = crate::lock_test_env();
        let temp = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
        }
        let hub = skillstar_core::infra::paths::hub_skills_dir();
        let repository = skillstar_core::infra::paths::repos_cache_dir().join("acme--channel");
        fs::create_dir_all(repository.join(".git")).unwrap();
        fs::create_dir_all(repository.join("skills/ordinary")).unwrap();
        fs::create_dir_all(repository.join("skills/channel-owned")).unwrap();
        fs::create_dir_all(&hub).unwrap();
        std::os::unix::fs::symlink(repository.join("skills/ordinary"), hub.join("ordinary"))
            .unwrap();
        std::os::unix::fs::symlink(
            repository.join("skills/channel-owned"),
            hub.join("channel-owned"),
        )
        .unwrap();
        let subscription = crate::shared_channels::ChannelSubscription {
            descriptor_version: crate::shared_channels::CHANNEL_SUBSCRIPTION_DESCRIPTOR_VERSION,
            repository_id: 42,
            organization_id: 7,
            repository_url_aliases: Vec::new(),
            target: crate::shared_channels::ChannelReleaseTarget {
                revision: 1,
                tag_name: "channel-v000001".into(),
                commit_sha: "a".repeat(40),
            },
            skills: vec![crate::shared_channels::ChannelSubscribedSkill {
                id: "channel-owned".into(),
                content_root: "skills/channel-owned".into(),
                release_content_hash: format!("sha256:{}", "b".repeat(64)),
                release_content_hash_version: crate::shared_channels::CHANNEL_CONTENT_HASH_VERSION,
                baseline_hash: format!("sha256:{}", "b".repeat(64)),
                baseline_hash_version: crate::shared_channels::CHANNEL_CONTENT_HASH_VERSION,
                provenance: crate::shared_channels::ChannelSkillProvenance {
                    repository_id: 42,
                    repository_url: "https://github.com/acme/channel.git".into(),
                    git_ref: "a".repeat(40),
                    source_folder: "skills/channel-owned".into(),
                },
            }],
            known_skill_ids: vec!["channel-owned".into()],
            pins: Vec::new(),
            last_update: None,
            auto_update: crate::shared_channels::ChannelAutoUpdateState::default(),
            remote_state: crate::shared_channels::ChannelSubscriptionRemoteState::default(),
            created_at: "2026-08-05T00:00:00Z".into(),
            updated_at: "2026-08-05T00:00:00Z".into(),
        };
        crate::shared_channels::DiskChannelSubscriptionRegistry
            .save(&crate::shared_channels::ChannelSubscriptionStore {
                schema_version: crate::shared_channels::CHANNEL_SUBSCRIPTION_STORE_VERSION,
                subscriptions: vec![subscription],
            })
            .unwrap();

        let skills = collect_hub_skills().unwrap();
        assert!(skills.is_empty());

        unsafe {
            std::env::remove_var("SKILLSTAR_DATA_DIR");
        }
    }
}
