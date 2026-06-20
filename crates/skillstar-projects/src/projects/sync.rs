use anyhow::{Context, Result};
use std::path::Path;
use std::sync::{OnceLock, RwLock};
use std::time::{Duration, Instant};
use tracing::warn;

use crate::projects::agents as agent_profile;

const PROFILE_CACHE_TTL: Duration = Duration::from_secs(2);

#[derive(Default)]
struct ProfileSnapshotCache {
    loaded_at: Option<Instant>,
    profiles: Vec<agent_profile::AgentProfile>,
}

fn profile_cache() -> &'static RwLock<ProfileSnapshotCache> {
    static CACHE: OnceLock<RwLock<ProfileSnapshotCache>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(ProfileSnapshotCache::default()))
}

pub fn invalidate_profile_cache() {
    if let Ok(mut cache) = profile_cache().write() {
        cache.loaded_at = None;
        cache.profiles.clear();
    }
}

/// Return a short-lived snapshot of agent profiles.
///
/// `agent_profile::list_profiles()` scans local config directories. Many sync
/// commands may run in quick succession (apply/import/toggle), so we keep a
/// tiny in-process cache to avoid repeated filesystem scans.
fn cached_profiles() -> Vec<agent_profile::AgentProfile> {
    if let Ok(cache) = profile_cache().read()
        && let Some(loaded_at) = cache.loaded_at
        && loaded_at.elapsed() < PROFILE_CACHE_TTL
    {
        return cache.profiles.clone();
    }

    let profiles = agent_profile::list_profiles();

    if let Ok(mut cache) = profile_cache().write() {
        cache.loaded_at = Some(Instant::now());
        cache.profiles = profiles.clone();
    }

    profiles
}

fn remove_managed_entry_for_overwrite(path: &Path) -> Result<bool> {
    let is_link = skillstar_core::infra::fs_ops::is_link(path);
    let is_copy = path.is_dir() && path.join("SKILL.md").exists();

    if !is_link && !is_copy {
        return Ok(false);
    }

    skillstar_core::infra::fs_ops::remove_link_or_copy(path)?;
    Ok(true)
}

fn remove_entry_for_unlink(path: &Path) -> Result<bool> {
    // Keep unlink idempotent: if nothing exists at the target, treat as no-op.
    if path.symlink_metadata().is_err() && !skillstar_core::infra::fs_ops::is_link(path) {
        return Ok(false);
    }

    // For unlink paths, attempt removal whenever an entry exists.
    // `remove_link_or_copy` already handles link/junction/copy differences,
    // including Windows-specific junction fallback behavior.
    skillstar_core::infra::fs_ops::remove_link_or_copy(path)?;
    Ok(true)
}

/// Sync or unsync a single skill to a specific agent profile.
pub fn toggle_skill_for_agent(skill_name: &str, agent_id: &str, enable: bool) -> Result<()> {
    tracing::info!(
        target: "sync",
        skill_name,
        agent_id,
        enable,
        "toggle_skill_for_agent called"
    );

    let hub_dir = skillstar_core::infra::paths::hub_skills_dir();
    let skill_path = hub_dir.join(skill_name);
    if enable && !skill_path.exists() {
        tracing::error!(target: "sync", skill_name, "Skill not found in hub");
        anyhow::bail!("Skill '{}' not found in hub", skill_name);
    }

    let profiles = cached_profiles();
    let profile = agent_profile::find_profile(&profiles, agent_id)?;
    let target = profile.global_skills_dir.join(skill_name);

    tracing::info!(
        target: "sync",
        target = %target.display(),
        is_link = skillstar_core::infra::fs_ops::is_link(&target),
        exists = target.exists(),
        is_dir = target.is_dir(),
        "Target path state before toggle"
    );

    if enable {
        // Ensure parent dir exists
        let created_skills_dir = !profile.global_skills_dir.exists();
        std::fs::create_dir_all(&profile.global_skills_dir)?;

        // Remove existing symlink/junction/copy if present
        if (target.symlink_metadata().is_ok()
            || skillstar_core::infra::fs_ops::is_link(&target)
            || target.exists())
            && !remove_managed_entry_for_overwrite(&target)?
        {
            tracing::error!(target: "sync", target = %target.display(), "Cannot overwrite real directory");
            anyhow::bail!("Target cannot be overwritten because it is a real directory");
        }
        // Symlink → junction → directory-copy ladder, same semantics as
        // project-level deploys (Windows without Developer Mode must not fail).
        let was_copy =
            match skillstar_core::infra::fs_ops::create_symlink_or_copy(&skill_path, &target) {
                Ok(was_copy) => was_copy,
                Err(err) => {
                    if created_skills_dir {
                        let _ = std::fs::remove_dir(&profile.global_skills_dir);
                    }
                    return Err(err);
                }
            };
        if was_copy {
            tracing::warn!(
                target: "sync",
                skill_name,
                agent_id,
                "Symlink unavailable — skill deployed to agent via copy fallback"
            );
        }
        tracing::info!(target: "sync", skill_name, agent_id, "Skill linked successfully");
    } else {
        // Remove symlink, junction, or directory copy
        if !remove_entry_for_unlink(&target)? {
            tracing::warn!(
                target: "sync",
                target = %target.display(),
                "Toggle off requested but target is not a link or directory — nothing to remove"
            );
        }
        tracing::info!(target: "sync", skill_name, agent_id, "Skill unlinked successfully");
    }

    Ok(())
}

