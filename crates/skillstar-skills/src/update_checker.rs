//! Update detection for repo-backed skills.
//!
//! Answers "does this skill have an update?" given a way to find each skill's
//! repo root. Link resolution is not this module's business — [`crate::repo_link`]
//! owns that, and production callers hand its `repo_root_of` in.
//!
//! # Batch workflow
//!
//! For efficiency, update checks follow a two-phase pattern:
//!
//! 1. **Prefetch**: [`prefetch_unique_repos`] deduplicates skills by repo
//!    root and issues one bounded-concurrent `git fetch` per unique repo
//!    (worker threads, one git child process each). When a GitHub API fast
//!    path answered for a repo ([`crate::update_api`]), that repo is skipped —
//!    the authoritative remote hashes come from the API instead.
//! 2. **Compare**: [`check_update_local_with_api_entry`] compares `HEAD` vs
//!    `origin/HEAD` without network access, or against API-provided subtree
//!    hashes when available.
//!
//! This avoids N redundant fetches when N skills share the same repo.

use anyhow::Result;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tracing::warn;

use crate::git::ops as git_ops;
use crate::git::transport::GitOperationSession;
use crate::lockfile::LockEntry;
use crate::update_api::ApiRemoteTree;
use crate::update_state::SkillUpdateState;
use crate::{lockfile, repo_link};
use skillstar_core::infra::path_env::command_with_path;
use skillstar_core::types::{UpstreamChange, UpstreamSuccessor};

// ── Upstream Verdict ────────────────────────────────────────────────

/// Everything one update check can conclude about a repo-cached Skill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpstreamStatus {
    Current,
    UpdateAvailable,
    /// The tracked revision no longer ships the Skill's folder.
    Removed(UpstreamChange),
}

impl UpstreamStatus {
    pub fn from_available(available: bool) -> Self {
        if available {
            Self::UpdateAvailable
        } else {
            Self::Current
        }
    }

    pub fn into_state(self, name: String) -> SkillUpdateState {
        match self {
            Self::Current => SkillUpdateState::new(name, false),
            Self::UpdateAvailable => SkillUpdateState::new(name, true),
            Self::Removed(change) => SkillUpdateState {
                name,
                update_available: false,
                upstream_change: Some(change),
            },
        }
    }
}

/// The revision an ordinary update resets a managed checkout to.
pub(crate) fn tracked_update_ref(repo_root: &Path) -> &'static str {
    if configured_git_ref(repo_root).is_some() {
        "FETCH_HEAD"
    } else {
        "origin/HEAD"
    }
}

/// Full verdict for one repo-cached Skill after the batch prefetch (or the
/// GitHub API pre-pass).
///
/// Extends [`check_update_local_with_api_entry`]: where that answers `None`
/// because the folder cannot be resolved at the tracked revision, this tells
/// a fetch that never happened (still `None` — keep the previous state) apart
/// from a folder the source really dropped (`Removed`, carrying the successor
/// it was renamed into when one can be found).
pub fn check_upstream_status(
    skill_path: &Path,
    failed_fetch_roots: &HashSet<PathBuf>,
    api_remote: Option<&ApiRemoteTree>,
    session: &GitOperationSession,
) -> Option<UpstreamStatus> {
    match check_update_local_with_api_entry(skill_path, failed_fetch_roots, api_remote) {
        Some(available) => Some(UpstreamStatus::from_available(available)),
        None => removal_status(skill_path, failed_fetch_roots, session),
    }
}

fn removal_status(
    skill_path: &Path,
    failed_fetch_roots: &HashSet<PathBuf>,
    session: &GitOperationSession,
) -> Option<UpstreamStatus> {
    let repo_root = repo_link::repo_root_of(skill_path)?;
    if failed_fetch_roots.contains(&repo_root) {
        return None;
    }
    // Root Skills are the checkout itself; "removed" has no meaning for them.
    let folder = lock_entry_for_path(skill_path)?
        .source_folder
        .filter(|folder| !folder.is_empty())?;
    let tracked = tracked_update_ref(&repo_root);
    // Never fetched: nothing can be concluded.
    git_ops::rev_parse(&repo_root, &format!("{tracked}^{{commit}}")).ok()?;
    // Still shipped locally but gone at the tracked revision — that, and only
    // that, is a removal.
    subtree_hash_at(&repo_root, "HEAD", Some(&folder))?;
    if git_ops::revision_contains_path(&repo_root, tracked, &folder) != Some(false) {
        return None;
    }
    let name = skill_path.file_name()?.to_str()?;
    Some(UpstreamStatus::Removed(UpstreamChange::Removed {
        suggested_local_name: crate::skill_update::suggested_local_name(name),
        successor: find_successor(&repo_root, &folder, tracked, skill_path, session),
    }))
}

