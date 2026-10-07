//! Many-skill deployments and the post-update resync.

use std::collections::HashSet;
use std::path::Path;

use anyhow::{Context, Result};

use super::ownership::{self, Ownership};
use super::{cached_profiles, mirror, require_enabled_global_profile};
use crate::agents as agent_profile;
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

/// Batch-link a list of skills to a specific agent.
///
/// Skips skills that are already linked, and entries SkillStar does not own.
/// Returns the number of new links created.
pub fn batch_link_skills_to_agent(skill_names: &[String], agent_id: &str) -> Result<u32> {
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
        return Ok(0);
    }

    let mut linked = 0u32;
    let mut skipped = 0u32;
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
            skipped += 1;
            continue;
        }

        match ownership::owned_deployment(&target, name) {
            Ownership::Link { .. } if link_already_has_hub_payload(&target, &skill_path) => {
                tracing::debug!(target: "sync", skill = %name, target = %target.display(), "· already linked — skipping");
                continue;
            }
            Ownership::Link { .. } | Ownership::Copy => {
                match swap_in_fresh_deploy(&skill_path, &target, name) {
                    Ok(_) => linked += 1,
                    Err(err) => failures.push(format!("{name}: {err:#}")),
                }
                continue;
            }
            Ownership::Foreign => {
                tracing::warn!(
                    target: "sync",
                    skill = %name,
                    target = %target.display(),
                    "Entry SkillStar does not own exists at target — skipping"
                );
                skipped += 1;
                continue;
            }
            Ownership::Missing => {}
        }

        if !target_dir.exists() {
            std::fs::create_dir_all(target_dir)?;
            created_target_dir = true;
        }

        match ownership::deploy_link_or_copy(&skill_path, &target, name) {
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
                    "✓ skill linked"
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
        "✓ batch_link_skills_to_agent completed"
    );

    Ok(linked)
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

    let mut deployed = 0u32;
    let mut failures = Vec::new();
    for (agent_id, target_dir) in target_dirs {
        let mut prepared_target_dir = false;
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
            let replace = match ownership::owned_deployment(&target, skill_name) {
                Ownership::Link { .. } if link_already_has_hub_payload(&target, &source) => {
                    continue;
                }
                // Stale: points at another harness copy or the old hub path.
                Ownership::Link { .. } => true,
                Ownership::Copy | Ownership::Foreign => continue,
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

            let result = match (mode, replace) {
                (crate::projects::ProjectDeployMode::Symlink, true) => {
                    swap_in_fresh_deploy(&source, &target, skill_name).map(|_| ())
                }
                (crate::projects::ProjectDeployMode::Copy, true) => swap_in(&target, |staging| {
                    ownership::deploy_copy(&source, staging, skill_name)
                }),
                (crate::projects::ProjectDeployMode::Symlink, false) => {
                    ownership::deploy_link_or_copy(&source, &target, skill_name).map(|_| ())
                }
                (crate::projects::ProjectDeployMode::Copy, false) => {
                    ownership::deploy_copy(&source, &target, skill_name)
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
        // user's assignment and never touches their own folders).
        if !ownership::owned_deployment(&target, skill_name).is_owned() {
            continue;
        }

        match swap_in_fresh_deploy(&skill_path, &target, skill_name) {
            Ok(was_copy) => {
                if was_copy {
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
