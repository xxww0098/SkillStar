//! Project copy inspection and metadata-only recovery.

use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use ss_core::infra::paths;

use super::index::load_index_strict;
use super::store::{load_skills_list_strict, save_skills_list};
use super::sync::{build_physical_path_plans, project_path_key};
use super::types::{ProjectCopyRecord, ProjectCopySnapshot, ProjectCopyState, ProjectDeployMode};
use crate::deployment::ownership::{self, Ownership};

pub(super) fn lock_copy_source() -> Result<crate::skill_update::UpdateTransactionGuard> {
    anyhow::ensure!(
        !super::write_lock::current_project_write_held()
            || crate::skill_update::transaction::current_update_transaction_held(),
        "Project deployment requires the skill update lock before the project lock; release the project operation and retry"
    );
    crate::skill_update::try_acquire_update_transaction_lock(Duration::from_secs(5))
}

/// Detect standard project rule files without creating any directories.
pub fn inspect_project_rules(project_path: &str) -> Vec<String> {
    let root = Path::new(project_path);
    [
        ("CLAUDE.md", "CLAUDE.md"),
        (".cursorrules", ".cursorrules"),
        (".cursor/rules", ".cursor/rules"),
        (".windsurfrules", ".windsurfrules"),
        (".windsurf/rules", ".windsurf/rules"),
        ("AGENTS.md", "AGENTS.md"),
        (".agents/skills", ".agents/skills"),
        (".claude/skills", ".claude/skills"),
        (".github/copilot-instructions.md", "copilot-instructions.md"),
        (".gemini", ".gemini"),
        (".vscode", ".vscode"),
    ]
    .into_iter()
    .filter(|(rel, _)| root.join(rel).exists())
    .map(|(_, name)| name.to_string())
    .collect()
}

/// Inspect persisted deployments even when the project has moved. No project
/// directories or deployment records are created; coordination locks may be opened.
pub fn inspect_project_copies(name: &str) -> Result<Vec<ProjectCopyRecord>> {
    inspect_project_copy_status(name).map(|snapshot| snapshot.copies)
}

/// Read configuration, rules and deployments from one stable project snapshot.
pub fn inspect_project_copy_status(name: &str) -> Result<ProjectCopySnapshot> {
    let _source = lock_copy_source()?;
    let _project = super::write_lock::lock_project_write()?;
    let entry = load_index_strict()?
        .projects
        .into_iter()
        .find(|entry| entry.name == name)
        .with_context(|| format!("Project '{name}' is not registered"))?;
    let list = load_skills_list_strict(name)?.unwrap_or_default();
    let profiles = crate::agents::list_profile_specs();
    super::store::validate_skills_list_with_profiles(&list, &profiles)?;
    let rules = if Path::new(&entry.path).is_dir() {
        inspect_project_rules(&entry.path)
    } else {
        Vec::new()
    };
    let copies = inspect_copies_locked(&entry, &list, &profiles)?;
    Ok(ProjectCopySnapshot {
        project: entry,
        skills_list: list,
        copies,
        rules,
    })
}

