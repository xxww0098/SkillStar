mod divergence;
mod plan;
mod resolve;
mod transaction;

use anyhow::{Context, Result};
use serde::Serialize;
use skillstar_core::types::{
    Skill, SkillCategory, SkillType, extract_github_source_from_url, extract_skill_description,
};
use std::collections::VecDeque;
use std::path::Path;
use std::sync::Mutex;

use crate::git::ops as git_ops;
use crate::lockfile::LockEntry;
use crate::{
    content, deployment, installed_skill, local_skill, lockfile, projects, repo_link, repo_scanner,
};
use divergence::SourceRemovedStop;
use plan::{SiblingState, UpdatePlan};
pub use transaction::acquire_update_transaction_lock;

#[cfg(test)]
use divergence::same_physical_path;
pub use divergence::{LocalDivergenceReason, LocalDivergenceResolution, SkillUpdateBlocked};
pub use divergence::{inspect_skill_local_divergence, suggested_local_name};
pub use resolve::resolve_skill_local_divergence_locked;
pub use resolve::{
    ResolveSkillUpdateResult, resolve_skill_update, resolve_skill_update_in_session,
};

/// Hold the process/file mutation lease while an application-level maintenance
/// operation changes Hub content, lock metadata, or repository caches.
pub fn acquire_skill_mutation_lease() -> Result<impl Drop> {
    acquire_update_transaction_lock()
}

