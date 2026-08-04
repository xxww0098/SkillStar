use super::*;

struct TestHub {
    previous: Option<std::ffi::OsString>,
    temp: tempfile::TempDir,
}

impl TestHub {
    fn new() -> Self {
        let previous = std::env::var_os("SKILLSTAR_HUB_DIR");
        let temp = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var("SKILLSTAR_HUB_DIR", temp.path());
        }
        Self { previous, temp }
    }

    fn skill_dir(&self, name: &str) -> PathBuf {
        self.temp.path().join("skills").join(name)
    }
}

impl Drop for TestHub {
    fn drop(&mut self) {
        unsafe {
            match self.previous.take() {
                Some(value) => std::env::set_var("SKILLSTAR_HUB_DIR", value),
                None => std::env::remove_var("SKILLSTAR_HUB_DIR"),
            }
        }
    }
}

#[test]
fn content_facade_reads_lists_and_updates_a_skill() {
    let _guard = crate::lock_test_env();
    let previous_hub = std::env::var_os("SKILLSTAR_HUB_DIR");
    let temp = tempfile::tempdir().unwrap();
    unsafe {
        std::env::set_var("SKILLSTAR_HUB_DIR", temp.path());
    }

    let skill_dir = skillstar_core::infra::paths::hub_skills_dir().join("demo");
    std::fs::create_dir_all(skill_dir.join("scripts")).unwrap();
    std::fs::create_dir_all(skill_dir.join(".git")).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        "---\ndescription: old\n---\n\nBody",
    )
    .unwrap();
    std::fs::write(skill_dir.join("scripts/run.sh"), "echo ok").unwrap();
    std::fs::write(skill_dir.join(".git/config"), "ignored").unwrap();

    assert_eq!(
        read_raw("demo").unwrap(),
        "---\ndescription: old\n---\n\nBody"
    );
    assert_eq!(
        list_files("demo").unwrap(),
        vec!["SKILL.md", "scripts/run.sh"]
    );

    update("demo", "---\ndescription: new\n---\n\nUpdated").unwrap();
    let parsed = read("demo").unwrap();
    assert_eq!(parsed.description.as_deref(), Some("new"));
    assert!(parsed.content.ends_with("Updated"));

    unsafe {
        match previous_hub {
            Some(value) => std::env::set_var("SKILLSTAR_HUB_DIR", value),
            None => std::env::remove_var("SKILLSTAR_HUB_DIR"),
        }
    }
}

#[test]
fn content_facade_preserves_typed_not_found_error() {
    let _guard = crate::lock_test_env();
    let previous_hub = std::env::var_os("SKILLSTAR_HUB_DIR");
    let temp = tempfile::tempdir().unwrap();
    unsafe {
        std::env::set_var("SKILLSTAR_HUB_DIR", temp.path());
    }

    assert!(matches!(
        read_raw("missing"),
        Err(AppError::SkillNotFound { name }) if name == "missing"
    ));

    unsafe {
        match previous_hub {
            Some(value) => std::env::set_var("SKILLSTAR_HUB_DIR", value),
            None => std::env::remove_var("SKILLSTAR_HUB_DIR"),
        }
    }
}

#[test]
fn snapshot_is_deterministic_sorted_and_excludes_git_metadata() {
    let _guard = crate::lock_test_env();
    let hub = TestHub::new();
    let skill_dir = hub.skill_dir("demo");
    std::fs::create_dir_all(skill_dir.join("z-dir")).unwrap();
    std::fs::create_dir_all(skill_dir.join("a-dir")).unwrap();
    std::fs::create_dir_all(skill_dir.join(".git/objects")).unwrap();
    std::fs::write(skill_dir.join("z-dir/z.txt"), b"z").unwrap();
    std::fs::write(skill_dir.join("SKILL.md"), b"# Demo").unwrap();
    std::fs::write(skill_dir.join("a-dir/a.txt"), b"a").unwrap();
    std::fs::write(skill_dir.join(".git/config"), b"secret metadata").unwrap();

    let first = snapshot("demo").unwrap();
    let second = snapshot("demo").unwrap();
    let paths = first
        .files
        .iter()
        .map(|file| file.relative_path.as_str())
        .collect::<Vec<_>>();

    assert_eq!(paths, vec!["SKILL.md", "a-dir/a.txt", "z-dir/z.txt"]);
    assert_eq!(first, second);
    assert_eq!(first.total_bytes, 8);
    assert!(first.content_hash.starts_with("sha256:"));
}

#[test]
fn snapshot_hash_changes_when_a_nested_file_changes() {
    let _guard = crate::lock_test_env();
    let hub = TestHub::new();
    let skill_dir = hub.skill_dir("demo");
    std::fs::create_dir_all(skill_dir.join("references/nested")).unwrap();
    std::fs::write(skill_dir.join("SKILL.md"), b"# Demo").unwrap();
    let nested = skill_dir.join("references/nested/guide.md");
    std::fs::write(&nested, b"version one").unwrap();

    let before = snapshot("demo").unwrap();
    std::fs::write(&nested, b"version two").unwrap();
    let after = snapshot("demo").unwrap();

    assert_ne!(before.content_hash, after.content_hash);
}

