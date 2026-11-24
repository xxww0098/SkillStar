//! Explicit recovery for locally modified agent copies (`SkillStarModifiedCopy`),
//! unmanaged same-name agent directories/links (`Ownership::Foreign`),
//! and deploy-mode change reconciliation.

use std::collections::HashSet;

use anyhow::{Context, Result, bail};
use serde::Serialize;

use super::ownership::{self, Ownership};
use super::{batch, cached_profiles, mirror, shared_coord};
use crate::agents::{
    AgentDeployMode, AgentProfile, agent_disallows_junction, agent_reads_shared_agents_skills,
    compatible_profile_id, effective_deploy_mode, find_profile, uses_shared_agents_skills_root,
};
use crate::backup::{self, BackupEntry, BackupReason};
use crate::materialize;
use ss_core::infra::{fs_ops, paths};

/// Metadata describing an agent copy deployment whose files diverge from its
/// recorded `.skillstar-deploy.json` baseline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModifiedAgentCopyInfo {
    pub skill_name: String,
    pub agent_id: String,
    pub agent_name: String,
    pub path: String,
}

/// Metadata describing an unmanaged directory or foreign symlink occupying an
/// enabled Agent's `<global_skills_dir>/<skill_name>` slot (`Ownership::Foreign`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UnmanagedAgentEntryInfo {
    pub skill_name: String,
    pub agent_id: String,
    pub agent_name: String,
    pub path: String,
    pub is_symlink: bool,
    pub can_adopt_to_hub: bool,
}

/// Summary of `reconcile_deployments_for_mode_change`.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ReconcileModeReport {
    pub updated: usize,
    pub skipped_modified: Vec<ModifiedAgentCopyInfo>,
    pub skipped_foreign: Vec<String>,
    pub failures: Vec<String>,
}

fn require_non_canonical_agent<'a>(
    profiles: &'a [AgentProfile],
    agent_id: &str,
) -> Result<&'a AgentProfile> {
    let profile = find_profile(profiles, agent_id)?;
    if !profile.has_global_skills() {
        bail!("Agent '{}' does not support global skills", agent_id);
    }
    if ownership::targets_canonical_root(&profile.global_skills_dir) {
        bail!(
            "Agent '{}' reads the canonical skills root directly",
            agent_id
        );
    }
    Ok(profile)
}

fn resolve_owner_profile<'a>(
    dir_profiles: &[&'a AgentProfile],
    skill_name: &str,
    lib: &crate::library::SkillLibrary,
) -> Option<&'a AgentProfile> {
    if let Some(entry) = lib.skills.get(skill_name) {
        for p in dir_profiles {
            if p.enabled && entry.desired_agents.iter().any(|id| id == &p.id) {
                return Some(*p);
            }
        }
        for p in dir_profiles {
            if entry.desired_agents.iter().any(|id| id == &p.id) {
                return Some(*p);
            }
        }
    }
    dir_profiles
        .iter()
        .find(|p| p.enabled)
        .copied()
        .or_else(|| dir_profiles.first().copied())
}

/// Inspect all (or one skill's) agent deployment directories for locally modified
/// `SkillStarModifiedCopy` folders that require explicit user choice before
/// overwrite or unlink.
pub fn inspect_modified_agent_copies(skill_name: Option<&str>) -> Vec<ModifiedAgentCopyInfo> {
    if let Some(name) = skill_name
        && crate::content::validate_skill_name(name).is_err()
    {
        return Vec::new();
    }

    let profiles = cached_profiles();
    let lib = crate::library::load();
    let mut by_dir: std::collections::BTreeMap<std::path::PathBuf, Vec<&AgentProfile>> =
        std::collections::BTreeMap::new();
    for profile in &profiles {
        if !profile.has_global_skills()
            || ownership::targets_canonical_root(&profile.global_skills_dir)
        {
            continue;
        }
        let key = fs_ops::canonicalize_existing_prefix(&profile.global_skills_dir);
        by_dir.entry(key).or_default().push(profile);
    }

    let mut out = Vec::new();
    let mut seen_targets = HashSet::new();
    for (dir_key, group) in by_dir {
        if !group.iter().any(|p| p.enabled)
            && !group.iter().any(|p| uses_shared_agents_skills_root(p))
        {
            continue;
        }
        let Some(first) = group.first().copied() else {
            continue;
        };
        let skills_dir = &first.global_skills_dir;
        if !skills_dir.is_dir() {
            continue;
        }
        let candidate_names: Vec<String> = match skill_name {
            Some(name) => vec![name.to_string()],
            None => std::fs::read_dir(skills_dir)
                .ok()
                .into_iter()
                .flatten()
                .flatten()
                .filter_map(|e| e.file_name().to_str().map(ToString::to_string))
                .filter(|n| !n.starts_with('.'))
                .collect(),
        };

        for name in candidate_names {
            let target = skills_dir.join(&name);
            let target_key = dir_key.join(&name);
            if !seen_targets.insert(target_key) {
                continue;
            }
            if ownership::owned_deployment(&target, &name) == Ownership::SkillStarModifiedCopy {
                let owner = resolve_owner_profile(&group, &name, &lib).unwrap_or(first);
                out.push(ModifiedAgentCopyInfo {
                    skill_name: name,
                    agent_id: owner.id.clone(),
                    agent_name: owner.display_name.clone(),
                    path: target.display().to_string(),
                });
            }
        }
    }
    out
}

