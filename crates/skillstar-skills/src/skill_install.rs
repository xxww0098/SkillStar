use crate::deployment;
use crate::git::ops as git_ops;
use crate::{installed_skill, local_skill, lockfile, projects, repo_scanner};
use skillstar_core::infra::{fs_ops, paths};
use skillstar_core::types::{
    Skill, SkillCategory, SkillType, extract_github_source_from_url, extract_skill_description,
};
use std::path::{Path, PathBuf};
use tracing::warn;

fn derive_name_hint(url: &str, name: Option<&str>) -> String {
    crate::source_resolver::derive_skill_name_hint(url, name)
}

pub fn find_target_skill<'a>(
    skills_found: &'a [repo_scanner::DiscoveredSkill],
    requested_name: Option<&str>,
    name_hint: &str,
) -> Option<&'a repo_scanner::DiscoveredSkill> {
    // An explicit identity must always match, even when the repository now
    // exposes exactly one differently named Skill. The single-Skill fallback is
    // retained only for callers that did not request an identity.
    if requested_name.is_none() && skills_found.len() == 1 {
        return skills_found.first();
    }

    let search_key = requested_name.unwrap_or(name_hint);
    let search_key_lower = search_key.to_lowercase();

    skills_found
        .iter()
        .find(|s| s.id == search_key || s.id.to_lowercase() == search_key_lower)
}

/// Normalize URL, materialize repo cache, run lockfile-aware scan.
pub fn fetch_repo_scanned(
    url: &str,
    full_depth: bool,
) -> Result<(String, String, PathBuf, Vec<repo_scanner::DiscoveredSkill>), String> {
    fetch_repo_scanned_in_session(
        url,
        full_depth,
        &crate::git::transport::GitOperationSession::public(),
    )
}

pub fn fetch_repo_scanned_in_session(
    url: &str,
    full_depth: bool,
    session: &crate::git::transport::GitOperationSession,
) -> Result<(String, String, PathBuf, Vec<repo_scanner::DiscoveredSkill>), String> {
    let _transaction_guard = crate::skill_update::acquire_update_transaction_lock()
        .map_err(|error| format!("Unable to lock repository scan: {error}"))?;
    ensure_generic_repository_input_mutable(url)?;
    fetch_repo_scanned_detailed_in_session(url, full_depth, session)
        .map_err(|error| format!("{error:#}"))
}

pub fn fetch_repo_scanned_detailed_in_session(
    url: &str,
    full_depth: bool,
    session: &crate::git::transport::GitOperationSession,
) -> anyhow::Result<(String, String, PathBuf, Vec<repo_scanner::DiscoveredSkill>)> {
    use anyhow::Context as _;
    let parsed = crate::source_resolver::Source::parse(url)
        .map_err(|error| anyhow::anyhow!("Invalid source: {error}"))?;
    let repo_dir = repo_scanner::clone_or_fetch_repo_at_in_session(
        &parsed.repo_url,
        &parsed.short,
        parsed.git_ref.as_deref(),
        session,
    )
    .context("Failed to fetch repo")?;
    let mut skills_found = match parsed.subpath.as_deref() {
        Some(subpath) => {
            repo_scanner::scan_skills_in_repo_at(&repo_dir, &parsed.repo_url, subpath, full_depth)
        }
        None => repo_scanner::scan_skills_in_repo(&repo_dir, &parsed.repo_url, full_depth),
    };
    if let Some(skill_filter) = parsed.skill_filter.as_deref() {
        skills_found.retain(|skill| skill.id.eq_ignore_ascii_case(skill_filter));
    }
    Ok((parsed.repo_url, parsed.short, repo_dir, skills_found))
}

#[inline]
fn local_skill_blocks_repo_install(skill_id: &str) -> bool {
    paths::local_skills_dir().join(skill_id).exists()
}

