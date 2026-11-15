//! Project-level deployment: full sync, additive add, and targeted cleanup.

use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::binding::contained_child;
use super::helpers::prune_empty_dirs_upward;
use super::index::{load_index_strict, register_project};
use super::store::{
    load_skills_list_strict, save_skills_list, validate_project_relative_path, validate_skills_list,
};
use super::types::{
    ProjectDeployMode, SkillsList, deploy_skill_with_mode, ensure_project_root_exists,
    prune_deploy_modes_for_agents,
};
use crate::agents as agent_profile;
use ss_core::infra::paths as fs_paths;

fn normalize_project_agents(
    agents: HashMap<String, Vec<String>>,
) -> Result<HashMap<String, Vec<String>>> {
    for name in agents.values().flatten() {
        if !name.trim().is_empty() {
            crate::content::validate_skill_name(name.trim())?;
        }
    }
    Ok(agents
        .into_iter()
        .filter_map(|(agent_id, skill_names)| {
            let mut seen = HashSet::new();
            let normalized = skill_names
                .into_iter()
                .filter_map(|name| {
                    let trimmed = name.trim();
                    if trimmed.is_empty() {
                        return None;
                    }

                    let normalized = trimmed.to_string();
                    if seen.insert(normalized.clone()) {
                        Some(normalized)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();

            if normalized.is_empty() {
                None
            } else {
                Some((agent_id, normalized))
            }
        })
        .collect())
}

pub(super) struct ProjectPathPlan<'a> {
    pub(super) profile: &'a agent_profile::AgentProfile,
    pub(super) skill_names: Vec<String>,
    pub(super) mode: ProjectDeployMode,
}

/// Collapse agent-owned manifest entries by configured relative directory.
/// Several compatible agents intentionally share `.agents/skills`; the union
/// is taken before live directory aliases are resolved by physical plans.
pub(super) fn build_path_plans<'a>(
    skills_list: &SkillsList,
    profiles: &'a [agent_profile::AgentProfile],
) -> Vec<ProjectPathPlan<'a>> {
    let mut plans = Vec::<ProjectPathPlan<'a>>::new();
    let mut seen_paths = HashSet::new();

    for profile in profiles {
        if !profile.has_project_skills() || !skills_list.agents.contains_key(&profile.id) {
            continue;
        }
        if !seen_paths.insert(profile.project_skills_rel.clone()) {
            continue;
        }
        let membership = super::owner::shared_path_membership(
            profiles,
            skills_list,
            &profile.project_skills_rel,
            "",
            &[],
        );
        plans.push(ProjectPathPlan {
            profile,
            skill_names: membership.members,
            mode: membership.mode.unwrap_or_default(),
        });
    }

    plans
}

/// Callers pass a live root only after checking it; None is metadata-only and
/// never resolves or probes any registered project path.
pub(super) fn project_path_key(root: Option<&Path>, relative: &str) -> Result<PathBuf> {
    validate_project_relative_path(relative)?;
    Ok(match root {
        Some(root) => ss_core::infra::fs_ops::canonicalize_existing_prefix(&root.join(relative)),
        None => PathBuf::from(relative),
    })
}

/// Inspect or refresh each physical directory once, preserving profile order
/// and the union of members across relative aliases. Copy wins for observation
/// when alias configurations disagree; deployment still rejects that conflict.
pub(super) fn build_physical_path_plans<'a>(
    root: Option<&Path>,
    skills_list: &SkillsList,
    profiles: &'a [agent_profile::AgentProfile],
) -> Result<Vec<ProjectPathPlan<'a>>> {
    merge_physical_path_plans(root, build_path_plans(skills_list, profiles), false)
}

fn merge_physical_path_plans<'a>(
    root: Option<&Path>,
    plans: Vec<ProjectPathPlan<'a>>,
    reject_mode_conflicts: bool,
) -> Result<Vec<ProjectPathPlan<'a>>> {
    let mut physical_plans: Vec<(PathBuf, ProjectPathPlan<'a>)> = Vec::new();
    for plan in plans {
        let key = project_path_key(root, &plan.profile.project_skills_rel)?;
        if let Some((_, existing)) = physical_plans.iter_mut().find(|(path, _)| path == &key) {
            if reject_mode_conflicts
                && !existing.skill_names.is_empty()
                && !plan.skill_names.is_empty()
                && existing.mode != plan.mode
            {
                anyhow::bail!(
                    "conflicting project deploy modes for shared directory {}",
                    key.display()
                );
            }
            if existing.skill_names.is_empty()
                || (!reject_mode_conflicts && plan.mode == ProjectDeployMode::Copy)
            {
                existing.mode = plan.mode;
            }
            for name in plan.skill_names {
                if !existing.skill_names.contains(&name) {
                    existing.skill_names.push(name);
                }
            }
        } else {
            physical_plans.push((key, plan));
        }
    }
    Ok(physical_plans.into_iter().map(|(_, plan)| plan).collect())
}

