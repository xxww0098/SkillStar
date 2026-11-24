use super::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

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
        "ss-git-ops-{}-{}-{}",
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
