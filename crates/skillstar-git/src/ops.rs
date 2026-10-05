use anyhow::{Context, Result, anyhow};
use std::path::{Path, PathBuf};

use skillstar_core::config::{github_health, github_mirror};
use skillstar_core::infra::path_env::command_with_path;
use tracing::{debug, warn};

use crate::transport::{self, GitOperationSession};
pub use crate::tree::{
    GitTreeEntry, list_tree_entries_at, list_tree_entries_with_trees, list_tree_paths,
    list_tree_paths_at, revision_contains_path,
};

/// Git `file://` URL for a local path.
///
/// `.gitconfig` treats `\` as an escape, so `file://C:\Users\foo` becomes
/// `file://C:Usersfoo` and clone fails. Drive-letter paths also need the
/// extra slash (`file:///C:/Users/foo`); `file://C:/Users/foo` treats `C:`
/// as a host.
pub fn local_file_url(path: &Path) -> String {
    let mut normalized = path.to_string_lossy().replace('\\', "/");
    if let Some(rest) = normalized.strip_prefix("//?/") {
        normalized = rest.to_string();
    }
    if normalized.len() >= 2 && normalized.as_bytes()[1] == b':' && !normalized.starts_with('/') {
        normalized.insert(0, '/');
    }
    format!("file://{normalized}")
}

/// Compute the tree-hash of a local Git repository.
///
/// Tries the in-process `gix` library first (fastest, no process spawn).
/// Falls back to `git rev-parse HEAD^{tree}` via CLI when `gix` fails —
pub fn compute_tree_hash(repo_path: &Path) -> Result<String> {
    match compute_tree_hash_gix(repo_path) {
        Ok(hash) => Ok(hash),
        Err(gix_err) => {
            let is_repo_discovery_miss = gix_err
                .to_string()
                .contains("Failed to discover git repository");

            // Non-git paths are common for local/copy-based skills; avoid
            // noisy warnings and skip CLI fallback when no `.git` ancestor exists.
            if is_repo_discovery_miss && !has_git_ancestor(repo_path) {
                debug!(
                    target: "git_ops",
                    path = %repo_path.display(),
                    "tree hash skipped: path is not inside a git repository"
                );
                return Err(gix_err);
            }

            if is_repo_discovery_miss {
                debug!(
                    target: "git_ops",
                    path = %repo_path.display(),
                    error = %gix_err,
                    "gix could not discover git repository, falling back to git CLI"
                );
            } else {
                warn!(
                    target: "git_ops",
                    path = %repo_path.display(),
                    error = %gix_err,
                    "gix failed to read HEAD tree hash, falling back to git CLI"
                );
            }
            compute_tree_hash_cli(repo_path).with_context(|| {
                format!(
                    "Both gix and git CLI failed to compute tree hash for {:?}. gix error: {}",
                    repo_path, gix_err
                )
            })
        }
    }
}

/// In-process tree hash via `gix` (no subprocess).
///
/// Uses `gix::discover` instead of `gix::open` so that subdirectory paths
/// (common for symlinked skills pointing into repo subdirs) correctly walk
/// up to find the `.git` root.
fn compute_tree_hash_gix(repo_path: &Path) -> Result<String> {
    let repo = gix::discover(repo_path).context("Failed to discover git repository")?;
    let head = repo.head_commit().context("Failed to get HEAD commit")?;
    let tree_id = head.tree_id().context("Failed to get tree ID")?;
    Ok(tree_id.to_string())
}

/// CLI fallback: `git rev-parse HEAD^{tree}`.
fn compute_tree_hash_cli(repo_path: &Path) -> Result<String> {
    run_git(repo_path, &["rev-parse", "HEAD^{tree}"])
}

