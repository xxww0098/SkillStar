use anyhow::{Context, Result, anyhow};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use skillstar_core::config::github_mirror;
use skillstar_core::infra::path_env::command_with_path;
use tracing::{debug, warn};

use crate::transport::{self, GitOperationSession};
pub use crate::tree::{
    GitTreeEntry, list_tree_entries_at, list_tree_paths, list_tree_paths_at, revision_contains_path,
};

/// Maximum number of retries for shallow fetch operations that hit the
/// `shallow file has changed since we read it` race condition.
const SHALLOW_FETCH_MAX_RETRIES: u32 = 3;
/// Backoff delays (ms) between retries.
const SHALLOW_FETCH_BACKOFF_MS: [u64; 3] = [200, 500, 1000];
/// Per-repository shallow-fetch mutexes to avoid concurrent `.git/shallow` races.
static SHALLOW_FETCH_LOCKS: LazyLock<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Compute the tree-hash of a local Git repository.
///
/// Tries the in-process `gix` library first (fastest, no process spawn).
/// Falls back to `git rev-parse HEAD^{tree}` via CLI when `gix` fails —
/// this is needed on Windows where `gix` can choke on shallow clones
/// due to NTFS file-locking or path-normalization quirks.
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

/// Git tree hash of a single folder inside a repo (`HEAD:<folder>`).
///
/// Used to tell apart skills that share a repo: each one hashes only its own
/// `source_folder`, so an unrelated commit elsewhere in the repo does not mark
/// every sibling as updated.
pub fn compute_subtree_hash(repo_path: &Path, folder_path: &str) -> Result<String> {
    rev_parse(repo_path, &format!("HEAD:{folder_path}"))
        .with_context(|| format!("Failed to read subtree hash for '{folder_path}'"))
}

/// Clone a repository from a URL to a destination path.
///
/// Always uses `--depth 1 --single-branch` to minimise network transfer
/// and disk usage. Skills only need the latest snapshot, not full history.
pub fn clone_repo(url: &str, dest: &Path) -> Result<()> {
    clone_repo_in_session(url, dest, &GitOperationSession::public())
}

pub fn clone_repo_in_session(url: &str, dest: &Path, session: &GitOperationSession) -> Result<()> {
    clone_repo_shallow_inner(url, dest, true, session)
}

/// Shallow-clone a repository (depth=1) for fast scanning.
///
/// Only fetches the latest commit – ideal for repo scanning where full
/// history is unnecessary.  Unlike `clone_repo`, does *not* pass
/// `--single-branch` so remote tracking refs are created for update checks.
pub fn clone_repo_shallow(url: &str, dest: &Path) -> Result<()> {
    clone_repo_shallow_in_session(url, dest, &GitOperationSession::public())
}

pub fn clone_repo_shallow_in_session(
    url: &str,
    dest: &Path,
    session: &GitOperationSession,
) -> Result<()> {
    clone_repo_shallow_inner(url, dest, false, session)
}

/// Shared implementation for shallow clones.
fn clone_repo_shallow_inner(
    url: &str,
    dest: &Path,
    single_branch: bool,
    session: &GitOperationSession,
) -> Result<()> {
    run_git_clone(url, dest, single_branch, session)
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

/// Sparse treeless clone: only downloads tree objects, no file blobs.
///
/// Uses `--filter=blob:none --depth 1 --no-checkout --sparse` so the initial
/// clone is extremely small (typically <500KB even for large repos). Callers
/// must follow up with `git sparse-checkout set <dirs>` + `git checkout` to
/// materialize only the directories they need.
pub fn clone_repo_sparse(url: &str, dest: &Path) -> Result<()> {
    clone_repo_sparse_in_session(url, dest, &GitOperationSession::public())
}

pub fn clone_repo_sparse_in_session(
    url: &str,
    dest: &Path,
    session: &GitOperationSession,
) -> Result<()> {
    let args = [
        "clone",
        "--filter=blob:none",
        "--depth",
        "1",
        "--no-checkout",
        "--sparse",
    ];
    // Mirror candidate chain handled inside `execute_remote_git`.
    run_git_clone_attempt(url, dest, &args, session)
}

/// Configure sparse-checkout for a repo and materialize the given directories.
///
/// Expects the repo to have been cloned with `clone_repo_sparse`. Sets cone-mode
/// sparse-checkout to the given directory patterns then runs `git checkout`.
pub fn apply_sparse_checkout(repo_path: &Path, dirs: &[&str]) -> Result<()> {
    apply_sparse_checkout_in_session(repo_path, dirs, &GitOperationSession::public())
}

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
            return Err(anyhow!("git checkout failed (blob fetch): {}", err.trim()));
        }

        // Truly non-fatal: minor warnings, modified-file notices, etc.
        warn!(target: "git_ops", warning = err.trim(), "sparse checkout warning");
    }

    Ok(())
}

