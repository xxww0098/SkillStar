//! D-081 install pipeline: parse → temp fetch → discover → copy to canonical
//! → install lock. One entry for GUI, CLI, carousel and batch installs.
//!
//! There is no persistent repository cache and no harness copy ranking:
//! every install re-fetches the source shallowly into a temp dir and
//! overwrites the canonical `~/.skillstar/data/skills/installed/<name>` copy. Agent linking is
//! a separate explicit step (`deployment`). A new install is not linked
//! into every enabled Agent.

use crate::fetch;
use crate::installer::{self, InstallUnit};
use crate::skill_lock;
use crate::source_resolver::{self, Source};
use crate::{installed_skill, local_skill};
use ss_core::infra::error::AppError;
use ss_core::infra::paths;
use ss_core::types::Skill;

/// Find the skill a single-name install refers to, fail-closed: a multi-skill
/// repo that no longer contains the requested identity is an error, never a
/// whole-repo fallback.
pub fn find_target_skill<'a>(
    skills: &'a [crate::discovery::DiscoveredSkill],
    wanted: Option<&str>,
    fallback_hint: &str,
) -> Result<&'a crate::discovery::DiscoveredSkill, String> {
    let pick = |id: &str| {
        skills
            .iter()
            .find(|skill| skill.id.eq_ignore_ascii_case(id))
    };
    let found = wanted
        .and_then(pick)
        .or_else(|| pick(fallback_hint))
        // No requested name and exactly one candidate: that one is the
        // target (CLI `install <url>` against a single-skill source).
        .or_else(|| (wanted.is_none() && skills.len() == 1).then(|| &skills[0]))
        .ok_or_else(|| {
            let names = skills
                .iter()
                .map(|skill| skill.id.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "Source does not contain Skill '{}'. It may have been removed or renamed upstream. Found: [{}]",
                wanted.unwrap_or(fallback_hint),
                names
            )
        })?;
    if !found.installable {
        return Err(format!(
            "Skill '{}' cannot be installed: {}",
            found.id,
            found
                .frontmatter_issues
                .first()
                .map(|code| code.as_str())
                .unwrap_or("invalid frontmatter")
        ));
    }
    Ok(found)
}

pub fn install_skill(url: String, name: Option<String>) -> Result<Skill, String> {
    install_skill_in_session(
        url,
        name,
        None,
        &crate::git::transport::GitOperationSession::public(),
    )
}

pub fn install_skill_for_agent(
    url: String,
    name: Option<String>,
    agent_id: &str,
) -> Result<Skill, AppError> {
    install_skill_in_session(
        url,
        name,
        Some(agent_id),
        &crate::git::transport::GitOperationSession::public(),
    )
    .map_err(AppError::from)
}

pub fn install_skill_in_session(
    url: String,
    name: Option<String>,
    _agent_id: Option<&str>,
    session: &crate::git::transport::GitOperationSession,
) -> Result<Skill, String> {
    let requested = name.clone();
    let mut installed =
        install_skills_batch_in_session(&url, &name.into_iter().collect::<Vec<_>>(), session)?;
    installed.pop().ok_or_else(|| {
        format!(
            "Skill '{}' could not be installed",
            requested.unwrap_or_default()
        )
    })
}

/// Install multiple skills from the same source URL in one batch.
pub fn install_skills_batch(url: &str, names: &[String]) -> Result<Vec<Skill>, String> {
    install_skills_batch_in_session(
        url,
        names,
        &crate::git::transport::GitOperationSession::public(),
    )
}

