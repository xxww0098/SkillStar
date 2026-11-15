//! Coordination for agents sharing `~/.agents/skills` and anti-double-loading
//! suppression for agents that natively read `~/.agents/skills` in addition to
//! their own private skills folder.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::Result;
use tracing::info;

use super::ownership::{self, Ownership, Removal};
use crate::agents::{
    self, AgentProfile, agent_disallows_junction, agent_reads_shared_agents_skills,
    effective_deploy_mode, shared_agents_skills_dir, uses_shared_agents_skills_root,
};
use crate::library::{self, AppliedTargetRecord, SkillLibrary};
use crate::materialize;

fn is_active_shared_disk_state(ownership: Ownership) -> bool {
    matches!(
        ownership,
        Ownership::Link { alive: true } | Ownership::Copy | Ownership::SkillStarModifiedCopy
    )
}

fn any_shared_root_desirer_in(
    lib: &SkillLibrary,
    skill_name: &str,
    profiles: &[AgentProfile],
    exclude_agent_id: Option<&str>,
    require_enabled_or_non_shared_applied: bool,
) -> bool {
    let Some(entry) = lib.skills.get(skill_name) else {
        return false;
    };
    entry.desired_agents.iter().any(|id| {
        if exclude_agent_id.is_some_and(|ex| ex == id) {
            return false;
        }
        let Some(profile) = profiles.iter().find(|p| p.id == *id) else {
            return false;
        };
        if !uses_shared_agents_skills_root(profile) {
            return false;
        }
        if !require_enabled_or_non_shared_applied {
            return true;
        }
        profile.enabled
            || entry
                .applied
                .get(id)
                .is_some_and(|rec| rec.mode != "shared")
    })
}

/// Whether `~/.agents/skills/<skill_name>` currently satisfies shared discovery.
pub(crate) fn shared_agents_entry_is_active(
    skill_name: &str,
    lib: &SkillLibrary,
    profiles: &[AgentProfile],
) -> bool {
    let shared_target = shared_agents_skills_dir().join(skill_name);
    if !is_active_shared_disk_state(ownership::owned_deployment(&shared_target, skill_name)) {
        return false;
    }
    any_shared_root_desirer_in(lib, skill_name, profiles, None, true)
}

/// Whether a profile with its own private skills dir (`cursor`, `gemini-cli`,
/// `droid`, `opencode`) has this skill satisfied via `~/.agents/skills/<skill>`.
pub(crate) fn is_satisfied_by_shared_agents_dir_in(
    profile: &AgentProfile,
    skill_name: &str,
    lib: &SkillLibrary,
    profiles: &[AgentProfile],
) -> bool {
    if !profile.enabled || !agent_reads_shared_agents_skills(&profile.id) {
        return false;
    }
    let Some(entry) = lib.skills.get(skill_name) else {
        return false;
    };
    if !entry.desired_agents.iter().any(|id| id == &profile.id) {
        return false;
    }
    shared_agents_entry_is_active(skill_name, lib, profiles)
}

/// Public check (reloads profiles & library once) for status inspection.
pub(crate) fn is_satisfied_by_shared_agents_dir(
    profile: &AgentProfile,
    skill_name: &str,
) -> bool {
    if !agent_reads_shared_agents_skills(&profile.id) {
        return false;
    }
    let profiles = super::cached_profiles();
    let lib = library::load();
    is_satisfied_by_shared_agents_dir_in(profile, skill_name, &lib, &profiles)
}

/// Whether `skill_name` is active for `profile` (either on disk or shared).
pub(crate) fn is_skill_visible_for_profile(
    profile: &AgentProfile,
    skill_name: &str,
    lib: &SkillLibrary,
    profiles: &[AgentProfile],
) -> bool {
    if !profile.has_global_skills() {
        return false;
    }
    let target = profile.global_skills_dir.join(skill_name);
    let owned = ownership::owned_deployment(&target, skill_name);
    let physical_active = is_active_shared_disk_state(owned);

    if uses_shared_agents_skills_root(profile) {
        let tracked_shared = any_shared_root_desirer_in(lib, skill_name, profiles, None, false);
        if tracked_shared {
            return physical_active
                && lib
                    .skills
                    .get(skill_name)
                    .is_some_and(|e| e.desired_agents.iter().any(|id| id == &profile.id));
        }
        return physical_active;
    }

    physical_active || is_satisfied_by_shared_agents_dir_in(profile, skill_name, lib, profiles)
}