/// Ensure repository worktree files are present.
///
/// Some historical installs were fetched without checkout and ended up with only `.git`.
/// This function detects that state and materializes files via `git checkout -f HEAD`.
pub fn ensure_worktree_checked_out(repo_path: &Path) -> Result<bool> {
    ensure_worktree_checked_out_in_session(repo_path, &GitOperationSession::public())
}

pub fn ensure_worktree_checked_out_in_session(
    repo_path: &Path,
    session: &GitOperationSession,
) -> Result<bool> {
    if !repo_path.exists() || !repo_path.is_dir() {
        return Ok(false);
    }

    if !repo_path.join(".git").exists() {
        return Ok(false);
    }

    let mut has_non_git_entries = false;
    for entry in fs::read_dir(repo_path).context("Failed to inspect repo directory")? {
        let entry = entry?;
        if entry.file_name() != ".git" {
            has_non_git_entries = true;
            break;
        }
    }

    if has_non_git_entries {
        return Ok(false);
    }

    if remote_origin_url(repo_path).is_ok() {
        checkout_in_session(repo_path, &["checkout", "-f", "HEAD"], session)?;
    } else {
        // Historical/local test repositories can have no remote at all; such a
        // checkout cannot lazily fetch and remains a purely local operation.
        run_git(repo_path, &["checkout", "-f", "HEAD"])?;
    }
    Ok(true)
}

/// Fetch latest changes and check if update is available.
///
/// Uses `--depth 1` to keep network transfer minimal — we only need the
/// tip commit hash, not additional history.
pub fn check_update(repo_path: &Path) -> Result<bool> {
    check_update_in_session(repo_path, &GitOperationSession::public())
}

pub fn check_update_in_session(repo_path: &Path, session: &GitOperationSession) -> Result<bool> {
    // Depth-1 fetch: updates remote refs without downloading extra history.
    // Retry on shallow-file race condition.
    run_git_shallow_fetch_in_session(repo_path, &["fetch", "--depth", "1", "--quiet"], session)?;

    // For shallow repos, rev-list --left-right can be unreliable.
    // Compare HEAD vs FETCH_HEAD / @{upstream} via rev-parse instead.
    let local_head = run_git(repo_path, &["rev-parse", "HEAD"])?;
    let remote_head = run_git(repo_path, &["rev-parse", "@{upstream}"])
        .or_else(|_| run_git(repo_path, &["rev-parse", "FETCH_HEAD"]))?;

    Ok(local_head != remote_head)
}

/// Pull a repository to the latest remote HEAD.
///
/// Uses `fetch --depth 1` + `reset --hard` instead of `git pull` so that:
/// - Shallow clones stay shallow (git pull can re-deepen).
/// - The result is always exactly at origin HEAD (no merge conflicts).
/// - Network transfer is bounded to a single commit.
pub fn pull_repo(repo_path: &Path) -> Result<()> {
    pull_repo_in_session(repo_path, &GitOperationSession::public())
}

pub fn pull_repo_in_session(repo_path: &Path, session: &GitOperationSession) -> Result<()> {
    run_git_shallow_fetch_in_session(repo_path, &["fetch", "--depth", "1", "--quiet"], session)?;

    // The fetch is a network operation lasting up to seconds; a user edit
    // landing in that window must not be destroyed by the reset below.
    // Fail closed instead — the error propagates and the caller preserves
    // the worktree (see WorktreeDirty handling).
    if !worktree_is_clean(repo_path)? {
        return Err(WorktreeDirty.into());
    }

    // Determine the correct reset target.
    // Try origin/HEAD first, then the tracking upstream, then FETCH_HEAD.
    let target = run_git(repo_path, &["rev-parse", "origin/HEAD"])
        .or_else(|_| run_git(repo_path, &["rev-parse", "@{upstream}"]))
        .or_else(|_| run_git(repo_path, &["rev-parse", "FETCH_HEAD"]))
        .unwrap_or_else(|_| "FETCH_HEAD".to_string());

    checkout_in_session(repo_path, &["reset", "--hard", &target], session)?;
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

/// Whether the repository worktree has uncommitted *modifications*.
///
/// Only modified/added/staged/unmerged tracked entries count as dirty.
/// Untracked files survive `git reset --hard` untouched, and deleted files
/// are restored from the index — neither loses user data — while a modified
/// tracked file would be overwritten. This also keeps sparse checkouts whose
/// directories were never materialized (reported as deleted) from false
/// positives.
pub fn worktree_is_clean(repo_path: &Path) -> Result<bool> {
    let output = run_git(repo_path, &["status", "--porcelain"])?;
    Ok(output.lines().all(|line| {
        let mut codes = line.chars();
        let index_code = codes.next().unwrap_or(' ');
        let worktree_code = codes.next().unwrap_or(' ');
        !matches!(index_code, 'M' | 'T' | 'A' | 'U') && !matches!(worktree_code, 'M' | 'T' | 'U')
    }))
}

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

/// Read the blob at `<revision>:<path>` through the operation session.
///
/// In a partial (`blob:none`) clone an object that was never checked out is
/// fetched lazily, so this is a remote operation: it must honor the session's
/// proxy, mirror, and credential policy exactly like a checkout. Blobs larger
/// than `max_bytes` are refused before being loaded into memory.
pub fn read_blob_in_session(
    repo_path: &Path,
    revision: &str,
    path: &str,
    max_bytes: u64,
    session: &GitOperationSession,
) -> Result<String> {
    let spec = format!("{revision}:{path}");
    let remote = remote_origin_url(repo_path)?;
    let size: u64 = run_remote_git(repo_path, &["cat-file", "-s", &spec], &remote, session)?
        .trim()
        .parse()
        .with_context(|| format!("git cat-file -s returned no size for '{spec}'"))?;
    if size > max_bytes {
        return Err(anyhow!(
            "'{spec}' is {size} bytes, above the {max_bytes}-byte limit"
        ));
    }
    run_remote_git(repo_path, &["cat-file", "blob", &spec], &remote, session)
}

/// One `R` record of `git diff -M --name-status` between two revisions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenamedPath {
    pub from: String,
    pub to: String,
    /// Similarity percentage git assigned (100 = identical content).
    pub similarity: u8,
}