pub fn install_skills_batch_in_session(
    url: &str,
    names: &[String],
    session: &crate::git::transport::GitOperationSession,
) -> Result<Vec<Skill>, String> {
    let spec = Source::parse(url).map_err(|error| format!("Invalid source: {error}"))?;
    crate::skill_mutation::policy()
        .ensure_repository_mutation_allowed(&spec.repo_url)
        .map_err(|error| error.to_string())?;
    for name in names {
        crate::skill_mutation::policy()
            .ensure_skill_mutation_allowed(name)
            .map_err(|error| error.to_string())?;
    }

    let checkout = fetch::fetch_for_scan(&spec, session).map_err(|error| format!("{error:#}"))?;
    session.emit_stage(
        crate::git::transport::InstallStage::Discovering,
        &spec.short,
        None,
    );
    let mut skills = match spec.subpath.as_deref() {
        Some(subpath) => crate::repo_scanner::scan_skills_in_repo_at(
            checkout.dir(),
            &spec.repo_url,
            subpath,
            true,
        ),
        None => crate::repo_scanner::scan_skills_in_repo(checkout.dir(), &spec.repo_url, true),
    };
    if let Some(filter) = spec.skill_filter.as_deref() {
        skills.retain(|skill| skill.id.eq_ignore_ascii_case(filter));
    }

    // No requested names → install everything discovered (CLI `-y` / scan
    // installs pre-filter, so an empty list here is a genuine "nothing found").
    let units: Vec<InstallUnit> = if names.is_empty() {
        skills
            .iter()
            .filter(|skill| skill.installable)
            .map(|skill| InstallUnit {
                id: skill.id.clone(),
                folder_path: skill.folder_path.clone(),
            })
            .collect()
    } else {
        let mut units = Vec::new();
        for name in names {
            let target = find_target_skill(&skills, Some(name), name)?;
            units.push(InstallUnit {
                id: target.id.clone(),
                folder_path: target.folder_path.clone(),
            });
        }
        units
    };
    if units.is_empty() {
        return Err(format!("No installable Skill found in '{}'", spec.short));
    }

    session.emit_stage(
        crate::git::transport::InstallStage::Materializing,
        &spec.short,
        None,
    );
    checkout
        .materialize(
            &units
                .iter()
                .map(|unit| unit.folder_path.as_str())
                .collect::<Vec<_>>(),
            session,
        )
        .map_err(|error| format!("{error:#}"))?;
    let installed = installer::install_units(checkout.dir(), &spec, &units)
        .map_err(|error| format!("{error:#}"))?;
    installed_skill::invalidate_cache();
    installed.iter().map(|name| load_skill_dto(name)).collect()
}

/// Build the public `Skill` DTO for an installed canonical skill.
pub fn load_skill_dto(name: &str) -> Result<Skill, String> {
    let dir = paths::agents_skill_dir(name);
    let lock = skill_lock::load();
    let entry = lock.skills.get(name);
    let description = ss_core::types::extract_skill_description(&dir);
    let git_url = entry
        .map(|entry| entry.source_url.clone())
        .unwrap_or_default();
    let mut skill = Skill {
        name: name.to_string(),
        description,
        localized_description: None,
        skill_type: if entry
            .is_some_and(|entry| matches!(entry.source_type, skill_lock::SourceType::Local))
        {
            ss_core::types::SkillType::Local
        } else {
            ss_core::types::SkillType::Hub
        },
        stars: 0,
        installed: true,
        update_available: false,
        upstream_change: None,
        last_updated: entry
            .map(|entry| entry.updated_at.clone())
            .unwrap_or_default(),
        git_url,
        tree_hash: entry.and_then(|entry| entry.skill_folder_hash.clone()),
        category: ss_core::types::SkillCategory::None,
        author: None,
        topics: Vec::new(),
        agent_links: Some(Vec::new()),
        source: Some(entry.map(|entry| entry.source.clone()).unwrap_or_default()),
        rank: None,
    };
    skill.agent_links = Some(crate::installed_skill::agent_links_for(name));
    Ok(skill)
}

pub fn uninstall_skill(name: &str) -> Result<(), String> {
    crate::content::validate_skill_name(name)
        .map_err(|error| format!("Invalid Skill name: {error}"))?;
    crate::skill_mutation::policy()
        .ensure_skill_mutation_allowed(name)
        .map_err(|error| error.to_string())?;
    uninstall_skill_locked_unchecked(name)
}

/// Remove a Skill while the caller holds no conflicting transaction.
///
/// Shared-channel install compensation uses this only for Skills it staged in
/// the current transaction; generic entry points go through
/// [`uninstall_skill`] so ownership is checked before content is removed.
pub fn uninstall_skill_locked_unchecked(name: &str) -> Result<(), String> {
    crate::content::validate_skill_name(name)
        .map_err(|error| format!("Invalid Skill name: {error}"))?;
    if local_skill::is_local_skill(name) {
        return local_skill::delete(name).map_err(|error| format!("{error:#}"));
    }
    remove_hub_skill(name).map_err(|error| format!("{error:#}"))
}