/// Inspect enabled agent deployment directories for unmanaged same-name entries
/// (`Ownership::Foreign`) colliding with an installed Hub skill.
pub fn inspect_unmanaged_agent_entries(skill_name: Option<&str>) -> Vec<UnmanagedAgentEntryInfo> {
    if let Some(name) = skill_name
        && crate::content::validate_skill_name(name).is_err()
    {
        return Vec::new();
    }

    let hub_dir = paths::hub_skills_dir();
    if let Some(name) = skill_name
        && !hub_dir.join(name).is_dir()
    {
        return Vec::new();
    }

    let installed_set: Option<HashSet<String>> = skill_name
        .is_none()
        .then(|| crate::installer::installed_names().into_iter().collect());

    let profiles = cached_profiles();
    let lib = crate::library::load();
    let mut by_dir: std::collections::BTreeMap<std::path::PathBuf, Vec<&AgentProfile>> =
        std::collections::BTreeMap::new();
    for profile in &profiles {
        if !profile.has_global_skills()
            || ownership::targets_canonical_root(&profile.global_skills_dir)
        {
            continue;
        }
        let key = fs_ops::canonicalize_existing_prefix(&profile.global_skills_dir);
        by_dir.entry(key).or_default().push(profile);
    }

    let mut out = Vec::new();
    let mut seen_targets = HashSet::new();
    for (dir_key, group) in by_dir {
        if !group.iter().any(|p| p.enabled) {
            continue;
        }
        let Some(first) = group.first().copied() else {
            continue;
        };
        let skills_dir = &first.global_skills_dir;
        if !skills_dir.is_dir() {
            continue;
        }
        let candidate_names: Vec<String> = match skill_name {
            Some(name) => vec![name.to_string()],
            None => std::fs::read_dir(skills_dir)
                .ok()
                .into_iter()
                .flatten()
                .flatten()
                .filter_map(|e| e.file_name().to_str().map(ToString::to_string))
                .filter(|n| {
                    !n.starts_with('.')
                        && installed_set
                            .as_ref()
                            .is_none_or(|installed| installed.contains(n))
                })
                .collect(),
        };

        for name in candidate_names {
            let target = skills_dir.join(&name);
            let target_key = dir_key.join(&name);
            if !seen_targets.insert(target_key) {
                continue;
            }
            if ownership::owned_deployment(&target, &name) == Ownership::Foreign {
                let owner = resolve_owner_profile(&group, &name, &lib).unwrap_or(first);
                let is_symlink = fs_ops::is_link(&target);
                let can_adopt_to_hub = !is_symlink
                    && target.is_dir()
                    && crate::validation::ensure_installable(&target).is_ok()
                    && crate::skill_mutation::policy()
                        .ensure_skill_mutation_allowed(&name)
                        .is_ok();
                out.push(UnmanagedAgentEntryInfo {
                    skill_name: name,
                    agent_id: owner.id.clone(),
                    agent_name: owner.display_name.clone(),
                    path: target.display().to_string(),
                    is_symlink,
                    can_adopt_to_hub,
                });
            }
        }
    }
    out
}

