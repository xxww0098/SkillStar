//! Temporary suspension and exact recovery of one Agent skills directory.
//!
//! The disk remains authoritative for what is currently active. The small
//! directory-keyed journal in Agent preferences exists only to remember which
//! managed names this use case removed, so a later restore can never synthesize
//! a Hub-wide deployment set.

use std::{
    collections::BTreeSet,
    sync::{Mutex, OnceLock},
};

use anyhow::Result;
use serde::Serialize;
use skillstar_skills::agents::AgentProfile;
use skillstar_skills::deployment::{self, ToggleSkillOutcome};

/// Current disk state plus the exact names still awaiting restoration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentManagedSkillsState {
    pub active_skill_names: Vec<String>,
    pub suspended_skill_names: Vec<String>,
}

/// Direction applied by one temporary skills operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentManagedSkillsAction {
    Paused,
    Restored,
}

/// An item left in place because the standard deployment toggle refused it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentManagedSkillsSkip {
    pub skill_name: String,
    pub code: String,
    pub path: String,
    pub reason: String,
}

/// An item whose standard deployment toggle returned an error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentManagedSkillsFailure {
    pub skill_name: String,
    pub error: String,
}

/// Structured result of pausing or restoring an exact directory snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentManagedSkillsToggleReport {
    pub action: AgentManagedSkillsAction,
    pub state: AgentManagedSkillsState,
    pub succeeded: Vec<String>,
    pub skipped: Vec<AgentManagedSkillsSkip>,
    pub failed: Vec<AgentManagedSkillsFailure>,
}

/// Pause/recovery is a destructive read-modify-write sequence. Serializing the
/// use case prevents two UI surfaces from replacing one target's journal from
/// stale snapshots. It intentionally covers all targets rather than inventing
/// a second in-memory target identity alongside the persisted physical key.
fn operation_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn normalized_names(names: impl IntoIterator<Item = String>) -> Vec<String> {
    names
        .into_iter()
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn global_profile(agent_id: &str) -> Result<AgentProfile> {
    let profiles = skillstar_skills::agents::list_profiles();
    let profile = skillstar_skills::agents::find_profile(&profiles, agent_id)?;
    if !profile.has_global_skills() {
        anyhow::bail!(
            "Agent profile '{}' does not support global skills",
            profile.id
        );
    }
    Ok(profile.clone())
}

fn state_for_profile(profile: &AgentProfile) -> Result<AgentManagedSkillsState> {
    Ok(AgentManagedSkillsState {
        active_skill_names: normalized_names(deployment::list_linked_skills(&profile.id)?),
        suspended_skill_names: normalized_names(skillstar_skills::agents::suspended_global_skill_names(
            &profile.global_skills_dir,
        )),
    })
}

/// Read the current managed links and the durable recovery set for one target.
///
/// The result is shared by every Agent profile that resolves to the same
/// physical Global skills directory.
pub fn get_agent_managed_skills_state(agent_id: &str) -> Result<AgentManagedSkillsState> {
    state_for_profile(&global_profile(agent_id)?)
}

fn toggle_names(
    names: &[String],
    agent_id: &str,
    enable: bool,
) -> (
    Vec<String>,
    Vec<AgentManagedSkillsSkip>,
    Vec<AgentManagedSkillsFailure>,
) {
    let mut succeeded = Vec::new();
    let mut skipped = Vec::new();
    let mut failed = Vec::new();

    for skill_name in names {
        match deployment::toggle_skill_for_agent(skill_name, agent_id, enable) {
            Ok(ToggleSkillOutcome::Applied) => succeeded.push(skill_name.clone()),
            Ok(ToggleSkillOutcome::Skipped { code, path, reason }) => {
                skipped.push(AgentManagedSkillsSkip {
                    skill_name: skill_name.clone(),
                    code,
                    path,
                    reason,
                });
            }
            Err(error) => failed.push(AgentManagedSkillsFailure {
                skill_name: skill_name.clone(),
                error: error.to_string(),
            }),
        }
    }

    (succeeded, skipped, failed)
}

fn remaining_absent(snapshot: &[String], active_skill_names: &[String]) -> Vec<String> {
    let active = active_skill_names.iter().collect::<BTreeSet<_>>();
    snapshot
        .iter()
        .filter(|name| !active.contains(name))
        .cloned()
        .collect()
}

/// Toggle only names already present in this Agent's Global skills directory.
///
/// With no recovery journal, this first records the exact active set to disk
/// and then unlinks it. With a journal, it restores only the still-absent
/// journal names. Failures and protected collisions remain journaled for a
/// future retry; no Hub inventory is consulted to fill any gap.
pub fn toggle_agent_managed_skills(agent_id: &str) -> Result<AgentManagedSkillsToggleReport> {
    let _operation = operation_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let profile = global_profile(agent_id)?;
    if !profile.enabled {
        anyhow::bail!(
            "Agent profile '{}' must be enabled before its managed skills can be changed",
            profile.id
        );
    }

    let before = state_for_profile(&profile)?;
    if before.suspended_skill_names.is_empty() {
        // Persist before the first destructive unlink. If the process stops
        // between individual items, the complete original set remains a safe
        // recovery intent and restore filters out anything still active.
        skillstar_skills::agents::replace_suspended_global_skill_names(
            &profile.global_skills_dir,
            &before.active_skill_names,
        )?;

        let (succeeded, skipped, failed) =
            toggle_names(&before.active_skill_names, &profile.id, false);
        let active_skill_names = normalized_names(deployment::list_linked_skills(&profile.id)?);
        let suspended_skill_names =
            remaining_absent(&before.active_skill_names, &active_skill_names);
        skillstar_skills::agents::replace_suspended_global_skill_names(
            &profile.global_skills_dir,
            &suspended_skill_names,
        )?;

        return Ok(AgentManagedSkillsToggleReport {
            action: AgentManagedSkillsAction::Paused,
            state: AgentManagedSkillsState {
                active_skill_names,
                suspended_skill_names,
            },
            succeeded,
            skipped,
            failed,
        });
    }

    let restore_names = remaining_absent(&before.suspended_skill_names, &before.active_skill_names);
    let (succeeded, skipped, failed) = toggle_names(&restore_names, &profile.id, true);
    let active_skill_names = normalized_names(deployment::list_linked_skills(&profile.id)?);
    let suspended_skill_names =
        remaining_absent(&before.suspended_skill_names, &active_skill_names);
    skillstar_skills::agents::replace_suspended_global_skill_names(
        &profile.global_skills_dir,
        &suspended_skill_names,
    )?;

    Ok(AgentManagedSkillsToggleReport {
        action: AgentManagedSkillsAction::Restored,
        state: AgentManagedSkillsState {
            active_skill_names,
            suspended_skill_names,
        },
        succeeded,
        skipped,
        failed,
    })
}