/// Walk up from `path` to the directory containing `.git`.
///
/// `path` may be a file or directory; parents are visited until the root.
pub fn find_repo_root(path: &Path) -> Option<PathBuf> {
    let mut current = path.to_path_buf();
    loop {
        if current.join(".git").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

fn has_git_ancestor(path: &Path) -> bool {
    find_repo_root(path).is_some()
}

/// Resolve a revision to its object id (`git rev-parse <rev>`).
///
/// Routed through [`run_git`] so the call inherits `command_with_path` — a GUI
/// launched from Finder has no login-shell PATH and would otherwise fail to
/// find git, silently reporting "no update".
pub fn rev_parse(repo_path: &Path, rev: &str) -> Result<String> {
    run_git(repo_path, &["rev-parse", rev])
}

pub fn clone_repo_shallow_in_session(
    url: &str,
    dest: &Path,
    session: &GitOperationSession,
) -> Result<()> {
    run_git_clone(url, dest, false, session)
}

pub fn clone_repo_shallow_at_ref_in_session(
    url: &str,
    dest: &Path,
    git_ref: &str,
    session: &GitOperationSession,
) -> Result<()> {
    let looks_like_sha =
        git_ref.len() == 40 && git_ref.chars().all(|ch| ch.is_ascii_hexdigit());
    if !looks_like_sha {
        let args = ["clone", "--depth", "1", "--branch", git_ref];
        return run_git_clone_attempt(url, dest, &args, session)
            .with_context(|| format!("Failed to clone '{url}' at ref '{git_ref}'"));
    }

    let mkdir = || std::fs::create_dir_all(dest);
    mkdir().with_context(|| format!("Failed to create clone dir '{}'", dest.display()))?;
    run_local_git(dest, &["init", "--quiet"], session)?;
    transport::execute_remote_git(
        Some(dest),
        &["remote", "add", "origin", url],
        url,
        session,
        false,
    )
    .map_err(anyhow::Error::from)?;
    transport::execute_remote_git(
        Some(dest),
        &["fetch", "--depth", "1", "--quiet", "origin", git_ref],
        url,
        session,
        true,
    )
    .map_err(anyhow::Error::from)?;
    run_local_git(dest, &["checkout", "--quiet", "FETCH_HEAD"], session)?;
    Ok(())
}

/// A repo-local command that never touches the network (no origin needed):
/// `init` and FETCH_HEAD checkouts in a fresh SHA-pin clone.
fn run_local_git(dir: &Path, args: &[&str], session: &GitOperationSession) -> Result<()> {
    let mut command = command_with_path("git");
    let output = transport::execute_remote_command(
        &mut command,
        Some(dir),
        args,
        "local-sha-pin",
        session,
    )?;
    if !output.status.success() {
        anyhow::bail!(
            "git {} failed in '{}': {}",
            args.join(" "),
            dir.display(),
            output.stderr.clone()
        );
    }
    Ok(())
}

fn run_git_clone(
    url: &str,
    dest: &Path,
    single_branch: bool,
    session: &GitOperationSession,
) -> Result<()> {
    let mut args = vec!["clone", "--depth", "1"];
    if single_branch {
        args.push("--single-branch");
    }
    // `execute_remote_git` already walks the mirror candidate chain (each
    // candidate in preference order, then a direct GitHub fallback), so the
    // clone attempt needs no outer retry loop.
    run_git_clone_attempt(url, dest, &args, session)
}

fn run_git_clone_attempt(
    url: &str,
    dest: &Path,
    args: &[&str],
    session: &GitOperationSession,
) -> Result<()> {
    let destination = dest.to_string_lossy();
    let mut command_args = args.to_vec();
    command_args.push(url);
    command_args.push(&destination);
    transport::execute_remote_git(None, &command_args, url, session, true)
        .map(|_| ())
        .map_err(anyhow::Error::from)
}

/// Configure sparse-checkout for a repo and materialize the given directories.
///
/// Expects a sparse promisor clone with partial blob filters.
/// Sets cone-mode sparse-checkout to the given directory patterns then runs
/// `git checkout`.
pub fn apply_sparse_checkout_in_session(
    repo_path: &Path,
    dirs: &[&str],
    session: &GitOperationSession,
) -> Result<()> {
    // Both init/set may update the worktree and lazily fetch promisor blobs.
    checkout_in_session(repo_path, &["sparse-checkout", "init", "--cone"], session)
        .context("Failed to init sparse-checkout")?;

    // Set the directories to materialize
    let mut args = vec!["sparse-checkout", "set"];
    for dir in dirs {
        args.push(dir);
    }
    checkout_in_session(repo_path, &args, session).context("Failed to set sparse-checkout")?;

    // Checkout materialized files — this is where blob:none clones actually
    // fetch file content from the remote.  A failure here (e.g. HTTP/2 framing
    // error, promisor remote offline) means nothing was materialised; treating
    // it as non-fatal would leave a broken cache that blocks future retries.
    let remote = remote_origin_url(repo_path)?;
    if let Err(error) = run_remote_git(repo_path, &["checkout"], &remote, session) {
        let err = error.to_string();
        let err_lower = err.to_lowercase();

        // Hard failures: promisor-remote fetch errors, RPC/HTTP failures,
        // packfile corruption — the checkout produced no usable files.
        let is_hard_failure = err_lower.contains("promisor remote")
            || err_lower.contains("rpc failed")
            || err_lower.contains("expected 'packfile'")
            || err_lower.contains("could not fetch")
            || err_lower.contains("http2 framing")
            || err_lower.contains("fatal:");

        if is_hard_failure {
            // The failure is usually the *mirror* choking on lazy blob batches
            // (ghproxy-style accelerators commonly break partial-clone on-demand
            // fetches), not GitHub itself. The promisor remote is resolved from
            // origin per invocation, so one direct retry — mirror rewrite
            // disabled — frequently materializes what the mirror could not.
            warn!(
                target: "git_ops",
                error = err.trim(),
                "sparse checkout blob fetch failed; retrying once without a mirror"
            );
            if let Err(direct_error) = transport::execute_remote_git(
                Some(repo_path),
                &["checkout"],
                &remote,
                session,
                false,
            ) {
                return Err(anyhow!(
                    "git checkout failed (blob fetch, retried direct): {} / direct attempt: {}",
                    err.trim(),
                    direct_error
                ));
            }
            return Ok(());
        }

        // Truly non-fatal: minor warnings, modified-file notices, etc.
        warn!(target: "git_ops", warning = err.trim(), "sparse checkout warning");
    }

    Ok(())
}

/// Return the exact commit currently checked out by a managed repository.
pub fn head_revision(repo_path: &Path) -> Result<String> {
    run_git(repo_path, &["rev-parse", "HEAD"])
}

/// Marker error: the worktree gained uncommitted modifications during a
/// fetch, so the subsequent `reset --hard` must NOT run — doing so would
/// destroy the user's edits. Callers that roll back failed updates on error
/// must downcast for this type and skip the reset (and any snapshot restore
/// that would overwrite the edited files).
#[derive(Debug)]
pub struct WorktreeDirty;

impl std::fmt::Display for WorktreeDirty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Skill checkout changed during the update; edits were preserved — retry to keep them or discard them explicitly"
        )
    }
}