/// Remove symlinks for a skill from all agent profiles.
pub fn remove_skill_from_all_agents(skill_name: &str) -> Result<Vec<String>> {
    let profiles = cached_profiles();
    let mut removed_from = Vec::with_capacity(profiles.len());

    for profile in &profiles {
        let target = profile.global_skills_dir.join(skill_name);
        match remove_entry_for_unlink(&target) {
            Ok(true) => {
                removed_from.push(profile.display_name.clone());
            }
            Ok(false) => {}
            Err(err) => {
                warn!(
                    target: "sync",
                    path = ?target,
                    skill = %skill_name,
                    agent = %profile.id,
                    error = %err,
                    "Failed to remove skill link from agent"
                );
            }
        }
    }

    Ok(removed_from)
}

/// Remove all skill symlinks from a specific agent profile.
pub fn unlink_all_skills_from_agent(agent_id: &str) -> Result<u32> {
    tracing::info!(target: "sync", agent_id, "unlink_all_skills_from_agent called");

    let profiles = cached_profiles();
    let profile = agent_profile::find_profile(&profiles, agent_id)?;

    let skills_dir = &profile.global_skills_dir;
    if !skills_dir.exists() {
        tracing::info!(target: "sync", agent_id, "Skills directory does not exist, nothing to unlink");
        return Ok(0);
    }

    let mut removed = 0u32;
    for entry in std::fs::read_dir(skills_dir).context("Failed to read agent skills directory")? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        match remove_entry_for_unlink(&path) {
            Ok(true) => {
                tracing::info!(target: "sync", name, path = %path.display(), "Removed managed skill deployment");
                removed += 1;
            }
            Ok(false) => {}
            Err(err) => {
                tracing::warn!(
                    target: "sync",
                    path = ?path,
                    agent = %agent_id,
                    error = %err,
                    "Failed to unlink skill from agent directory entry"
                );
            }
        }
    }

    tracing::info!(target: "sync", agent_id, removed, "unlink_all_skills_from_agent completed");
    Ok(removed)
}

/// List all skill names currently linked (symlinked) to a specific agent.
pub fn list_linked_skills(agent_id: &str) -> Result<Vec<String>> {
    let profiles = cached_profiles();
    let profile = agent_profile::find_profile(&profiles, agent_id)?;

    let skills_dir = &profile.global_skills_dir;
    if !skills_dir.exists() {
        return Ok(Vec::new());
    }

    let mut names = Vec::new();
    for entry in std::fs::read_dir(skills_dir)? {
        let entry = entry?;
        let path = entry.path();
        // Include symlinks/junctions AND copy-based deployments
        let is_managed = skillstar_core::infra::fs_ops::is_link(&path)
            || (path.is_dir() && path.join("SKILL.md").exists());
        if is_managed && let Some(name) = entry.file_name().to_str() {
            names.push(name.to_string());
        }
    }
    names.sort();
    Ok(names)
}

/// Unlink a single skill from a specific agent.
pub fn unlink_skill_from_agent(skill_name: &str, agent_id: &str) -> Result<()> {
    tracing::info!(
        target: "sync",
        skill_name,
        agent_id,
        "unlink_skill_from_agent called"
    );

    let profiles = cached_profiles();
    let profile = agent_profile::find_profile(&profiles, agent_id)?;

    let target = profile.global_skills_dir.join(skill_name);
    tracing::info!(
        target: "sync",
        path = %target.display(),
        is_link = skillstar_core::infra::fs_ops::is_link(&target),
        exists = target.exists(),
        is_dir = target.is_dir(),
        "Target path state"
    );

    if !remove_entry_for_unlink(&target)? {
        tracing::warn!(
            target: "sync",
            path = %target.display(),
            "Target is not a managed entry — cannot unlink"
        );
    }

    tracing::info!(target: "sync", skill_name, agent_id, "unlink_skill_from_agent completed");
    Ok(())
}

