use anyhow::{Context, Result, anyhow};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use crate::core::config::github_mirror;
use crate::core::path_env::command_with_path;
use tracing::{debug, warn};

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

/// Clone a repository from a URL to a destination path.
///
/// Always uses `--depth 1 --single-branch` to minimise network transfer
/// and disk usage. Skills only need the latest snapshot, not full history.
pub fn clone_repo(url: &str, dest: &Path) -> Result<()> {
    clone_repo_shallow_inner(url, dest, true)
}

/// Shallow-clone a repository (depth=1) for fast scanning.
///
/// Only fetches the latest commit – ideal for repo scanning where full
/// history is unnecessary.  Unlike `clone_repo`, does *not* pass
/// `--single-branch` so remote tracking refs are created for update checks.
pub fn clone_repo_shallow(url: &str, dest: &Path) -> Result<()> {
    clone_repo_shallow_inner(url, dest, false)
}

/// Shared implementation for shallow clones.
fn clone_repo_shallow_inner(url: &str, dest: &Path, single_branch: bool) -> Result<()> {
    let mut cmd = command_with_path("git");
    github_mirror::apply_mirror_args(&mut cmd);
    cmd.args(["clone", "--depth", "1"]);
    if single_branch {
        cmd.arg("--single-branch");
    }
    let output = cmd
        .arg(url)
        .arg(dest)
        .output()
        .context("Failed to execute git clone")?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("git clone failed: {}", err.trim()));
    }

    Ok(())
}

/// Sparse treeless clone: only downloads tree objects, no file blobs.
///
/// Uses `--filter=blob:none --depth 1 --no-checkout --sparse` so the initial
/// clone is extremely small (typically <500KB even for large repos). Callers
/// must follow up with `git sparse-checkout set <dirs>` + `git checkout` to
/// materialize only the directories they need.
pub fn clone_repo_sparse(url: &str, dest: &Path) -> Result<()> {
    let mut cmd = command_with_path("git");
    github_mirror::apply_mirror_args(&mut cmd);
    let output = cmd
        .args([
            "clone",
            "--filter=blob:none",
            "--depth",
            "1",
            "--no-checkout",
            "--sparse",
        ])
        .arg(url)
        .arg(dest)
        .output()
        .context("Failed to execute sparse treeless clone")?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("git sparse clone failed: {}", err.trim()));
    }

    Ok(())
}

/// List all file paths in the repository tree without a checkout.
///
/// Requires at least tree objects to be present (works after a treeless clone
/// with `--filter=blob:none`). Returns paths relative to the repo root.
pub fn list_tree_paths(repo_path: &Path) -> Result<Vec<String>> {
    let output = command_with_path("git")
        .current_dir(repo_path)
        .args(["ls-tree", "-r", "--name-only", "HEAD"])
        .output()
        .context("Failed to execute git ls-tree")?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("git ls-tree failed: {}", err.trim()));
    }

    let paths = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|l| l.to_string())
        .collect();
    Ok(paths)
}

/// Configure sparse-checkout for a repo and materialize the given directories.
///
/// Expects the repo to have been cloned with `clone_repo_sparse`. Sets cone-mode
/// sparse-checkout to the given directory patterns then runs `git checkout`.
pub fn apply_sparse_checkout(repo_path: &Path, dirs: &[&str]) -> Result<()> {
    // Init sparse-checkout in cone mode
    let output = command_with_path("git")
        .current_dir(repo_path)
        .args(["sparse-checkout", "init", "--cone"])
        .output()
        .context("Failed to init sparse-checkout")?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("git sparse-checkout init failed: {}", err.trim()));
    }

    // Set the directories to materialize
    let mut cmd = command_with_path("git");
    cmd.current_dir(repo_path).args(["sparse-checkout", "set"]);
    for dir in dirs {
        cmd.arg(dir);
    }
    let output = cmd.output().context("Failed to set sparse-checkout")?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("git sparse-checkout set failed: {}", err.trim()));
    }

    // Checkout materialized files — this is where blob:none clones actually
    // fetch file content from the remote.  A failure here (e.g. HTTP/2 framing
    // error, promisor remote offline) means nothing was materialised; treating
    // it as non-fatal would leave a broken cache that blocks future retries.
    let mut checkout_cmd = command_with_path("git");
    github_mirror::apply_mirror_args(&mut checkout_cmd);
    let output = checkout_cmd
        .current_dir(repo_path)
        .arg("checkout")
        .output()
        .context("Failed to execute git checkout after sparse-checkout")?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
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

    run_git(repo_path, &["checkout", "-f", "HEAD"])?;
    Ok(true)
}