/// Renames git detects between `from` and `to`, limited to `pathspecs`.
///
/// Similarity needs both blobs, so in a partial clone this may lazy-fetch the
/// `to` side — hence the session. Keep `pathspecs` tight (the folder that
/// disappeared plus the folders that appeared) so only those blobs are pulled.
pub fn diff_renames_in_session(
    repo_path: &Path,
    from: &str,
    to: &str,
    pathspecs: &[&str],
    session: &GitOperationSession,
) -> Result<Vec<RenamedPath>> {
    let remote = remote_origin_url(repo_path)?;
    let mut args = vec![
        "diff",
        "-M",
        "--name-status",
        "--diff-filter=R",
        "-z",
        from,
        to,
        "--",
    ];
    args.extend(pathspecs);
    let output = transport::execute_remote_git(Some(repo_path), &args, &remote, session, true)
        .map_err(anyhow::Error::from)?;
    Ok(parse_rename_records(&output.stdout))
}

/// `-z` output is `R<score>\0<from>\0<to>\0` per rename; anything else
/// (`A`, `D`, `M` records) carries one path and is skipped.
fn parse_rename_records(output: &str) -> Vec<RenamedPath> {
    let mut fields = output.split('\0');
    let mut renames = Vec::new();
    while let Some(status) = fields.next() {
        if status.is_empty() {
            continue;
        }
        let Some(from) = fields.next() else { break };
        if let Some(score) = status.strip_prefix('R') {
            let Some(to) = fields.next() else { break };
            renames.push(RenamedPath {
                from: from.to_string(),
                to: to.to_string(),
                similarity: score.parse().unwrap_or(0).min(100),
            });
        } else if status.starts_with('C') {
            // Copies also carry two paths; not a rename, just keep in step.
            let _ = fields.next();
        }
    }
    renames
}

/// Restore a repository to a previously captured commit after a failed update.
pub fn reset_to_revision(repo_path: &Path, revision: &str) -> Result<()> {
    reset_to_revision_in_session(repo_path, revision, &GitOperationSession::public())
}

pub fn reset_to_revision_in_session(
    repo_path: &Path,
    revision: &str,
    session: &GitOperationSession,
) -> Result<()> {
    checkout_in_session(repo_path, &["reset", "--hard", revision], session).map(|_| ())
}

/// Capture cone-mode sparse paths so a failed update can restore its old view.
pub fn sparse_checkout_paths(repo_path: &Path) -> Option<Vec<String>> {
    run_git(repo_path, &["sparse-checkout", "list"])
        .ok()
        .map(|output| {
            output
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_string)
                .collect()
        })
}

pub fn restore_sparse_checkout_paths(repo_path: &Path, paths: &[String]) -> Result<()> {
    restore_sparse_checkout_paths_in_session(repo_path, paths, &GitOperationSession::public())
}

pub fn restore_sparse_checkout_paths_in_session(
    repo_path: &Path,
    paths: &[String],
    session: &GitOperationSession,
) -> Result<()> {
    if paths.is_empty() {
        checkout_in_session(repo_path, &["sparse-checkout", "disable"], session).map(|_| ())
    } else {
        let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
        apply_sparse_checkout_in_session(repo_path, &refs, session)
    }
}