/// Record desired + applied state after deploying `skill_name` for `agent_id`.
pub(crate) fn record_deployment(
    skill_name: &str,
    agent_id: &str,
    was_copy: bool,
    shared_suppressed: bool,
) -> Result<()> {
    let mode = if shared_suppressed {
        "shared"
    } else if was_copy {
        "copy"
    } else {
        "symlink"
    };
    let now = chrono::Utc::now().to_rfc3339();
    library::mutate(|lib| {
        let entry = lib.skills.entry(skill_name.to_string()).or_default();
        if entry.installed_at.is_empty() {
            entry.installed_at = now.clone();
        }
        if !entry.desired_agents.iter().any(|id| id == agent_id) {
            entry.desired_agents.push(agent_id.to_string());
            entry.desired_agents.sort();
        }
        entry.applied.insert(
            agent_id.to_string(),
            AppliedTargetRecord {
                mode: mode.to_string(),
                content_hash: None,
                tree_max_mtime_ms: None,
                file_count: None,
                updated_at: now,
            },
        );
        entry.updated_at = chrono::Utc::now().to_rfc3339();
    })
}

/// Drop `agent_id` from `desired_agents` and `applied` for `skill_name`.
pub(crate) fn clear_agent_tracking(skill_name: &str, agent_id: &str) -> Result<()> {
    library::mutate(|lib| {
        if let Some(entry) = lib.skills.get_mut(skill_name) {
            entry.desired_agents.retain(|id| id != agent_id);
            entry.applied.remove(agent_id);
            entry.updated_at = chrono::Utc::now().to_rfc3339();
        }
    })
}

/// Drop all `desired_agents` and `applied` entries for `skill_name`.
pub(crate) fn clear_skill_tracking(skill_name: &str) -> Result<()> {
    library::mutate(|lib| {
        if let Some(entry) = lib.skills.get_mut(skill_name) {
            entry.desired_agents.clear();
            entry.applied.clear();
            entry.updated_at = chrono::Utc::now().to_rfc3339();
        }
    })
}

/// If `skill_name` is actively shared via `~/.agents/skills/<skill_name>`,
/// remove any owned private duplicate for enabled secondary agents that also
/// desire it (so Cursor / Gemini CLI / Droid / OpenCode don't load it twice).
pub(crate) fn suppress_secondary_agents_if_shared(
    skill_name: &str,
    profiles: &[AgentProfile],
) -> Result<()> {
    let lib = library::load();
    if !shared_agents_entry_is_active(skill_name, &lib, profiles) {
        return Ok(());
    }
    let Some(entry) = lib.skills.get(skill_name) else {
        return Ok(());
    };
    let secondary_ids: Vec<String> = entry
        .desired_agents
        .iter()
        .filter(|id| agent_reads_shared_agents_skills(id))
        .cloned()
        .collect();

    for sec_id in secondary_ids {
        let Some(sec_profile) = profiles.iter().find(|p| p.id == sec_id) else {
            continue;
        };
        if !sec_profile.enabled || !sec_profile.has_global_skills() {
            continue;
        }
        let target = sec_profile.global_skills_dir.join(skill_name);
        match ownership::owned_deployment(&target, skill_name) {
            Ownership::Link { .. } | Ownership::Copy => {
                if ownership::remove_owned(&target, skill_name)? == Removal::Removed {
                    info!(
                        target: "sync",
                        skill = %skill_name,
                        agent = %sec_id,
                        path = %target.display(),
                        "suppressed private deployment because skill is shared via ~/.agents/skills"
                    );
                }
                super::mirror::sync(&sec_profile.id, &sec_profile.global_skills_dir);
                record_deployment(skill_name, &sec_id, false, true)?;
            }
            Ownership::Missing => {
                record_deployment(skill_name, &sec_id, false, true)?;
            }
            Ownership::Foreign | Ownership::SkillStarModifiedCopy => {}
        }
    }
    Ok(())
}

/// Remove `skill_name` for `profile` with `~/.agents/skills` ref-counting and
/// private re-materialization for secondary agents when shared leaves disk.
pub(crate) fn remove_target_with_coordination(
    skill_name: &str,
    profile: &AgentProfile,
    profiles: &[AgentProfile],
) -> Result<Removal> {
    let is_shared_root = uses_shared_agents_skills_root(profile);
    clear_agent_tracking(skill_name, &profile.id)?;

    if is_shared_root {
        let lib = library::load();
        if any_shared_root_desirer_in(&lib, skill_name, profiles, Some(&profile.id), true) {
            info!(
                target: "sync",
                skill = %skill_name,
                agent = %profile.id,
                "kept shared ~/.agents/skills deployment for other desired shared-root agents"
            );
            return Ok(Removal::Removed);
        }
    }

    let target = profile.global_skills_dir.join(skill_name);
    let removal = ownership::remove_owned(&target, skill_name)?;
    super::mirror::sync(&profile.id, &profile.global_skills_dir);

    if is_shared_root && removal != Removal::Foreign {
        materialize_secondary_agents_if_unshared(skill_name, profiles)?;
    }

    Ok(removal)
}

