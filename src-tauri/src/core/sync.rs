use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};
use std::time::{Duration, Instant};

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

/// Sync or unsync a single skill to a specific agent profile.
pub fn toggle_skill_for_agent(skill_name: &str, agent_id: &str, enable: bool) -> Result<()> {
    let hub_dir = get_hub_skills_dir();
    let skill_path = hub_dir.join(skill_name);
    if enable && !skill_path.exists() {
        anyhow::bail!("Skill '{}' not found in hub", skill_name);
    }

    let profiles = cached_profiles();
    if let Some(profile) = profiles.into_iter().find(|p| p.id == agent_id) {
        let target = profile.global_skills_dir.join(skill_name);

        if enable {
            // Ensure parent dir exists
            std::fs::create_dir_all(&profile.global_skills_dir)?;

            // Remove existing symlink if present
            if target.symlink_metadata().is_ok() {
                if target.is_symlink() {
                    std::fs::remove_file(&target)?;
                } else {
                    anyhow::bail!("Target cannot be overwritten because it is a real directory");
                }
            }
            create_symlink(&skill_path, &target)?;
        } else {
            // Remove symlink
            if target.symlink_metadata().is_ok() && target.is_symlink() {
                std::fs::remove_file(&target)?;
            }
        }
    } else {
        anyhow::bail!("Agent profile '{}' not found", agent_id);
    }

    Ok(())
}

/// Remove symlinks for a skill from all agent profiles.
pub fn remove_skill_from_all_agents(skill_name: &str) -> Result<Vec<String>> {
    let profiles = cached_profiles();
    let mut removed_from = Vec::new();

    for profile in &profiles {
        let target = profile.global_skills_dir.join(skill_name);
        if target.is_symlink() {
            std::fs::remove_file(&target)?;
            removed_from.push(profile.display_name.clone());
        }
    }

    Ok(removed_from)
}

/// Remove all skill symlinks from a specific agent profile.
pub fn unlink_all_skills_from_agent(agent_id: &str) -> Result<u32> {
    let profiles = cached_profiles();
    let profile = profiles
        .iter()
        .find(|p| p.id == agent_id)
        .ok_or_else(|| anyhow::anyhow!("Agent profile '{}' not found", agent_id))?;

    let skills_dir = &profile.global_skills_dir;
    if !skills_dir.exists() {
        return Ok(0);
    }

    let mut removed = 0u32;
    for entry in std::fs::read_dir(skills_dir).context("Failed to read agent skills directory")? {
        let entry = entry?;
        let path = entry.path();
        if path.is_symlink() {
            std::fs::remove_file(&path)
                .with_context(|| format!("Failed to remove symlink {:?}", path))?;
            removed += 1;
        }
    }

    Ok(removed)
}

/// List all skill names currently linked (symlinked) to a specific agent.
pub fn list_linked_skills(agent_id: &str) -> Result<Vec<String>> {
    let profiles = cached_profiles();
    let profile = profiles
        .iter()
        .find(|p| p.id == agent_id)
        .ok_or_else(|| anyhow::anyhow!("Agent profile '{}' not found", agent_id))?;

    let skills_dir = &profile.global_skills_dir;
    if !skills_dir.exists() {
        return Ok(Vec::new());
    }

    let mut names = Vec::new();
    for entry in std::fs::read_dir(skills_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_symlink() {
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
    let profiles = cached_profiles();
    let profile = profiles
        .iter()
        .find(|p| p.id == agent_id)
        .ok_or_else(|| anyhow::anyhow!("Agent profile '{}' not found", agent_id))?;

    let target = profile.global_skills_dir.join(skill_name);
    if target.is_symlink() {
        std::fs::remove_file(&target)
            .with_context(|| format!("Failed to remove symlink {:?}", target))?;
    }
    Ok(())
}

/// Batch-link a list of skills to a specific agent.
///
/// Skips skills that are already linked. Returns the number of new links created.
pub fn batch_link_skills_to_agent(skill_names: &[String], agent_id: &str) -> Result<u32> {
    let hub_dir = get_hub_skills_dir();
    let profiles = cached_profiles();
    let profile = profiles
        .iter()
        .find(|p| p.id == agent_id)
        .ok_or_else(|| anyhow::anyhow!("Agent profile '{}' not found", agent_id))?;

    std::fs::create_dir_all(&profile.global_skills_dir)?;

    let mut linked = 0u32;
    for name in skill_names {
        let skill_path = hub_dir.join(name);
        if !skill_path.exists() {
            continue; // Skip missing skills silently
        }
        let target = profile.global_skills_dir.join(name);
        if target.is_symlink() {
            continue; // Already linked
        }
        // Remove non-symlink entry if it exists (shouldn't happen, but be safe)
        if target.exists() {
            continue;
        }
        create_symlink(&skill_path, &target)?;
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
    let hub_dir = get_hub_skills_dir();
    let profiles = cached_profiles();

    let mut rel_dirs: Vec<String> = agent_types
        .iter()
        .filter_map(|id| {
            profiles
                .iter()
                .find(|p| &p.id == id)
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
            if target.symlink_metadata().is_ok() {
                if target.is_symlink() {
                    let _ = std::fs::remove_file(&target);
                } else {
                    continue;
                }
            }
            if create_symlink(&source, &target).is_ok() {
                total_linked += 1;
            }
        }
    }

    Ok(total_linked)
}

/// Cross-platform symlink creation.
fn create_symlink(src: &Path, dst: &Path) -> Result<()> {
    #[cfg(unix)]
    std::os::unix::fs::symlink(src, dst)
        .with_context(|| format!("Failed to symlink {:?} -> {:?}", src, dst))?;

    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(src, dst)
        .with_context(|| format!("Failed to symlink {:?} -> {:?}", src, dst))?;

    Ok(())
}

/// Re-sync a skill only to agents that already have it linked.
///
/// After a `git pull` updates the skill content, the symlinks themselves stay
/// valid (they point at the directory, not individual files), but this function
/// ensures the link is recreated cleanly and returns the agent display names
/// that remain linked.
pub fn resync_existing_links(skill_name: &str) -> Result<Vec<String>> {
    let hub_dir = get_hub_skills_dir();
    let skill_path = hub_dir.join(skill_name);
    if !skill_path.exists() {
        anyhow::bail!("Skill '{}' not found in hub", skill_name);
    }

    let profiles = cached_profiles();
    let mut linked_to = Vec::new();

    for profile in profiles.iter().filter(|p| p.enabled) {
        let target = profile.global_skills_dir.join(skill_name);
        // Only re-link if a symlink already exists (preserves user's assignment)
        if target.is_symlink() {
            std::fs::remove_file(&target)?;
            create_symlink(&skill_path, &target)?;
            linked_to.push(profile.display_name.clone());
        }
    }

    Ok(linked_to)
}

/// Get the default skills hub directory.
pub fn get_hub_skills_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".agents")
        .join("skills")
}