/// Fetch latest changes and check if update is available.
///
/// Uses `--depth 1` to keep network transfer minimal — we only need the
/// tip commit hash, not additional history.
pub fn check_update(repo_path: &Path) -> Result<bool> {
    // Depth-1 fetch: updates remote refs without downloading extra history.
    // Retry on shallow-file race condition.
    run_git_shallow_fetch(repo_path, &["fetch", "--depth", "1", "--quiet"])?;

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
    run_git_shallow_fetch(repo_path, &["fetch", "--depth", "1", "--quiet"])?;

    // Determine the correct reset target.
    // Try origin/HEAD first, then the tracking upstream, then FETCH_HEAD.
    let target = run_git(repo_path, &["rev-parse", "origin/HEAD"])
        .or_else(|_| run_git(repo_path, &["rev-parse", "@{upstream}"]))
        .or_else(|_| run_git(repo_path, &["rev-parse", "FETCH_HEAD"]))
        .unwrap_or_else(|_| "FETCH_HEAD".to_string());

    run_git(repo_path, &["reset", "--hard", &target])?;
    Ok(())
}

/// Run a git fetch with retry logic for the shallow-file race condition.
///
/// When multiple processes or threads run `git fetch --depth 1` on the same
/// shallow repo concurrently, Git can fail with:
///   `fatal: shallow file has changed since we read it`
/// This is a transient condition — retrying after a short backoff resolves it.
pub fn run_git_shallow_fetch(repo_path: &Path, args: &[&str]) -> Result<String> {
    let repo_lock = shallow_fetch_lock(repo_path);
    // Poisoned mutex: another thread panicked while holding the lock (e.g. antivirus
    // injection). `into_inner()` recovers the guard safely — the HashMap inside holds
    // only PathBuf keys and has no permanent invariant to corrupt.
    let _fetch_guard = repo_lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    let mut last_err = None;
    for attempt in 0..SHALLOW_FETCH_MAX_RETRIES {
        match run_git(repo_path, args) {
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
    let mut cmd = command_with_path("git");
    // Mirror args (-c url.*.insteadOf) must precede the subcommand.
    // For local-only operations (rev-parse, reset) this is harmless.
    github_mirror::apply_mirror_args(&mut cmd);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn check_update_returns_false_when_up_to_date() -> Result<()> {
        let temp_root = make_temp_root("up-to-date")?;
        let source = setup_remote_and_source(&temp_root)?;

        write_and_push_commit(&source, "README.md", "v1", "initial")?;
        let local = clone_remote_to_local(&temp_root)?;

        assert!(!check_update(&local)?);

        let _ = fs::remove_dir_all(temp_root);
        Ok(())
    }

    #[test]
    fn check_update_returns_true_when_remote_has_new_commit() -> Result<()> {
        let temp_root = make_temp_root("remote-new-commit")?;
        let source = setup_remote_and_source(&temp_root)?;

        write_and_push_commit(&source, "README.md", "v1", "initial")?;
        let local = clone_remote_to_local(&temp_root)?;
        assert!(!check_update(&local)?);

        write_and_push_commit(&source, "README.md", "v2", "second")?;

        assert!(check_update(&local)?);

        let _ = fs::remove_dir_all(temp_root);
        Ok(())
    }

    fn make_temp_root(suffix: &str) -> Result<PathBuf> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("Failed to read system time")?
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "skillstar-git-ops-{}-{}-{}",
            suffix,
            std::process::id(),
            stamp
        ));
        fs::create_dir_all(&dir).with_context(|| format!("Failed to create {}", dir.display()))?;
        Ok(dir)
    }

    fn setup_remote_and_source(root: &Path) -> Result<PathBuf> {
        let source = root.join("source");

        run_git(root, &["init", "--bare", "remote.git"])?;
        run_git(root, &["clone", "remote.git", "source"])?;
        Ok(source)
    }

    fn clone_remote_to_local(root: &Path) -> Result<PathBuf> {
        let local = root.join("local");
        run_git(root, &["clone", "remote.git", "local"])?;
        Ok(local)
    }

    fn write_and_push_commit(
        repo_path: &Path,
        file_name: &str,
        content: &str,
        message: &str,
    ) -> Result<()> {
        fs::write(repo_path.join(file_name), content)
            .with_context(|| format!("Failed to write {}", file_name))?;

        run_git(repo_path, &["add", file_name])?;
        run_git(
            repo_path,
            &[
                "-c",
                "user.name=Test User",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-m",
                message,
            ],
        )?;
        run_git(repo_path, &["push", "-u", "origin", "HEAD"])?;

        Ok(())
    }
}