fn project_target_dir(project: &Path, relative: &str) -> Result<PathBuf> {
    validate_project_relative_path(relative)?;
    let target = project.join(relative);
    // Check the directory rather than the final skill entry: an owned skill
    // link deliberately resolves outside the project into the hub.
    contained_child(project, &target)?;
    match target.symlink_metadata() {
        Ok(_) if !target.is_dir() => {
            anyhow::bail!(
                "project skills path is not a directory: {}",
                target.display()
            );
        }
        Err(err) if err.kind() != std::io::ErrorKind::NotFound => {
            return Err(err).with_context(|| format!("failed to inspect {}", target.display()));
        }
        _ => {}
    }
    Ok(target)
}

fn validate_project_plan(
    list: &SkillsList,
    profiles: &[agent_profile::AgentProfile],
    cleanup_agents: Option<&[String]>,
) -> Result<()> {
    validate_skills_list(list)?;
    for profile in profiles {
        if profile.has_project_skills()
            && (list.agents.contains_key(&profile.id)
                || cleanup_agents.is_some_and(|ids| ids.contains(&profile.id)))
        {
            validate_project_relative_path(&profile.project_skills_rel)?;
        }
    }
    Ok(())
}

/// Path migration preflights every desired name before changing either root.
/// Source/copy failures are reconciled by full_sync and may leave retryable
/// deployments at the new root, but a known foreign conflict writes nothing.
pub(super) fn preflight_project_targets(project: &Path, list: &SkillsList) -> Result<()> {
    let profiles = agent_profile::list_profiles();
    validate_project_plan(list, &profiles, None)?;
    let mut failures = Vec::new();
    for plan in build_path_plans(list, &profiles) {
        let directory = match project_target_dir(project, &plan.profile.project_skills_rel) {
            Ok(directory) => directory,
            Err(err) => {
                failures.push(format!("{}: {err:#}", plan.profile.project_skills_rel));
                continue;
            }
        };
        for name in &plan.skill_names {
            let target = directory.join(name);
            match crate::deployment::ownership::owned_project_deployment(&target, name) {
                crate::deployment::Ownership::Foreign => {
                    failures.push(format!(
                        "foreign entry occupies project skill '{}' at {}",
                        name,
                        target.display()
                    ));
                }
                crate::deployment::Ownership::SkillStarModifiedCopy => {
                    failures.push(format!(
                        "modified copy occupies project skill '{}' at {}",
                        name,
                        target.display()
                    ));
                }
                _ => {}
            }
        }
    }
    if !failures.is_empty() {
        anyhow::bail!("Project path preflight failed: {}", failures.join("; "));
    }
    Ok(())
}

fn reconcile_project_skill(
    source: &Path,
    target: &Path,
    name: &str,
    mode: ProjectDeployMode,
) -> Result<()> {
    use crate::deployment::{Ownership, ownership};

    if !source.is_dir() {
        anyhow::bail!(
            "hub source is missing or not a directory: {}",
            source.display()
        );
    }
    match (mode, ownership::owned_project_deployment(target, name)) {
        (_, Ownership::Foreign) => anyhow::bail!(
            "foreign entry occupies project skill at {}",
            target.display()
        ),
        (_, Ownership::SkillStarModifiedCopy) => anyhow::bail!(
            "modified copy occupies project skill at {}",
            target.display()
        ),
        (ProjectDeployMode::Copy, Ownership::Copy) => {
            ownership::refresh_project_copy(source, target, name)
        }
        (ProjectDeployMode::Copy, Ownership::Link { .. }) => {
            let mut staged = crate::materialize::StagedReplace::stage(target, |stage| {
                ownership::deploy_copy(source, stage, name)
            })?;
            if !matches!(
                ownership::owned_project_deployment(target, name),
                Ownership::Link { .. }
            ) {
                anyhow::bail!(
                    "project deployment changed while staging {}",
                    target.display()
                );
            }
            staged.swap()?;
            staged.commit();
            Ok(())
        }
        (ProjectDeployMode::Symlink, Ownership::Copy) => {
            ownership::replace_project_copy_with_link_or_copy(source, target, name).map(|_| ())
        }
        (ProjectDeployMode::Symlink, Ownership::Link { .. }) => {
            crate::deployment::swap_in_fresh_deploy(source, target, name).map(|_| ())
        }
        (_, Ownership::Missing) => deploy_skill_with_mode(source, target, mode),
    }
}