/// Compute tree hash directly from an installed skill's path.
///
/// For symlinked (repo-cached) skills this resolves the symlink target
/// and computes the hash from the real directory, avoiding a redundant
/// lockfile re-read that `install_from_repo` has just written to.
fn compute_tree_hash_for(skills_dir: &Path, installed_name: &str) -> Option<String> {
    let skill_path = skills_dir.join(installed_name);
    let effective_path = fs_ops::read_link_resolved(&skill_path).unwrap_or(skill_path);
    git_ops::compute_tree_hash(&effective_path).ok()
}

fn new_skill_from_install(
    name: String,
    description: String,
    git_url: String,
    tree_hash: Option<String>,
) -> Skill {
    let source = extract_github_source_from_url(&git_url);
    Skill {
        name,
        description,
        localized_description: None,
        skill_type: SkillType::Hub,
        stars: 0,
        installed: true,
        update_available: false,
        upstream_change: None,
        last_updated: chrono::Utc::now().to_rfc3339(),
        git_url,
        tree_hash,
        category: SkillCategory::None,
        author: None,
        topics: Vec::new(),
        agent_links: Some(Vec::new()),
        rank: None,
        source,
    }
}

fn rollback_repo_cache_installs(skills_dir: &Path, names: &[String]) {
    for name in names {
        let path = skills_dir.join(name);
        if path.symlink_metadata().is_ok() {
            let _ = fs_ops::remove_link_or_copy(&path);
        }
    }

    if let Ok(_lock) = lockfile::get_mutex().lock() {
        let lock_path = lockfile::lockfile_path();
        if let Ok(mut lockfile) = lockfile::Lockfile::load(&lock_path) {
            for name in names {
                lockfile.remove(name);
            }
            let _ = lockfile.save(&lock_path);
        }
    }
    installed_skill::invalidate_cache();
}

/// Finalize a batch install from the repo cache.
///
/// The lockfile entries (git URL, source folder, tree hash, content baseline)
/// were already written by [`repo_scanner::install_from_repo_at`]; this step
/// only builds the public `Skill` results and rolls the batch back if that
/// fails. Provenance deliberately never touches the checked-out `SKILL.md`:
/// the shared checkout is read-only for installs, so an update's
/// `git reset --hard` can never wipe locally injected metadata and produce a
/// self-inflicted content divergence.
fn finalize_repo_cache_installs(
    skills_dir: &Path,
    repo_url: &str,
    installed: &[String],
) -> Result<Vec<Skill>, String> {
    let result: Result<Vec<Skill>, String> = {
        let skills = installed
            .iter()
            .map(|name| {
                let dest = skills_dir.join(name);
                new_skill_from_install(
                    name.clone(),
                    extract_skill_description(&dest),
                    repo_url.to_string(),
                    compute_tree_hash_for(skills_dir, name),
                )
            })
            .collect::<Vec<_>>();
        Ok(skills)
    };

    if result.is_err() {
        rollback_repo_cache_installs(skills_dir, installed);
    }
    result
}

enum SameRepoAction {
    Reuse,
    Retarget,
    Reject,
}

fn lock_entry_for(name: &str) -> Option<lockfile::LockEntry> {
    lockfile::Lockfile::load(&lockfile::lockfile_path())
        .ok()?
        .skills
        .into_iter()
        .find(|entry| entry.name == name)
}

fn source_folder_eq(entry: Option<&lockfile::LockEntry>, folder: &str) -> bool {
    entry
        .and_then(|entry| entry.source_folder.as_deref())
        .unwrap_or("")
        == folder
}

