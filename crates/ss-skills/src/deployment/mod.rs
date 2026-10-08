mod batch;
mod mirror;
pub mod ownership;
mod status;

pub(crate) use batch::swap_in_fresh_deploy;
pub use batch::{
    ResyncReport, batch_deploy_skills_to_agents, batch_link_skills_to_agent, resync_existing_links,
};
pub(crate) use mirror::{drift as mirror_drift, sync as sync_mirrors};
pub use ownership::{Ownership, owned_deployment, targets_canonical_root};
pub use status::{
    AgentDeployStatus, DeployKind, developer_mode_available, get_skill_deploy_status,
};

use anyhow::{Context, Result};
use std::collections::HashSet;
use std::path::Path;
use std::sync::{OnceLock, RwLock};
use std::time::{Duration, Instant};
use tracing::warn;

use crate::agents as agent_profile;
use ownership::Removal;

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

fn require_global_profile<'a>(
    profiles: &'a [agent_profile::AgentProfile],
    agent_id: &str,
) -> Result<&'a agent_profile::AgentProfile> {
    let profile = agent_profile::find_profile(profiles, agent_id)?;
    if !profile.has_global_skills() {
        anyhow::bail!("Agent '{}' does not support global skills", agent_id);
    }
    Ok(profile)
}

/// Resolve a GUI deployment target and require the user to have activated it.
///
/// The CLI has a separate explicit-target path (`batch_deploy_skills_to_agents`):
/// `--agent` / `--all` is itself authorization to deploy. The GUI's card and
/// batch actions, however, must fail closed when a stale request names an
/// inactive profile, otherwise merely handling that request can provision a
/// new `~/.agent/skills`-style directory.
fn require_enabled_global_profile<'a>(
    profiles: &'a [agent_profile::AgentProfile],
    agent_id: &str,
) -> Result<&'a agent_profile::AgentProfile> {
    let profile = require_global_profile(profiles, agent_id)?;
    if !profile.enabled {
        anyhow::bail!("Agent '{}' is not enabled", agent_id);
    }
    Ok(profile)
}

/// Stable skip code for an unmanaged real directory occupying the skill name.
pub const SKIP_UNMANAGED_REAL_DIRECTORY: &str = "unmanaged_real_directory";

/// Stable skip code for an Agent whose Global skills directory is the
/// canonical root itself: every installed Skill is already visible to it.
pub const SKIP_CANONICAL_ROOT_AGENT: &str = "canonical_root_agent";

/// Result of a single skill ↔ agent toggle.
///
/// `Skipped` covers name collisions with an entry SkillStar does not own
/// (e.g. Hermes' own `research/` category folder, or a folder the user made)
/// and Agents served by the canonical root directly. Batch callers surface
/// these separately from hard failures; single-skill actions still map them
/// back to an error so the user notices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToggleSkillOutcome {
    Applied,
    Skipped {
        code: String,
        path: String,
        reason: String,
    },
}

fn skip_unmanaged_real_directory(path: &Path) -> ToggleSkillOutcome {
    let path = path.display().to_string();
    ToggleSkillOutcome::Skipped {
        code: SKIP_UNMANAGED_REAL_DIRECTORY.to_string(),
        reason: format!(
            "name collision: target '{path}' is not managed by SkillStar (left in place)"
        ),
        path,
    }
}

fn skip_canonical_root_agent(dir: &Path) -> ToggleSkillOutcome {
    let path = dir.display().to_string();
    ToggleSkillOutcome::Skipped {
        code: SKIP_CANONICAL_ROOT_AGENT.to_string(),
        reason: format!(
            "this Agent reads the canonical skills root '{path}' directly; install or uninstall the Skill instead"
        ),
        path,
    }
}