fn remove_deselected_project_skills(
    project: &Path,
    directory: &Path,
    desired: &[String],
) -> Result<()> {
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => {
            return Err(err).with_context(|| format!("failed to read {}", directory.display()));
        }
    };
    let mut failures = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                failures.push(err.to_string());
                continue;
            }
        };
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if desired.contains(&name) || crate::content::validate_skill_name(&name).is_err() {
            continue;
        }
        if let Err(err) = crate::deployment::ownership::remove_project_owned(&entry.path(), &name) {
            failures.push(format!("{}: {err:#}", entry.path().display()));
        }
    }
    // A directory alias may point at a nonempty directory elsewhere inside
    // the project. Do not try to remove the alias as if it were an empty dir.
    if !ss_core::infra::fs_ops::is_link(directory)
        && let Err(err) = prune_empty_dirs_upward(directory, project)
    {
        failures.push(format!("{}: {err:#}", directory.display()));
    }
    if !failures.is_empty() {
        anyhow::bail!(
            "failed to clean deselected project skills: {}",
            failures.join("; ")
        );
    }
    Ok(())
}

/// Uninstall cleanup removes one named owned deployment and its persisted
/// membership from each project, preserving all unmanaged entries.
pub fn remove_skill_from_all_projects(skill_name: &str) -> Result<Vec<String>> {
    crate::content::validate_skill_name(skill_name)?;
    let _guard = super::write_lock::lock_project_write()?;
    let profiles = agent_profile::list_profiles();
    let mut touched_projects = Vec::new();

    for entry in load_index_strict()?.projects {
        let mut touched = false;

        if let Some(mut skills_list) = load_skills_list_strict(&entry.name)? {
            let mut list_changed = false;
            for skill_names in skills_list.agents.values_mut() {
                let before = skill_names.len();
                skill_names.retain(|name| name != skill_name);
                if skill_names.len() != before {
                    list_changed = true;
                }
            }
            if list_changed {
                skills_list.agents.retain(|_, skills| !skills.is_empty());
                skills_list.updated_at = chrono::Utc::now().to_rfc3339();
                save_skills_list(&entry.name, &skills_list)?;
                touched = true;
            }
        }

        let project_root = Path::new(&entry.path);
        if !project_root.is_dir() {
            if touched {
                touched_projects.push(entry.name);
            }
            continue;
        }
        let mut seen_paths = HashSet::new();
        for profile in &profiles {
            if !profile.has_project_skills() {
                continue;
            }
            let directory = project_target_dir(project_root, &profile.project_skills_rel)?;
            if !seen_paths.insert(ss_core::infra::fs_ops::canonicalize_existing_prefix(
                &directory,
            )) {
                continue;
            }
            let skill_path = directory.join(skill_name);
            // Remove SkillStar's link, junction, or marked copy; a folder the
            // user made under the same name stays.
            let removal =
                crate::deployment::ownership::remove_project_owned(&skill_path, skill_name)
                    .with_context(|| {
                        format!(
                            "failed to remove project skill '{}' from {}",
                            skill_name, entry.path
                        )
                    })?;
            if removal != crate::deployment::ownership::Removal::Removed {
                continue;
            }

            if let Some(parent) = skill_path.parent() {
                prune_empty_dirs_upward(parent, project_root)?;
            }

            touched = true;
        }

        if touched {
            touched_projects.push(entry.name);
        }
    }

    Ok(touched_projects)
}

/// Reconcile desired deployments once per physical project directory.
/// Owned entries are staged before replacement; a failed item keeps the old
/// deployment. Only deselected owned entries are removed, and foreign entries
/// are kept. Missing hub sources and desired-name conflicts are reported.
/// Returns the number of successfully reconciled deployments (links or copies).
pub fn full_sync(
    project_path: &str,
    skills_list: &SkillsList,
    cleanup_agents: Option<&[String]>,
) -> Result<u32> {
    let _source = super::upkeep::lock_copy_source()?;
    let _guard = super::write_lock::lock_project_write()?;
    full_sync_locked(project_path, skills_list, cleanup_agents)
}

