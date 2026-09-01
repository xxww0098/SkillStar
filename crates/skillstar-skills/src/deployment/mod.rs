mod mirror;
mod status;

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
/// `--agent` / `--all` is itself authorization to deploy. Tauri's card and
/// batch actions, however, must fail closed when a stale UI or IPC request
/// names an inactive profile, otherwise merely handling that request can
/// provision a new `~/.agent/skills`-style directory.
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

/// Pin already-deployed Agent links to the hub's current payload.
///
/// A later harness retarget rewrites the single hub symlink + lock
/// `source_folder`. Agent links that still pointed at the hub would then
/// silently receive the other copy. Resolve those links onto the current
/// folder first so carousel / `--agent` clicks stay independent.
///
/// `except_agent_id` is the Agent being retargeted: leave its deploy alone
/// so a later deploy can point it at the new harness instead of pinning it
/// to the old one.
pub fn pin_existing_global_links_to_current_source(
    skill_name: &str,
    except_agent_id: Option<&str>,
) -> Result<()> {
    let hub = skillstar_core::infra::paths::hub_skills_dir().join(skill_name);
    if !skillstar_core::infra::fs_ops::is_link(&hub) {
        return Ok(());
    }
    let hub_payload = skillstar_core::infra::fs_ops::read_link_resolved(&hub)?;
    let hub_canon = std::fs::canonicalize(&hub).unwrap_or_else(|_| hub.clone());
    let payload_canon = std::fs::canonicalize(&hub_payload).unwrap_or(hub_payload);

    invalidate_profile_cache();
    for profile in cached_profiles() {
        if !profile.has_global_skills() {
            continue;
        }
        if except_agent_id.is_some_and(|id| id == profile.id) {
            continue;
        }
        let target = profile.global_skills_dir.join(skill_name);
        if !skillstar_core::infra::fs_ops::is_link(&target) {
            continue;
        }
        let resolved = match skillstar_core::infra::fs_ops::read_link_resolved(&target) {
            Ok(path) => std::fs::canonicalize(&path).unwrap_or(path),
            Err(_) => continue,
        };
        if resolved != hub_canon && resolved != payload_canon {
            continue;
        }
        skillstar_core::infra::fs_ops::remove_symlink(&target)?;
        skillstar_core::infra::fs_ops::create_symlink(&payload_canon, &target)?;
    }
    Ok(())
}

fn canonical_path(path: &Path) -> std::path::PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// True when `target` is a link that already resolves to the hub skill
/// (the hub path itself or the folder it currently points at).
fn link_already_has_hub_payload(target: &Path, hub: &Path) -> bool {
    if !skillstar_core::infra::fs_ops::is_link(target) {
        return false;
    }
    let Ok(resolved) = skillstar_core::infra::fs_ops::read_link_resolved(target) else {
        return false;
    };
    canonical_path(&resolved) == canonical_path(hub)
}

/// Whether `path` is a deployment SkillStar owns: a link/junction into the
/// hub, or a directory copy carrying `SKILL.md`.
///
/// Anything else in an Agent's skills directory belongs to the user or to the
/// Agent itself. `fs_ops::remove_link_or_copy` enforces the same rule before it
/// deletes a directory, so this is the read-side twin of that guard — use it
/// wherever a caller needs to *classify* an entry rather than remove one.
fn is_managed_deployment(path: &Path) -> bool {
    skillstar_core::infra::fs_ops::is_link(path)
        || (path.is_dir() && path.join("SKILL.md").exists())
}

