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

#[test]
fn check_update_refuses_a_directory_without_its_own_git() -> Result<()> {
    // A plain hub directory (bundle/pack install) must not let git discovery
    // escape upwards and fetch an ancestor repository.
    let plain = tempfile::tempdir()?;
    fs::create_dir_all(plain.path().join("skills/plain"))?;

    let error = check_update(&plain.path().join("skills/plain")).unwrap_err();
    assert!(error.to_string().contains("not a git repository"));

    Ok(())
}

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
fn ensure_worktree_checked_out_materializes_files() -> Result<()> {
    let temp_root = make_temp_root("worktree")?;
    let source = temp_root.join("source");
    fs::create_dir_all(&source)?;
    run_git(&source, &["init"])?;
    fs::write(source.join("a.txt"), "a")?;
    run_git(&source, &["add", "a.txt"])?;
    run_git(
        &source,
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

    let dest = temp_root.join("dest");
    run_git(&temp_root, &["clone", source.to_str().unwrap(), "dest"])?;

    // Simulate a historical install with only .git present
    for entry in fs::read_dir(&dest)? {
        let entry = entry?;
        if entry.file_name() != ".git" {
            if entry.file_type()?.is_dir() {
                fs::remove_dir_all(entry.path())?;
            } else {
                fs::remove_file(entry.path())?;
            }
        }
    }
    assert!(!dest.join("a.txt").exists());

    let fixed = ensure_worktree_checked_out(&dest)?;
    assert!(fixed);
    assert!(dest.join("a.txt").exists());

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
fn rename_records_are_parsed_from_z_output() {
    let output = "R087\0skills/one/SKILL.md\0skills/engineering/one-spec/SKILL.md\0\
                  A\0skills/new/SKILL.md\0\
                  C075\0a.md\0b.md\0\
                  R100\0old/x.md\0new/x.md\0";
    let renames = parse_rename_records(output);
    assert_eq!(
        renames,
        vec![
            RenamedPath {
                from: "skills/one/SKILL.md".into(),
                to: "skills/engineering/one-spec/SKILL.md".into(),
                similarity: 87,
            },
            RenamedPath {
                from: "old/x.md".into(),
                to: "new/x.md".into(),
                similarity: 100,
            },
        ]
    );
    assert!(parse_rename_records("").is_empty());
}