/// Inner reconciliation for callers already holding source then project locks.
pub(super) fn full_sync_locked(
    project_path: &str,
    skills_list: &SkillsList,
    cleanup_agents: Option<&[String]>,
) -> Result<u32> {
    reconcile_project_locked(project_path, skills_list, cleanup_agents, true)
}

/// A path rebind only reconciles tracked names. Forgotten copies and other
/// manifest-external entries at the new root remain on disk.
pub(super) fn sync_project_path_rebind_locked(
    project_path: &str,
    skills_list: &SkillsList,
) -> Result<u32> {
    reconcile_project_locked(project_path, skills_list, None, false)
}

fn reconcile_project_locked(
    project_path: &str,
    skills_list: &SkillsList,
    cleanup_agents: Option<&[String]>,
    remove_deselected: bool,
) -> Result<u32> {
    let hub_dir = fs_paths::hub_skills_dir();
    let profiles = agent_profile::list_profiles();
    let project = ensure_project_root_exists(project_path)?;
    validate_project_plan(skills_list, &profiles, cleanup_agents)?;
    let mut plans = build_path_plans(skills_list, &profiles);
    for profile in &profiles {
        if profile.has_project_skills()
            && cleanup_agents.is_some_and(|ids| ids.contains(&profile.id))
            && !plans
                .iter()
                .any(|plan| plan.profile.project_skills_rel == profile.project_skills_rel)
        {
            plans.push(ProjectPathPlan {
                profile,
                skill_names: Vec::new(),
                mode: ProjectDeployMode::default(),
            });
        }
    }

    // Resolve directory aliases before touching disk, so cleanup cannot remove
    // desired entries from another configured path to the same directory.
    let mut safe_plans = Vec::new();
    let mut failures = Vec::new();
    for plan in plans {
        match project_target_dir(&project, &plan.profile.project_skills_rel) {
            Ok(_) => safe_plans.push(plan),
            Err(err) => failures.push(format!("{}: {err:#}", plan.profile.project_skills_rel)),
        }
    }
    let physical_plans = merge_physical_path_plans(Some(&project), safe_plans, true)?;

    let mut total = 0u32;
    for plan in physical_plans {
        let target_dir = project.join(&plan.profile.project_skills_rel);
        for skill_name in &plan.skill_names {
            let source = hub_dir.join(skill_name);
            let target = target_dir.join(skill_name);
            let result = (|| -> Result<()> {
                let _ = project_target_dir(&project, &plan.profile.project_skills_rel)?;
                if !source.is_dir() {
                    anyhow::bail!(
                        "hub source is missing or not a directory: {}",
                        source.display()
                    );
                }
                match crate::deployment::ownership::owned_project_deployment(&target, skill_name) {
                    crate::deployment::Ownership::Foreign => anyhow::bail!(
                        "foreign entry occupies project skill at {}",
                        target.display()
                    ),
                    crate::deployment::Ownership::SkillStarModifiedCopy => anyhow::bail!(
                        "modified copy occupies project skill at {}",
                        target.display()
                    ),
                    _ => {}
                }
                std::fs::create_dir_all(&target_dir).with_context(|| {
                    format!("failed to create skill dir: {}", target_dir.display())
                })?;
                let _ = project_target_dir(&project, &plan.profile.project_skills_rel)?;
                reconcile_project_skill(&source, &target, skill_name, plan.mode)
            })();
            match result {
                Ok(()) => total += 1,
                Err(err) => failures.push(format!(
                    "Failed to deploy '{skill_name}' for path '{}' at {}: {err:#}",
                    plan.profile.project_skills_rel,
                    target.display()
                )),
            }
        }
        if remove_deselected {
            let cleanup = project_target_dir(&project, &plan.profile.project_skills_rel).and_then(
                |directory| {
                    remove_deselected_project_skills(&project, &directory, &plan.skill_names)
                },
            );
            if let Err(err) = cleanup {
                failures.push(format!("{}: {err:#}", target_dir.display()));
            }
        }
    }

    if !failures.is_empty() {
        let failed = failures.len();
        let preview = failures.into_iter().take(6).collect::<Vec<_>>().join("\n");
        anyhow::bail!(
            "Project sync incomplete: reconciled {total} deployment(s), {failed} failure(s).\n{preview}"
        );
    }
    Ok(total)
}

