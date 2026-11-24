//! Many-skill deployments and the post-update resync.

use std::collections::HashSet;
use std::path::Path;

use anyhow::{Context, Result};

use super::ownership::{self, Ownership};
use super::{cached_profiles, mirror, require_enabled_global_profile, shared_coord};
use crate::agents::{
    self as agent_profile, AgentDeployMode, agent_disallows_junction,
    agent_reads_shared_agents_skills, effective_deploy_mode, uses_shared_agents_skills_root,
};
use crate::materialize;

fn canonical_path(path: &Path) -> std::path::PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// True when `target` is a link that already resolves to the hub skill
/// (the hub path itself or the folder it currently points at).
fn link_already_has_hub_payload(target: &Path, hub: &Path) -> bool {
    if !ss_core::infra::fs_ops::is_link(target) {
        return false;
    }
    let Ok(resolved) = ss_core::infra::fs_ops::read_link_resolved(target) else {
        return false;
    };
    canonical_path(&resolved) == canonical_path(hub)
}

/// Skipped skill item returned by [`batch_link_skills_to_agent_with_report`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchLinkSkippedItem {
    pub skill_name: String,
    pub code: String,
    pub path: String,
}

/// Detailed outcome of [`batch_link_skills_to_agent_with_report`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BatchLinkOutcome {
    pub linked: u32,
    pub skipped: Vec<BatchLinkSkippedItem>,
}

/// Batch-link a list of skills to a specific agent.
///
/// Skips skills that are already linked, and entries SkillStar does not own.
/// Returns the number of new links created.
pub fn batch_link_skills_to_agent(skill_names: &[String], agent_id: &str) -> Result<u32> {
    Ok(batch_link_skills_to_agent_with_report(skill_names, agent_id)?.linked)
}

