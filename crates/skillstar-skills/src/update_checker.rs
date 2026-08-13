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
//!    root and issues one `git fetch` per unique repo. When a GitHub API fast
//!    path answered for a repo ([`crate::update_api`]), that repo is skipped —
//!    the authoritative remote hashes come from the API instead.
//! 2. **Compare**: [`check_update_local`] compares `HEAD` vs `origin/HEAD`
//!    without network access; [`check_update_local_with_api`] compares
//!    against API-provided subtree hashes when available.
//!
//! This avoids N redundant fetches when N skills share the same repo.

use anyhow::Result;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tracing::warn;

use crate::git::ops as git_ops;
use crate::lockfile::LockEntry;
use crate::update_api::ApiRemoteTree;
use crate::{lockfile, repo_link};
use skillstar_core::infra::path_env::command_with_path;

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
    G: Fn(&Path) -> Result<()>,
{
    let mut fetched = HashSet::new();
    let mut failed = HashSet::new();

    for path in skill_paths {
        if let Some(root) = repo_root_of(path)
            && fetched.insert(root.clone())
            && fetch_repo(&root).is_err()
        {
            failed.insert(root);
        }
    }

    failed
}

// ── Update Detection ────────────────────────────────────────────────

/// Check if a repo-backed skill has updates available **without fetching**.
///
/// `None` means "the prefetch failed for this skill's repo, status unknown" —
/// callers must preserve the previous state rather than clearing the badge.
pub fn check_update_local(
    skill_path: &Path,
    failed_fetch_roots: &HashSet<PathBuf>,
) -> Option<bool> {
    let entry = lock_entry_for_path(skill_path);
    check_update_local_with(
        skill_path,
        failed_fetch_roots,
        repo_link::repo_root_of,
        entry.as_ref(),
    )
}