/// Absorb edits from an agent's locally modified copy (`SkillStarModifiedCopy`)
/// back into the canonical library (`installed/<skill_name>`), archiving the
/// previous canonical copy to `data/skills/backups/`, then re-syncing deployments.
pub fn take_agent_copy_edits(skill_name: &str, agent_id: &str) -> Result<()> {
    crate::content::validate_skill_name(skill_name)?;
    let _transaction = crate::skill_update::acquire_update_transaction_lock()?;
    crate::skill_mutation::policy().ensure_skill_mutation_allowed(skill_name)?;

    let profiles = cached_profiles();
    let profile = require_non_canonical_agent(&profiles, agent_id)?;
    let target = profile.global_skills_dir.join(skill_name);
    if ownership::owned_deployment(&target, skill_name) != Ownership::SkillStarModifiedCopy {
        bail!(
            "Target '{}' is not a locally modified SkillStar copy deployment",
            target.display()
        );
    }
    if !target.join("SKILL.md").is_file() {
        bail!(
            "Modified copy '{}' is missing SKILL.md and cannot be absorbed",
            target.display()
        );
    }

    absorb_agent_dir_into_hub(skill_name, profile, &profiles, &target)
}

/// Archive an agent's locally modified copy (`SkillStarModifiedCopy`) into
/// `data/skills/backups/`, then overwrite it cleanly from the canonical library.
pub fn overwrite_agent_copy_with_backup(skill_name: &str, agent_id: &str) -> Result<BackupEntry> {
    crate::content::validate_skill_name(skill_name)?;
    let _transaction = crate::skill_update::acquire_update_transaction_lock()?;

    let profiles = cached_profiles();
    let profile = require_non_canonical_agent(&profiles, agent_id)?;
    let target = profile.global_skills_dir.join(skill_name);
    if ownership::owned_deployment(&target, skill_name) != Ownership::SkillStarModifiedCopy {
        bail!(
            "Target '{}' is not a locally modified SkillStar copy deployment",
            target.display()
        );
    }

    let canon = paths::hub_skills_dir().join(skill_name);
    if !canon.is_dir() {
        bail!("Skill '{}' not found in library", skill_name);
    }

    let backup_entry = backup::archive_skill(skill_name, &target, BackupReason::CopyConflict)
        .context("Failed to backup modified agent copy before overwrite")?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Modified agent copy '{}' disappeared before backup",
                target.display()
            )
        })?;

    let was_copy =
        shared_coord::swap_in_deployment_for_agent(&canon, &target, skill_name, &profile.id)?;
    mirror::sync(&profile.id, &profile.global_skills_dir);
    shared_coord::record_deployment(skill_name, &profile.id, was_copy, false)?;
    crate::installed_skill::invalidate_cache();
    Ok(backup_entry)
}

/// Archive an unmanaged real directory (if not a symlink) into `data/skills/backups/`,
/// replace the conflicting entry (and any conflicted non-reserved mirrors) with
/// SkillStar's managed symlink/copy deployment, and record ownership.
pub fn take_over_unmanaged_agent_entry(
    skill_name: &str,
    agent_id: &str,
) -> Result<Option<BackupEntry>> {
    crate::content::validate_skill_name(skill_name)?;
    let _transaction = crate::skill_update::acquire_update_transaction_lock()?;

    let profiles = cached_profiles();
    let profile = require_non_canonical_agent(&profiles, agent_id)?;
    let canon = paths::hub_skills_dir().join(skill_name);
    if !canon.is_dir() {
        bail!("Skill '{}' not found in library", skill_name);
    }

    let target = profile.global_skills_dir.join(skill_name);
    let current = ownership::owned_deployment(&target, skill_name);
    if !matches!(current, Ownership::Foreign | Ownership::SkillStarModifiedCopy) {
        bail!(
            "Target '{}' is not an unmanaged or conflicted entry",
            target.display()
        );
    }

    let mut backup_entry = if !fs_ops::is_link(&target) && target.is_dir() {
        backup::archive_skill(skill_name, &target, BackupReason::CopyConflict)
            .context("Failed to backup unmanaged agent directory before takeover")?
    } else {
        None
    };

    replace_agent_target_with_managed(
        skill_name,
        &canon,
        &target,
        profile,
        &profiles,
        &mut backup_entry,
    )?;

    crate::installed_skill::invalidate_cache();
    Ok(backup_entry)
}