/// Batch-link a list of skills to a specific agent.
///
/// Skips skills that are already linked. Returns the number of new links created.
pub fn batch_link_skills_to_agent(skill_names: &[String], agent_id: &str) -> Result<u32> {
    tracing::info!(
        target: "sync",
        agent_id,
        count = skill_names.len(),
        "batch_link_skills_to_agent called"
    );

    let hub_dir = skillstar_core::infra::paths::hub_skills_dir();
    let profiles = cached_profiles();
    let profile = agent_profile::find_profile(&profiles, agent_id)?;
    let target_dir = &profile.global_skills_dir;

    let mut linked = 0u32;
    let mut skipped = 0u32;
    let mut failures: Vec<String> = Vec::new();
    let mut created_target_dir = false;
    for name in skill_names {
        let skill_path = hub_dir.join(name);
        let target = target_dir.join(name);

        let skill_exists = skill_path.exists();
        let skill_is_link = skillstar_core::infra::fs_ops::is_link(&skill_path);

        if !skill_exists {
            if skill_is_link {
                tracing::warn!(
                    target: "sync",
                    skill = %name,
                    skill_path = %skill_path.display(),
                    "Skill hub entry is a broken symlink — removing and skipping"
                );
                let _ = skillstar_core::infra::fs_ops::remove_link_or_copy(&skill_path);
            } else {
                tracing::warn!(
                    target: "sync",
                    skill = %name,
                    skill_path = %skill_path.display(),
                    "Skill not found in hub directory — skipping"
                );
            }
            skipped += 1;
            continue;
        }

        if skillstar_core::infra::fs_ops::is_link(&target) {
            tracing::debug!(target: "sync", skill = %name, target = %target.display(), "Already linked — skipping");
            continue;
        }
        if target.exists() {
            tracing::warn!(
                target: "sync",
                skill = %name,
                target = %target.display(),
                "Real directory exists at target — skipping"
            );
            skipped += 1;
            continue;
        }

        if !target_dir.exists() {
            std::fs::create_dir_all(target_dir)?;
            created_target_dir = true;
        }

        match skillstar_core::infra::fs_ops::create_symlink_or_copy(&skill_path, &target) {
            Ok(was_copy) => {
                if was_copy {
                    tracing::warn!(
                        target: "sync",
                        skill = %name,
                        target = %target.display(),
                        "Symlink unavailable — skill deployed to agent via copy fallback"
                    );
                }
                tracing::info!(
                    target: "sync",
                    skill = %name,
                    source = %skill_path.display(),
                    target = %target.display(),
                    "Skill linked successfully"
                );
                linked += 1;
            }
            Err(e) => {
                tracing::error!(
                    target: "sync",
                    skill = %name,
                    source = %skill_path.display(),
                    target = %target.display(),
                    error = %e,
                    "Failed to deploy skill to agent"
                );
                failures.push(format!("{name}: {e:#}"));
            }
        }
    }

    if !failures.is_empty() {
        if linked == 0 && created_target_dir {
            let _ = std::fs::remove_dir(target_dir);
        }
        // Links created before a failure stay in place — re-running is
        // idempotent (already-linked skills are skipped above).
        anyhow::bail!(
            "Failed to deploy {} of {} skills: {}",
            failures.len(),
            skill_names.len(),
            failures.join("; ")
        );
    }

    tracing::info!(
        target: "sync",
        agent_id,
        linked,
        skipped,
        total = skill_names.len(),
        "batch_link_skills_to_agent completed"
    );

    Ok(linked)
}

/// Create project-level skill symlinks in a project directory.
///
/// This is a thin facade over `project_manifest::add_skills_to_project()` — all
/// project-level skill management is canonically owned by `project_manifest`.
///
/// The function registers the project (if not already registered), merges the
/// requested skills into `skills-list.json`, and creates symlinks incrementally
/// without clearing other agents' directories.
pub fn create_project_skills(
    project_path: &Path,
    selected_skills: &[String],
    agent_types: &[String],
) -> Result<u32> {
    crate::projects::project_manifest::add_skills_to_project(
        &project_path.to_string_lossy(),
        selected_skills,
        agent_types,
    )
}