/// Reuse only when the hub already points at the requested harness folder.
/// A different folder from the same clone must retarget; another git URL
/// is still a hard collision.
fn existing_same_repo_action(
    skill_id: &str,
    repo_url: &str,
    requested_folder: &str,
    harness_prefix: Option<&str>,
) -> Result<SameRepoAction, String> {
    let entry = lock_entry_for(skill_id);
    let same_repo = entry
        .as_ref()
        .is_some_and(|entry| crate::source_resolver::same_remote_url(&entry.git_url, repo_url));
    if !same_repo {
        return Ok(SameRepoAction::Reject);
    }
    if source_folder_eq(entry.as_ref(), requested_folder) {
        return Ok(SameRepoAction::Reuse);
    }
    if harness_prefix.is_some() {
        return Ok(SameRepoAction::Retarget);
    }
    Ok(SameRepoAction::Reuse)
}

fn requested_skill_not_found_error(names: &[String]) -> String {
    format!(
        "Requested Skill{} '{}' not found in the scanned repository; the source may no longer provide {} or {} may have been deleted or renamed",
        if names.len() == 1 { "" } else { "s" },
        names.join(", "),
        if names.len() == 1 {
            "this Skill"
        } else {
            "these Skills"
        },
        if names.len() == 1 { "it" } else { "they" },
    )
}

fn try_install_from_repo_cache(
    url: &str,
    requested_name: Option<&str>,
    name_hint: &str,
    skills_dir: &Path,
    session: &crate::git::transport::GitOperationSession,
    harness_prefix: Option<&str>,
) -> Result<Option<Skill>, String> {
    let Ok((repo_url, _source, repo_dir, scan_found)) =
        fetch_repo_scanned_detailed_in_session(url, false, session)
    else {
        return Ok(None);
    };
    let parsed =
        crate::source_resolver::Source::parse(url).map_err(|e| format!("Invalid source: {e}"))?;
    let skills_found = if harness_prefix.is_some()
        || scan_found.iter().any(|skill| skill.folder_path.is_empty())
    {
        match crate::discovery::resolve_install_skills(&repo_dir, requested_name, harness_prefix) {
            Ok(skills) => skills,
            Err(error) if harness_prefix.is_some() => return Err(error),
            Err(_) => return Ok(None),
        }
    } else {
        scan_found
    };
    let target = find_target_skill(&skills_found, requested_name, name_hint);

    // Guard against overwriting a local skill whose name matches the repo skill
    if let Some(skill) = &target
        && local_skill_blocks_repo_install(&skill.id)
    {
        warn!(
            target: "install_skill",
            skill_id = %skill.id,
            "repo-cache skill would collide with existing local skill, skipping"
        );
        return Ok(None);
    }

    let Some(skill) = target else {
        if let Some(requested_name) = requested_name {
            return Err(requested_skill_not_found_error(&[
                requested_name.to_string()
            ]));
        }
        return Ok(None);
    };

    let existing_path = skills_dir.join(&skill.id);
    if existing_path.symlink_metadata().is_ok() {
        match existing_same_repo_action(&skill.id, &repo_url, &skill.folder_path, harness_prefix)? {
            SameRepoAction::Reuse => {
                return Ok(Some(new_skill_from_install(
                    skill.id.clone(),
                    extract_skill_description(&existing_path),
                    repo_url,
                    compute_tree_hash_for(skills_dir, &skill.id),
                )));
            }
            SameRepoAction::Retarget => {
                deployment::pin_existing_global_links_to_current_source(&skill.id)
                    .map_err(|error| error.to_string())?;
            }
            SameRepoAction::Reject => {
                return Err(format!("Skill '{}' is already installed", skill.id));
            }
        }
    }

    let targets = vec![repo_scanner::SkillInstallTarget {
        id: skill.id.clone(),
        folder_path: skill.folder_path.clone(),
    }];

    match repo_scanner::install_from_repo_at(
        &repo_dir,
        &repo_url,
        parsed.git_ref.as_deref(),
        &targets,
    ) {
        Ok(installed) if !installed.is_empty() => {
            let mut installed_skills =
                finalize_repo_cache_installs(skills_dir, &repo_url, &installed)?;
            installed_skill::invalidate_cache();
            Ok(installed_skills.pop())
        }
        Ok(_) => Ok(None),
        Err(err) => {
            warn!(target: "install_skill", error = %err, "repo-cache install failed, falling back");
            Ok(None)
        }
    }
}