/// The folder the source renamed or moved `old_folder` into, if any.
///
/// Candidates are the container-style Skill folders present at the tracked
/// revision but absent from the local `HEAD` tree. Git's own rename detection
/// on the two `SKILL.md` files decides first and scores the match; when it
/// finds nothing, a candidate whose frontmatter `name` equals the old one is
/// accepted instead.
fn find_successor(
    repo_root: &Path,
    old_folder: &str,
    tracked: &str,
    skill_path: &Path,
    session: &GitOperationSession,
) -> Option<UpstreamSuccessor> {
    let added = crate::repo_scanner::upstream_added_dirs(repo_root, tracked)?;
    if added.is_empty() {
        return None;
    }
    let old_manifest = format!("{old_folder}/SKILL.md");
    let mut pathspecs: Vec<&str> = vec![old_folder];
    pathspecs.extend(added.iter().map(String::as_str));
    let by_content =
        git_ops::diff_renames_in_session(repo_root, "HEAD", tracked, &pathspecs, session)
            .map_err(|error| {
                warn!(
                    target: "update_checker",
                    path = %repo_root.display(),
                    error = %error,
                    "rename detection failed; falling back to frontmatter names"
                );
            })
            .unwrap_or_default()
            .into_iter()
            .find(|rename| rename.from == old_manifest)
            .and_then(|rename| {
                let folder = rename.to.strip_suffix("/SKILL.md")?;
                added
                    .iter()
                    .any(|candidate| candidate == folder)
                    .then(|| (folder.to_string(), Some(rename.similarity)))
            });
    let (folder, similarity) = match by_content {
        Some(hit) => hit,
        None => {
            let old_name = crate::validation::inspect_skill_frontmatter(skill_path).name?;
            let hit = added.iter().find(|candidate| {
                crate::repo_scanner::skill_at_revision(repo_root, tracked, candidate, session)
                    .is_some_and(|skill| skill.id == old_name)
            })?;
            (hit.clone(), None)
        }
    };
    let skill = crate::repo_scanner::skill_at_revision(repo_root, tracked, &folder, session)?;
    Some(UpstreamSuccessor {
        skill_id: skill.id,
        folder_path: folder,
        description: skill.description,
        similarity,
    })
}

// ── Batch Prefetch ──────────────────────────────────────────────────

pub fn prefetch_unique_repos_in_session(
    skill_paths: &[PathBuf],
    session: &crate::git::transport::GitOperationSession,
) -> HashSet<PathBuf> {
    prefetch_unique_repos_with(skill_paths, repo_link::repo_root_of, |root| {
        fetch_tracked_ref_in_session(root, session).map_err(|e| {
            warn!(
                target: "update_checker",
                path = %root.display(),
                error = %e,
                "prefetch git fetch failed — will preserve existing update state"
            );
            e
        })
    })
}

/// Prefetch like [`prefetch_unique_repos_in_session`], but repositories with
/// an authoritative GitHub API answer are skipped entirely — their remote
/// subtree hashes come from [`check_update_local_with_api`], so the fetch is
/// unnecessary until an update is actually applied.
pub fn prefetch_unique_repos_in_session_skipping(
    skill_paths: &[PathBuf],
    session: &crate::git::transport::GitOperationSession,
    api_ok_roots: &HashSet<PathBuf>,
) -> HashSet<PathBuf> {
    prefetch_unique_repos_with(skill_paths, repo_link::repo_root_of, |root| {
        if api_ok_roots.contains(root) {
            return Ok(());
        }
        fetch_tracked_ref_in_session(root, session).map_err(|e| {
            warn!(
                target: "update_checker",
                path = %root.display(),
                error = %e,
                "prefetch git fetch failed — will preserve existing update state"
            );
            e
        })
    })
}

pub(crate) fn prefetch_unique_repos_with<F, G>(
    skill_paths: &[PathBuf],
    repo_root_of: F,
    fetch_repo: G,
) -> HashSet<PathBuf>
where
    F: Fn(&Path) -> Option<PathBuf>,
    G: Fn(&Path) -> Result<()> + Sync,
{
    let mut unique = Vec::new();
    let mut seen = HashSet::new();
    for path in skill_paths {
        if let Some(root) = repo_root_of(path)
            && seen.insert(root.clone())
        {
            unique.push(root);
        }
    }

    skillstar_core::infra::parallel::map_bounded(
        unique,
        skillstar_core::infra::parallel::git_fetch_concurrency_limit(),
        |root| {
            if fetch_repo(&root).is_err() {
                Some(root)
            } else {
                None
            }
        },
    )
    .into_iter()
    .flatten()
    .collect()
}

// ── Update Detection ────────────────────────────────────────────────