/// Outcome of [`resync_existing_links`]: which agents were refreshed and
/// which failed (per-agent, formatted as "Display Name: error").
#[derive(Debug, Clone, Default)]
pub struct ResyncReport {
    pub linked_to: Vec<String>,
    pub failures: Vec<String>,
}

/// Staging sibling used by [`swap_in_fresh_deploy`]; same directory so the
/// final rename never crosses filesystems.
fn resync_staging_path(target: &Path) -> std::path::PathBuf {
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "skill".to_string());
    target.with_file_name(format!(".{name}.skillstar-resync"))
}

/// Replace the deployment at `target` with a fresh one, never destroying the
/// existing entry unless the replacement already materialized.
///
/// Order matters: the fresh deploy is created under a staging name FIRST, so
/// the common failure (symlink creation denied — e.g. Windows after Developer
/// Mode was turned off) leaves the user's existing link untouched. Only after
/// staging succeeds is the old entry removed and the staging renamed in.
/// Returns `true` when the fresh deploy is a directory copy.
fn swap_in_fresh_deploy(skill_path: &Path, target: &Path) -> Result<bool> {
    use skillstar_core::infra::fs_ops;

    let staging = resync_staging_path(target);
    if staging.symlink_metadata().is_ok() {
        // Stale leftovers from an interrupted resync — clear before reuse.
        fs_ops::remove_link_or_copy(&staging)
            .with_context(|| format!("Failed to clear stale staging '{}'", staging.display()))?;
    }

    // 1. Materialize the fresh deploy beside the target (symlink → junction →
    //    copy ladder). Failure here is safe: the old deployment still works.
    let was_copy = fs_ops::create_symlink_or_copy(skill_path, &staging)
        .with_context(|| format!("Failed to stage fresh deploy at '{}'", staging.display()))?;

    // 2. Swap it in.
    if let Err(remove_err) = fs_ops::remove_link_or_copy(target) {
        let _ = fs_ops::remove_link_or_copy(&staging);
        return Err(remove_err)
            .with_context(|| format!("Failed to remove old deploy '{}'", target.display()));
    }
    if let Err(rename_err) = std::fs::rename(&staging, target) {
        // Old entry is gone; land the fresh deploy directly as a last resort
        // before reporting, so we never finish in an unlinked state silently.
        let direct = fs_ops::create_symlink_or_copy(skill_path, target);
        let _ = fs_ops::remove_link_or_copy(&staging);
        return direct.with_context(|| {
            format!(
                "Failed to move staged deploy into '{}' ({rename_err}); direct re-deploy also failed",
                target.display()
            )
        });
    }

    Ok(was_copy)
}

