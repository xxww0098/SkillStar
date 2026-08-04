//! Owns one judgement: is this hub entry a link into the repo cache, and
//! which git repo backs it?
//!
//! This judgement used to live in two places — `update_checker` (plain
//! symlinks only, via `std::fs::read_link`) and `repo_scanner::ops` (symlinks
//! *and* Windows junctions, via `fs_ops::read_link_resolved`). The two had
//! already diverged: on Windows a junction-backed skill was repo-cached
//! according to one and not the other, so update detection and update
//! application disagreed about which path a skill was on.
//!
//! Everything now resolves links through [`fs_ops::read_link_resolved`], which
//! falls back to `junction::get_target` on Windows.

use skillstar_core::infra::{fs_ops, paths};
use std::path::{Path, PathBuf};

use crate::git::ops as git_ops;

/// Is this hub entry a link (symlink or Windows junction) into the repo cache?
pub(crate) fn is_repo_cached(skill_path: &Path) -> bool {
    is_repo_cached_with(skill_path, &paths::repos_cache_dir())
}

/// Resolve a repo-cached hub entry to the git repo root backing it.
///
/// `None` for anything that is not a link into the repo cache — callers use
/// that to fall back to treating the skill as a standalone clone.
pub(crate) fn repo_root_of(skill_path: &Path) -> Option<PathBuf> {
    repo_root_of_with(
        skill_path,
        &paths::repos_cache_dir(),
        git_ops::find_repo_root,
    )
}

// ── internal seam: tests inject the cache dir and root resolution ────

pub(crate) fn is_repo_cached_with(skill_path: &Path, repo_cache_dir: &Path) -> bool {
    if !fs_ops::is_link(skill_path) {
        return false;
    }
    let Ok(target) = fs_ops::read_link_resolved(skill_path) else {
        return false;
    };
    is_inside(&target, repo_cache_dir)
}

pub(crate) fn repo_root_of_with<F>(
    skill_path: &Path,
    repo_cache_dir: &Path,
    find_repo_root: F,
) -> Option<PathBuf>
where
    F: Fn(&Path) -> Option<PathBuf>,
{
    if !is_repo_cached_with(skill_path, repo_cache_dir) {
        return None;
    }
    let target = fs_ops::read_link_resolved(skill_path).ok()?;
    let target = std::fs::canonicalize(&target).unwrap_or(target);
    find_repo_root(&target)
}

/// Containment test that survives Windows separators and case folding.
fn is_inside(target: &Path, repo_cache_dir: &Path) -> bool {
    // macOS may expose the same temporary directory through both `/var/...`
    // and `/private/var/...`.  Repo installs canonicalize their link targets,
    // while the configured cache path can retain the shorter spelling.  Compare
    // canonical spellings when both paths exist so those links still classify
    // as belonging to the shared checkout.
    let target = std::fs::canonicalize(target).unwrap_or_else(|_| target.to_path_buf());
    let repo_cache_dir =
        std::fs::canonicalize(repo_cache_dir).unwrap_or_else(|_| repo_cache_dir.to_path_buf());
    let target = normalize(&target);
    let cache = normalize(&repo_cache_dir);
    target == cache || target.starts_with(&(cache + "/"))
}

fn normalize(path: &Path) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn cache_containment_matches_root_and_descendants() {
        let cache = Path::new("/tmp/cache/repos");
        assert!(is_inside(Path::new("/tmp/cache/repos"), cache));
        assert!(is_inside(Path::new("/tmp/cache/repos/acme--demo"), cache));
        assert!(!is_inside(Path::new("/tmp/cache/repos-other"), cache));
        assert!(!is_inside(Path::new("/tmp/elsewhere"), cache));
    }

    #[cfg(unix)]
    #[test]
    fn cache_containment_accepts_an_equivalent_symlinked_prefix() {
        let physical = tempfile::tempdir().unwrap();
        let cache = physical.path().join("repos");
        let target = cache.join("acme--demo/skill");
        fs::create_dir_all(&target).unwrap();

        let alias_parent = tempfile::tempdir().unwrap();
        let alias = alias_parent.path().join("cache-alias");
        std::os::unix::fs::symlink(&cache, &alias).unwrap();

        assert!(is_inside(&target, &alias));
    }

    // Unix-only: fixtures use std::os::unix::fs::symlink.
    #[cfg(unix)]
    #[test]
    fn detects_repo_cached_link_and_resolves_its_root() {
        let repo_cache = tempfile::tempdir().unwrap();
        let repo = repo_cache.path().join("acme--demo");
        fs::create_dir_all(repo.join("skill")).unwrap();
        let link_parent = tempfile::tempdir().unwrap();
        let link = link_parent.path().join("demo");
        std::os::unix::fs::symlink(repo.join("skill"), &link).unwrap();

        assert!(is_repo_cached_with(&link, repo_cache.path()));

        let root = repo_root_of_with(&link, repo_cache.path(), |path| {
            Some(path.parent()?.to_path_buf())
        });
        assert_eq!(root, Some(std::fs::canonicalize(repo.as_path()).unwrap()));
    }

    // Unix-only: fixtures use std::os::unix::fs::symlink.
    #[cfg(unix)]
    #[test]
    fn link_outside_the_cache_is_not_repo_cached() {
        let repo_cache = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        let target = elsewhere.path().join("skill");
        fs::create_dir_all(&target).unwrap();
        let link = elsewhere.path().join("demo");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        assert!(!is_repo_cached_with(&link, repo_cache.path()));
        assert_eq!(
            repo_root_of_with(&link, repo_cache.path(), |path| Some(path.to_path_buf())),
            None
        );
    }

    #[test]
    fn plain_directory_is_not_repo_cached() {
        let repo_cache = tempfile::tempdir().unwrap();
        let plain = repo_cache.path().join("not-a-link");
        fs::create_dir_all(&plain).unwrap();
        assert!(!is_repo_cached_with(&plain, repo_cache.path()));
    }

    // Unix-only: fixtures use std::os::unix::fs::symlink.
    #[cfg(unix)]
    #[test]
    fn app_level_entry_point_reads_repos_cache_dir() {
        let _guard = crate::lock_test_env();
        let temp = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
        }

        let repo_cache = paths::repos_cache_dir();
        fs::create_dir_all(&repo_cache).unwrap();
        let repo = repo_cache.join("demo-repo");
        fs::create_dir_all(&repo).unwrap();

        let link_parent = tempfile::tempdir().unwrap();
        let link = link_parent.path().join("demo");
        std::os::unix::fs::symlink(&repo, &link).unwrap();

        assert!(is_repo_cached(&link));

        unsafe {
            std::env::remove_var("SKILLSTAR_DATA_DIR");
        }
    }
}