impl std::error::Error for WorktreeDirty {}

/// Read the configured origin URL without mutating the repository.
pub fn remote_origin_url(repo_path: &Path) -> Result<String> {
    run_git(repo_path, &["remote", "get-url", "origin"])
}

/// Run a checkout that may lazily download blobs from a promisor remote.
pub fn checkout_in_session(
    repo_path: &Path,
    args: &[&str],
    session: &GitOperationSession,
) -> Result<String> {
    let remote = remote_origin_url(repo_path)?;
    run_remote_git(repo_path, args, &remote, session)
}

/// Git failed because the recorded remote ref no longer exists.
///
/// Typical stderr: `fatal: couldn't find remote ref <name>`. Callers with a
/// usable local checkout must not treat this as a hard install/update failure.
pub fn is_missing_remote_ref(error: &dyn std::error::Error) -> bool {
    let mut current = Some(error);
    while let Some(err) = current {
        let text = err.to_string();
        if text.contains("couldn't find remote ref")
            || text.contains("Could not find remote branch")
        {
            return true;
        }
        current = err.source();
    }
    false
}

fn run_git(repo_path: &Path, args: &[&str]) -> Result<String> {
    // Local git operations can still touch the network (lazy fetch on
    // checkout, remote get-url). Walk the mirror candidate chain, then fall
    // back to a direct GitHub connection only after every candidate fails.
    let candidates = github_mirror::candidate_mirror_urls();
    if candidates.is_empty() {
        return run_git_with_mirror(repo_path, args, None);
    }

    let mut last_error: Option<anyhow::Error> = None;
    for mirror in &candidates {
        match run_git_with_mirror(repo_path, args, Some(mirror)) {
            Ok(output) => {
                github_health::record_success(mirror, None);
                return Ok(output);
            }
            Err(e) if github_mirror::is_mirror_transport_error(&e.to_string()) => {
                github_health::record_failure(mirror);
                last_error = Some(e);
            }
            Err(e) => return Err(e),
        }
    }

    match run_git_with_mirror(repo_path, args, None) {
        Ok(output) => Ok(output),
        Err(e) => Err(last_error.unwrap_or(e)),
    }
}

fn run_git_with_mirror(repo_path: &Path, args: &[&str], mirror: Option<&str>) -> Result<String> {
    let mut cmd = command_with_path("git");
    // Mirror args (-c url.*.insteadOf) must precede the subcommand.
    // For local-only operations (rev-parse, reset) this is harmless.
    if let Some(mirror_url) = mirror {
        github_mirror::apply_mirror_args_for(&mut cmd, mirror_url);
    }
    let output = cmd
        .current_dir(repo_path)
        .args(args)
        .output()
        .with_context(|| format!("Failed to execute git {}", args.join(" ")))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("git {} failed: {}", args.join(" "), err.trim()));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn run_remote_git(
    repo_path: &Path,
    args: &[&str],
    remote: &str,
    session: &GitOperationSession,
) -> Result<String> {
    // `execute_remote_git` already walks the mirror candidate chain and falls
    // back to direct GitHub; no outer retry loop needed here.
    transport::execute_remote_git(Some(repo_path), args, remote, session, true)
        .map(|output| output.stdout.trim().to_string())
        .map_err(anyhow::Error::from)
}

#[cfg(test)]
mod tests;