/// Result of a hub skill update, including any project-level cascade work.
#[derive(Debug, Clone)]
struct UpdateOutcome {
    tree_hash: String,
    git_url: String,
    sibling_names: Vec<String>,
    agent_links: Vec<String>,
    /// Per-agent re-link failures ("Agent: error"). The update itself
    /// succeeded; these tell the UI which agent deployments need attention.
    agent_link_failures: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateResult {
    pub skill: Skill,
    pub siblings_cleared: Vec<String>,
    pub agent_link_failures: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillUpdateFailure {
    pub name: String,
    pub error: String,
}

/// A Skill a shared channel owns, which the generic update path declines by
/// design rather than fails on.
#[derive(Debug, Clone, Serialize)]
pub struct SkillUpdateChannelManaged {
    pub name: String,
    pub repository_id: u64,
}

/// Outcome of a batch update.
///
/// `skipped` names were not updated because a skill sharing their repository
/// was — their content moved anyway. A failed update reports every name it
/// would have covered, so nothing is quietly counted as done.
///
/// `channel_managed` is a declined-by-design bucket, not a failure: a shared
/// channel owns those Skills and updates them through its own flow. They used
/// to land in `failed`, which made `skillstar update` print errors and exit
/// nonzero on any machine that had ever subscribed to a channel.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SkillUpdateReport {
    pub updated: Vec<UpdateResult>,
    pub blocked: Vec<SkillUpdateBlocked>,
    pub failed: Vec<SkillUpdateFailure>,
    pub skipped: Vec<String>,
    pub channel_managed: Vec<SkillUpdateChannelManaged>,
}

/// Update several skills, pulling each repository at most once.
///
/// This is where "skills sharing a repository need one update between them"
/// lives. It used to live in the UI, alongside a second copy of the sibling
/// fan-out it was mirroring.
pub fn update_skills(names: &[String]) -> SkillUpdateReport {
    update_skills_in_session(names, &crate::git::transport::GitOperationSession::public())
}

pub fn update_skills_in_session(
    names: &[String],
    session: &crate::git::transport::GitOperationSession,
) -> SkillUpdateReport {
    let mut report = SkillUpdateReport::default();
    let valid_names: Vec<String> = names
        .iter()
        .filter_map(|name| match content::validate_skill_name(name) {
            Ok(()) => match ensure_ordinary_update_allowed(name) {
                Ok(()) => Some(name.clone()),
                // The gate stays the authority on *whether* to proceed; the
                // ownership query below only explains why. If it cannot
                // positively name an owning channel, this remains a failure.
                Err(error) => {
                    match channel_owner_for_ordinary_update(name) {
                        Ok(Some(repository_id)) => {
                            report.channel_managed.push(SkillUpdateChannelManaged {
                                name: name.clone(),
                                repository_id,
                            });
                        }
                        _ => report.failed.push(SkillUpdateFailure {
                            name: name.clone(),
                            error: error.to_string(),
                        }),
                    }
                    None
                }
            },
            Err(error) => {
                report.failed.push(SkillUpdateFailure {
                    name: name.clone(),
                    error: format!("Invalid Skill name: {error}"),
                });
                None
            }
        })
        .collect();
    if valid_names.is_empty() {
        return report;
    }
    let entries = match lockfile::Lockfile::load(&lockfile::lockfile_path()) {
        Ok(lockfile) => lockfile.skills,
        Err(error) => {
            let error = format!("failed to load the Skill lockfile before update: {error:#}");
            report
                .failed
                .extend(valid_names.iter().map(|name| SkillUpdateFailure {
                    name: name.clone(),
                    error: error.clone(),
                }));
            return report;
        }
    };
    let groups = plan::group_by_repo(&entries, &valid_names, checkout_key);

    let mut runnable = Vec::new();
    for group in groups {
        let blocked = divergence::local_divergences_for_group(&entries, &group);
        if blocked.is_empty() {
            runnable.push(group);
        } else {
            report.blocked.extend(blocked);
        }
    }

    for (group, outcome) in run_groups(runnable, session) {
        match outcome {
            Ok(result) => {
                report.updated.push(result);
                report.skipped.extend(group.covered);
            }
            // The pull found the source no longer ships these Skills and rolled
            // the checkout back, so this is a decision to put to the user — not
            // a failure to report.
            Err(err) => match take_source_removed_stop(err) {
                Ok(blocked) => report.blocked.extend(blocked),
                Err(err) => {
                    let error = format!("{err:#}");
                    report.failed.push(SkillUpdateFailure {
                        name: group.representative,
                        error: error.clone(),
                    });
                    // The rest of the repository did not move either.
                    for name in group.covered {
                        report.failed.push(SkillUpdateFailure {
                            name,
                            error: error.clone(),
                        });
                    }
                }
            },
        }
    }
    report
}

fn checkout_key(entry: &LockEntry) -> Option<String> {
    let path = skillstar_core::infra::paths::hub_skills_dir().join(&entry.name);
    repo_link::repo_root_of(&path).map(|root| root.to_string_lossy().into_owned())
}

pub fn reconstruct_lock_entry(name: &str) -> Result<LockEntry> {
    let skill_path = skillstar_core::infra::paths::hub_skills_dir().join(name);
    let cached_root = repo_link::repo_root_of(&skill_path);
    let repo_root = cached_root
        .clone()
        .or_else(|| git_ops::find_repo_root(&skill_path));
    let repo_root = repo_root
        .with_context(|| format!("Cannot reconstruct the managed Git checkout for '{name}'"))?;
    let source_folder = if cached_root.is_some() {
        actual_repo_pathspec(&skill_path, &repo_root)?
    } else {
        None
    };
    let tree_hash = match source_folder.as_deref() {
        Some(folder) => git_ops::compute_subtree_hash(&repo_root, folder)?,
        None => git_ops::compute_tree_hash(&repo_root)?,
    };

    Ok(LockEntry {
        name: name.to_string(),
        git_url: git_ops::remote_origin_url(&repo_root)?,
        git_ref: crate::update_checker::configured_git_ref(&repo_root),
        tree_hash,
        content_hash: None,
        content_hash_version: None,
        installed_at: chrono::Utc::now().to_rfc3339(),
        source_folder,
    })
}

/// Run one update per repository, a few at a time, and restore request order.
///
/// Separate repositories are separate checkouts, so they pull independently;
/// the bound keeps a large "update all" from opening a fetch per repository at
/// once.
fn run_groups(
    groups: Vec<plan::RepoGroup>,
    session: &crate::git::transport::GitOperationSession,
) -> Vec<(plan::RepoGroup, Result<UpdateResult>)> {
    if groups.len() <= 1 {
        return groups
            .into_iter()
            .map(|group| {
                let outcome = update_skill_in_session(&group.representative, session);
                (group, outcome)
            })
            .collect();
    }

    let queue: Mutex<VecDeque<(usize, plan::RepoGroup)>> =
        Mutex::new(groups.into_iter().enumerate().collect());
    let done: Mutex<Vec<(usize, plan::RepoGroup, Result<UpdateResult>)>> = Mutex::new(Vec::new());
    let workers = update_concurrency_limit().min(lock(&queue).len());

    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let Some((index, group)) = lock(&queue).pop_front() else {
                        return;
                    };
                    let outcome = update_skill_in_session(&group.representative, session);
                    lock(&done).push((index, group, outcome));
                }
            });
        }
    });

    let mut results = done.into_inner().unwrap_or_else(|err| err.into_inner());
    results.sort_by_key(|(index, _, _)| *index);
    results
        .into_iter()
        .map(|(_, group, outcome)| (group, outcome))
        .collect()
}