fn remove_managed_entry_for_overwrite(path: &Path) -> Result<bool> {
    if !is_managed_deployment(path) {
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

/// Stable skip code for an unmanaged real directory occupying the skill name.
pub const SKIP_UNMANAGED_REAL_DIRECTORY: &str = "unmanaged_real_directory";

/// Result of a single skill ↔ agent toggle.
///
/// `Skipped` is reserved for name collisions with an unmanaged path that
/// SkillStar refuses to overwrite (e.g. Hermes' own `research/` category
/// folder). Batch callers surface these separately from hard failures;
/// single-skill IPC still maps them back to an error so the user notices.
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
            "name collision: target '{path}' is an unmanaged real directory (left in place)"
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
    let profile = if enable {
        require_enabled_global_profile(&profiles, agent_id)?
    } else {
        // Cleanup remains available after a profile is disabled.
        require_global_profile(&profiles, agent_id)?
    };
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

        // Remove existing symlink/junction/copy if present. An unmanaged real
        // directory (no SkillStar-managed SKILL.md copy, not a link) is a name
        // collision — leave it alone and report Skipped so batch "link all"
        // stays green for every skill that *can* link.
        if (target.symlink_metadata().is_ok()
            || skillstar_core::infra::fs_ops::is_link(&target)
            || target.exists())
            && !remove_managed_entry_for_overwrite(&target)?
        {
            tracing::warn!(
                target: "sync",
                operation = "toggle_skill_for_agent",
                phase = "skipped_name_collision",
                skill_name,
                agent_id,
                target = %target.display(),
                "skipping link — unmanaged real directory occupies the skill name"
            );
            return Ok(skip_unmanaged_real_directory(&target));
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
        // Only remove SkillStar-managed entries. An unmanaged real directory
        // (e.g. Hermes category folder) is left alone and reported as skipped
        // so bulk unlink does not look like a hard failure.
        match remove_entry_for_unlink(&target) {
            Ok(true) => {
                tracing::info!(target: "sync", skill_name, agent_id, "Skill unlinked successfully");
            }
            Ok(false) => {
                tracing::info!(
                    target: "sync",
                    target = %target.display(),
                    "Toggle off requested but nothing at target — already unlinked"
                );
            }
            Err(err) => {
                let message = format!("{err:#}");
                if message.contains("does not appear to be a managed skill copy") {
                    tracing::warn!(
                        target: "sync",
                        operation = "toggle_skill_for_agent",
                        phase = "skipped_name_collision",
                        skill_name,
                        agent_id,
                        target = %target.display(),
                        "skipping unlink — unmanaged real directory occupies the skill name"
                    );
                    return Ok(skip_unmanaged_real_directory(&target));
                }
                return Err(err);
            }
        }
    }

    mirror::sync(&profile.id, &profile.global_skills_dir);
    Ok(ToggleSkillOutcome::Applied)
}

/// Remove symlinks for a skill from all agent profiles.
pub fn remove_skill_from_all_agents(skill_name: &str) -> Result<Vec<String>> {
    let profiles = cached_profiles();
    let mut removed_from = Vec::with_capacity(profiles.len());
    let mut failures = Vec::new();

    for profile in &profiles {
        if !profile.has_global_skills() {
            continue;
        }
        let target = profile.global_skills_dir.join(skill_name);
        let outcome = remove_entry_for_unlink(&target);
        mirror::sync(&profile.id, &profile.global_skills_dir);
        match outcome {
            Ok(true) => {
                removed_from.push(profile.display_name.clone());
            }
            Ok(false) => {}
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

/// Remove all skill symlinks from a specific agent profile.
pub fn unlink_all_skills_from_agent(agent_id: &str) -> Result<u32> {
    tracing::info!(target: "sync", agent_id, "unlink_all_skills_from_agent called");

    let profiles = cached_profiles();
    let profile = require_global_profile(&profiles, agent_id)?;

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

    mirror::sync(&profile.id, skills_dir);
    tracing::info!(target: "sync", agent_id, removed, "unlink_all_skills_from_agent completed");
    Ok(removed)
}

/// List all skill names currently linked (symlinked) to a specific agent.
pub fn list_linked_skills(agent_id: &str) -> Result<Vec<String>> {
    let profiles = cached_profiles();
    let profile = require_global_profile(&profiles, agent_id)?;

    let skills_dir = &profile.global_skills_dir;
    if !skills_dir.exists() {
        return Ok(Vec::new());
    }

    let mut names = Vec::new();
    for entry in std::fs::read_dir(skills_dir)? {
        let entry = entry?;
        let path = entry.path();
        // Include symlinks/junctions AND copy-based deployments
        if is_managed_deployment(&path)
            && let Some(name) = entry.file_name().to_str()
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
        "unlink_skill_from_agent called"
    );

    let profiles = cached_profiles();
    let profile = require_global_profile(&profiles, agent_id)?;

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

    mirror::sync(&profile.id, &profile.global_skills_dir);
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
    let profile = require_enabled_global_profile(&profiles, agent_id)?;
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
            if link_already_has_hub_payload(&target, &skill_path) {
                tracing::debug!(target: "sync", skill = %name, target = %target.display(), "Already linked — skipping");
                continue;
            }
            match swap_in_fresh_deploy(&skill_path, &target) {
                Ok(_) => {
                    linked += 1;
                    continue;
                }
                Err(err) => {
                    failures.push(format!("{name}: {err:#}"));
                    continue;
                }
            }
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

    mirror::sync(&profile.id, target_dir);

    // `agent_links` is part of the cached installed-skill snapshot, so every
    // exit that may have changed a link must drop the cache — including the
    // failure exit, where links created before the failure stay in place.
    // Callers used to do this themselves and new call sites kept forgetting;
    // the cache has no TTL, so a miss leaves the UI showing stale link counts
    // until an unrelated mutation happens to clear it.
    crate::installed_skill::invalidate_cache();

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

/// Deploy skills to one or more Agent global directories using an explicit
/// install method. Physical target directories are deduplicated so aliases or
/// compatible profiles that share a directory are only mutated once.
pub fn batch_deploy_skills_to_agents(
    skill_names: &[String],
    agent_ids: &[String],
    mode: crate::projects::ProjectDeployMode,
) -> Result<u32> {
    let hub_dir = skillstar_core::infra::paths::hub_skills_dir();
    let profiles = cached_profiles();
    let mut target_dirs = Vec::new();
    let mut seen_dirs = HashSet::new();
    let mut invalid = Vec::new();

    for agent_id in agent_ids {
        let profile = match agent_profile::find_profile(&profiles, agent_id) {
            Ok(profile) if profile.has_global_skills() => profile,
            Ok(_) => {
                invalid.push(format!("{agent_id} (project-only)"));
                continue;
            }
            Err(_) => {
                invalid.push(agent_id.clone());
                continue;
            }
        };
        if seen_dirs.insert(profile.global_skills_dir.clone()) {
            target_dirs.push((profile.id.clone(), profile.global_skills_dir.clone()));
        }
    }
    if !invalid.is_empty() {
        anyhow::bail!("Unknown agent id(s): {}", invalid.join(", "));
    }
    if target_dirs.is_empty() {
        anyhow::bail!("No target agents selected");
    }

    let mut deployed = 0u32;
    let mut failures = Vec::new();
    for (agent_id, target_dir) in target_dirs {
        let mut prepared_target_dir = false;
        for skill_name in skill_names {
            let source = hub_dir.join(skill_name);
            if !source.exists() {
                failures.push(format!(
                    "{agent_id}/{skill_name}: skill is missing from hub"
                ));
                continue;
            }
            let target = target_dir.join(skill_name);
            if skillstar_core::infra::fs_ops::is_link(&target) {
                if link_already_has_hub_payload(&target, &source) {
                    continue;
                }
                // Stale: points at another harness copy or the old hub path.
                if !prepared_target_dir {
                    std::fs::create_dir_all(&target_dir).with_context(|| {
                        format!(
                            "Failed to create Agent skills dir '{}'",
                            target_dir.display()
                        )
                    })?;
                    prepared_target_dir = true;
                }
                let result = match mode {
                    crate::projects::ProjectDeployMode::Symlink => {
                        swap_in_fresh_deploy(&source, &target).map(|_| ())
                    }
                    crate::projects::ProjectDeployMode::Copy => {
                        skillstar_core::infra::fs_ops::remove_link_or_copy(&target).and_then(|_| {
                            skillstar_core::infra::fs_ops::create_copy_deploy(&source, &target)
                        })
                    }
                };
                match result {
                    Ok(()) => deployed += 1,
                    Err(err) => failures.push(format!(
                        "{agent_id}/{skill_name} at {}: {err:#}",
                        target.display()
                    )),
                }
                continue;
            }
            if target.exists() {
                // Unmanaged real directory — leave it alone.
                continue;
            }
            if !prepared_target_dir {
                std::fs::create_dir_all(&target_dir).with_context(|| {
                    format!(
                        "Failed to create Agent skills dir '{}'",
                        target_dir.display()
                    )
                })?;
                prepared_target_dir = true;
            }

            let result = match mode {
                crate::projects::ProjectDeployMode::Symlink => {
                    skillstar_core::infra::fs_ops::create_symlink_or_copy(&source, &target)
                        .map(|_| ())
                }
                crate::projects::ProjectDeployMode::Copy => {
                    skillstar_core::infra::fs_ops::create_copy_deploy(&source, &target)
                }
            };
            match result {
                Ok(()) => deployed += 1,
                Err(err) => failures.push(format!(
                    "{agent_id}/{skill_name} at {}: {err:#}",
                    target.display()
                )),
            }
        }
        mirror::sync(&agent_id, &target_dir);
    }

    // Same contract as `batch_link_skills_to_agent`: deployments made before a
    // failure stay on disk, so both exits must drop the cached `agent_links`.
    crate::installed_skill::invalidate_cache();

    if !failures.is_empty() {
        anyhow::bail!(
            "Global deploy incomplete: created {deployed} deployment(s), {} failure(s): {}",
            failures.len(),
            failures.into_iter().take(6).collect::<Vec<_>>().join("; ")
        );
    }
    Ok(deployed)
}

/// Create project-level skill symlinks in a project directory.
///
/// This is a thin facade over `crate::projects::add_skills_to_project()` — all
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
    crate::projects::add_skills_to_project(
        &project_path.to_string_lossy(),
        selected_skills,
        agent_types,
    )
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
        if !profile.has_global_skills() {
            continue;
        }
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
mod tests;
