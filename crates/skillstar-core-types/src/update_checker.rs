use anyhow::{Result, anyhow};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn resolve_symlink(link_path: &Path) -> Option<PathBuf> {
    let target = std::fs::read_link(link_path).ok()?;
    if target.is_absolute() {
        Some(target)
    } else {
        Some(link_path.parent()?.join(target))
    }
}

pub fn normalize_path_for_compare(path: &Path) -> String {
    let normalized = path
        .to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string();
    #[cfg(windows)]
    {
        normalized.to_ascii_lowercase()
    }
    #[cfg(not(windows))]
    {
        normalized
    }
}

pub fn is_repo_cached_skill_target_path(target: &Path, repo_cache_dir: &Path) -> bool {
    let target_norm = normalize_path_for_compare(target);
    let repo_root_norm = normalize_path_for_compare(repo_cache_dir);
    target_norm == repo_root_norm || target_norm.starts_with(&(repo_root_norm + "/"))
}

pub fn is_repo_cached_skill(skill_path: &Path, repo_cache_dir: &Path) -> bool {
    let Ok(meta) = std::fs::symlink_metadata(skill_path) else {
        return false;
    };
    if !meta.file_type().is_symlink() {
        return false;
    }
    let Some(target) = resolve_symlink(skill_path) else {
        return false;
    };
    is_repo_cached_skill_target_path(&target, repo_cache_dir)
}

pub fn resolve_skill_repo_root<F>(
    skill_path: &Path,
    repo_cache_dir: &Path,
    find_repo_root: F,
) -> Option<PathBuf>
where
    F: Fn(&Path) -> Option<PathBuf>,
{
    if !is_repo_cached_skill(skill_path, repo_cache_dir) {
        return None;
    }
    let real_path = resolve_symlink(skill_path)?;
    find_repo_root(&real_path)
}

pub fn prefetch_unique_repos<F, G>(
    skill_paths: &[PathBuf],
    repo_cache_dir: &Path,
    find_repo_root: F,
    fetch_repo: G,
) -> HashSet<PathBuf>
where
    F: Fn(&Path) -> Option<PathBuf>,
    G: Fn(&Path) -> Result<()>,
{
    let mut fetched = HashSet::new();
    let mut failed = HashSet::new();

    for path in skill_paths {
        if let Some(root) = resolve_skill_repo_root(path, repo_cache_dir, &find_repo_root)
            && fetched.insert(root.clone())
            && fetch_repo(&root).is_err()
        {
            failed.insert(root);
        }
    }

    failed
}

pub fn check_update<F, G>(skill_path: &Path, fetch_repo: F, find_repo_root: G) -> bool
where
    F: Fn(&Path) -> Result<()>,
    G: Fn(&Path) -> Option<PathBuf>,
{
    let real_path = match resolve_symlink(skill_path) {
        Some(path) => path,
        None => return false,
    };
    let repo_root = match find_repo_root(&real_path) {
        Some(path) => path,
        None => return false,
    };
    if fetch_repo(&repo_root).is_err() {
        return false;
    }
    compare_heads(&repo_root).unwrap_or(false)
}

pub fn check_update_local<F>(
    skill_path: &Path,
    failed_fetch_roots: &HashSet<PathBuf>,
    find_repo_root: F,
) -> Option<bool>
where
    F: Fn(&Path) -> Option<PathBuf>,
{
    let real_path = match resolve_symlink(skill_path) {
        Some(path) => path,
        None => return Some(false),
    };
    let repo_root = match find_repo_root(&real_path) {
        Some(path) => path,
        None => return Some(false),
    };
    if failed_fetch_roots.contains(&repo_root) {
        return None;
    }
    Some(compare_heads(&repo_root).unwrap_or(false))
}

fn compare_heads(repo_root: &Path) -> Option<bool> {
    let local_head = git_rev_parse(repo_root, "HEAD")?;
    let remote_head = git_rev_parse(repo_root, "origin/HEAD")?;
    Some(!local_head.is_empty() && !remote_head.is_empty() && local_head != remote_head)
}