#[cfg(unix)]
#[test]
fn snapshot_records_internal_symlinks_without_following_them() {
    use std::os::unix::fs::symlink;

    let _guard = crate::lock_test_env();
    let hub = TestHub::new();
    let skill_dir = hub.skill_dir("demo");
    let outside = hub.temp.path().join("outside");
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(skill_dir.join("SKILL.md"), b"# Demo").unwrap();
    std::fs::write(outside.join("secret.txt"), b"must not be read").unwrap();
    symlink(&outside, skill_dir.join("linked-dir")).unwrap();

    let captured = snapshot("demo").unwrap();
    let link = captured
        .files
        .iter()
        .find(|file| file.relative_path == "linked-dir")
        .unwrap();

    assert_eq!(link.kind, SnapshotFileKind::Symlink);
    assert_eq!(link.symlink_target(), outside.to_str());
    assert!(
        captured
            .files
            .iter()
            .all(|file| file.relative_path != "linked-dir/secret.txt")
    );
}

#[test]
fn snapshot_rejects_traversal_names() {
    let _guard = crate::lock_test_env();
    let _hub = TestHub::new();

    assert!(matches!(snapshot("../demo"), Err(AppError::Other(_))));
    assert!(matches!(snapshot("demo/other"), Err(AppError::Other(_))));
    assert!(matches!(snapshot("demo."), Err(AppError::Other(_))));
    assert!(matches!(snapshot("NUL.txt"), Err(AppError::Other(_))));
    assert!(matches!(snapshot("demo*copy"), Err(AppError::Other(_))));
}

#[test]
fn content_facades_reject_traversal_before_read_write_or_delete() {
    let _guard = crate::lock_test_env();
    let hub = TestHub::new();
    let outside = hub.temp.path().join("victim");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("SKILL.md"), "# Must stay\n").unwrap();

    assert!(matches!(read_raw("../victim"), Err(AppError::Other(_))));
    assert!(matches!(list_files("../victim"), Err(AppError::Other(_))));
    assert!(matches!(read("../victim"), Err(AppError::Other(_))));
    assert!(matches!(
        update("../victim", "destroyed"),
        Err(AppError::Other(_))
    ));
    assert!(matches!(delete_local("../victim"), Err(AppError::Other(_))));
    assert_eq!(
        std::fs::read_to_string(outside.join("SKILL.md")).unwrap(),
        "# Must stay\n"
    );
}

#[cfg(unix)]
#[test]
fn materialized_snapshot_checks_out_a_nested_repo_cache_skill() {
    use std::os::unix::fs::symlink;
    use std::process::Command;

    let _guard = crate::lock_test_env();
    let hub = TestHub::new();
    let repo = hub.temp.path().join("repos/demo-repo");
    std::fs::create_dir_all(repo.join("skills/demo")).unwrap();
    std::fs::write(repo.join("skills/demo/SKILL.md"), "# Demo\n").unwrap();

    let git = |args: &[&str]| {
        let status = Command::new("git")
            .args(args)
            .current_dir(&repo)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed");
    };
    git(&["init", "--quiet"]);
    git(&["config", "user.email", "tests@skillstar.local"]);
    git(&["config", "user.name", "SkillStar Tests"]);
    git(&["add", "."]);
    git(&["commit", "--quiet", "-m", "fixture"]);

    std::fs::remove_dir_all(repo.join("skills")).unwrap();
    std::fs::create_dir_all(hub.temp.path().join("skills")).unwrap();
    let canonical_repo = std::fs::canonicalize(&repo).unwrap();
    symlink(canonical_repo.join("skills/demo"), hub.skill_dir("demo")).unwrap();

    assert!(snapshot("demo").is_err());
    assert!(!repo.join("skills").exists());

    let captured = snapshot_materialized("demo").unwrap();
    assert!(repo.join("skills/demo/SKILL.md").exists());
    assert_eq!(captured.name, "demo");
    assert_eq!(captured.files[0].relative_path, "SKILL.md");
}

#[cfg(unix)]
#[test]
fn snapshot_rejects_a_hub_entry_that_resolves_outside_the_hub() {
    use std::os::unix::fs::symlink;

    let _guard = crate::lock_test_env();
    let hub = TestHub::new();
    let outside = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(hub.temp.path().join("skills")).unwrap();
    std::fs::write(outside.path().join("SKILL.md"), b"# Outside").unwrap();
    symlink(outside.path(), hub.skill_dir("escape")).unwrap();

    assert!(matches!(snapshot("escape"), Err(AppError::Other(_))));
}
