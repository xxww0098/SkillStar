use anyhow::{anyhow, Context, Result};
use std::fs;
use std::path::Path;

use super::path_env::command_with_path;

/// Compute the tree-hash of a local Git repository using gix
pub fn compute_tree_hash(repo_path: &Path) -> Result<String> {
    let repo = gix::open(repo_path).context("Failed to open git repository")?;
    let head = repo.head_commit().context("Failed to get HEAD commit")?;
    let tree_id = head.tree_id().context("Failed to get tree ID")?;
    Ok(tree_id.to_string())
}

/// Clone a repository from a URL to a destination path
pub fn clone_repo(url: &str, dest: &Path) -> Result<()> {
    let output = command_with_path("git")
        .arg("clone")
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

/// Shallow-clone a repository (depth=1) for fast scanning.
///
/// Only fetches the latest commit – ideal for repo scanning where full
/// history is unnecessary.
pub fn clone_repo_shallow(url: &str, dest: &Path) -> Result<()> {
    let output = command_with_path("git")
        .args(["clone", "--depth", "1"])
        .arg(url)
        .arg(dest)
        .output()
        .context("Failed to execute git clone --depth 1")?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("git shallow clone failed: {}", err.trim()));
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

/// Fetch latest changes and check if update is available
pub fn check_update(repo_path: &Path) -> Result<bool> {
    // Keep local remote refs fresh before comparing ahead/behind.
    run_git(repo_path, &["fetch", "--quiet"])?;

    let counts = run_git(
        repo_path,
        &["rev-list", "--left-right", "--count", "HEAD...@{upstream}"],
    )?;
    let (_ahead, behind) = parse_ahead_behind_counts(&counts)?;

    Ok(behind > 0)
}

/// Pull (fetch + fast-forward) a repository
pub fn pull_repo(repo_path: &Path) -> Result<()> {
    let output = command_with_path("git")
        .current_dir(repo_path)
        .arg("pull")
        .output()
        .context("Failed to execute git pull")?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("git pull failed: {}", err));
    }
    Ok(())
}

fn run_git(repo_path: &Path, args: &[&str]) -> Result<String> {
    let output = command_with_path("git")
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

fn parse_ahead_behind_counts(output: &str) -> Result<(u32, u32)> {
    let mut parts = output.split_whitespace();
    let ahead = parts
        .next()
        .ok_or_else(|| anyhow!("git rev-list output missing ahead count"))?
        .parse::<u32>()
        .context("Failed to parse ahead count from git rev-list output")?;
    let behind = parts
        .next()
        .ok_or_else(|| anyhow!("git rev-list output missing behind count"))?
        .parse::<u32>()
        .context("Failed to parse behind count from git rev-list output")?;

    if parts.next().is_some() {
        return Err(anyhow!("git rev-list output has unexpected extra fields"));
    }

    Ok((ahead, behind))
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