/// A panicking worker leaves the queue intact — its entries are plain data
/// with no invariant to corrupt — so recover rather than poisoning the batch.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|err| err.into_inner())
}

fn update_concurrency_limit() -> usize {
    std::thread::available_parallelism()
        .map(|parallelism| parallelism.get().clamp(2, 4))
        .unwrap_or(3)
}

fn compute_hash_for_skill_entry(skill_path: &Path, source_folder: Option<&str>) -> Option<String> {
    if let Some(folder) = source_folder.filter(|folder| !folder.is_empty()) {
        let repo_root = repo_link::repo_root_of(skill_path)?;
        return git_ops::compute_subtree_hash(&repo_root, folder).ok();
    }

    if let Some(repo_root) = repo_link::repo_root_of(skill_path) {
        return git_ops::compute_tree_hash(&repo_root)
            .ok()
            .or_else(|| git_ops::compute_tree_hash(skill_path).ok());
    }

    git_ops::compute_tree_hash(skill_path).ok()
}

pub fn update_skill(name: &str) -> Result<UpdateResult> {
    update_skill_in_session(name, &crate::git::transport::GitOperationSession::public())
}

pub fn update_skill_in_session(
    name: &str,
    session: &crate::git::transport::GitOperationSession,
) -> Result<UpdateResult> {
    let _transaction_guard = acquire_update_transaction_lock()?;
    content::validate_skill_name(name).map_err(anyhow::Error::from)?;
    ensure_ordinary_update_allowed(name)?;
    let entries = lockfile::Lockfile::load(&lockfile::lockfile_path())
        .context("failed to load the Skill lockfile before update")?
        .skills;
    let group = plan::RepoGroup {
        representative: name.to_string(),
        covered: Vec::new(),
    };
    if let Some(blocked) = divergence::local_divergences_for_group(&entries, &group)
        .into_iter()
        .next()
    {
        if blocked.reason.is_source_gone() {
            anyhow::bail!(
                "Skill '{}' is no longer shipped by its source; keep it as '{}' or remove it before updating",
                blocked.name,
                blocked.suggested_local_name
            );
        }
        anyhow::bail!(
            "Skill '{}' has local divergence; preserve it as '{}' or explicitly discard it before updating",
            blocked.name,
            blocked.suggested_local_name
        );
    }
    update_skill_unchecked_locked(name, session)
}

/// Take a "source removed" stop out of a failed update, or hand the error back.
///
/// The update already rolled its checkout back, so the Skills named in the stop
/// still have their content on disk. Callers turn them into a decision for the
/// user instead of reporting a failure.
pub(crate) fn take_source_removed_stop(
    error: anyhow::Error,
) -> std::result::Result<Vec<SkillUpdateBlocked>, anyhow::Error> {
    match error.downcast_ref::<SourceRemovedStop>() {
        Some(stop) => Ok(divergence::blocked_for_dropped_source(&stop.names)),
        None => Err(error),
    }
}

/// Channel that owns `name` for the purposes of the generic update path, if
/// any — the Skill itself, a sibling sharing its checkout, or that sibling's
/// repository. Mirrors [`ensure_ordinary_update_allowed`]'s reach so a refusal
/// can be reported as "a channel owns this" instead of as an error.
fn channel_owner_for_ordinary_update(name: &str) -> Result<Option<u64>> {
    let policy = crate::skill_mutation::policy();
    if let Some(repository_id) = policy.managed_repository_for_skill(name)? {
        return Ok(Some(repository_id));
    }
    let entries = lockfile::Lockfile::load(&lockfile::lockfile_path())?.skills;
    let hub = skillstar_core::infra::paths::hub_skills_dir();
    let requested_root = repo_link::repo_root_of(&hub.join(name));
    for entry in &entries {
        let same_checkout = requested_root.as_ref().is_some_and(|root| {
            repo_link::repo_root_of(&hub.join(&entry.name)).as_ref() == Some(root)
        });
        if entry.name.eq_ignore_ascii_case(name) || same_checkout {
            if let Some(repository_id) = policy.managed_repository_for_skill(&entry.name)? {
                return Ok(Some(repository_id));
            }
            if let Some(repository_id) = policy.managed_repository_for_url(&entry.git_url)? {
                return Ok(Some(repository_id));
            }
        }
    }
    Ok(None)
}