/// Batch-link a list of skills to a specific agent, returning both new link
/// count and any collision-skipped items (`unmanaged_real_directory`,
/// `foreign_link`, or `modified_agent_copy`).
pub fn batch_link_skills_to_agent_with_report(
    skill_names: &[String],
    agent_id: &str,
) -> Result<BatchLinkOutcome> {
    tracing::info!(
        target: "sync",
        agent_id,
        count = skill_names.len(),
        "→ batch_link_skills_to_agent"
    );

    let _transaction = crate::skill_update::acquire_update_transaction_lock()?;
    let hub_dir = ss_core::infra::paths::hub_skills_dir();
    let profiles = cached_profiles();
    let profile = require_enabled_global_profile(&profiles, agent_id)?;
    let target_dir = &profile.global_skills_dir;
    if ownership::targets_canonical_root(target_dir) {
        tracing::info!(target: "sync", agent_id, "· Agent reads the canonical root directly — nothing to link");
        return Ok(BatchLinkOutcome::default());
    }

    let mode = effective_deploy_mode(&profile.id);
    let disallow_junction = agent_disallows_junction(&profile.id);
    let is_shared_root = uses_shared_agents_skills_root(profile);
    let is_secondary = agent_reads_shared_agents_skills(&profile.id);

    let mut linked = 0u32;
    let mut skipped_missing = 0u32;
    let mut skipped_items: Vec<BatchLinkSkippedItem> = Vec::new();
    let mut failures: Vec<String> = Vec::new();
    let mut created_target_dir = false;
    for name in skill_names {
        if crate::content::validate_skill_name(name).is_err() {
            failures.push(format!("{name}: invalid skill name"));
            continue;
        }
        let skill_path = hub_dir.join(name);
        let target = target_dir.join(name);

        if !skill_path.exists() {
            tracing::warn!(
                target: "sync",
                skill = %name,
                skill_path = %skill_path.display(),
                "Skill not found in hub directory — skipping"
            );
            skipped_missing += 1;
            continue;
        }

        if is_secondary {
            let lib = crate::library::load();
            if shared_coord::shared_agents_entry_is_active(name, &lib, &profiles) {
                match ownership::owned_deployment(&target, name) {
                    owned @ (Ownership::Foreign | Ownership::SkillStarModifiedCopy) => {
                        tracing::warn!(
                            target: "sync",
                            skill = %name,
                            target = %target.display(),
                            "Entry SkillStar does not own or edited copy exists at target — skipping"
                        );
                        skipped_items.push(BatchLinkSkippedItem {
                            skill_name: name.clone(),
                            code: if owned == Ownership::SkillStarModifiedCopy {
                                super::SKIP_MODIFIED_AGENT_COPY.to_string()
                            } else {
                                super::foreign_skip_code(&target).to_string()
                            },
                            path: target.display().to_string(),
                        });
                        continue;
                    }
                    Ownership::Link { .. } | Ownership::Copy => {
                        let _ = ownership::remove_owned(&target, name);
                    }
                    Ownership::Missing => {}
                }
                let _ = shared_coord::record_deployment(name, &profile.id, false, true);
                linked += 1;
                continue;
            }
        }

        match ownership::owned_deployment(&target, name) {
            Ownership::Link { .. }
                if mode == AgentDeployMode::Symlink
                    && link_already_has_hub_payload(&target, &skill_path) =>
            {
                let _ = shared_coord::record_deployment(name, &profile.id, false, false);
                tracing::debug!(target: "sync", skill = %name, target = %target.display(), "· already linked — skipping");
                continue;
            }
            Ownership::Link { .. } | Ownership::Copy => {
                match shared_coord::swap_in_deployment_for_agent(&skill_path, &target, name, &profile.id) {
                    Ok(was_copy) => {
                        let _ = shared_coord::record_deployment(name, &profile.id, was_copy, false);
                        if is_shared_root {
                            let _ = shared_coord::suppress_secondary_agents_if_shared(name, &profiles);
                        }
                        linked += 1;
                    }
                    Err(err) => failures.push(format!("{name}: {err:#}")),
                }
                continue;
            }
            owned @ (Ownership::Foreign | Ownership::SkillStarModifiedCopy) => {
                tracing::warn!(
                    target: "sync",
                    skill = %name,
                    target = %target.display(),
                    "Entry SkillStar does not own or edited copy exists at target — skipping"
                );
                skipped_items.push(BatchLinkSkippedItem {
                    skill_name: name.clone(),
                    code: if owned == Ownership::SkillStarModifiedCopy {
                        super::SKIP_MODIFIED_AGENT_COPY.to_string()
                    } else {
                        super::foreign_skip_code(&target).to_string()
                    },
                    path: target.display().to_string(),
                });
                continue;
            }
            Ownership::Missing => {}
        }

        if !target_dir.exists() {
            std::fs::create_dir_all(target_dir)?;
            created_target_dir = true;
        }

        match ownership::deploy_for_agent(&skill_path, &target, name, mode, disallow_junction) {
            Ok(was_copy) => {
                if was_copy && mode == AgentDeployMode::Symlink {
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
                    "✓ skill linked"
                );
                let _ = shared_coord::record_deployment(name, &profile.id, was_copy, false);
                if is_shared_root {
                    let _ = shared_coord::suppress_secondary_agents_if_shared(name, &profiles);
                }
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
        skipped = skipped_missing + skipped_items.len() as u32,
        total = skill_names.len(),
        "✓ batch_link_skills_to_agent completed"
    );

    Ok(BatchLinkOutcome {
        linked,
        skipped: skipped_items,
    })
}

/// Deploy skills to one or more Agent global directories using an explicit
/// install method. Physical target directories are deduplicated so aliases or
/// compatible profiles that share a directory are only mutated once; Agents
/// that read the canonical root directly need no deployment and are skipped.
pub fn batch_deploy_skills_to_agents(
    skill_names: &[String],
    agent_ids: &[String],
    mode: crate::projects::ProjectDeployMode,
) -> Result<u32> {
    let _transaction = crate::skill_update::acquire_update_transaction_lock()?;
    let hub_dir = ss_core::infra::paths::hub_skills_dir();
    let profiles = cached_profiles();
    let mut target_dirs = Vec::new();
    let mut seen_dirs = HashSet::new();
    let mut invalid = Vec::new();
    let mut served_by_canonical_root = false;

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
        if ownership::targets_canonical_root(&profile.global_skills_dir) {
            served_by_canonical_root = true;
            continue;
        }
        let physical =
            ss_core::infra::fs_ops::canonicalize_existing_prefix(&profile.global_skills_dir);
        if seen_dirs.insert(physical) {
            target_dirs.push((profile.id.clone(), profile.global_skills_dir.clone()));
        }
    }
    if !invalid.is_empty() {
        anyhow::bail!("Unknown agent id(s): {}", invalid.join(", "));
    }
    if target_dirs.is_empty() {
        if served_by_canonical_root {
            return Ok(0);
        }
        anyhow::bail!("No target agents selected");
    }

    // Deploy shared `~/.agents/skills` targets before secondary agents so
    // anti-double-loading suppression inspects live shared directories.
    target_dirs.sort_by_key(|(id, _)| {
        if agent_reads_shared_agents_skills(id) {
            1
        } else {
            0
        }
    });

    let mut deployed = 0u32;
    let mut failures = Vec::new();
    for (agent_id, target_dir) in target_dirs {
        let mut prepared_target_dir = false;
        let disallow_junction = agent_disallows_junction(&agent_id);
        let is_secondary = agent_reads_shared_agents_skills(&agent_id);
        for skill_name in skill_names {
            if crate::content::validate_skill_name(skill_name).is_err() {
                failures.push(format!("{agent_id}/{skill_name}: invalid skill name"));
                continue;
            }
            let source = hub_dir.join(skill_name);
            if !source.exists() {
                failures.push(format!(
                    "{agent_id}/{skill_name}: skill is missing from hub"
                ));
                continue;
            }
            let target = target_dir.join(skill_name);
            if is_secondary {
                let lib = crate::library::load();
                if shared_coord::shared_agents_entry_is_active(skill_name, &lib, &profiles) {
                    match ownership::owned_deployment(&target, skill_name) {
                        Ownership::Copy | Ownership::Link { .. } => {
                            let _ = ownership::remove_owned(&target, skill_name);
                            let _ = shared_coord::record_deployment(
                                skill_name, &agent_id, false, true,
                            );
                            deployed += 1;
                            continue;
                        }
                        Ownership::Missing => {
                            let _ = shared_coord::record_deployment(
                                skill_name, &agent_id, false, true,
                            );
                            deployed += 1;
                            continue;
                        }
                        Ownership::SkillStarModifiedCopy | Ownership::Foreign => continue,
                    }
                }
            }
            let replace = match ownership::owned_deployment(&target, skill_name) {
                Ownership::Link { .. }
                    if mode == crate::projects::ProjectDeployMode::Symlink
                        && link_already_has_hub_payload(&target, &source) =>
                {
                    let _ = shared_coord::record_deployment(skill_name, &agent_id, false, false);
                    continue;
                }
                // Stale: points at another harness copy or the old hub path.
                Ownership::Link { .. } => true,
                Ownership::Copy
                | Ownership::SkillStarModifiedCopy
                | Ownership::Foreign => continue,
                Ownership::Missing => false,
            };
            if !prepared_target_dir {
                std::fs::create_dir_all(&target_dir).with_context(|| {
                    format!(
                        "Failed to create Agent skills dir '{}'",
                        target_dir.display()
                    )
                })?;
                prepared_target_dir = true;
            }

            let agent_mode = match mode {
                crate::projects::ProjectDeployMode::Symlink => AgentDeployMode::Symlink,
                crate::projects::ProjectDeployMode::Copy => AgentDeployMode::Copy,
            };
            let result: Result<bool> = match (mode, replace) {
                (crate::projects::ProjectDeployMode::Symlink, true) => {
                    let mut was_copy = false;
                    swap_in(&target, |staging| {
                        was_copy = ownership::deploy_for_agent(
                            &source,
                            staging,
                            skill_name,
                            AgentDeployMode::Symlink,
                            disallow_junction,
                        )?;
                        Ok(())
                    })
                    .map(|()| was_copy)
                }
                (crate::projects::ProjectDeployMode::Copy, true) => {
                    let mut staged = materialize::StagedReplace::stage(&target, |staging| {
                        ownership::deploy_copy(&source, staging, skill_name)
                    })?;
                    staged.swap()?;
                    if let Some(old_copy) = staged.backup_path() {
                        let _ = crate::runtime_ignore::carry_runtime_artifacts(old_copy, &target);
                    }
                    staged.commit();
                    Ok(true)
                }
                (_, false) => ownership::deploy_for_agent(
                    &source,
                    &target,
                    skill_name,
                    agent_mode,
                    disallow_junction,
                ),
            };
            match result {
                Ok(was_copy) => {
                    let _ = shared_coord::record_deployment(skill_name, &agent_id, was_copy, false);
                    deployed += 1;
                }
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

/// Outcome of [`resync_existing_links`]: which agents were refreshed and
/// which failed (per-agent, formatted as "Display Name: error").
#[derive(Debug, Clone, Default)]
pub struct ResyncReport {
    pub linked_to: Vec<String>,
    pub failures: Vec<String>,
}

/// Replace the owned entry at `target` with whatever `fill` builds, never
/// destroying the existing entry unless the replacement already materialized.
fn swap_in(target: &Path, fill: impl FnOnce(&Path) -> Result<()>) -> Result<()> {
    let mut staged = materialize::StagedReplace::stage(target, fill)?;
    staged.swap()?;
    staged.commit();
    Ok(())
}

/// Replace the deployment at `target` with a fresh link (or copy fallback),
/// staged beside it so a failed re-create leaves the existing entry working.
/// Returns `true` when the fresh deploy is a directory copy.
pub(crate) fn swap_in_fresh_deploy(skill_path: &Path, target: &Path, name: &str) -> Result<bool> {
    let mut was_copy = false;
    swap_in(target, |staging| {
        was_copy = ownership::deploy_link_or_copy(skill_path, staging, name)?;
        Ok(())
    })
    .with_context(|| format!("Failed to refresh deploy at '{}'", target.display()))?;
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
    let _transaction = crate::skill_update::acquire_update_transaction_lock()?;
    resync_existing_links_locked(skill_name)
}

/// Inner implementation of [`resync_existing_links`] for callers already
/// holding the update-transaction lock.
pub(crate) fn resync_existing_links_locked(skill_name: &str) -> Result<ResyncReport> {
    crate::content::validate_skill_name(skill_name)?;
    let hub_dir = ss_core::infra::paths::hub_skills_dir();
    let skill_path = hub_dir.join(skill_name);
    if !skill_path.exists() {
        anyhow::bail!("Skill '{}' not found in hub", skill_name);
    }

    let profiles = cached_profiles();
    let mut report = ResyncReport::default();
    let mut seen_dirs = HashSet::new();

    for profile in profiles.iter() {
        if !profile.has_global_skills()
            || ownership::targets_canonical_root(&profile.global_skills_dir)
            || !seen_dirs.insert(canonical_path(&profile.global_skills_dir))
        {
            continue;
        }
        let target = profile.global_skills_dir.join(skill_name);
        // Only refresh existing deployments SkillStar owns (preserves the
        // user's assignment and never touches locally modified copies or own folders).
        if !ownership::owned_deployment(&target, skill_name).is_owned() {
            continue;
        }

        match shared_coord::swap_in_deployment_for_agent(
            &skill_path,
            &target,
            skill_name,
            &profile.id,
        ) {
            Ok(was_copy) => {
                if was_copy && effective_deploy_mode(&profile.id) == AgentDeployMode::Symlink {
                    tracing::info!(
                        target: "sync",
                        skill = %skill_name,
                        agent = %profile.id,
                        "✓ resynced via copy fallback (symlink unavailable)"
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
