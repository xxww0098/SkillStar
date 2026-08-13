//! Turn a reported update stop into an applied decision.
//!
//! Every exit from [`super::SkillUpdateBlocked`] runs through here, and each
//! one has to leave the checkout in a state the ordinary updater can continue
//! from. A local divergence is resolved against the Skill's own subtree so
//! siblings sharing the checkout stay untouched until their turn; a Skill its
//! source dropped is removed instead, and the checkout it shared keeps
//! updating through a Skill that survived.

use anyhow::{Context, Result};
use serde::Serialize;
use skillstar_core::types::Skill;
use std::path::Path;

use crate::git::ops as git_ops;
use crate::{content, local_skill, lockfile, repo_link, skill_install};

use super::divergence::{
    self, LocalDivergenceReason, LocalDivergenceResolution, SkillUpdateBlocked,
};
use super::{UpdateResult, plan};

#[derive(Debug, Clone, Serialize)]
pub struct ResolveSkillUpdateResult {
    pub update: Option<UpdateResult>,
    pub local_copy: Option<Skill>,
    /// Skills removed because their source dropped them. The caller must drop
    /// them from its own view; they are not coming back.
    pub uninstalled: Vec<String>,
    pub remaining_blocked: Vec<SkillUpdateBlocked>,
}

/// Resolve a previously reported update stop and continue through the same
/// update transaction. Preservation finishes before the source tree is
/// allowed to move.
pub fn resolve_skill_update(
    name: &str,
    resolution: LocalDivergenceResolution,
) -> Result<ResolveSkillUpdateResult> {
    resolve_skill_update_in_session(
        name,
        resolution,
        &crate::git::transport::GitOperationSession::public(),
    )
}

pub fn resolve_skill_update_in_session(
    name: &str,
    resolution: LocalDivergenceResolution,
    session: &crate::git::transport::GitOperationSession,
) -> Result<ResolveSkillUpdateResult> {
    let _transaction_guard = super::acquire_update_transaction_lock()?;
    resolve_locked(name, resolution, session, true)
}

/// Resolve local divergence while the caller owns the cross-process update
/// transaction. Channel subscriptions use this without continuing through the
/// ordinary branch/ref updater; their next write must come from the reviewed,
/// immutable channel release commit.
pub fn resolve_skill_local_divergence_locked(
    name: &str,
    resolution: LocalDivergenceResolution,
    session: &crate::git::transport::GitOperationSession,
) -> Result<ResolveSkillUpdateResult> {
    resolve_locked(name, resolution, session, false)
}

fn resolve_locked(
    name: &str,
    resolution: LocalDivergenceResolution,
    session: &crate::git::transport::GitOperationSession,
    continue_with_ordinary_update: bool,
) -> Result<ResolveSkillUpdateResult> {
    content::validate_skill_name(name).map_err(anyhow::Error::from)?;
    if continue_with_ordinary_update {
        super::ensure_ordinary_update_allowed(name)?;
        // A Skill its source dropped has no clean upstream state to restore,
        // so it cannot go through the divergence exits at all. Channel Skills
        // are excluded: removal there belongs to the channel's own reviewed
        // flow, not to the generic updater.
        if let Some(reason) = divergence::removed_source_reason(name) {
            return resolve_removed_source(name, reason, resolution, session);
        }
    }
    resolve_divergence(name, resolution, session, continue_with_ordinary_update)
}

/// Apply the user's decision for a Skill the tracked source no longer ships.
///
/// Both exits end in removal — what differs is whether the content survives as
/// an independent local Skill first. Nothing is restored from Git: the commit
/// that still had this Skill is exactly what the update is trying to leave.
fn resolve_removed_source(
    name: &str,
    reason: LocalDivergenceReason,
    resolution: LocalDivergenceResolution,
    session: &crate::git::transport::GitOperationSession,
) -> Result<ResolveSkillUpdateResult> {
    let hub_path = skillstar_core::infra::paths::hub_skills_dir().join(name);
    let repo_root = repo_link::repo_root_of(&hub_path);

    let local_copy = match resolution {
        LocalDivergenceResolution::Preserve { local_name } => {
            if reason == LocalDivergenceReason::SourceMissing {
                anyhow::bail!(
                    "Skill '{name}' was dropped by its source and its content is already gone; it can only be removed"
                );
            }
            Some(local_skill::preserve_installed_copy(name, &local_name)?)
        }
        LocalDivergenceResolution::Uninstall => None,
        LocalDivergenceResolution::Discard => anyhow::bail!(
            "Skill '{name}' was dropped by its source; discarding local changes cannot bring it back — keep a local copy or remove it"
        ),
    };

    skill_install::uninstall_skill_locked_unchecked(name).map_err(|error| {
        anyhow::anyhow!("failed to remove '{name}' after its source dropped it: {error}")
    })?;

    // The rest of the checkout still wants its update, so hand the pull to a
    // Skill that is still installed from it.
    let Some(successor) = repo_root.as_deref().and_then(surviving_checkout_skill) else {
        return Ok(ResolveSkillUpdateResult {
            update: None,
            local_copy,
            uninstalled: vec![name.to_string()],
            remaining_blocked: Vec::new(),
        });
    };

    let entries = lockfile::Lockfile::load(&lockfile::lockfile_path())?.skills;
    let group = plan::RepoGroup {
        representative: successor.clone(),
        covered: Vec::new(),
    };
    let remaining_blocked = divergence::local_divergences_for_group(&entries, &group);
    if !remaining_blocked.is_empty() {
        return Ok(ResolveSkillUpdateResult {
            update: None,
            local_copy,
            uninstalled: vec![name.to_string()],
            remaining_blocked,
        });
    }

    let (update, remaining_blocked) =
        match super::update_skill_unchecked_locked(&successor, session) {
            Ok(update) => (Some(update), Vec::new()),
            Err(error) => (None, super::take_source_removed_stop(error)?),
        };
    Ok(ResolveSkillUpdateResult {
        update,
        local_copy,
        uninstalled: vec![name.to_string()],
        remaining_blocked,
    })
}