fn ensure_ordinary_update_allowed(name: &str) -> Result<()> {
    crate::skill_mutation::policy().ensure_skill_mutation_allowed(name)?;
    let entries = lockfile::Lockfile::load(&lockfile::lockfile_path())
        .context("failed to load the Skill lockfile before checking shared-channel ownership")?
        .skills;
    let hub = skillstar_core::infra::paths::hub_skills_dir();
    let requested_root = repo_link::repo_root_of(&hub.join(name));
    for entry in &entries {
        let same_checkout = requested_root.as_ref().is_some_and(|root| {
            repo_link::repo_root_of(&hub.join(&entry.name)).as_ref() == Some(root)
        });
        if entry.name.eq_ignore_ascii_case(name) || same_checkout {
            crate::skill_mutation::policy().ensure_skill_mutation_allowed(&entry.name)?;
            crate::skill_mutation::policy().ensure_repository_mutation_allowed(&entry.git_url)?;
        }
    }
    Ok(())
}

fn actual_repo_pathspec(skill_path: &Path, repo_root: &Path) -> Result<Option<String>> {
    let target =
        skillstar_core::infra::fs_ops::read_link_resolved(skill_path).with_context(|| {
            format!(
                "failed to resolve managed Skill link '{}'",
                skill_path.display()
            )
        })?;
    let target = canonicalize_with_missing_tail(&target).with_context(|| {
        format!(
            "failed to resolve managed Skill target '{}'",
            target.display()
        )
    })?;
    let repo_root = std::fs::canonicalize(repo_root).with_context(|| {
        format!(
            "failed to resolve managed repository '{}'",
            repo_root.display()
        )
    })?;
    let relative = target.strip_prefix(&repo_root).with_context(|| {
        format!(
            "managed Skill target '{}' escapes repository '{}'",
            target.display(),
            repo_root.display()
        )
    })?;
    if relative.as_os_str().is_empty() {
        return Ok(None);
    }
    if !relative
        .components()
        .all(|component| matches!(component, std::path::Component::Normal(_)))
    {
        anyhow::bail!("managed Skill path is not a safe repository-relative path");
    }
    Ok(Some(relative.to_string_lossy().replace('\\', "/")))
}

fn canonicalize_with_missing_tail(path: &Path) -> Result<std::path::PathBuf> {
    let mut existing = path.to_path_buf();
    let mut tail = Vec::new();
    loop {
        match std::fs::canonicalize(&existing) {
            Ok(mut canonical) => {
                for component in tail.iter().rev() {
                    canonical.push(component);
                }
                return Ok(canonical);
            }
            Err(error) => {
                let Some(name) = existing.file_name().map(|name| name.to_os_string()) else {
                    return Err(error.into());
                };
                tail.push(name);
                if !existing.pop() {
                    return Err(error.into());
                }
            }
        }
    }
}

fn update_skill_unchecked_locked(
    name: &str,
    session: &crate::git::transport::GitOperationSession,
) -> Result<UpdateResult> {
    let outcome = apply_update_locked(name, session)?;
    let path = skillstar_core::infra::paths::hub_skills_dir().join(name);
    let description = content::resolve_content_dir(name)
        .map(|dir| extract_skill_description(&dir))
        .unwrap_or_else(|| extract_skill_description(&path));
    let source = extract_github_source_from_url(&outcome.git_url);
    let skill_type = if local_skill::is_local_skill(name) {
        SkillType::Local
    } else {
        SkillType::Hub
    };

    Ok(UpdateResult {
        skill: Skill {
            name: name.to_string(),
            description,
            localized_description: None,
            skill_type,
            stars: 0,
            installed: true,
            update_available: false,
            last_updated: chrono::Utc::now().to_rfc3339(),
            git_url: outcome.git_url,
            tree_hash: Some(outcome.tree_hash),
            category: SkillCategory::None,
            author: None,
            topics: Vec::new(),
            agent_links: Some(outcome.agent_links),
            rank: None,
            source,
        },
        siblings_cleared: outcome.sibling_names,
        agent_link_failures: outcome.agent_link_failures,
    })
}

