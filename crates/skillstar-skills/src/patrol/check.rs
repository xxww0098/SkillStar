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
    if repo_link::is_repo_cached(skill_path) {
        return update_checker::check_update_local(skill_path, failed_fetch_roots);
    }

    // Fallback for non-repo-cached hub skills.
    let _ = crate::git::ops::ensure_worktree_checked_out(skill_path);
    match crate::git::ops::check_update(skill_path) {
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
        skills.push(HubSkillEntry { name, path });
    }
    skills.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(skills)
}

/// Prefetch unique repos for a skill path batch; returns failed roots.
pub fn prefetch_failed_repos(skill_paths: &[PathBuf]) -> HashSet<PathBuf> {
    update_checker::prefetch_unique_repos(skill_paths)
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
}