/// Another installed Skill backed by the same checkout, so a resolution that
/// removed the requested one can still pull the repository they shared.
fn surviving_checkout_skill(repo_root: &Path) -> Option<String> {
    let hub = skillstar_core::infra::paths::hub_skills_dir();
    let entries = lockfile::Lockfile::load(&lockfile::lockfile_path())
        .ok()?
        .skills;
    entries.into_iter().map(|entry| entry.name).find(|name| {
        let path = hub.join(name);
        path.exists() && repo_link::repo_root_of(&path).as_deref() == Some(repo_root)
    })
}

fn resolve_divergence(
    name: &str,
    resolution: LocalDivergenceResolution,
    session: &crate::git::transport::GitOperationSession,
    continue_with_ordinary_update: bool,
) -> Result<ResolveSkillUpdateResult> {
    let lock_path = lockfile::lockfile_path();
    let entry = lockfile::Lockfile::load(&lock_path)
        .context("failed to load the Skill lockfile before resolving divergence")?
        .skills
        .into_iter()
        .find(|entry| entry.name == name)
        .map(Ok)
        .unwrap_or_else(|| super::reconstruct_lock_entry(name))?;

    let skill_path = skillstar_core::infra::paths::hub_skills_dir().join(name);
    let repo_location = repo_link::repo_root_of(&skill_path)
        .map(|repo_root| {
            super::actual_repo_pathspec(&skill_path, &repo_root)
                .map(|pathspec| (repo_root, pathspec))
        })
        .transpose()?;
    if repo_location
        .as_ref()
        .is_some_and(|(_, pathspec)| pathspec.is_none())
    {
        let entries = lockfile::Lockfile::load(&lock_path)?.skills;
        let group = plan::RepoGroup {
            representative: name.to_string(),
            covered: Vec::new(),
        };
        let unresolved_nested: Vec<_> = divergence::local_divergences_for_group(&entries, &group)
            .into_iter()
            .filter(|blocked| blocked.name != name)
            .map(|blocked| blocked.name)
            .collect();
        if !unresolved_nested.is_empty() {
            anyhow::bail!(
                "Resolve nested Skill changes before the checkout-root Skill '{}': {}",
                name,
                unresolved_nested.join(", ")
            );
        }
    }

    let local_copy = match resolution {
        LocalDivergenceResolution::Preserve { local_name } => {
            Some(local_skill::preserve_installed_copy(name, &local_name)?)
        }
        LocalDivergenceResolution::Discard => None,
        LocalDivergenceResolution::Uninstall => anyhow::bail!(
            "Skill '{name}' still comes from its source; keep or discard the local changes instead of removing it"
        ),
    };

    let (repo_root, pathspec) = match repo_location {
        Some((repo_root, pathspec)) => (repo_root, pathspec),
        None => (
            git_ops::find_repo_root(&skill_path)
                .with_context(|| format!("Cannot find the managed Git checkout for '{name}'"))?,
            None,
        ),
    };
    git_ops::restore_worktree_to_head_in_session(&repo_root, pathspec.as_deref(), session)
        .with_context(|| format!("failed to discard local divergence for '{name}'"))?;

    {
        let _lock = lockfile::get_mutex()
            .lock()
            .map_err(|_| anyhow::anyhow!("Lockfile mutex poisoned"))?;
        let mut lockfile = lockfile::Lockfile::load(&lock_path)?;
        let baseline = content::snapshot(name)
            .with_context(|| format!("failed to capture clean baseline for '{name}'"))?;
        if let Some(current) = lockfile
            .skills
            .iter_mut()
            .find(|candidate| candidate.name == name)
        {
            current.content_hash = Some(baseline.content_hash);
            current.content_hash_version = Some(content::SNAPSHOT_HASH_VERSION);
        } else {
            let mut reconstructed = entry.clone();
            reconstructed.content_hash = Some(baseline.content_hash);
            reconstructed.content_hash_version = Some(content::SNAPSHOT_HASH_VERSION);
            lockfile.upsert(reconstructed);
        }
        lockfile.save(&lock_path)?;
    }

    let entries = lockfile::Lockfile::load(&lock_path)?.skills;
    let group = plan::RepoGroup {
        representative: name.to_string(),
        covered: Vec::new(),
    };
    let mut remaining_blocked = divergence::local_divergences_for_group(&entries, &group);
    if !continue_with_ordinary_update && pathspec.is_some() {
        remaining_blocked.retain(|blocked| blocked.name == name);
    }
    if !remaining_blocked.is_empty() || !continue_with_ordinary_update {
        return Ok(ResolveSkillUpdateResult {
            update: None,
            local_copy,
            uninstalled: Vec::new(),
            remaining_blocked,
        });
    }

    let (update, remaining_blocked) = match super::update_skill_unchecked_locked(name, session) {
        Ok(update) => (Some(update), Vec::new()),
        Err(error) => (None, super::take_source_removed_stop(error)?),
    };
    Ok(ResolveSkillUpdateResult {
        update,
        local_copy,
        uninstalled: Vec::new(),
        remaining_blocked,
    })
}