/// Look up what a sibling of the updated skill looks like on disk.
///
/// This is the one thing [`plan::plan_update`] cannot decide on its own, so it
/// is injected — which is also what lets the planning tests run without a hub
/// directory or a git remote.
fn sibling_state_on_disk(
    skills_dir: &Path,
    target_repo_root: Option<&Path>,
    entry: &LockEntry,
) -> SiblingState {
    let sibling_path = skills_dir.join(&entry.name);
    if !sibling_path.exists() {
        return SiblingState::Absent;
    }
    if target_repo_root.is_some()
        && repo_link::repo_root_of(&sibling_path).as_deref() != target_repo_root
    {
        return SiblingState::Absent;
    }
    SiblingState::Present(compute_hash_for_skill_entry(
        &sibling_path,
        entry.source_folder.as_deref(),
    ))
}

fn apply_hash_writes(lockfile: &mut lockfile::Lockfile, writes: &[(String, String)]) {
    for (name, hash) in writes {
        if let Some(entry) = lockfile.skills.iter_mut().find(|entry| entry.name == *name) {
            entry.tree_hash = hash.clone();
        }
    }
}

/// Re-baseline every installed Skill backed by `repo_dir` after that checkout
/// was moved under them.
///
/// A repo cache is shared by every Skill installed from the same repository,
/// but only the Skill being installed or updated gets a fresh `content_hash`.
/// Installing a second Skill from a repository therefore used to advance the
/// first one's files via `reset --hard` while leaving its baseline on the old
/// commit, and the resulting phantom divergence was unrecoverable: further
/// installs and scans failed in `ensure_installed_checkout_is_clean`, updates
/// failed in `local_divergences_for_group`, and both told the user to preserve
/// or discard edits they never made.
///
/// Only call this immediately after a successful reset whose caller already
/// proved the worktree was clean — then any content change is upstream's, not
/// the user's. Skills whose content is absent after the reset are left
/// untouched so the update path can still report a dropped source.
///
/// Returns the names whose baseline moved.
pub fn refresh_baselines_after_checkout_reset(repo_dir: &Path) -> Result<Vec<String>> {
    let canonical_repo = std::fs::canonicalize(repo_dir)
        .with_context(|| format!("failed to resolve repo cache '{}'", repo_dir.display()))?;
    let skills_dir = skillstar_core::infra::paths::hub_skills_dir();

    let _lock = lockfile::get_mutex()
        .lock()
        .map_err(|_| anyhow::anyhow!("lockfile mutex poisoned"))?;
    let lock_path = lockfile::lockfile_path();
    let mut lf = lockfile::Lockfile::load(&lock_path)
        .with_context(|| format!("failed to load lockfile '{}'", lock_path.display()))?;

    let cotenants = lf
        .skills
        .iter()
        .filter(|entry| {
            repo_link::repo_root_of(&skills_dir.join(&entry.name))
                .and_then(|root| std::fs::canonicalize(root).ok())
                .is_some_and(|root| root == canonical_repo)
        })
        .map(|entry| entry.name.clone())
        .collect::<Vec<_>>();

    let mut refreshed = Vec::new();
    for name in cotenants {
        // A Skill the reset removed from the checkout keeps its old baseline:
        // that mismatch is how the update path recognises a dropped source.
        let Ok(snapshot) = content::snapshot(&name) else {
            continue;
        };
        let Some(entry) = lf.skills.iter_mut().find(|entry| entry.name == name) else {
            continue;
        };
        let unchanged = entry.content_hash.as_deref() == Some(snapshot.content_hash.as_str())
            && entry.content_hash_version == Some(content::SNAPSHOT_HASH_VERSION);
        if unchanged {
            continue;
        }
        entry.content_hash = Some(snapshot.content_hash);
        entry.content_hash_version = Some(content::SNAPSHOT_HASH_VERSION);
        refreshed.push(name);
    }

    if !refreshed.is_empty() {
        lf.save(&lock_path)
            .with_context(|| format!("failed to save lockfile '{}'", lock_path.display()))?;
        tracing::info!(
            target: "skills",
            repo = %repo_dir.display(),
            skills = ?refreshed,
            "Refreshed content baselines for Skills sharing a checkout that was reset"
        );
    }
    Ok(refreshed)
}