/// Agent display names and project names whose SkillStar deployments were removed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeploymentCleanup {
    pub agents: Vec<String>,
    pub projects: Vec<String>,
}

/// Remove SkillStar-owned Agent deployments, then Project deployments.
///
/// Does not touch the canonical copy, the local original, or the install lock.
/// `Err` means an owned deployment is still present; callers must not delete
/// the body or the lock after that.
pub fn clear_owned_deployments(name: &str) -> anyhow::Result<DeploymentCleanup> {
    let agents = crate::deployment::remove_skill_from_all_agents(name)?;
    let projects = crate::projects::remove_skill_from_all_projects(name)?;
    Ok(DeploymentCleanup { agents, projects })
}

fn begin_removal(name: &str) -> anyhow::Result<crate::skill_update::UpdateTransactionGuard> {
    crate::skill_lock::ensure_writable()?;
    let guard = crate::skill_update::acquire_update_transaction_lock()?;
    clear_owned_deployments(name)?;
    Ok(guard)
}

fn remove_hub_skill(name: &str) -> anyhow::Result<()> {
    let _guard = begin_removal(name)?;
    installer::uninstall_canonical(name)?;
    crate::update_state::set(name, false);
    installed_skill::invalidate_cache();
    Ok(())
}

pub(crate) fn remove_local_skill(name: &str) -> anyhow::Result<()> {
    let _guard = begin_removal(name)?;
    local_skill::delete_files_and_lock(name)?;
    installed_skill::invalidate_cache();
    Ok(())
}

/// Settings reset for one local hub link.
///
/// Clears owned deployments, then the hub symlink and the lock entry. The
/// directory under `skills/local` stays. A deployment or lock failure leaves
/// the hub link in place. Does not apply the mutation gate; the settings
/// reset admits names only after `notify_bulk_skill_removal`.
pub fn release_local_hub_link(name: &str) -> anyhow::Result<()> {
    crate::content::validate_skill_name(name)
        .map_err(|error| anyhow::anyhow!("Invalid Skill name: {error}"))?;
    let _guard = begin_removal(name)?;
    local_skill::unlink_hub_and_lock(name)?;
    installed_skill::invalidate_cache();
    Ok(())
}

#[derive(Debug)]
pub struct UninstallSkillFailure {
    pub message: String,
    pub committed: bool,
    pub rollback_complete: bool,
}