/// Sync or unsync a single skill to a specific agent profile.
pub fn toggle_skill_for_agent(
    skill_name: &str,
    agent_id: &str,
    enable: bool,
) -> Result<ToggleSkillOutcome> {
    tracing::info!(
        target: "sync",
        skill_name,
        agent_id,
        enable = if enable { "on" } else { "off" },
        "→ toggle_skill_for_agent"
    );

    crate::content::validate_skill_name(skill_name)?;
    let _transaction = crate::skill_update::acquire_update_transaction_lock()?;
    let hub_dir = ss_core::infra::paths::hub_skills_dir();
    let skill_path = hub_dir.join(skill_name);
    if enable && !skill_path.exists() {
        tracing::error!(target: "sync", skill_name, "Skill not found in hub");
        anyhow::bail!("Skill '{}' not found in hub", skill_name);
    }

    let profiles = cached_profiles();
    let profile = if enable {
        require_enabled_global_profile(&profiles, agent_id)?
    } else {
        // Cleanup remains available after a profile is disabled.
        require_global_profile(&profiles, agent_id)?
    };
    if targets_canonical_root(&profile.global_skills_dir) {
        return Ok(skip_canonical_root_agent(&profile.global_skills_dir));
    }
    let target = profile.global_skills_dir.join(skill_name);
    let current = owned_deployment(&target, skill_name);

    tracing::debug!(
        target: "sync",
        target = %target.display(),
        ownership = ?current,
        "· target state before toggle"
    );

    if current == Ownership::Foreign {
        tracing::warn!(
            target: "sync",
            operation = "toggle_skill_for_agent",
            phase = "skipped_name_collision",
            skill_name,
            agent_id,
            target = %target.display(),
            "skipping — an entry SkillStar does not own occupies the skill name"
        );
        return Ok(skip_unmanaged_real_directory(&target));
    }

    if enable {
        let created_skills_dir = !profile.global_skills_dir.exists();
        std::fs::create_dir_all(&profile.global_skills_dir)?;

        // Symlink → junction → directory-copy ladder, same semantics as
        // project-level deploys (Windows without Developer Mode must not fail).
        let deployed = if current.is_owned() {
            batch::swap_in_fresh_deploy(&skill_path, &target, skill_name)
        } else {
            ownership::deploy_link_or_copy(&skill_path, &target, skill_name)
        };
        let was_copy = match deployed {
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
        tracing::info!(target: "sync", skill_name, agent_id, "✓ skill linked");
    } else {
        match ownership::remove_owned(&target, skill_name)? {
            Removal::Removed => {
                tracing::info!(target: "sync", skill_name, agent_id, "✓ skill unlinked");
            }
            Removal::Missing => {
                tracing::info!(
                    target: "sync",
                    target = %target.display(),
                    "· nothing at target — already unlinked"
                );
            }
            Removal::Foreign => return Ok(skip_unmanaged_real_directory(&target)),
        }
    }

    mirror::sync(&profile.id, &profile.global_skills_dir);
    // The skill list cache stores agent_links. Leave it and a carousel
    // refresh paints the icon as still linked.
    crate::installed_skill::invalidate_cache();
    Ok(ToggleSkillOutcome::Applied)
}

/// Remove SkillStar's deployments of a skill from all agent profiles.
///
/// Entries SkillStar does not own stay in place; Agents that read the
/// canonical root directly are skipped (removing the canonical folder is the
/// uninstall itself, not an unlink).
pub fn remove_skill_from_all_agents(skill_name: &str) -> Result<Vec<String>> {
    crate::content::validate_skill_name(skill_name)?;
    let _transaction = crate::skill_update::acquire_update_transaction_lock()?;
    let profiles = cached_profiles();
    let mut removed_from = Vec::with_capacity(profiles.len());
    let mut failures = Vec::new();
    let mut seen_dirs = HashSet::new();

    for profile in &profiles {
        if !profile.has_global_skills() || targets_canonical_root(&profile.global_skills_dir) {
            continue;
        }
        let dir_key = std::fs::canonicalize(&profile.global_skills_dir)
            .unwrap_or_else(|_| profile.global_skills_dir.clone());
        if !seen_dirs.insert(dir_key) {
            continue;
        }
        let target = profile.global_skills_dir.join(skill_name);
        let outcome = ownership::remove_owned(&target, skill_name);
        mirror::sync(&profile.id, &profile.global_skills_dir);
        match outcome {
            Ok(Removal::Removed) => {
                removed_from.push(profile.display_name.clone());
            }
            Ok(Removal::Missing) => {}
            Ok(Removal::Foreign) => {
                tracing::info!(
                    target: "sync",
                    path = %target.display(),
                    agent = %profile.id,
                    "· left an entry SkillStar does not own in place"
                );
            }
            Err(err) => {
                failures.push(format!("{}: {err:#}", profile.display_name));
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

    if failures.is_empty() {
        Ok(removed_from)
    } else {
        anyhow::bail!(
            "Failed to remove Skill '{}' from every Agent: {}",
            skill_name,
            failures.join(", ")
        )
    }
}

/// Remove all of SkillStar's deployments from a specific agent profile.
pub fn unlink_all_skills_from_agent(agent_id: &str) -> Result<u32> {
    tracing::info!(target: "sync", agent_id, "→ unlink_all_skills_from_agent");

    let _transaction = crate::skill_update::acquire_update_transaction_lock()?;
    let profiles = cached_profiles();
    let profile = require_global_profile(&profiles, agent_id)?;

    let skills_dir = &profile.global_skills_dir;
    if targets_canonical_root(skills_dir) {
        tracing::info!(target: "sync", agent_id, "· Agent reads the canonical root directly — nothing to unlink");
        return Ok(0);
    }
    if !skills_dir.exists() {
        tracing::info!(target: "sync", agent_id, "· skills directory missing — nothing to unlink");
        return Ok(0);
    }

    let mut removed = 0u32;
    for entry in std::fs::read_dir(skills_dir).context("Failed to read agent skills directory")? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        match ownership::remove_owned(&path, &name) {
            Ok(Removal::Removed) => {
                tracing::info!(target: "sync", name, path = %path.display(), "✓ removed managed deployment");
                removed += 1;
            }
            Ok(_) => {}
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

    mirror::sync(&profile.id, skills_dir);
    crate::installed_skill::invalidate_cache();
    tracing::info!(target: "sync", agent_id, removed, "✓ unlink_all_skills_from_agent completed");
    Ok(removed)
}

/// List the skills SkillStar has deployed to a specific agent.
///
/// An Agent that reads the canonical root directly has no deployments of its
/// own, so the list is empty.
pub fn list_linked_skills(agent_id: &str) -> Result<Vec<String>> {
    let profiles = cached_profiles();
    let profile = require_global_profile(&profiles, agent_id)?;

    let skills_dir = &profile.global_skills_dir;
    if !skills_dir.exists() || targets_canonical_root(skills_dir) {
        return Ok(Vec::new());
    }

    let mut names = Vec::new();
    for entry in std::fs::read_dir(skills_dir)? {
        let entry = entry?;
        if let Some(name) = entry.file_name().to_str()
            && owned_deployment(&entry.path(), name).is_owned()
        {
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
        "→ unlink_skill_from_agent"
    );

    crate::content::validate_skill_name(skill_name)?;
    let _transaction = crate::skill_update::acquire_update_transaction_lock()?;
    let profiles = cached_profiles();
    let profile = require_global_profile(&profiles, agent_id)?;
    if targets_canonical_root(&profile.global_skills_dir) {
        anyhow::bail!(
            "Agent '{agent_id}' reads the canonical skills root directly; uninstall the Skill instead"
        );
    }

    let target = profile.global_skills_dir.join(skill_name);
    if ownership::remove_owned(&target, skill_name)? == Removal::Foreign {
        tracing::warn!(
            target: "sync",
            path = %target.display(),
            "Target is not a managed entry — left in place"
        );
    }

    mirror::sync(&profile.id, &profile.global_skills_dir);
    crate::installed_skill::invalidate_cache();
    tracing::info!(target: "sync", skill_name, agent_id, "✓ unlink_skill_from_agent completed");
    Ok(())
}

pub fn create_project_skills_with_mode(
    project_path: &Path,
    selected_skills: &[String],
    agent_types: &[String],
    mode: crate::projects::ProjectDeployMode,
) -> Result<u32> {
    crate::projects::add_skills_to_project_with_mode(
        &project_path.to_string_lossy(),
        selected_skills,
        agent_types,
        mode,
    )
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod production_layout_tests;