/// Absorb an unmanaged real skill directory from `agent_id` into the canonical
/// Hub (`installed/<skill_name>`), archiving the previous canonical copy into
/// `data/skills/backups/`, then switch `agent_id` and existing linked agents
/// to the absorbed version.
pub fn adopt_unmanaged_agent_entry(skill_name: &str, agent_id: &str) -> Result<()> {
    crate::content::validate_skill_name(skill_name)?;
    let _transaction = crate::skill_update::acquire_update_transaction_lock()?;
    crate::skill_mutation::policy().ensure_skill_mutation_allowed(skill_name)?;

    let profiles = cached_profiles();
    let profile = require_non_canonical_agent(&profiles, agent_id)?;
    let target = profile.global_skills_dir.join(skill_name);
    if ownership::owned_deployment(&target, skill_name) != Ownership::Foreign {
        bail!(
            "Target '{}' is not an unmanaged agent entry",
            target.display()
        );
    }
    if fs_ops::is_link(&target) || !target.is_dir() {
        bail!(
            "Target '{}' is not a real directory and cannot be absorbed",
            target.display()
        );
    }
    crate::validation::ensure_installable(&target).map_err(|reason| {
        anyhow::anyhow!(
            "Unmanaged directory '{}' cannot be absorbed: {reason}",
            target.display()
        )
    })?;

    absorb_agent_dir_into_hub(skill_name, profile, &profiles, &target)
}

fn absorb_agent_dir_into_hub(
    skill_name: &str,
    profile: &AgentProfile,
    profiles: &[AgentProfile],
    target: &std::path::Path,
) -> Result<()> {
    let canon = paths::hub_skills_dir().join(skill_name);
    if canon.is_dir() {
        backup::archive_skill(skill_name, &canon, BackupReason::CopyConflict)
            .context("Failed to archive canonical skill before absorbing agent directory")?;
    }

    std::fs::create_dir_all(paths::hub_skills_dir())?;
    let mut staged = materialize::StagedReplace::stage(&canon, |staging| {
        ownership::copy_clean_skill_tree(target, staging)?;
        if crate::validation::inspect_skill_frontmatter(staging)
            .name
            .as_deref()
            .and_then(|n| materialize::canonical_skill_name(n).ok())
            .as_deref()
            != Some(skill_name)
        {
            materialize::rewrite_frontmatter_skill_name(staging, skill_name)?;
        }
        Ok(())
    })?;
    staged.swap()?;
    if let Some(old_canon) = staged.backup_path() {
        let _ = crate::runtime_ignore::carry_runtime_artifacts(old_canon, &canon);
    }
    staged.commit();

    crate::install_baseline::record(&[skill_name.to_string()]);

    let mut mirror_backup = None;
    replace_agent_target_with_managed(
        skill_name,
        &canon,
        target,
        profile,
        profiles,
        &mut mirror_backup,
    )?;

    // Refresh any other owned agent deployments without re-acquiring the transaction lock.
    let _ = batch::resync_existing_links_locked(skill_name);
    crate::installed_skill::invalidate_cache();
    Ok(())
}

fn replace_agent_target_with_managed(
    skill_name: &str,
    canon: &std::path::Path,
    target: &std::path::Path,
    profile: &AgentProfile,
    profiles: &[AgentProfile],
    mirror_backup: &mut Option<BackupEntry>,
) -> Result<()> {
    let lib = crate::library::load();
    if agent_reads_shared_agents_skills(&profile.id)
        && shared_coord::shared_agents_entry_is_active(skill_name, &lib, profiles)
    {
        materialize::remove_entry(target)?;
        clear_mirrors_best_effort(&profile.id, &profile.global_skills_dir, skill_name, mirror_backup);
        mirror::sync(&profile.id, &profile.global_skills_dir);
        shared_coord::record_deployment(skill_name, &profile.id, false, true)?;
        return Ok(());
    }

    let was_copy =
        shared_coord::swap_in_deployment_for_agent(canon, target, skill_name, &profile.id)?;
    clear_mirrors_best_effort(&profile.id, &profile.global_skills_dir, skill_name, mirror_backup);
    mirror::sync(&profile.id, &profile.global_skills_dir);
    shared_coord::record_deployment(skill_name, &profile.id, was_copy, false)?;
    if uses_shared_agents_skills_root(profile) {
        shared_coord::suppress_secondary_agents_if_shared(skill_name, profiles)?;
    }
    Ok(())
}