/// Discard every local change in one managed Skill subtree without moving HEAD.
///
/// A repo-cached Skill supplies its source folder so siblings in the same
/// checkout remain untouched until the user resolves them too. A standalone
/// clone supplies `None`, which restores the complete checkout.
pub fn restore_worktree_to_head(repo_path: &Path, pathspec: Option<&str>) -> Result<()> {
    restore_worktree_to_head_in_session(repo_path, pathspec, &GitOperationSession::public())
}

pub fn restore_worktree_to_head_in_session(
    repo_path: &Path,
    pathspec: Option<&str>,
    session: &GitOperationSession,
) -> Result<()> {
    let clean_args = |pathspec: Option<&str>| {
        let mut args: Vec<String> = vec![
            "clean",
            "-fdx",
            "-e",
            ".skillstar/",
            "-e",
            ".DS_Store",
            "-e",
            "Thumbs.db",
            "-e",
            "desktop.ini",
            "-e",
            "._*",
            "-e",
            "*~",
            "-e",
            "*.swp",
            "-e",
            "*.swo",
            "-e",
            "*.tmp",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
        if let Some(pathspec) = pathspec {
            args.extend(["--".to_string(), pathspec.to_string()]);
        }
        args
    };
    match pathspec.filter(|value| !value.is_empty()) {
        Some(pathspec) => {
            validate_relative_pathspec(pathspec)?;
            let literal_pathspec = format!(":(literal){pathspec}");
            checkout_in_session(
                repo_path,
                &["checkout", "-f", "HEAD", "--", &literal_pathspec],
                session,
            )?;
            let clean_args = clean_args(Some(&literal_pathspec));
            let clean_refs: Vec<&str> = clean_args.iter().map(String::as_str).collect();
            run_git(repo_path, &clean_refs)?;
        }
        None => {
            checkout_in_session(repo_path, &["reset", "--hard", "HEAD"], session)?;
            let clean_args = clean_args(None);
            let clean_refs: Vec<&str> = clean_args.iter().map(String::as_str).collect();
            run_git(repo_path, &clean_refs)?;
        }
    }
    Ok(())
}

fn validate_relative_pathspec(pathspec: &str) -> Result<()> {
    let path = Path::new(pathspec);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        anyhow::bail!("Unsafe Git pathspec: {pathspec:?}");
    }
    Ok(())
}

/// Run a git fetch with retry logic for the shallow-file race condition.
///
/// When multiple processes or threads run `git fetch --depth 1` on the same
/// shallow repo concurrently, Git can fail with:
///   `fatal: shallow file has changed since we read it`
/// This is a transient condition — retrying after a short backoff resolves it.
pub fn run_git_shallow_fetch(repo_path: &Path, args: &[&str]) -> Result<String> {
    run_git_shallow_fetch_in_session(repo_path, args, &GitOperationSession::public())
}

pub fn run_git_shallow_fetch_in_session(
    repo_path: &Path,
    args: &[&str],
    session: &GitOperationSession,
) -> Result<String> {
    let repo_lock = shallow_fetch_lock(repo_path);
    // Poisoned mutex: another thread panicked while holding the lock (e.g. antivirus
    // injection). `into_inner()` recovers the guard safely — the HashMap inside holds
    // only PathBuf keys and has no permanent invariant to corrupt.
    let _fetch_guard = repo_lock
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let mut last_err = None;
    for attempt in 0..SHALLOW_FETCH_MAX_RETRIES {
        let remote = remote_origin_url(repo_path)?;
        match run_remote_git(repo_path, args, &remote, session) {
            Ok(output) => return Ok(output),
            Err(e) => {
                let err_msg = e.to_string();
                if err_msg.contains("shallow file has changed") {
                    let delay = SHALLOW_FETCH_BACKOFF_MS
                        .get(attempt as usize)
                        .copied()
                        .unwrap_or(1000);
                    warn!(
                        target: "git_ops",
                        path = %repo_path.display(),
                        attempt = attempt + 1,
                        max = SHALLOW_FETCH_MAX_RETRIES,
                        delay_ms = delay,
                        "shallow file race detected, retrying after backoff"
                    );
                    std::thread::sleep(std::time::Duration::from_millis(delay));
                    last_err = Some(e);
                } else {
                    return Err(e);
                }
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow!("shallow fetch failed after retries")))
}

fn shallow_fetch_lock(repo_path: &Path) -> Arc<Mutex<()>> {
    let mut locks = SHALLOW_FETCH_LOCKS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    locks
        .entry(repo_path.to_path_buf())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
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
            Ok(output) => return Ok(output),
            Err(e) if github_mirror::is_mirror_transport_error(&e.to_string()) => {
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