/// Register, persist the desired membership, then reconcile selected paths and
/// clean removed agents. Unrelated paths remain untouched; failed desired
/// membership stays persisted for retry. Returns `(project_name, deploy_count)`.
pub fn save_and_sync(
    project_path: &str,
    agents: HashMap<String, Vec<String>>,
    deploy_modes: HashMap<String, ProjectDeployMode>,
) -> Result<(String, u32)> {
    let _source = super::upkeep::lock_copy_source()?;
    let _guard = super::write_lock::lock_project_write()?;
    let agents = normalize_project_agents(agents)?;
    for relative in deploy_modes.keys() {
        validate_project_relative_path(relative)?;
    }
    let profiles = agent_profile::list_profiles();
    for profile in profiles
        .iter()
        .filter(|profile| profile.has_project_skills() && agents.contains_key(&profile.id))
    {
        let project = ensure_project_root_exists(project_path)?;
        let _ = project_target_dir(&project, &profile.project_skills_rel)?;
    }
    let entry = register_project(project_path)?;

    // Snapshot the old config so we can compute which agents were removed.
    let old_list = load_skills_list_strict(&entry.name)?;

    let mut deploy_modes = deploy_modes;
    // Drop deploy-mode entries whose directory no longer has an enabled agent.
    prune_deploy_modes_for_agents(&mut deploy_modes, &agents, &profiles);

    let skills_list = SkillsList {
        agents,
        deploy_modes,
        updated_at: chrono::Utc::now().to_rfc3339(),
    };

    // Agents that existed in old config but are absent from new config need
    // their project directories cleaned up.
    let cleanup: Vec<String> = old_list
        .map(|old| {
            old.agents
                .keys()
                .filter(|id| !skills_list.agents.contains_key(*id))
                .cloned()
                .collect()
        })
        .unwrap_or_default();

    validate_project_plan(&skills_list, &profiles, Some(&cleanup))?;
    let project = ensure_project_root_exists(project_path)?;
    for profile in profiles
        .iter()
        .filter(|profile| profile.has_project_skills() && cleanup.contains(&profile.id))
    {
        let _ = project_target_dir(&project, &profile.project_skills_rel)?;
    }
    save_skills_list(&entry.name, &skills_list)?;
    let count = full_sync_locked(project_path, &skills_list, Some(&cleanup))?;

    Ok((entry.name, count))
}

/// Register and persist skills-list.json for metadata-only updates such as
/// shared-path ownership changes, preserving all project deployments.
pub fn save_skills_list_only(
    project_path: &str,
    agents: HashMap<String, Vec<String>>,
) -> Result<SkillsList> {
    let _guard = super::write_lock::lock_project_write()?;
    let agents = normalize_project_agents(agents)?;
    let entry = register_project(project_path)?;

    let profiles = agent_profile::list_profiles();
    let mut skills_list = load_skills_list_strict(&entry.name)?.unwrap_or_default();
    skills_list.agents = agents;
    skills_list.updated_at = chrono::Utc::now().to_rfc3339();
    prune_deploy_modes_for_agents(
        &mut skills_list.deploy_modes,
        &skills_list.agents,
        &profiles,
    );

    save_skills_list(&entry.name, &skills_list)?;

    Ok(skills_list)
}

/// Merge CLI/quick-deploy membership and deploy the requested skills without
/// cleaning other deployments. Empty agent selection prefers `.agents/skills`,
/// then the first available project-capable profile.
pub fn add_skills_to_project(
    project_path: &str,
    skill_names: &[String],
    agent_ids: &[String],
) -> Result<u32> {
    add_skills_to_project_with_mode(
        project_path,
        skill_names,
        agent_ids,
        ProjectDeployMode::Symlink,
    )
}