fn inspect_copies_locked(
    entry: &super::types::ProjectEntry,
    list: &super::types::SkillsList,
    profiles: &[crate::agents::AgentProfile],
) -> Result<Vec<ProjectCopyRecord>> {
    let root = Path::new(&entry.path);
    let live_root = root.is_dir().then_some(root);
    let mut records = Vec::new();
    for plan in build_physical_path_plans(live_root, list, profiles)? {
        let rel = &plan.profile.project_skills_rel;
        for skill in plan.skill_names {
            crate::content::validate_skill_name(&skill)?;
            let record = |state, detail| ProjectCopyRecord {
                project_name: entry.name.clone(),
                project_path: entry.path.clone(),
                relative_path: rel.clone(),
                skill_name: skill.clone(),
                state,
                detail,
            };
            if live_root.is_none() {
                records.push(record(ProjectCopyState::ProjectMissing, None));
                continue;
            }
            let target = root.join(rel).join(&skill);
            if let Err(error) = super::binding::contained_child(root, &root.join(rel)) {
                records.push(record(
                    ProjectCopyState::ReadFailed,
                    Some(format!("{error:#}")),
                ));
                continue;
            }
            let owned = ownership::owned_project_deployment(&target, &skill);
            let state = match owned {
                Ownership::Link { alive: true } => continue,
                Ownership::Link { alive: false } => ProjectCopyState::BrokenLink,
                Ownership::Missing => ProjectCopyState::Missing,
                Ownership::SkillStarModifiedCopy => ProjectCopyState::Modified,
                Ownership::Foreign if target.is_dir() || plan.mode == ProjectDeployMode::Copy => {
                    if ownership::marker_content_hash(&target).is_some() {
                        ProjectCopyState::Modified
                    } else {
                        ProjectCopyState::Unmanaged
                    }
                }
                Ownership::Foreign => continue,
                Ownership::Copy => {
                    let source = paths::hub_skills_dir().join(&skill);
                    if !source.is_dir() {
                        ProjectCopyState::SourceMissing
                    } else {
                        match (
                            ownership::dir_content_hash(&source),
                            ownership::dir_content_hash(&target),
                        ) {
                            (Ok(source), Ok(copy)) if source == copy => ProjectCopyState::Current,
                            (Ok(_), Ok(_)) => ProjectCopyState::Stale,
                            (Err(error), _) | (_, Err(error)) => {
                                records.push(record(
                                    ProjectCopyState::ReadFailed,
                                    Some(format!("{error:#}")),
                                ));
                                continue;
                            }
                        }
                    }
                }
            };
            records.push(record(state, None));
        }
    }
    records.sort_by(|left, right| {
        (&left.relative_path, &left.skill_name).cmp(&(&right.relative_path, &right.skill_name))
    });
    Ok(records)
}

/// Retry the currently selected copies without fetching a newer upstream version.
pub fn retry_project_copies(name: &str) -> Result<Vec<ProjectCopyRecord>> {
    let _source = lock_copy_source()?;
    let _project = super::write_lock::lock_project_write()?;
    let entry = load_index_strict()?
        .projects
        .into_iter()
        .find(|entry| entry.name == name)
        .with_context(|| format!("Project '{name}' is not registered"))?;
    super::refresh::refresh_stale_copies(&entry.path)?;
    inspect_project_copies(name)
}

/// Forget one physical deployment across all its Agent readers. A missing root
/// is not required; files are never changed or removed.
pub fn forget_project_copy(name: &str, relative_path: &str, skill: &str) -> Result<()> {
    crate::content::validate_skill_name(skill)?;
    let _project = super::write_lock::lock_project_write()?;
    let entry = load_index_strict()?
        .projects
        .into_iter()
        .find(|entry| entry.name == name)
        .with_context(|| format!("Project '{name}' is not registered"))?;
    let mut list =
        load_skills_list_strict(name)?.context("Project skill configuration is missing")?;
    let profiles = crate::agents::list_profile_specs();
    let root = Path::new(&entry.path);
    let live_root = root.is_dir().then_some(root);
    let target = project_path_key(live_root, relative_path)?;
    let mut readers = std::collections::HashSet::new();
    for profile in &profiles {
        if list.agents.contains_key(&profile.id)
            && profile.has_project_skills()
            && project_path_key(live_root, &profile.project_skills_rel)? == target
        {
            readers.insert(profile.id.as_str());
        }
    }
    let mut changed = false;
    for (id, members) in &mut list.agents {
        if readers.contains(id.as_str()) {
            let before = members.len();
            members.retain(|member| member != skill);
            changed |= before != members.len();
        }
    }
    anyhow::ensure!(
        changed,
        "Project '{name}' no longer tracks '{skill}' at '{relative_path}'; refresh the project and try again"
    );
    list.agents.retain(|_, members| !members.is_empty());
    super::types::prune_deploy_modes_for_agents(&mut list.deploy_modes, &list.agents, &profiles);
    list.updated_at = chrono::Utc::now().to_rfc3339();
    save_skills_list(name, &list)
}

#[cfg(test)]
#[path = "upkeep_tests.rs"]
mod tests;