/// Check a repo-cached skill against GitHub API-provided remote hashes,
/// loading the lockfile entry internally.
///
/// `None` means "the prefetch failed for this skill's repo, status unknown" —
/// callers must preserve the previous state rather than clearing the badge.
pub fn check_update_local_with_api_entry(
    skill_path: &Path,
    failed_fetch_roots: &HashSet<PathBuf>,
    api_remote: Option<&ApiRemoteTree>,
) -> Option<bool> {
    let entry = lock_entry_for_path(skill_path);
    check_update_local_with_api(
        skill_path,
        failed_fetch_roots,
        api_remote,
        repo_link::repo_root_of,
        entry.as_ref(),
    )
}

/// Load the lockfile entry backing `skill_path`, if any.
fn lock_entry_for_path(skill_path: &Path) -> Option<LockEntry> {
    let name = skill_path.file_name()?.to_string_lossy().to_string();
    lockfile::Lockfile::load(&lockfile::lockfile_path())
        .ok()?
        .skills
        .into_iter()
        .find(|entry| entry.name == name)
}

pub fn check_update_local_with<F>(
    skill_path: &Path,
    failed_fetch_roots: &HashSet<PathBuf>,
    repo_root_of: F,
    entry: Option<&LockEntry>,
) -> Option<bool>
where
    F: Fn(&Path) -> Option<PathBuf>,
{
    check_update_local_with_api(skill_path, failed_fetch_roots, None, repo_root_of, entry)
}

/// Like [`check_update_local_with`], but compares against GitHub API-provided
/// remote subtree hashes when the caller supplies them.
///
/// `api_remote` must come from a successful [`crate::update_api`] fetch for
/// this skill's repo root. When present it is authoritative: the local HEAD
/// subtree hash is compared against the API hash at the tracked ref (pinned
/// ref or default branch). The same contracts as the git-fetch path hold:
/// `None` means "status unknown, preserve the previous badge" — a missing
/// folder in the API tree (source moved/deleted remotely) or an unresolvable
/// local subtree both report `None`, never a fabricated "no update".
pub fn check_update_local_with_api<F>(
    skill_path: &Path,
    failed_fetch_roots: &HashSet<PathBuf>,
    api_remote: Option<&ApiRemoteTree>,
    repo_root_of: F,
    entry: Option<&LockEntry>,
) -> Option<bool>
where
    F: Fn(&Path) -> Option<PathBuf>,
{
    // No resolvable repo root: nothing to compare against, so report "no
    // update" rather than "unknown". `None` is reserved for a failed fetch,
    // where the previous badge must survive.
    let Some(repo_root) = repo_root_of(skill_path) else {
        return Some(false);
    };
    if failed_fetch_roots.contains(&repo_root) {
        return None;
    }

    if let Some(api) = api_remote {
        let source_folder = entry.and_then(|entry| entry.source_folder.clone());
        // The API listing is shallow: commit SHA plus top-level dirs. Deeper
        // folders resolve from the local tracked ref instead — identical by
        // construction, since the tip gate in `api_prefetch_remote_trees`
        // only admits trees whose commit IS the local tracked commit. A
        // folder in neither place no longer exists remotely; return `None`
        // so the previous badge is preserved rather than cleared.
        let remote = match api.subtree_hash(source_folder.as_deref()) {
            Some(sha) => sha.to_string(),
            None => subtree_hash_at(
                &repo_root,
                tracked_update_ref(&repo_root),
                source_folder.as_deref(),
            )?,
        };
        // No fetch happened for API-covered repos, so HEAD is still the
        // installed commit — exactly the local baseline to compare against.
        let local = subtree_hash_at(&repo_root, "HEAD", source_folder.as_deref())?;
        if local == remote {
            return Some(false);
        }
        // Root skills only. `GET /git/trees/{branch}` puts the *commit* SHA
        // in `sha`, not the peeled tree SHA that `HEAD^{tree}` returns. A
        // root skill already on that commit would otherwise keep lighting
        // the badge after every successful update.
        if source_folder
            .as_deref()
            .is_none_or(|folder| folder.is_empty())
        {
            let local_commit = git_ops::rev_parse(&repo_root, "HEAD").ok()?;
            return Some(!local_commit.is_empty() && local_commit != remote);
        }
        return Some(true);
    }

    // Precise per-skill comparison. The lockfile knows the skill's source
    // folder inside the shared checkout, so compare the subtree hash at the
    // local HEAD against the same subtree at the fetched ref. A repo-wide
    // HEAD change alone no longer lights every badge of a shared checkout;
    // only Skills whose content actually moved report an update.
    if let Some(folder) = entry.and_then(|entry| entry.source_folder.clone()) {
        let remote_ref = tracked_update_ref(&repo_root);
        let local = subtree_hash_at(&repo_root, "HEAD", Some(&folder));
        let remote = subtree_hash_at(&repo_root, remote_ref, Some(&folder));
        if let (Some(local), Some(remote)) = (local, remote) {
            return Some(local != remote);
        }
        // Unresolvable subtree (e.g. the source moved or dropped the folder):
        // preserve the previous badge rather than guessing.
        return None;
    }

    Some(compare_heads(&repo_root).unwrap_or(false))
}