pub fn add_skills_to_project_with_mode(
    project_path: &str,
    skill_names: &[String],
    agent_ids: &[String],
    mode: ProjectDeployMode,
) -> Result<u32> {
    let _source = super::upkeep::lock_copy_source()?;
    let _guard = super::write_lock::lock_project_write()?;
    let hub_dir = fs_paths::hub_skills_dir();
    for name in skill_names {
        if !name.trim().is_empty() {
            crate::content::validate_skill_name(name.trim())?;
        }
    }
    let mut seen_skill_names = HashSet::new();
    let deployable_skill_names = skill_names
        .iter()
        .filter_map(|name| {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                return None;
            }
            if !seen_skill_names.insert(trimmed.to_string()) {
                return None;
            }
            hub_dir.join(trimmed).exists().then(|| trimmed.to_string())
        })
        .collect::<Vec<_>>();

    if deployable_skill_names.is_empty() {
        return Ok(0);
    }

    let entry = register_project(project_path)?;
    let profiles = agent_profile::list_profiles();
    let project = ensure_project_root_exists(project_path)?;

    // Resolve which agents to target
    let mut target_agent_ids: Vec<String> = agent_ids
        .iter()
        .filter(|id| {
            profiles
                .iter()
                .any(|p| p.id == **id && p.has_project_skills())
        })
        .cloned()
        .collect();

    if target_agent_ids.is_empty() {
        if !agent_ids.is_empty() {
            return Err(anyhow::anyhow!(
                "No valid project-level agents selected: {}",
                agent_ids.join(", ")
            ));
        }
        // Fallback: prefer the universal open-skills path, then first available.
        if let Some(fallback) = profiles
            .iter()
            .find(|p| p.project_skills_rel == ".agents/skills" && p.has_project_skills())
            .or_else(|| profiles.iter().find(|p| p.has_project_skills()))
        {
            target_agent_ids.push(fallback.id.clone());
        }
    }

    // Merge into existing skills-list.json
    let mut skills_list = load_skills_list_strict(&entry.name)?.unwrap_or_default();
    let mut target_profiles = Vec::new();
    let mut selected_paths = HashSet::new();

    for agent_id in &target_agent_ids {
        let Some(profile) = profiles.iter().find(|profile| &profile.id == agent_id) else {
            continue;
        };
        if !selected_paths.insert(profile.project_skills_rel.clone()) {
            continue;
        }
        let _ = project_target_dir(&project, &profile.project_skills_rel)?;

        // One owner and one member set for this physical path. Copy vs symlink
        // stays with this call.
        super::owner::collapse_shared_path(
            &mut skills_list,
            &profiles,
            &profile.project_skills_rel,
            &profile.id,
            &deployable_skill_names,
        );
        skills_list
            .deploy_modes
            .insert(profile.project_skills_rel.clone(), mode);
        target_profiles.push(profile);
    }
    skills_list.updated_at = chrono::Utc::now().to_rfc3339();
    save_skills_list(&entry.name, &skills_list)?;

    // Create only the new symlinks (incremental — no clearing)
    let mut total = 0u32;
    let mut failures: Vec<String> = Vec::new();

    for profile in target_profiles {
        let target_dir = project.join(&profile.project_skills_rel);
        let mut prepared_target_dir = false;
        let mut created_for_path = 0u32;

        for name in &deployable_skill_names {
            let source = hub_dir.join(name);
            let target = target_dir.join(name);

            // Additive installs keep existing copies and foreign entries.
            // Refresh an owned link through staging so a failure keeps it alive.
            match crate::deployment::ownership::owned_project_deployment(&target, name) {
                crate::deployment::Ownership::Link { .. }
                | crate::deployment::Ownership::Missing => {}
                _ => continue,
            }

            if !prepared_target_dir {
                let _ = project_target_dir(&project, &profile.project_skills_rel)?;
                std::fs::create_dir_all(&target_dir).with_context(|| {
                    format!("failed to create skill dir: {}", target_dir.display())
                })?;
                prepared_target_dir = true;
            }

            match reconcile_project_skill(&source, &target, name, mode) {
                Ok(()) => {
                    total += 1;
                    created_for_path += 1;
                }
                Err(err) => failures.push(format!(
                    "Failed to deploy '{}' for path '{}' at {}: {}",
                    name,
                    profile.project_skills_rel,
                    target.display(),
                    err
                )),
            }
        }

        if created_for_path == 0 && prepared_target_dir && target_dir.exists() {
            prune_empty_dirs_upward(&target_dir, project.as_path())?;
        }
    }

    if !failures.is_empty() {
        let failed = failures.len();
        let preview = failures.into_iter().take(6).collect::<Vec<_>>().join("\n");
        anyhow::bail!(
            "Project deploy incomplete: created {total} link(s), {failed} failure(s).\n{preview}"
        );
    }

    Ok(total)
}

#[cfg(test)]
#[path = "sync_tests.rs"]
mod tests;
