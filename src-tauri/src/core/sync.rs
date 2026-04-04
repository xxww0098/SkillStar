use anyhow::{Context, Result};
use std::path::Path;
use std::sync::{OnceLock, RwLock};
use std::time::{Duration, Instant};
use tracing::warn;

use super::agent_profile;

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
    if let Ok(cache) = profile_cache().read() {
        if let Some(loaded_at) = cache.loaded_at {
            if loaded_at.elapsed() < PROFILE_CACHE_TTL {
                return cache.profiles.clone();
            }
        }
    }

    let profiles = agent_profile::list_profiles();

    if let Ok(mut cache) = profile_cache().write() {
        cache.loaded_at = Some(Instant::now());
        cache.profiles = profiles.clone();
    }

    profiles
}

fn remove_managed_entry_for_overwrite(path: &Path) -> Result<bool> {
    let is_link = super::paths::is_link(path);
    let is_copy = path.is_dir() && path.join("SKILL.md").exists();

    if !is_link && !is_copy {
        return Ok(false);
    }

    super::paths::remove_link_or_copy(path)?;
    Ok(true)
}

fn remove_entry_for_unlink(path: &Path) -> Result<bool> {
    // Keep unlink idempotent: if nothing exists at the target, treat as no-op.
    if path.symlink_metadata().is_err() && !super::paths::is_link(path) {
        return Ok(false);
    }

    // For unlink paths, attempt removal whenever an entry exists.
    // `remove_link_or_copy` already handles link/junction/copy differences,
    // including Windows-specific junction fallback behavior.
    super::paths::remove_link_or_copy(path)?;
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

    let hub_dir = super::paths::hub_skills_dir();
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
        is_link = super::paths::is_link(&target),
        exists = target.exists(),
        is_dir = target.is_dir(),
        "Target path state before toggle"
    );

    if enable {
        // Ensure parent dir exists
        std::fs::create_dir_all(&profile.global_skills_dir)?;

        // Remove existing symlink/junction/copy if present
        if target.symlink_metadata().is_ok() || super::paths::is_link(&target) || target.exists() {
            if !remove_managed_entry_for_overwrite(&target)? {
                tracing::error!(target: "sync", target = %target.display(), "Cannot overwrite real directory");
                anyhow::bail!("Target cannot be overwritten because it is a real directory");
            }
        }
        super::paths::create_symlink(&skill_path, &target)?;
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
    let mut removed_from = Vec::new();

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
        let is_managed = super::paths::is_link(&path)
            || (path.is_dir() && path.join("SKILL.md").exists());
        if is_managed {
            if let Some(name) = entry.file_name().to_str() {
                names.push(name.to_string());
            }
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
        is_link = super::paths::is_link(&target),
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
    let hub_dir = super::paths::hub_skills_dir();
    let profiles = cached_profiles();
    let profile = agent_profile::find_profile(&profiles, agent_id)?;

    std::fs::create_dir_all(&profile.global_skills_dir)?;

    let mut linked = 0u32;
    for name in skill_names {
        let skill_path = hub_dir.join(name);
        if !skill_path.exists() {
            continue; // Skip missing skills silently
        }
        let target = profile.global_skills_dir.join(name);
        if super::paths::is_link(&target) {
            continue; // Already linked
        }
        // Remove non-symlink entry if it exists (shouldn't happen, but be safe)
        if target.exists() {
            continue;
        }
        super::paths::create_symlink(&skill_path, &target)?;
        linked += 1;
    }

    Ok(linked)
}

/// Create project-level skill symlinks in a project directory.
pub fn create_project_skills(
    project_path: &Path,
    selected_skills: &[String],
    agent_types: &[String],
) -> Result<u32> {
    let hub_dir = super::paths::hub_skills_dir();
    let profiles = cached_profiles();

    let mut rel_dirs: Vec<String> = agent_types
        .iter()
        .filter_map(|id| {
            profiles
                .iter()
                .find(|p| &p.id == id)
                .filter(|p| p.has_project_skills())
                .map(|p| p.project_skills_rel.clone())
        })
        .collect();

    if rel_dirs.is_empty() {
        rel_dirs.push(".agents/skills".to_string());
    }

    let mut total_linked = 0u32;

    for rel_dir in rel_dirs {
        let skills_folder = project_path.join(&rel_dir);
        std::fs::create_dir_all(&skills_folder)
            .with_context(|| format!("Failed to create {}", skills_folder.display()))?;

        for name in selected_skills {
            let source = hub_dir.join(name);
            if !source.exists() {
                continue;
            }
            let target = skills_folder.join(name);
            if super::paths::is_link(&target) {
                let _ = super::paths::remove_symlink(&target);
            } else if target.exists() {
                continue;
            }
            if super::paths::create_symlink_or_copy(&source, &target).is_ok() {
                total_linked += 1;
            }
        }
    }

    Ok(total_linked)
}

/// Re-sync a skill only to agents that already have it linked.
///
/// After a `git pull` updates the skill content, the symlinks themselves stay
/// valid (they point at the directory, not individual files), but this function
/// ensures the link is recreated cleanly and returns the agent display names
/// that remain linked.
pub fn resync_existing_links(skill_name: &str) -> Result<Vec<String>> {
    let hub_dir = super::paths::hub_skills_dir();
    let skill_path = hub_dir.join(skill_name);
    if !skill_path.exists() {
        anyhow::bail!("Skill '{}' not found in hub", skill_name);
    }

    let profiles = cached_profiles();
    let mut linked_to = Vec::new();

    for profile in profiles.iter() {
        let target = profile.global_skills_dir.join(skill_name);
        // Only re-link if a symlink/junction already exists (preserves user's assignment)
        if super::paths::is_link(&target) {
            super::paths::remove_symlink(&target)?;
            super::paths::create_symlink(&skill_path, &target)?;
            linked_to.push(profile.display_name.clone());
        }
    }

    Ok(linked_to)
}