fn refresh_content_baselines(lockfile: &mut lockfile::Lockfile, names: &[String]) -> Result<()> {
    for name in names {
        let snapshot = content::snapshot(name)
            .with_context(|| format!("failed to capture updated content baseline for '{name}'"))?;
        if let Some(entry) = lockfile.skills.iter_mut().find(|entry| entry.name == *name) {
            entry.content_hash = Some(snapshot.content_hash);
            entry.content_hash_version = Some(content::SNAPSHOT_HASH_VERSION);
        }
    }
    Ok(())
}

/// Every installed Skill this checkout backs, whether or not its content is
/// currently on disk.
///
/// The missing ones matter: after the pull they are the proof that the source
/// dropped a Skill, so they cannot be filtered out here.
fn installed_names_in_checkout(
    skills_dir: &Path,
    target_repo_root: Option<&Path>,
    name: &str,
    entries: &[LockEntry],
) -> Vec<String> {
    let Some(target_repo_root) = target_repo_root else {
        return vec![name.to_string()];
    };
    entries
        .iter()
        .filter(|entry| {
            repo_link::repo_root_of(&skills_dir.join(&entry.name)).as_deref()
                == Some(target_repo_root)
        })
        .map(|entry| entry.name.clone())
        .collect()
}

fn capture_checkout_for_rollback(
    skills_dir: &Path,
    installed_names: &[String],
) -> Result<Vec<content::SkillSnapshot>> {
    let mut snapshots = installed_names
        .iter()
        .filter(|skill_name| skills_dir.join(skill_name).exists())
        .map(|skill_name| {
            content::snapshot(skill_name).with_context(|| {
                format!("failed to capture '{skill_name}' before updating its checkout")
            })
        })
        .collect::<Result<Vec<_>>>()?;
    // A checkout-root Skill can contain nested Skills. Restore the outer tree
    // first, then let the more specific snapshots win.
    snapshots.sort_by_key(|snapshot| snapshot.root.components().count());
    Ok(snapshots)
}