/// Re-sync a skill only to agents that already have it deployed.
///
/// After a `git pull` updates the skill content, symlinks stay live on their
/// own (they point at the directory), but copy deployments go stale and links
/// benefit from a clean re-create. Refreshes both forms via a staged swap
/// that preserves the existing deployment when re-creation fails, and never
/// aborts the remaining agents on a per-agent failure.
pub fn resync_existing_links(skill_name: &str) -> Result<ResyncReport> {
    let hub_dir = skillstar_core::infra::paths::hub_skills_dir();
    let skill_path = hub_dir.join(skill_name);
    if !skill_path.exists() {
        anyhow::bail!("Skill '{}' not found in hub", skill_name);
    }

    let profiles = cached_profiles();
    let mut report = ResyncReport::default();

    for profile in profiles.iter() {
        let target = profile.global_skills_dir.join(skill_name);
        let is_link = skillstar_core::infra::fs_ops::is_link(&target);
        let is_managed_copy = !is_link && target.is_dir() && target.join("SKILL.md").exists();
        // Only refresh existing deployments (preserves user's assignment).
        if !is_link && !is_managed_copy {
            continue;
        }

        match swap_in_fresh_deploy(&skill_path, &target) {
            Ok(was_copy) => {
                if was_copy {
                    tracing::info!(
                        target: "sync",
                        skill = %skill_name,
                        agent = %profile.id,
                        "Resynced via copy fallback (symlink unavailable)"
                    );
                }
                report.linked_to.push(profile.display_name.clone());
            }
            Err(err) => {
                tracing::error!(
                    target: "sync",
                    skill = %skill_name,
                    agent = %profile.id,
                    error = %err,
                    "Failed to resync skill deployment for agent"
                );
                report
                    .failures
                    .push(format!("{}: {err:#}", profile.display_name));
            }
        }
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::fs;

    fn set_env<K: AsRef<OsStr>, V: AsRef<OsStr>>(key: K, value: V) {
        unsafe { std::env::set_var(key, value) }
    }

    fn remove_env<K: AsRef<OsStr>>(key: K) {
        unsafe { std::env::remove_var(key) }
    }

    fn make_skill_dir(root: &Path, name: &str) -> std::path::PathBuf {
        let dir = root.join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), "# test skill\n").unwrap();
        dir
    }

    #[test]
    fn batch_link_skips_missing_skills_without_creating_agent_dir() -> Result<()> {
        let _guard = crate::projects::lock_test_env();
        invalidate_profile_cache();

        let tmp = tempfile::tempdir()?;
        let home = tmp.path().join("home");
        fs::create_dir_all(&home)?;

        let previous_home = std::env::var_os("HOME");
        let previous_data_dir = std::env::var_os("SKILLSTAR_DATA_DIR");
        set_env("HOME", &home);
        set_env("SKILLSTAR_DATA_DIR", home.join(".skillstar"));
        #[cfg(windows)]
        let previous_userprofile = std::env::var_os("USERPROFILE");
        #[cfg(windows)]
        set_env("USERPROFILE", &home);

        let result = (|| -> Result<()> {
            let missing = vec!["missing-skill".to_string()];
            let linked = batch_link_skills_to_agent(&missing, "claude")?;
            assert_eq!(linked, 0);
            assert!(
                !home.join(".claude").exists(),
                "skipping missing skills must not create the agent config root"
            );
            Ok(())
        })();

        match previous_home {
            Some(value) => set_env("HOME", value),
            None => remove_env("HOME"),
        }
        match previous_data_dir {
            Some(value) => set_env("SKILLSTAR_DATA_DIR", value),
            None => remove_env("SKILLSTAR_DATA_DIR"),
        }
        #[cfg(windows)]
        match previous_userprofile {
            Some(value) => set_env("USERPROFILE", value),
            None => remove_env("USERPROFILE"),
        }
        invalidate_profile_cache();

        result
    }

    #[test]
    fn swap_refreshes_an_existing_symlink() {
        let tmp = tempfile::tempdir().unwrap();
        let skill = make_skill_dir(tmp.path(), "hub-skill");
        let agent_dir = tmp.path().join("agent");
        fs::create_dir_all(&agent_dir).unwrap();
        let target = agent_dir.join("hub-skill");
        skillstar_core::infra::fs_ops::create_symlink(&skill, &target).unwrap();

        let was_copy = swap_in_fresh_deploy(&skill, &target).unwrap();

        assert!(!was_copy);
        assert!(skillstar_core::infra::fs_ops::is_link(&target));
        assert!(target.join("SKILL.md").exists());
        assert!(
            !resync_staging_path(&target).symlink_metadata().is_ok(),
            "staging entry must not be left behind"
        );
    }

    #[test]
    fn swap_refreshes_a_stale_copy_deployment() {
        let tmp = tempfile::tempdir().unwrap();
        let skill = make_skill_dir(tmp.path(), "hub-skill");
        fs::write(skill.join("SKILL.md"), "# fresh content\n").unwrap();

        let agent_dir = tmp.path().join("agent");
        fs::create_dir_all(&agent_dir).unwrap();
        let target = agent_dir.join("hub-skill");
        // Simulate an old copy deployment with stale content.
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("SKILL.md"), "# stale content\n").unwrap();

        swap_in_fresh_deploy(&skill, &target).unwrap();

        let refreshed = fs::read_to_string(
            skillstar_core::infra::fs_ops::read_link_resolved(&target)
                .map(|p| p.join("SKILL.md"))
                .unwrap_or_else(|_| target.join("SKILL.md")),
        )
        .unwrap();
        assert!(refreshed.contains("fresh content"));
    }

    #[cfg(unix)]
    #[test]
    fn swap_keeps_old_link_when_staging_fails() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().unwrap();
        let skill = make_skill_dir(tmp.path(), "hub-skill");
        let agent_dir = tmp.path().join("agent");
        fs::create_dir_all(&agent_dir).unwrap();
        let target = agent_dir.join("hub-skill");
        skillstar_core::infra::fs_ops::create_symlink(&skill, &target).unwrap();

        // Make the agent dir read-only so staging creation fails.
        fs::set_permissions(&agent_dir, fs::Permissions::from_mode(0o555)).unwrap();
        let result = swap_in_fresh_deploy(&skill, &target);
        fs::set_permissions(&agent_dir, fs::Permissions::from_mode(0o755)).unwrap();

        assert!(result.is_err());
        assert!(
            skillstar_core::infra::fs_ops::is_link(&target),
            "the pre-existing link must survive a failed resync"
        );
    }
}