fn git_rev_parse(repo_dir: &Path, rev: &str) -> Option<String> {
    Command::new("git")
        .current_dir(repo_dir)
        .args(["rev-parse", rev])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

pub fn compute_subtree_hash(repo_dir: &Path, folder_path: &str) -> Result<String> {
    let output = Command::new("git")
        .current_dir(repo_dir)
        .args(["rev-parse", &format!("HEAD:{folder_path}")])
        .output()
        .map_err(|e| anyhow!("Failed to execute git rev-parse for subtree: {e}"))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("git rev-parse failed: {}", err.trim()));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

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

    #[test]
    fn normalize_repo_cache_paths() {
        let repo_cache = Path::new("/tmp/cache/repos");
        let target = Path::new("/tmp/cache/repos/acme--demo");
        assert!(is_repo_cached_skill_target_path(target, repo_cache));
    }

    #[cfg(unix)]
    #[test]
    fn repo_cached_symlink_detection_and_root_resolution() {
        let repo_cache = tempfile::tempdir().unwrap();
        let repo = repo_cache.path().join("acme--demo");
        fs::create_dir_all(repo.join("skill")).unwrap();
        let link_parent = tempfile::tempdir().unwrap();
        let link = link_parent.path().join("demo");
        std::os::unix::fs::symlink(repo.join("skill"), &link).unwrap();

        assert!(is_repo_cached_skill(&link, repo_cache.path()));
        let root = resolve_skill_repo_root(&link, repo_cache.path(), |path| {
            Some(path.parent()?.to_path_buf())
        });
        assert_eq!(root.as_deref(), Some(repo.as_path()));
    }

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

        let initial_hash = compute_subtree_hash(&clone_path, "skills/demo").unwrap();
        assert!(!initial_hash.is_empty());

        fs::write(remote.path().join("skills/demo/SKILL.md"), "v2").unwrap();
        run_git(remote.path(), &["add", "."]);
        run_git(remote.path(), &["commit", "-m", "update"]);

        run_git(&clone_path, &["fetch", "--depth", "1", "--quiet"]);

        let skill_link_parent = tempfile::tempdir().unwrap();
        let skill_link = skill_link_parent.path().join("demo");
        #[cfg(unix)]
        std::os::unix::fs::symlink(clone_path.join("skills/demo"), &skill_link).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(clone_path.join("skills/demo"), &skill_link).unwrap();

        let result = check_update_local(&skill_link, &HashSet::new(), |path| {
            Some(path.parent()?.parent()?.to_path_buf())
        });
        assert_eq!(result, Some(true));
    }

    #[test]
    fn prefetch_unique_repos_deduplicates_and_tracks_failures() {
        let repo_cache = tempfile::tempdir().unwrap();
        let repo_a = repo_cache.path().join("repo_a");
        let repo_b = repo_cache.path().join("repo_b");
        fs::create_dir_all(&repo_a).unwrap();
        fs::create_dir_all(&repo_b).unwrap();

        let link_parent = tempfile::tempdir().unwrap();
        let skill_a1 = link_parent.path().join("skill_a1");
        let skill_a2 = link_parent.path().join("skill_a2");
        let skill_b = link_parent.path().join("skill_b");

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&repo_a, &skill_a1).unwrap();
            std::os::unix::fs::symlink(&repo_a, &skill_a2).unwrap();
            std::os::unix::fs::symlink(&repo_b, &skill_b).unwrap();
        }
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_dir(&repo_a, &skill_a1).unwrap();
            std::os::windows::fs::symlink_dir(&repo_a, &skill_a2).unwrap();
            std::os::windows::fs::symlink_dir(&repo_b, &skill_b).unwrap();
        }

        let find_repo_root = |path: &Path| -> Option<PathBuf> { Some(path.to_path_buf()) };

        let fetch_calls = std::cell::RefCell::new(Vec::new());
        let fetch_repo = |root: &Path| -> Result<()> {
            fetch_calls.borrow_mut().push(root.to_path_buf());
            if root == repo_b {
                Err(anyhow!("fetch failed"))
            } else {
                Ok(())
            }
        };

        let failed = prefetch_unique_repos(
            &[skill_a1.clone(), skill_a2.clone(), skill_b.clone()],
            repo_cache.path(),
            find_repo_root,
            fetch_repo,
        );

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

    #[test]
    fn check_update_calls_fetch_and_returns_false_on_head_compare_failure() {
        let repo_cache = tempfile::tempdir().unwrap();
        let repo = repo_cache.path().join("repo");
        fs::create_dir_all(&repo).unwrap();

        let link_parent = tempfile::tempdir().unwrap();
        let link = link_parent.path().join("skill");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&repo, &link).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(&repo, &link).unwrap();

        let fetch_called = std::cell::Cell::new(false);
        let fetch_repo = |_: &Path| -> Result<()> {
            fetch_called.set(true);
            Ok(())
        };

        let find_repo_root = |_: &Path| -> Option<PathBuf> { Some(repo.clone()) };

        let result = check_update(&link, fetch_repo, find_repo_root);
        assert!(fetch_called.get(), "fetch should be called");
        assert_eq!(
            result, false,
            "should return false when heads can't be compared (not a git repo)"
        );
    }

    #[test]
    fn resolve_symlink_relative_target() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        fs::write(&target, "x").unwrap();
        let link = dir.path().join("link");
        #[cfg(unix)]
        std::os::unix::fs::symlink("target", &link).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_file("target", &link).unwrap();

        let resolved = resolve_symlink(&link).unwrap();
        assert_eq!(resolved, target);
    }
}