/// Check a repo-cached skill against GitHub API-provided remote hashes,
/// loading the lockfile entry internally like [`check_update_local`].
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
        // The API tree is the remote truth. If the folder no longer exists
        // remotely, preserve the previous badge rather than clearing it.
        let remote = api.subtree_hash(source_folder.as_deref())?;
        // No fetch happened for API-covered repos, so HEAD is still the
        // installed commit — exactly the local baseline to compare against.
        let local = subtree_hash_at(&repo_root, "HEAD", source_folder.as_deref())?;
        return Some(local != remote);
    }

    // Precise per-skill comparison. The lockfile knows the skill's source
    // folder inside the shared checkout, so compare the subtree hash at the
    // local HEAD against the same subtree at the fetched ref. A repo-wide
    // HEAD change alone no longer lights every badge of a shared checkout;
    // only Skills whose content actually moved report an update.
    if let Some(folder) = entry.and_then(|entry| entry.source_folder.clone()) {
        let remote_ref = if configured_git_ref(&repo_root).is_some() {
            "FETCH_HEAD"
        } else {
            "origin/HEAD"
        };
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
fn subtree_hash_at(
    repo_root: &Path,
    git_ref: &str,
    source_folder: Option<&str>,
) -> Option<String> {
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
    let remote_ref = if configured_git_ref(repo_root).is_some() {
        "FETCH_HEAD"
    } else {
        "origin/HEAD"
    };
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
    git_ops::run_git_shallow_fetch_in_session(repo_root, &args, session).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;
    use std::fs;
    use std::process::Command;

    fn run_git(repo: &Path, args: &[&str]) {
        let output = Command::new("git")
            .current_dir(repo)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn init_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        run_git(dir.path(), &["init", "--initial-branch=main"]);
        run_git(dir.path(), &["config", "user.email", "test@example.com"]);
        run_git(dir.path(), &["config", "user.name", "SkillStar Tests"]);
        dir
    }

    // Unix-only: fixtures use std::os::unix::fs::symlink.
    #[cfg(unix)]
    #[test]
    fn subtree_hash_and_local_update_detection_work() {
        let remote = init_repo();
        fs::create_dir_all(remote.path().join("skills/demo")).unwrap();
        fs::write(remote.path().join("skills/demo/SKILL.md"), "v1").unwrap();
        run_git(remote.path(), &["add", "."]);
        run_git(remote.path(), &["commit", "-m", "initial"]);

        let clone_parent = tempfile::tempdir().unwrap();
        let clone_path = clone_parent.path().join("clone");
        run_git(
            clone_parent.path(),
            &[
                "clone",
                remote.path().to_str().unwrap(),
                clone_path.to_str().unwrap(),
            ],
        );

        let initial_hash = git_ops::compute_subtree_hash(&clone_path, "skills/demo").unwrap();
        assert!(!initial_hash.is_empty());

        fs::write(remote.path().join("skills/demo/SKILL.md"), "v2").unwrap();
        run_git(remote.path(), &["add", "."]);
        run_git(remote.path(), &["commit", "-m", "update"]);

        run_git(&clone_path, &["fetch", "--depth", "1", "--quiet"]);

        let skill_link_parent = tempfile::tempdir().unwrap();
        let skill_link = skill_link_parent.path().join("demo");
        std::os::unix::fs::symlink(clone_path.join("skills/demo"), &skill_link).unwrap();

        let result = check_update_local_with(&skill_link, &HashSet::new(), |path| {
            let real = std::fs::read_link(path).ok()?;
            Some(real.parent()?.parent()?.to_path_buf())
        }, None);
        assert_eq!(result, Some(true));
    }

    // Unix-only: fixtures use std::os::unix::fs::symlink.
    #[cfg(unix)]
    #[test]
    fn api_remote_hashes_drive_update_detection_without_fetching() {
        let remote = init_repo();
        fs::create_dir_all(remote.path().join("skills/demo")).unwrap();
        fs::write(remote.path().join("skills/demo/SKILL.md"), "v1").unwrap();
        fs::write(remote.path().join("README.md"), "readme v1").unwrap();
        run_git(remote.path(), &["add", "."]);
        run_git(remote.path(), &["commit", "-m", "initial"]);

        let clone_parent = tempfile::tempdir().unwrap();
        let clone_path = clone_parent.path().join("clone");
        run_git(
            clone_parent.path(),
            &[
                "clone",
                remote.path().to_str().unwrap(),
                clone_path.to_str().unwrap(),
            ],
        );

        let skill_link_parent = tempfile::tempdir().unwrap();
        let skill_link = skill_link_parent.path().join("demo");
        std::os::unix::fs::symlink(clone_path.join("skills/demo"), &skill_link).unwrap();
        let repo_root_of = |path: &Path| {
            let real = std::fs::read_link(path).ok()?;
            Some(real.parent()?.parent()?.to_path_buf())
        };

        let entry = LockEntry {
            name: "demo".into(),
            git_url: "https://github.com/example/demo".into(),
            git_ref: None,
            tree_hash: "tree".into(),
            content_hash: None,
            content_hash_version: None,
            installed_at: String::new(),
            source_folder: Some("skills/demo".into()),
        };

        // API says "same as local HEAD" → no update, and no fetch happened.
        let v1_hash = git_ops::compute_subtree_hash(&clone_path, "skills/demo").unwrap();
        let mut folders = std::collections::HashMap::new();
        folders.insert("skills/demo".to_string(), v1_hash.clone());
        let api = crate::update_api::ApiRemoteTree { folders };
        assert_eq!(
            check_update_local_with_api(&skill_link, &HashSet::new(), Some(&api), repo_root_of, Some(&entry)),
            Some(false)
        );

        // Remote advances without the local clone fetching anything.
        fs::write(remote.path().join("skills/demo/SKILL.md"), "v2").unwrap();
        fs::create_dir_all(remote.path().join("skills/other")).unwrap();
        fs::write(remote.path().join("skills/other/SKILL.md"), "other").unwrap();
        run_git(remote.path(), &["add", "."]);
        run_git(remote.path(), &["commit", "-m", "update"]);

        let v2_hash = git_ops::compute_subtree_hash(remote.path(), "skills/demo").unwrap();
        assert_ne!(v1_hash, v2_hash);
        let mut folders = std::collections::HashMap::new();
        folders.insert("skills/demo".to_string(), v2_hash.clone());
        let api = crate::update_api::ApiRemoteTree { folders };
        assert_eq!(
            check_update_local_with_api(&skill_link, &HashSet::new(), Some(&api), repo_root_of, Some(&entry)),
            Some(true)
        );

        // An unrelated repo change must NOT light the badge: same folder hash.
        let mut folders = std::collections::HashMap::new();
        folders.insert("skills/demo".to_string(), v1_hash.clone());
        let api = crate::update_api::ApiRemoteTree { folders };
        assert_eq!(
            check_update_local_with_api(&skill_link, &HashSet::new(), Some(&api), repo_root_of, Some(&entry)),
            Some(false)
        );

        // The skill's folder vanished from the remote tree → unknown, not "no
        // update" (the previous badge must survive).
        let api = crate::update_api::ApiRemoteTree::default();
        assert_eq!(
            check_update_local_with_api(&skill_link, &HashSet::new(), Some(&api), repo_root_of, Some(&entry)),
            None
        );

        // A failed fetch for this repo overrides the API answer.
        let failed: HashSet<PathBuf> = [clone_path.clone()].into_iter().collect();
        let mut folders = std::collections::HashMap::new();
        folders.insert("skills/demo".to_string(), v2_hash);
        let api = crate::update_api::ApiRemoteTree { folders };
        assert_eq!(
            check_update_local_with_api(&skill_link, &failed, Some(&api), repo_root_of, Some(&entry)),
            None
        );
    }

    #[test]
    fn subtree_comparison_ignores_unrelated_repo_changes() {
        let remote = init_repo();
        fs::create_dir_all(remote.path().join("skills/demo")).unwrap();
        fs::write(remote.path().join("skills/demo/SKILL.md"), "v1").unwrap();
        fs::write(remote.path().join("README.md"), "readme v1").unwrap();
        run_git(remote.path(), &["add", "."]);
        run_git(remote.path(), &["commit", "-m", "initial"]);

        let clone_parent = tempfile::tempdir().unwrap();
        let clone_path = clone_parent.path().join("clone");
        run_git(
            clone_parent.path(),
            &[
                "clone",
                remote.path().to_str().unwrap(),
                clone_path.to_str().unwrap(),
            ],
        );

        // The remote moves forward, but only outside the skill's folder.
        fs::write(remote.path().join("README.md"), "readme v2").unwrap();
        run_git(remote.path(), &["add", "."]);
        run_git(remote.path(), &["commit", "-m", "docs only"]);

        run_git(&clone_path, &["fetch", "--depth", "1", "--quiet"]);

        let skill_link_parent = tempfile::tempdir().unwrap();
        let skill_link = skill_link_parent.path().join("demo");
        std::os::unix::fs::symlink(clone_path.join("skills/demo"), &skill_link).unwrap();

        let entry = crate::lockfile::LockEntry {
            name: "demo".to_string(),
            git_url: String::new(),
            git_ref: None,
            tree_hash: String::new(),
            content_hash: None,
            content_hash_version: None,
            installed_at: String::new(),
            source_folder: Some("skills/demo".to_string()),
        };
        let result = check_update_local_with(&skill_link, &HashSet::new(), |path| {
            let real = std::fs::read_link(path).ok()?;
            Some(real.parent()?.parent()?.to_path_buf())
        }, Some(&entry));
        assert_eq!(
            result,
            Some(false),
            "a repo-wide HEAD change must not badge a Skill whose folder did not move"
        );
    }

    #[test]
    fn subtree_comparison_badges_skills_whose_folder_moved() {
        let remote = init_repo();
        fs::create_dir_all(remote.path().join("skills/demo")).unwrap();
        fs::write(remote.path().join("skills/demo/SKILL.md"), "v1").unwrap();
        run_git(remote.path(), &["add", "."]);
        run_git(remote.path(), &["commit", "-m", "initial"]);

        let clone_parent = tempfile::tempdir().unwrap();
        let clone_path = clone_parent.path().join("clone");
        run_git(
            clone_parent.path(),
            &[
                "clone",
                remote.path().to_str().unwrap(),
                clone_path.to_str().unwrap(),
            ],
        );

        fs::write(remote.path().join("skills/demo/SKILL.md"), "v2").unwrap();
        run_git(remote.path(), &["add", "."]);
        run_git(remote.path(), &["commit", "-m", "skill changed"]);

        run_git(&clone_path, &["fetch", "--depth", "1", "--quiet"]);

        let skill_link_parent = tempfile::tempdir().unwrap();
        let skill_link = skill_link_parent.path().join("demo");
        std::os::unix::fs::symlink(clone_path.join("skills/demo"), &skill_link).unwrap();

        let entry = crate::lockfile::LockEntry {
            name: "demo".to_string(),
            git_url: String::new(),
            git_ref: None,
            tree_hash: String::new(),
            content_hash: None,
            content_hash_version: None,
            installed_at: String::new(),
            source_folder: Some("skills/demo".to_string()),
        };
        let result = check_update_local_with(&skill_link, &HashSet::new(), |path| {
            let real = std::fs::read_link(path).ok()?;
            Some(real.parent()?.parent()?.to_path_buf())
        }, Some(&entry));
        assert_eq!(result, Some(true));
    }

    #[test]
    fn unresolvable_repo_root_reports_no_update() {
        let dir = tempfile::tempdir().unwrap();
        let result = check_update_local_with(&dir.path().join("nope"), &HashSet::new(), |_| None, None);
        assert_eq!(
            result,
            Some(false),
            "None is reserved for failed fetches, not for missing repo roots"
        );
    }

    #[test]
    fn failed_prefetch_root_preserves_previous_state() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        let failed = HashSet::from([repo.clone()]);
        let result = check_update_local_with(
            &dir.path().join("skill"),
            &failed,
            |_| Some(repo.clone()),
            None,
        );
        assert_eq!(result, None, "failed fetch must not clear the badge");
    }

    #[test]
    fn prefetch_unique_repos_deduplicates_and_tracks_failures() {
        let dir = tempfile::tempdir().unwrap();
        let repo_a = dir.path().join("repo_a");
        let repo_b = dir.path().join("repo_b");
        let skill_a1 = dir.path().join("skill_a1");
        let skill_a2 = dir.path().join("skill_a2");
        let skill_b = dir.path().join("skill_b");

        let repo_a_for_lookup = repo_a.clone();
        let repo_b_for_lookup = repo_b.clone();
        let skill_b_for_lookup = skill_b.clone();
        let repo_root_of = move |path: &Path| -> Option<PathBuf> {
            if path == skill_b_for_lookup {
                Some(repo_b_for_lookup.clone())
            } else {
                Some(repo_a_for_lookup.clone())
            }
        };

        let fetch_calls = std::cell::RefCell::new(Vec::new());
        let fetch_repo = |root: &Path| -> Result<()> {
            fetch_calls.borrow_mut().push(root.to_path_buf());
            if root == repo_b {
                Err(anyhow!("fetch failed"))
            } else {
                Ok(())
            }
        };

        let failed =
            prefetch_unique_repos_with(&[skill_a1, skill_a2, skill_b], repo_root_of, fetch_repo);

        assert_eq!(
            fetch_calls.borrow().len(),
            2,
            "should fetch only unique repos"
        );
        assert!(fetch_calls.borrow().contains(&repo_a));
        assert!(fetch_calls.borrow().contains(&repo_b));
        assert!(failed.contains(&repo_b));
        assert!(!failed.contains(&repo_a));
    }
}