/// Tree hash of `source_folder` at `git_ref` in `repo_root`, or the whole
/// tree when the skill is the checkout root.
fn subtree_hash_at(repo_root: &Path, git_ref: &str, source_folder: Option<&str>) -> Option<String> {
    let spec = match source_folder.filter(|folder| !folder.is_empty()) {
        Some(folder) => format!("{git_ref}:{folder}"),
        None => format!("{git_ref}^{{tree}}"),
    };
    let output = command_with_path("git")
        .current_dir(repo_root)
        .args(["rev-parse", "--verify", "--quiet", &spec])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let hash = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!hash.is_empty()).then_some(hash)
}

fn compare_heads(repo_root: &Path) -> Option<bool> {
    let local_head = git_ops::rev_parse(repo_root, "HEAD").ok()?;
    let remote_ref = tracked_update_ref(repo_root);
    let remote_head = git_ops::rev_parse(repo_root, remote_ref).ok()?;
    Some(!local_head.is_empty() && !remote_head.is_empty() && local_head != remote_head)
}

pub(crate) fn configured_git_ref(repo_root: &Path) -> Option<String> {
    let output = command_with_path("git")
        .current_dir(repo_root)
        .args(["config", "--get", "skillstar.ref"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!value.is_empty()).then_some(value)
}

pub(crate) fn fetch_tracked_ref_in_session(
    repo_root: &Path,
    session: &crate::git::transport::GitOperationSession,
) -> Result<()> {
    let mut args = vec!["fetch", "--depth", "1", "--quiet"];
    let git_ref = configured_git_ref(repo_root);
    if let Some(git_ref) = git_ref.as_deref() {
        args.extend(["origin", git_ref]);
    }
    match git_ops::run_git_shallow_fetch_in_session(repo_root, &args, session) {
        Ok(_) => Ok(()),
        Err(error)
            if git_ref.is_some()
                && git_ops::is_missing_remote_ref(error.as_ref())
                && crate::repo_scanner::cache::checkout_has_skill_payload(repo_root) =>
        {
            warn!(
                target: "update_checker",
                path = %repo_root.display(),
                git_ref = git_ref.as_deref().unwrap_or(""),
                "recorded git ref is gone — retargeting lock and fetching the default branch"
            );
            retarget_deleted_git_ref(repo_root)?;
            git_ops::run_git_shallow_fetch_in_session(
                repo_root,
                &["fetch", "--depth", "1", "--quiet"],
                session,
            )
            .map(|_| ())
        }
        Err(error) => Err(error),
    }
}

/// Drop a lock / `skillstar.ref` pin that the remote no longer has.
///
/// Next update tracks the repository default branch (`origin/HEAD`) when the
/// name can be resolved, otherwise the pin is omitted.
pub(crate) fn retarget_deleted_git_ref(repo_root: &Path) -> Result<()> {
    let _ = command_with_path("git")
        .current_dir(repo_root)
        .args(["config", "--unset", "skillstar.ref"])
        .output();

    let default_branch = crate::update_api::remote_ref_for(repo_root, None);
    if let Some(branch) = default_branch.as_deref() {
        let output = command_with_path("git")
            .current_dir(repo_root)
            .args(["config", "skillstar.ref", branch])
            .output();
        if output.is_ok_and(|output| !output.status.success()) {
            warn!(
                target: "update_checker",
                path = %repo_root.display(),
                branch,
                "failed to persist retargeted default-branch pin"
            );
        }
    }

    let canonical = std::fs::canonicalize(repo_root).ok();
    let _guard = crate::lockfile::get_mutex()
        .lock()
        .map_err(|_| anyhow::anyhow!("Lockfile mutex poisoned"))?;
    let lock_path = crate::lockfile::lockfile_path();
    let mut lockfile = crate::lockfile::Lockfile::load(&lock_path)?;
    let hub = skillstar_core::infra::paths::hub_skills_dir();
    let mut changed = false;
    for entry in &mut lockfile.skills {
        let belongs = repo_link::repo_root_of(&hub.join(&entry.name))
            .and_then(|root| std::fs::canonicalize(root).ok())
            .zip(canonical.as_ref())
            .is_some_and(|(root, expected)| root == *expected);
        if belongs && entry.git_ref.is_some() {
            entry.git_ref = default_branch.clone();
            changed = true;
        }
    }
    if changed {
        lockfile.save(&lock_path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod upstream_tests;