/// Channels removal seam: stage-move the canonical copy, drop the lock entry,
/// run the caller's commit, then delete links — restoring the staged copy if
/// the commit fails.
pub fn uninstall_hub_skill_with_commit<E>(
    name: &str,
    commit: impl FnOnce() -> Result<(), E>,
) -> Result<(), UninstallSkillFailure>
where
    E: std::fmt::Display,
{
    let fail = |message: String, committed: bool, rollback_complete: bool| {
        Err(UninstallSkillFailure {
            message,
            committed,
            rollback_complete,
        })
    };
    crate::content::validate_skill_name(name).map_err(|error| UninstallSkillFailure {
        message: format!("Invalid Skill name: {error}"),
        committed: false,
        rollback_complete: true,
    })?;
    if local_skill::is_local_skill(name) {
        return fail(
            format!("Skill '{name}' is local and cannot use the Hub removal transaction"),
            false,
            true,
        );
    }

    let _transaction = match crate::skill_update::acquire_update_transaction_lock() {
        Ok(guard) => guard,
        Err(error) => return fail(format!("{error:#}"), false, true),
    };
    let canonical = paths::agents_skill_dir(name);
    let staging = paths::agents_skills_root().join(format!(".skillstar-remove-{name}"));
    if staging.symlink_metadata().is_ok()
        && let Err(error) = crate::materialize::remove_entry(&staging)
    {
        return fail(
            format!(
                "A previous removal staging path still exists and could not be cleaned: '{}': {error}",
                staging.display()
            ),
            false,
            true,
        );
    }
    let moved = canonical.symlink_metadata().is_ok();
    if moved && let Err(error) = std::fs::rename(&canonical, &staging) {
        return fail(
            format!("Failed to stage Skill '{name}' for removal: {error}"),
            false,
            true,
        );
    }
    if moved {
        crate::materialize::note_transient_born(&staging);
    }
    let previous_entry = skill_lock::load().skills.get(name).cloned();
    if let Err(error) = skill_lock::mutate(|lock| lock.remove(name)) {
        if moved && std::fs::rename(&staging, &canonical).is_ok() {
            crate::materialize::forget_transient_born(&staging);
        }
        return fail(
            format!("Failed to update the install lock: {error}"),
            false,
            true,
        );
    }
    if let Err(error) = commit() {
        // Restore the staged copy and the lock entry.
        let restored = if moved {
            let renamed = std::fs::rename(&staging, &canonical).is_ok();
            if renamed {
                crate::materialize::forget_transient_born(&staging);
            }
            renamed
        } else {
            true
        };
        let lock_restored = match previous_entry {
            Some(entry) => skill_lock::mutate(|lock| {
                lock.skills.insert(name.to_string(), entry);
            })
            .is_ok(),
            None => true,
        };
        return fail(format!("{error}"), false, restored && lock_restored);
    }
    if moved && let Err(error) = crate::materialize::remove_entry(&staging) {
        return fail(
            format!("Skill '{name}' was removed but cleanup failed: {error}"),
            true,
            true,
        );
    }
    // The canonical copy and lock are already committed. A deployment failure
    // stays on this result as committed cleanup, not as a rollback.
    if let Err(error) = clear_owned_deployments(name) {
        return fail(
            format!("Skill '{name}' was removed but deployment cleanup failed: {error:#}"),
            true,
            true,
        );
    }
    crate::update_state::set(name, false);
    installed_skill::invalidate_cache();
    Ok(())
}

#[cfg(test)]
#[path = "skill_removal_tests.rs"]
mod removal_tests;

/// Discover within a fetched source, preserving its ref, scope and identity filter.
pub fn scan_parsed_checkout(
    parsed: &Source,
    repo_dir: std::path::PathBuf,
    full_depth: bool,
) -> (
    String,
    String,
    std::path::PathBuf,
    Vec<crate::repo_scanner::DiscoveredSkill>,
) {
    let mut skills_found = match parsed.subpath.as_deref() {
        Some(subpath) => crate::repo_scanner::scan_skills_in_repo_at(
            &repo_dir,
            &parsed.repo_url,
            subpath,
            full_depth,
        ),
        None => crate::repo_scanner::scan_skills_in_repo(&repo_dir, &parsed.repo_url, full_depth),
    };
    if let Some(skill_filter) = parsed.skill_filter.as_deref() {
        skills_found.retain(|skill| skill.id.eq_ignore_ascii_case(skill_filter));
    }
    (
        parsed.repo_url.clone(),
        parsed.short.clone(),
        repo_dir,
        skills_found,
    )
}

/// A fully materialized, scanned checkout for channel/snapshot consumers.
/// Keep this value alive while reading `dir` —
/// dropping it deletes the temp clone (local sources are borrowed, not owned).
pub struct FetchedScan {
    pub source_url: String,
    pub short: String,
    pub dir: std::path::PathBuf,
    pub skills: Vec<crate::repo_scanner::DiscoveredSkill>,
    _keep: fetch::Checkout,
}

/// Fetch a source and scan it; the temp checkout lives as long as the guard.
pub fn fetch_repo_scanned_in_session(
    input: &str,
    full_depth: bool,
    session: &crate::git::transport::GitOperationSession,
) -> Result<FetchedScan, String> {
    let spec = source_resolver::Source::parse(input)
        .map_err(|error| format!("Invalid source: {error}"))?;
    let checkout = fetch::fetch_source(&spec, session).map_err(|error| format!("{error:#}"))?;
    let (_, _, dir, skills) = scan_parsed_checkout(&spec, checkout.dir().to_path_buf(), full_depth);
    Ok(FetchedScan {
        source_url: spec.repo_url.clone(),
        short: spec.short.clone(),
        dir,
        skills,
        _keep: checkout,
    })
}

#[cfg(test)]
#[path = "skill_install_tests.rs"]
mod tests;