pub fn harness_prefix_for_agent(agent_id: &str) -> Result<String, String> {
    let profiles = skillstar_agents::list_profiles();
    let profile = profiles.iter().find(|profile| profile.id == agent_id);
    let global = profile.map(|profile| profile.global_skills_dir.to_string_lossy().into_owned());
    crate::pack_layout::pack_harness_prefix(
        agent_id,
        global.as_deref(),
        profile.map(|profile| profile.project_skills_rel.as_str()),
    )
    .ok_or_else(|| format!("No pack harness folder is known for agent '{agent_id}'"))
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
) -> Result<Skill, String> {
    install_skill_in_session(
        url,
        name,
        Some(agent_id),
        &crate::git::transport::GitOperationSession::public(),
    )
}

pub fn install_skill_in_session(
    url: String,
    name: Option<String>,
    agent_id: Option<&str>,
    session: &crate::git::transport::GitOperationSession,
) -> Result<Skill, String> {
    let _transaction_guard = crate::skill_update::acquire_update_transaction_lock()
        .map_err(|error| format!("Unable to lock Skill installation: {error}"))?;
    let harness_prefix = match agent_id {
        Some(id) => Some(harness_prefix_for_agent(id)?),
        None => None,
    };
    install_skill_in_session_locked(url, name, session, harness_prefix.as_deref())
}

fn install_skill_in_session_locked(
    url: String,
    name: Option<String>,
    session: &crate::git::transport::GitOperationSession,
    harness_prefix: Option<&str>,
) -> Result<Skill, String> {
    let skills_dir = paths::hub_skills_dir();
    // Safe here because the caller holds the update transaction lock, so no
    // other install or removal owns a live staging entry. Clearing residue up
    // front keeps a crashed transaction from accumulating in the hub.
    crate::hub_entry::sweep_stale_staging(&skills_dir);
    let name_hint = derive_name_hint(&url, name.as_deref());
    crate::content::validate_skill_name(&name_hint)
        .map_err(|error| format!("Invalid Skill name: {error}"))?;
    crate::skill_mutation::policy()
        .ensure_skill_mutation_allowed(&name_hint)
        .map_err(|error| error.to_string())?;
    ensure_generic_repository_input_mutable(&url)?;

    if harness_prefix.is_none() && skills_dir.join(&name_hint).symlink_metadata().is_ok() {
        return Err(format!("Skill '{}' is already installed", name_hint));
    }
    if local_skill_blocks_repo_install(&name_hint) {
        return Err(format!(
            "Skill '{}' already exists as a local skill",
            name_hint
        ));
    }

    if let Some(skill) = try_install_from_repo_cache(
        &url,
        name.as_deref(),
        &name_hint,
        &skills_dir,
        session,
        harness_prefix,
    )? {
        return Ok(skill);
    }

    let dest = skills_dir.join(&name_hint);
    if dest.symlink_metadata().is_ok() {
        return Err(format!("Skill '{}' is already installed", name_hint));
    }

    if harness_prefix.is_some() {
        return Err(
            "Repository scan failed; refusing to clone the whole repository for a harness-specific install."
                .to_string(),
        );
    }

    if let Err(error) = git_ops::clone_repo_in_session(&url, &dest, session) {
        let _ = fs_ops::remove_dir_all_retry(&dest);
        return Err(error.to_string());
    }

    if crate::discovery::discover_skills_without_dedup(&dest, true, None)
        .iter()
        .any(|skill| !skill.folder_path.is_empty())
    {
        let _ = fs_ops::remove_dir_all_retry(&dest);
        return Err(
            "Repository scan failed; refusing to install the whole repository because catalog or harness skill folders exist."
                .to_string(),
        );
    }

    // The direct-clone fallback is the legacy whole-repo install path. It
    // must not reopen the door the scan path closed: an invalid SKILL.md is
    // just as un-installable here. Fail closed with the same actionable
    // reason and clean up the clone.
    if let Err(reason) = crate::validation::ensure_installable(&dest) {
        let _ = fs_ops::remove_dir_all_retry(&dest);
        return Err(format!(
            "Repository is not installable as a single Skill: {reason}"
        ));
    }

    let result = (|| -> Result<Skill, String> {
        let tree_hash = git_ops::compute_tree_hash(&dest).map_err(|e| e.to_string())?;
        let content_hash = crate::content::snapshot(&name_hint)
            .map_err(|e| format!("Failed to capture installed Skill baseline: {e}"))?
            .content_hash;

        let _lock = lockfile::get_mutex()
            .lock()
            .map_err(|_| "Lockfile mutex poisoned".to_string())?;
        let lock_path = lockfile::lockfile_path();
        let mut lockfile = lockfile::Lockfile::load(&lock_path)
            .map_err(|e| format!("Failed to load lockfile '{}': {}", lock_path.display(), e))?;
        lockfile.upsert(crate::lockfile::LockEntry {
            name: name_hint.clone(),
            git_url: url.clone(),
            git_ref: None,
            tree_hash: tree_hash.clone(),
            content_hash: Some(content_hash),
            content_hash_version: Some(crate::content::SNAPSHOT_HASH_VERSION),
            installed_at: chrono::Utc::now().to_rfc3339(),
            source_folder: None,
        });
        lockfile
            .save(&lock_path)
            .map_err(|e| format!("Failed to save lockfile '{}': {}", lock_path.display(), e))?;
        installed_skill::invalidate_cache();

        let description = extract_skill_description(&dest);
        Ok(new_skill_from_install(
            name_hint.clone(),
            description,
            url.clone(),
            Some(tree_hash),
        ))
    })();
    if result.is_err() {
        let _ = skillstar_core::infra::fs_ops::remove_dir_all_retry(&dest);
    }
    result
}

