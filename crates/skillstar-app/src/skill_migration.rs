//! Move an installed Skill onto the successor its source renamed it into.
//!
//! An update check that finds a Skill's folder gone at the tracked revision
//! records the folder it was renamed or moved to as `upstream_change.successor`
//! (see `skillstar_skills::update_checker`). Acting on that spans three
//! domains — hub install, Agent/Project deployment, hub removal — so the
//! sequence lives here rather than in a command or in the skills crate.
//!
//! Order matters: installing the successor from the same repository resets
//! that checkout, which is what makes the old folder disappear from disk. The
//! old Skill's deployments are therefore recorded *before* the install and
//! re-applied to the new name afterwards, and the old entry is removed last.
//! Install and removal each hold the update transaction lock on their own, so
//! this is a sequence of transactions, not one: a step that fails is reported
//! as such, and the next update check classifies whatever is left over.

use anyhow::{Context, Result, bail};
use serde::Serialize;
use skillstar_core::types::UpstreamChange;
use skillstar_skills::git_skill::GitSkillFacade;
use skillstar_skills::repo_scanner::SkillInstallTarget;
use skillstar_skills::{
    deployment, installed_skill, lockfile, projects, skill_install, source_resolver, update_state,
};

#[derive(Debug, Clone, Serialize)]
pub struct SkillMigrationReport {
    /// Name the successor was installed under.
    pub installed: String,
    /// The Skill that was migrated away from.
    pub removed: String,
    /// Agent ids the successor is now linked to, matching the old Skill.
    pub agents_relinked: Vec<String>,
    pub agent_failures: Vec<String>,
    /// Project paths the successor was added to, matching the old Skill.
    pub projects_relinked: Vec<String>,
    pub project_failures: Vec<String>,
    /// The old entry could not be removed and is still installed; the next
    /// update check reports it as removed upstream so the user can finish.
    pub removal_failure: Option<String>,
}

/// Install the recorded successor of `name`, carry its deployments over, and
/// remove `name`.
pub fn migrate_renamed_skill(name: &str, facade: &GitSkillFacade) -> Result<SkillMigrationReport> {
    let Some(UpstreamChange::Removed {
        successor: Some(successor),
        ..
    }) = update_state::upstream_change(name)
    else {
        bail!("No upstream successor is recorded for '{name}'; run an update check first");
    };
    // Channel-owned Skills never carry an upstream change (patrol skips them),
    // so the guard above already keeps them out; install and removal below
    // re-check the mutation gate themselves.

    let entry = lockfile::Lockfile::load(&lockfile::lockfile_path())
        .context("failed to load the Skill lockfile")?
        .skills
        .into_iter()
        .find(|entry| entry.name == name)
        .with_context(|| format!("'{name}' has no lock entry to migrate from"))?;
    let source = source_resolver::Source::parse(&entry.git_url)
        .with_context(|| format!("'{name}' has an unusable source URL"))?;

    // 1. Remember where the old Skill is deployed — the install below moves
    //    the shared checkout and takes the old folder with it.
    let agents = linked_agents(name);
    let project_targets = projects_with(name);

    // 2. Install the successor by its exact folder, not by id: ids can repeat
    //    across buckets (e.g. a deprecated copy), folders cannot.
    let installed = facade
        .install_from_scan(
            &source.short,
            &source.repo_url,
            &[SkillInstallTarget {
                id: successor.skill_id.clone(),
                folder_path: successor.folder_path.clone(),
            }],
        )
        .with_context(|| {
            format!(
                "failed to install '{}' from {}",
                successor.skill_id, successor.folder_path
            )
        })?;
    let new_name = installed
        .into_iter()
        .next()
        .with_context(|| format!("installing '{}' produced no Skill", successor.skill_id))?;

    // 3. Carry the deployments over to the new name.
    let mut agents_relinked = Vec::new();
    let mut agent_failures = Vec::new();
    for agent_id in agents {
        match deployment::batch_link_skills_to_agent(std::slice::from_ref(&new_name), &agent_id) {
            Ok(_) => agents_relinked.push(agent_id),
            Err(error) => agent_failures.push(format!("{agent_id}: {error:#}")),
        }
    }
    let mut projects_relinked = Vec::new();
    let mut project_failures = Vec::new();
    for (project_path, agent_ids) in project_targets {
        match projects::add_skills_to_project(
            &project_path,
            std::slice::from_ref(&new_name),
            &agent_ids,
        ) {
            Ok(_) => projects_relinked.push(project_path),
            Err(error) => project_failures.push(format!("{project_path}: {error:#}")),
        }
    }

    // 4. Remove the old entry (hub, lockfile, Agent and Project deployments).
    let removal_failure = skill_install::uninstall_skill(name).err();
    if removal_failure.is_none() {
        update_state::forget(name);
    }
    installed_skill::invalidate_cache();

    Ok(SkillMigrationReport {
        installed: new_name,
        removed: name.to_string(),
        agents_relinked,
        agent_failures,
        projects_relinked,
        project_failures,
        removal_failure,
    })
}

/// Global Agents that currently deploy `name`.
fn linked_agents(name: &str) -> Vec<String> {
    skillstar_agents::list_profiles()
        .into_iter()
        .filter(|profile| profile.has_global_skills())
        .filter(|profile| {
            deployment::list_linked_skills(&profile.id)
                .is_ok_and(|linked| linked.iter().any(|linked| linked == name))
        })
        .map(|profile| profile.id)
        .collect()
}

/// Registered projects that deploy `name`, with the Agents it is deployed for.
fn projects_with(name: &str) -> Vec<(String, Vec<String>)> {
    projects::list_projects()
        .into_iter()
        .filter_map(|project| {
            let list = projects::load_skills_list(&project.name)?;
            let agent_ids: Vec<String> = list
                .agents
                .iter()
                .filter(|(_, skills)| skills.iter().any(|skill| skill == name))
                .map(|(agent_id, _)| agent_id.clone())
                .collect();
            (!agent_ids.is_empty()).then_some((project.path, agent_ids))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{ENV_LOCK, EnvGuard};
    use skillstar_git::transport::GitOperationSession;

    #[tokio::test]
    async fn migration_refuses_a_skill_without_a_recorded_successor() {
        let _lock = ENV_LOCK.lock().await;
        let temp = tempfile::tempdir().unwrap();
        let _env = EnvGuard::set(&[
            ("SKILLSTAR_DATA_DIR", &temp.path().join("data")),
            ("SKILLSTAR_HUB_DIR", &temp.path().join("hub")),
        ]);
        update_state::reset_for_test();

        let facade = GitSkillFacade::new(GitOperationSession::public());
        let error = migrate_renamed_skill("nope", &facade)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("No upstream successor"),
            "a Skill nothing points at must not be touched: {error}"
        );
    }
}