fn apply_update_locked(
    name: &str,
    session: &crate::git::transport::GitOperationSession,
) -> Result<UpdateOutcome> {
    let skills_dir = skillstar_core::infra::paths::hub_skills_dir();
    let path = skills_dir.join(name);

    if !path.exists() && !skillstar_core::infra::fs_ops::is_link(&path) {
        anyhow::bail!("Skill '{}' not found in hub", name);
    }

    let is_repo_skill = repo_link::is_repo_cached(&path);
    let target_repo_root = repo_link::repo_root_of(&path);
    let lock_path = lockfile::lockfile_path();

    // Read unlocked to learn which subfolder to pull. The planning read below
    // happens again under the mutex: holding it across a network fetch would
    // serialise every concurrent update, and another update may land meanwhile.
    let pre_update_lockfile = lockfile::Lockfile::load(&lock_path)
        .with_context(|| format!("Failed to load lockfile '{}'", lock_path.display()))?;
    let group = plan::RepoGroup {
        representative: name.to_string(),
        covered: Vec::new(),
    };
    if let Some(blocked) =
        divergence::local_divergences_for_group(&pre_update_lockfile.skills, &group)
            .into_iter()
            .next()
    {
        anyhow::bail!(
            "Skill '{}' changed while waiting to update; retry to preserve it as '{}' or explicitly discard it",
            blocked.name,
            blocked.suggested_local_name
        );
    }
    let source_folder = pre_update_lockfile
        .skills
        .iter()
        .find(|entry| entry.name == name)
        .and_then(|entry| entry.source_folder.clone());

    let checkout_root = target_repo_root.as_deref().unwrap_or(&path);
    let previous_revision = git_ops::head_revision(checkout_root)
        .context("failed to capture the pre-update Git revision")?;
    let previous_sparse_paths = is_repo_skill
        .then(|| git_ops::sparse_checkout_paths(checkout_root))
        .flatten();
    let installed_names = installed_names_in_checkout(
        &skills_dir,
        target_repo_root.as_deref(),
        name,
        &pre_update_lockfile.skills,
    );
    let rollback_snapshots = capture_checkout_for_rollback(&skills_dir, &installed_names)?;

    let transaction = (|| -> Result<(String, UpdatePlan)> {
        let tree_hash = if is_repo_skill {
            repo_scanner::pull_repo_skill_update_in_session(
                &path,
                source_folder.as_deref(),
                session,
            )
            .context("failed to pull repo-cached skill update")?
        } else {
            git_ops::pull_repo_in_session(&path, session)
                .context("failed to pull hub skill update")?;
            git_ops::compute_tree_hash(&path).context("failed to compute updated tree hash")?
        };

        // The pull is the only thing that can prove the source dropped an
        // installed Skill. Stop here so the rollback below puts its content
        // back, and the user decides what to keep — deleting first and asking
        // afterwards would leave nothing to decide about.
        let dropped = divergence::skills_dropped_by_source(&installed_names);
        if !dropped.is_empty() {
            return Err(anyhow::Error::new(SourceRemovedStop { names: dropped }));
        }

        let plan = {
            let _lock = lockfile::get_mutex()
                .lock()
                .map_err(|_| anyhow::anyhow!("Lockfile mutex poisoned"))?;
            let mut lockfile = lockfile::Lockfile::load(&lock_path)
                .with_context(|| format!("Failed to load lockfile '{}'", lock_path.display()))?;

            let plan =
                plan::plan_update(&lockfile.skills, name, &tree_hash, is_repo_skill, |entry| {
                    sibling_state_on_disk(&skills_dir, target_repo_root.as_deref(), entry)
                });

            // Post-pull integrity re-check: the pull just reset the checkout
            // to the new revision, so a dirty worktree NOW means local edits
            // landed during the network window (fetch → lock). Refreshing
            // content baselines from a dirty worktree would silently canonize
            // those edits as the new "official" content. Fail closed instead —
            // the caller preserves the worktree (see WorktreeDirty handling).
            if !git_ops::worktree_is_clean(checkout_root)? {
                return Err(git_ops::WorktreeDirty.into());
            }

            apply_hash_writes(&mut lockfile, &plan.hash_writes);
            refresh_content_baselines(&mut lockfile, &plan.affected)?;
            lockfile
                .save(&lock_path)
                .with_context(|| format!("Failed to save lockfile '{}'", lock_path.display()))?;
            plan
        };
        Ok((tree_hash, plan))
    })();

    let (tree_hash, plan) = match transaction {
        Ok(result) => result,
        Err(error) => {
            // Local edits landed during the update and were preserved — do
            // NOT roll back, or the reset/snapshot-restore would destroy
            // them. The lockfile was not updated, so the next attempt
            // surfaces a normal divergence decision for the user.
            if error.downcast_ref::<git_ops::WorktreeDirty>().is_some() {
                return Err(error);
            }
            let recovery_session = session.recovery_view();
            git_ops::reset_to_revision_in_session(
                checkout_root,
                &previous_revision,
                &recovery_session,
            )
                .with_context(|| {
                    format!(
                        "update failed ({error:#}) and the previous Skill revision could not be restored"
                )
            })?;
            if let Some(paths) = previous_sparse_paths {
                git_ops::restore_sparse_checkout_paths_in_session(
                    checkout_root,
                    &paths,
                    &recovery_session,
                )
                .context("failed to restore the pre-update sparse checkout")?;
            }
            for snapshot in &rollback_snapshots {
                snapshot.restore_owned_at(&snapshot.root).with_context(|| {
                    format!(
                        "failed to restore pre-update Skill content at '{}'",
                        snapshot.root.display()
                    )
                })?;
            }
            return Err(error.context("update was rolled back to the previous Skill revision"));
        }
    };

    installed_skill::invalidate_cache();

    for skill_name in &plan.affected {
        installed_skill::clear_update_state(skill_name);
    }

    let mut agent_links = Vec::new();
    let mut agent_link_failures = Vec::new();
    for skill_name in &plan.affected {
        match deployment::resync_existing_links(skill_name) {
            Ok(report) => {
                for failure in report.failures {
                    agent_link_failures.push(if skill_name == name {
                        failure
                    } else {
                        format!("{skill_name} → {failure}")
                    });
                }
                if skill_name == name {
                    agent_links = report.linked_to;
                }
            }
            Err(err) => {
                // The update itself succeeded — report the re-link problem
                // instead of failing the whole update or silently dropping it.
                agent_link_failures.push(format!("{skill_name}: {err:#}"));
            }
        }
    }

    projects::cascade_skill_update_to_projects(&plan.affected);

    Ok(UpdateOutcome {
        tree_hash,
        git_url: plan.git_url,
        sibling_names: plan.siblings_cleared,
        agent_links,
        agent_link_failures,
    })
}

#[cfg(test)]
mod tests;