/// Install multiple skills from the same repository URL in a single batch.
/// This prevents git clone/fetch overlap and lockfile serialization issues when
/// multiple skills share the same repository.
pub fn install_skills_batch(url: &str, names: &[String]) -> Result<Vec<Skill>, String> {
    install_skills_batch_in_session(
        url,
        names,
        None,
        &crate::git::transport::GitOperationSession::public(),
    )
}

pub fn install_skills_batch_in_session(
    url: &str,
    names: &[String],
    agent_id: Option<&str>,
    session: &crate::git::transport::GitOperationSession,
) -> Result<Vec<Skill>, String> {
    let _transaction_guard = crate::skill_update::acquire_update_transaction_lock()
        .map_err(|error| format!("Unable to lock Skill batch installation: {error}"))?;
    if names.is_empty() {
        return Ok(Vec::new());
    }

    for name in names {
        crate::skill_mutation::policy()
            .ensure_skill_mutation_allowed(name)
            .map_err(|error| error.to_string())?;
    }

    let parsed =
        crate::source_resolver::Source::parse(url).map_err(|e| format!("Invalid source: {e}"))?;
    crate::skill_mutation::policy()
        .ensure_repository_mutation_allowed(&parsed.repo_url)
        .map_err(|error| error.to_string())?;
    let skills_dir = paths::hub_skills_dir();
    let (repo_url, _source, repo_dir, scan_found) =
        fetch_repo_scanned_detailed_in_session(url, false, session)
            .map_err(|error| format!("{error:#}"))?;
    let harness_prefix = match agent_id {
        Some(id) => Some(harness_prefix_for_agent(id)?),
        None => None,
    };
    let skills_found = if harness_prefix.is_some()
        || scan_found.iter().any(|skill| skill.folder_path.is_empty())
    {
        let mut resolved = Vec::new();
        for name in names {
            resolved.extend(crate::discovery::resolve_install_skills(
                &repo_dir,
                Some(name),
                harness_prefix.as_deref(),
            )?);
        }
        resolved
    } else {
        scan_found
    };
    let existing_lock = lockfile::Lockfile::load(&lockfile::lockfile_path())
        .map_err(|error| format!("Failed to load Skill lockfile: {error}"))?;

    let mut targets = Vec::new();
    let mut fallback_names = Vec::new();
    let mut missing_names = Vec::new();

    for name in names {
        // First try to find a match in the scanned repo
        let target = find_target_skill(&skills_found, Some(name), name);
        if let Some(skill) = target {
            if local_skill_blocks_repo_install(&skill.id) {
                warn!(
                    target: "install_skills_batch",
                    skill_id = %skill.id,
                    "repo-cache skill would collide with existing local skill, skipping"
                );
                continue;
            }
            if skills_dir.join(&skill.id).symlink_metadata().is_ok() {
                let entry = existing_lock
                    .skills
                    .iter()
                    .find(|entry| entry.name == skill.id);
                let same_source = entry.is_some_and(|entry| {
                    crate::source_resolver::same_remote_url(&entry.git_url, &repo_url)
                });
                if same_source {
                    if harness_prefix.is_some() && !source_folder_eq(entry, &skill.folder_path) {
                        deployment::pin_existing_global_links_to_current_source(&skill.id)
                            .map_err(|error| error.to_string())?;
                    } else {
                        continue;
                    }
                } else {
                    return Err(format!(
                        "Skill '{}' already exists from a different source; remove it or choose another skill",
                        skill.id
                    ));
                }
            }
            targets.push(repo_scanner::SkillInstallTarget {
                id: skill.id.clone(),
                folder_path: skill.folder_path.clone(),
            });
        } else {
            missing_names.push(name.clone());
        }
    }

    if !missing_names.is_empty() {
        return Err(requested_skill_not_found_error(&missing_names));
    }

    let mut installed_skills = Vec::new();

    if !targets.is_empty() {
        match repo_scanner::install_from_repo_at(
            &repo_dir,
            &repo_url,
            parsed.git_ref.as_deref(),
            &targets,
        ) {
            Ok(installed) => {
                installed_skills.extend(finalize_repo_cache_installs(
                    &skills_dir,
                    &repo_url,
                    &installed,
                )?);
                installed_skill::invalidate_cache();
            }
            Err(e) => {
                warn!(target: "install_skills_batch", error = %e, "batch repo install failed");
                // Fallback: all targets must be installed via direct fallback
                for t in targets {
                    fallback_names.push(t.id);
                }
            }
        }
    }

    // Process fallbacks one by one
    let mut fallback_installed = Vec::new();
    for name in fallback_names {
        match install_skill_in_session_locked(
            url.to_string(),
            Some(name),
            session,
            harness_prefix.as_deref(),
        ) {
            Ok(skill) => {
                fallback_installed.push(skill.name.clone());
                installed_skills.push(skill);
            }
            Err(error) => {
                let mut rollback_errors = Vec::new();
                for installed_name in fallback_installed.iter().rev() {
                    if let Err(rollback_error) = uninstall_skill_locked_unchecked(installed_name) {
                        rollback_errors.push(format!("{installed_name}: {rollback_error}"));
                    }
                }
                let rollback = if rollback_errors.is_empty() {
                    String::new()
                } else {
                    format!("; rollback also failed: {}", rollback_errors.join(", "))
                };
                return Err(format!("Fallback install failed: {error}{rollback}"));
            }
        }
    }

    Ok(installed_skills)
}