fn clear_mirrors_best_effort(
    agent_id: &str,
    global_skills_dir: &std::path::Path,
    skill_name: &str,
    mirror_backup: &mut Option<BackupEntry>,
) {
    if let Err(err) = mirror::clear_foreign_skill_in_mirrors(
        agent_id,
        global_skills_dir,
        skill_name,
        mirror_backup,
    ) {
        tracing::warn!(
            target: "sync",
            agent = %agent_id,
            skill = %skill_name,
            error = %err,
            "Failed to clear foreign skill in mirror directories"
        );
    }
}

/// Reconcile existing owned deployments when default or per-agent deploy modes
/// change (`Symlink` <-> `Copy`). Preserves `SkillStarModifiedCopy` and `Foreign`.
pub fn reconcile_deployments_for_mode_change(
    agent_id: Option<&str>,
) -> Result<ReconcileModeReport> {
    let _transaction = crate::skill_update::acquire_update_transaction_lock()?;
    let profiles = cached_profiles();
    let filter_id = agent_id.map(compatible_profile_id);
    let hub_dir = paths::hub_skills_dir();
    let mut report = ReconcileModeReport::default();

    let mut targets: Vec<&AgentProfile> = Vec::new();
    for p in &profiles {
        if let Some(fid) = filter_id {
            if p.id != fid {
                continue;
            }
        } else if !p.enabled {
            continue;
        }
        if !p.has_global_skills() || ownership::targets_canonical_root(&p.global_skills_dir) {
            continue;
        }
        targets.push(p);
    }

    let mut seen_paths = HashSet::new();
    for profile in targets {
        let dir = &profile.global_skills_dir;
        if !dir.is_dir() {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        let desired_mode = effective_deploy_mode(&profile.id);
        let disallow_junction = agent_disallows_junction(&profile.id);
        let dir_canon = fs_ops::canonicalize_existing_prefix(dir);

        for entry in entries.flatten() {
            let Some(name) = entry.file_name().to_str().map(ToString::to_string) else {
                continue;
            };
            if name.starts_with('.') {
                continue;
            }
            let target = entry.path();
            if !seen_paths.insert(dir_canon.join(&name)) {
                continue;
            }
            let canon = hub_dir.join(&name);
            if !canon.is_dir() {
                continue;
            }
            let ownership = ownership::owned_deployment(&target, &name);
            match ownership {
                Ownership::SkillStarModifiedCopy => {
                    report.skipped_modified.push(ModifiedAgentCopyInfo {
                        skill_name: name,
                        agent_id: profile.id.clone(),
                        agent_name: profile.display_name.clone(),
                        path: target.display().to_string(),
                    });
                }
                Ownership::Foreign => {
                    report.skipped_foreign.push(target.display().to_string());
                }
                Ownership::Link { alive } => {
                    if desired_mode == AgentDeployMode::Copy || !alive {
                        match shared_coord::swap_in_deployment_for_agent(
                            &canon,
                            &target,
                            &name,
                            &profile.id,
                        ) {
                            Ok(was_copy) => {
                                let _ = shared_coord::record_deployment(
                                    &name,
                                    &profile.id,
                                    was_copy,
                                    false,
                                );
                                report.updated += 1;
                            }
                            Err(err) => {
                                report.failures.push(format!("{}/{}: {err:#}", profile.id, name))
                            }
                        }
                    }
                }
                Ownership::Copy => {
                    if desired_mode == AgentDeployMode::Symlink {
                        match shared_coord::swap_in_deployment_for_agent(
                            &canon,
                            &target,
                            &name,
                            &profile.id,
                        ) {
                            Ok(was_copy) => {
                                let _ = shared_coord::record_deployment(
                                    &name,
                                    &profile.id,
                                    was_copy,
                                    false,
                                );
                                if !was_copy || disallow_junction {
                                    report.updated += 1;
                                }
                            }
                            Err(err) => {
                                report.failures.push(format!("{}/{}: {err:#}", profile.id, name))
                            }
                        }
                    }
                }
                Ownership::Missing => {}
            }
        }
        mirror::sync(&profile.id, dir);
    }

    crate::installed_skill::invalidate_cache();
    Ok(report)
}
