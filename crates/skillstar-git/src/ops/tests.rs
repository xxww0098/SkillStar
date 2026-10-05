use super::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn find_repo_root_finds_git_ancestor() -> Result<()> {
    let temp_root = make_temp_root("find-root")?;
    let repo = temp_root.join("repo");
    fs::create_dir_all(&repo)?;
    run_git(&repo, &["init"])?;
    let nested = repo.join("deep").join("nested");
    fs::create_dir_all(&nested)?;

    assert_eq!(find_repo_root(&nested), Some(repo.clone()));
    assert_eq!(find_repo_root(&repo), Some(repo.clone()));

    let _ = fs::remove_dir_all(temp_root);
    Ok(())
}

#[test]
fn find_repo_root_returns_none_outside_repo() -> Result<()> {
    let temp_root = make_temp_root("no-root")?;
    assert_eq!(find_repo_root(&temp_root), None);
    let _ = fs::remove_dir_all(temp_root);
    Ok(())
}

#[test]
fn compute_tree_hash_on_real_repo() -> Result<()> {
    let temp_root = make_temp_root("tree-hash")?;
    let repo = temp_root.join("repo");
    fs::create_dir_all(&repo)?;
    run_git(&repo, &["init"])?;
    fs::write(repo.join("file.txt"), "hello")?;
    run_git(&repo, &["add", "file.txt"])?;
    run_git(
        &repo,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "init",
        ],
    )?;

    let hash = compute_tree_hash(&repo)?;
    assert!(!hash.is_empty());
    assert_eq!(hash.len(), 40);

    let _ = fs::remove_dir_all(temp_root);
    Ok(())
}

#[test]
fn compute_tree_hash_fallback_on_non_git_path() -> Result<()> {
    let temp_root = make_temp_root("no-git")?;
    let result = compute_tree_hash(&temp_root);
    assert!(result.is_err());
    let _ = fs::remove_dir_all(temp_root);
    Ok(())
}

#[test]
fn list_tree_paths_returns_file_names() -> Result<()> {
    let temp_root = make_temp_root("ls-tree")?;
    let repo = temp_root.join("repo");
    fs::create_dir_all(&repo)?;
    run_git(&repo, &["init"])?;
    fs::write(repo.join("top.txt"), "top")?;
    fs::create_dir_all(repo.join("sub"))?;
    fs::write(repo.join("sub").join("nested.txt"), "nested")?;
    #[cfg(unix)]
    fs::write(repo.join("line\nbreak.txt"), "odd")?;
    run_git(&repo, &["add", "."])?;
    run_git(
        &repo,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "init",
        ],
    )?;

    let paths = list_tree_paths(&repo)?;
    assert!(paths.contains(&"top.txt".to_string()));
    assert!(paths.contains(&"sub/nested.txt".to_string()));
    #[cfg(unix)]
    assert!(paths.contains(&"line\nbreak.txt".to_string()));

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

#[test]
fn local_file_url_uses_forward_slashes_and_a_drive_slash() {
    assert_eq!(local_file_url(Path::new("/tmp/repo")), "file:///tmp/repo");
    assert_eq!(
        local_file_url(Path::new(r"C:\Users\runner\AppData\Local\Temp\repo")),
        "file:///C:/Users/runner/AppData/Local/Temp/repo"
    );
    assert_eq!(
        local_file_url(Path::new(r"\\?\C:\Users\repo")),
        "file:///C:/Users/repo"
    );
}

#[test]
fn missing_remote_ref_is_detected_through_anyhow_context() {
    let inner = anyhow!("other: fatal: couldn't find remote ref cursor/gone");
    let wrapped = inner.context("git fetch for ref 'cursor/gone' failed");
    assert!(is_missing_remote_ref(wrapped.as_ref()));
    assert!(!is_missing_remote_ref(
        anyhow!("fatal: could not read from remote repository").as_ref()
    ));
}