pub fn uninstall_skill(name: &str) -> Result<(), String> {
    crate::content::validate_skill_name(name)
        .map_err(|error| format!("Invalid Skill name: {error}"))?;
    let _transaction_guard = crate::skill_update::acquire_update_transaction_lock()
        .map_err(|error| format!("Unable to lock Skill removal: {error}"))?;
    crate::skill_mutation::policy()
        .ensure_skill_mutation_allowed(name)
        .map_err(|error| error.to_string())?;
    uninstall_skill_locked_unchecked(name)
}

/// Remove a Skill while the caller holds the global update transaction lock.
///
/// Shared-channel install compensation uses this only for Skills it staged in
/// the current transaction. Generic entry points must use [`uninstall_skill`]
/// so ownership is checked before content is removed.
pub fn uninstall_skill_locked_unchecked(name: &str) -> Result<(), String> {
    if local_skill::is_local_skill(name) {
        local_skill::delete(name).map_err(|e| e.to_string())?;
        installed_skill::invalidate_cache();
        return Ok(());
    }

    uninstall_hub_skill_with_commit(name, || Ok::<(), std::convert::Infallible>(()))
        .map_err(|failure| failure.message)
}

fn ensure_generic_repository_input_mutable(input: &str) -> Result<(), String> {
    if let Ok(source) = crate::source_resolver::Source::parse(input) {
        return crate::skill_mutation::policy()
            .ensure_repository_mutation_allowed(&source.repo_url)
            .map_err(|error| error.to_string());
    }
    let path = std::path::Path::new(input);
    if !path.exists() {
        return Ok(());
    }
    if let Ok(remote) = crate::git::ops::remote_origin_url(path) {
        crate::skill_mutation::policy()
            .ensure_repository_mutation_allowed(&remote)
            .map_err(|error| error.to_string())?;
    }
    let canonical = std::fs::canonicalize(path).map_err(|error| error.to_string())?;
    let hub = paths::hub_skills_dir();
    let lockfile =
        lockfile::Lockfile::load(&lockfile::lockfile_path()).map_err(|error| error.to_string())?;
    for entry in lockfile.skills {
        let same_checkout = crate::repo_link::repo_root_of(&hub.join(&entry.name))
            .and_then(|root| std::fs::canonicalize(root).ok())
            .is_some_and(|root| root == canonical);
        if same_checkout {
            crate::skill_mutation::policy()
                .ensure_skill_mutation_allowed(&entry.name)
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

#[derive(Debug)]
pub struct UninstallSkillFailure {
    pub message: String,
    pub committed: bool,
    pub rollback_complete: bool,
}

pub fn uninstall_hub_skill_with_commit<E>(
    name: &str,
    commit: impl FnOnce() -> Result<(), E>,
) -> Result<(), UninstallSkillFailure>
where
    E: std::fmt::Display,
{
    crate::content::validate_skill_name(name).map_err(|error| UninstallSkillFailure {
        message: format!("Invalid Skill name: {error}"),
        committed: false,
        rollback_complete: true,
    })?;
    if local_skill::is_local_skill(name) {
        return Err(UninstallSkillFailure {
            message: format!("Skill '{name}' is local and cannot use the Hub removal transaction"),
            committed: false,
            rollback_complete: true,
        });
    }

    let skills_dir = paths::hub_skills_dir();
    let path = skills_dir.join(name);
    let _lock = lockfile::get_mutex()
        .lock()
        .map_err(|_| UninstallSkillFailure {
            message: "Lockfile mutex poisoned".to_string(),
            committed: false,
            rollback_complete: true,
        })?;
    let lock_path = lockfile::lockfile_path();
    let mut lf = lockfile::Lockfile::load(&lock_path).map_err(|error| UninstallSkillFailure {
        message: format!("Failed to load lockfile '{}': {error}", lock_path.display()),
        committed: false,
        rollback_complete: true,
    })?;
    let previous_entry = lf
        .skills
        .iter()
        .find(|entry| entry.name.eq_ignore_ascii_case(name))
        .cloned();
    // Deliberately pid-free. Every caller of this function holds the update
    // transaction lock (an in-process mutex plus a cross-process file lock), so
    // no second removal can be live at the same time — while a pid in the name
    // meant the self-heal below only ever matched residue from the *current*
    // process, leaving anything a crash or power loss left behind forever.
    crate::hub_entry::sweep_stale_staging(&skills_dir);
    let staging = skills_dir.join(format!(".skillstar-remove-{name}"));
    if staging.symlink_metadata().is_ok() {
        // A leftover staging path means a previous removal crashed mid-flight.
        // Clean it up instead of permanently blocking future uninstalls of
        // this Skill.
        if let Err(error) = fs_ops::remove_link_or_copy(&staging) {
            return Err(UninstallSkillFailure {
                message: format!(
                    "A previous removal staging path still exists and could not be cleaned: '{}': {error}",
                    staging.display()
                ),
                committed: false,
                rollback_complete: true,
            });
        }
    }
    let moved = path.symlink_metadata().is_ok();
    if moved {
        std::fs::rename(&path, &staging).map_err(|error| UninstallSkillFailure {
            message: format!("Failed to stage Skill '{name}' for removal: {error}"),
            committed: false,
            rollback_complete: true,
        })?;
    }
    lf.remove(name);
    if let Err(error) = lf.save(&lock_path) {
        let restore_failure = moved
            .then(|| std::fs::rename(&staging, &path).err())
            .flatten();
        let restore_error = restore_failure
            .as_ref()
            .map(|restore| format!("; restoring the Skill also failed: {restore}"))
            .unwrap_or_default();
        return Err(UninstallSkillFailure {
            message: format!(
                "Failed to save lockfile '{}': {error}{restore_error}",
                lock_path.display(),
            ),
            committed: false,
            rollback_complete: restore_failure.is_none(),
        });
    }
    if let Err(error) = commit() {
        if let Some(entry) = previous_entry {
            lf.upsert(entry);
        }
        let lock_restore_failure = lf.save(&lock_path).err();
        let lock_restore = lock_restore_failure
            .as_ref()
            .map(|restore| format!("; restoring the lockfile also failed: {restore}"))
            .unwrap_or_default();
        let content_restore_failure = moved
            .then(|| std::fs::rename(&staging, &path).err())
            .flatten();
        let content_restore = content_restore_failure
            .as_ref()
            .map(|restore| format!("; restoring the Skill also failed: {restore}"))
            .unwrap_or_default();
        return Err(UninstallSkillFailure {
            message: format!(
                "Unable to commit removed Skill metadata: {error}{lock_restore}{content_restore}"
            ),
            committed: false,
            rollback_complete: lock_restore_failure.is_none() && content_restore_failure.is_none(),
        });
    }
    drop(_lock);

    let mut cleanup_failures = Vec::new();
    if moved && let Err(error) = fs_ops::remove_link_or_copy(&staging) {
        cleanup_failures.push(format!(
            "remove staged hub content '{}': {error}",
            staging.display()
        ));
        warn!(
            target: "uninstall_skill",
            path = %staging.display(),
            error = %error,
            "Skill was removed but its staging path could not be cleaned"
        );
    }

    if let Err(error) = deployment::remove_skill_from_all_agents(name) {
        cleanup_failures.push(format!("remove Agent deployments: {error:#}"));
    }
    if let Err(error) = projects::remove_skill_from_all_projects(name) {
        cleanup_failures.push(format!("remove Project deployments: {error:#}"));
    }
    installed_skill::invalidate_cache();

    if cleanup_failures.is_empty() {
        Ok(())
    } else {
        Err(UninstallSkillFailure {
            message: format!(
                "Skill '{name}' was removed from the hub, but cleanup is incomplete: {}",
                cleanup_failures.join(", ")
            ),
            committed: true,
            rollback_complete: false,
        })
    }
}

#[cfg(test)]
#[path = "skill_install_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "skill_install_harness_tests.rs"]
mod harness_retarget_tests;