/// Re-materialize private links/copies for enabled secondary agents that
/// still desire `skill_name` once `~/.agents/skills/<skill>` is no longer active.
pub(crate) fn materialize_secondary_agents_if_unshared(
    skill_name: &str,
    profiles: &[AgentProfile],
) -> Result<()> {
    let lib = library::load();
    if shared_agents_entry_is_active(skill_name, &lib, profiles) {
        return Ok(());
    }
    let Some(entry) = lib.skills.get(skill_name) else {
        return Ok(());
    };
    let canonical = ss_core::infra::paths::hub_skills_dir().join(skill_name);
    if !canonical.is_dir() {
        return Ok(());
    }
    let secondary_ids: Vec<String> = entry
        .desired_agents
        .iter()
        .filter(|id| agent_reads_shared_agents_skills(id))
        .cloned()
        .collect();

    for sec_id in secondary_ids {
        let Some(sec_profile) = profiles.iter().find(|p| p.id == sec_id) else {
            continue;
        };
        if !sec_profile.enabled || !sec_profile.has_global_skills() {
            continue;
        }
        let target = sec_profile.global_skills_dir.join(skill_name);
        match ownership::owned_deployment(&target, skill_name) {
            Ownership::Missing => {
                std::fs::create_dir_all(&sec_profile.global_skills_dir)?;
                let mode = effective_deploy_mode(&sec_profile.id);
                let disallow = agent_disallows_junction(&sec_profile.id);
                let was_copy = ownership::deploy_for_agent(
                    &canonical,
                    &target,
                    skill_name,
                    mode,
                    disallow,
                )?;
                super::mirror::sync(&sec_profile.id, &sec_profile.global_skills_dir);
                record_deployment(skill_name, &sec_profile.id, was_copy, false)?;
            }
            Ownership::Link { alive: false } => {
                let mut was_copy = false;
                let mut staged = materialize::StagedReplace::stage(&target, |staging| {
                    let mode = effective_deploy_mode(&sec_profile.id);
                    let disallow = agent_disallows_junction(&sec_profile.id);
                    was_copy = ownership::deploy_for_agent(
                        &canonical,
                        staging,
                        skill_name,
                        mode,
                        disallow,
                    )?;
                    Ok(())
                })?;
                staged.swap()?;
                staged.commit();
                super::mirror::sync(&sec_profile.id, &sec_profile.global_skills_dir);
                record_deployment(skill_name, &sec_profile.id, was_copy, false)?;
            }
            Ownership::Link { alive: true }
            | Ownership::Copy
            | Ownership::SkillStarModifiedCopy
            | Ownership::Foreign => {}
        }
    }
    Ok(())
}

/// Reconcile shared/secondary deployments when a profile is enabled/disabled.
pub fn reconcile_shared_agents_on_profile_toggle(agent_id: &str) -> Result<()> {
    let normalized = agents::compatible_profile_id(agent_id);
    let profiles = super::cached_profiles();
    let Some(toggled) = profiles.iter().find(|p| p.id == normalized) else {
        return Ok(());
    };
    if !uses_shared_agents_skills_root(toggled) && !agent_reads_shared_agents_skills(&toggled.id) {
        return Ok(());
    }

    let _transaction = crate::skill_update::acquire_update_transaction_lock()?;
    let lib = library::load();
    let mut skills = BTreeSet::new();
    for (name, entry) in &lib.skills {
        if entry.desired_agents.iter().any(|id| {
            id == &toggled.id
                || agent_reads_shared_agents_skills(id)
                || profiles
                    .iter()
                    .find(|p| p.id == *id)
                    .is_some_and(uses_shared_agents_skills_root)
        }) {
            skills.insert(name.clone());
        }
    }

    for skill in skills {
        suppress_secondary_agents_if_shared(&skill, &profiles)?;
        materialize_secondary_agents_if_unshared(&skill, &profiles)?;
    }
    crate::installed_skill::invalidate_cache();
    Ok(())
}

/// Reconcile mirror deployment links using explicit `effective_deploy_mode` or `disallow_junction`.
pub(crate) fn swap_in_deployment_for_agent(
    skill_path: &Path,
    target: &Path,
    name: &str,
    agent_id: &str,
) -> Result<bool> {
    let mode = effective_deploy_mode(agent_id);
    let disallow = agent_disallows_junction(agent_id);
    let mut was_copy = false;
    let mut staged = materialize::StagedReplace::stage(target, |staging| {
        was_copy = ownership::deploy_for_agent(skill_path, staging, name, mode, disallow)?;
        Ok(())
    })?;
    staged.swap()?;
    if was_copy && let Some(old_copy) = staged.backup_path() {
        let _ = crate::runtime_ignore::carry_runtime_artifacts(old_copy, target);
    }
    staged.commit();
    Ok(was_copy)
}
