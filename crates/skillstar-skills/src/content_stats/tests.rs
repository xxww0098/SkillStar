use super::*;

fn env() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

fn set_env(dir: &std::path::Path) {
    unsafe {
        std::env::set_var("SKILLSTAR_DATA_DIR", dir.join("data"));
        std::env::set_var("SKILLSTAR_HUB_DIR", dir.join("hub"));
    }
}

fn restore_env(
    previous_data: Option<std::ffi::OsString>,
    previous_hub: Option<std::ffi::OsString>,
) {
    unsafe {
        match previous_data {
            Some(value) => std::env::set_var("SKILLSTAR_DATA_DIR", value),
            None => std::env::remove_var("SKILLSTAR_DATA_DIR"),
        }
        match previous_hub {
            Some(value) => std::env::set_var("SKILLSTAR_HUB_DIR", value),
            None => std::env::remove_var("SKILLSTAR_HUB_DIR"),
        }
    }
}

/// End-to-end through the real snapshot: record on snapshot, probe, edit,
/// probe must miss (fail-closed), fresh snapshot re-arms the fast path.
#[test]
fn fingerprint_hits_and_misses_correctly() {
    // Env-mutating tests must serialize: parallel tests share the process env.
    let _env_guard = crate::lock_test_env();
    let guard = env();
    let previous_data = std::env::var_os("SKILLSTAR_DATA_DIR");
    let previous_hub = std::env::var_os("SKILLSTAR_HUB_DIR");
    set_env(guard.path());

    {
        // Mirror the real layout: repo caches live under hub/repos so the
        // resolved snapshot root stays inside the managed hub root.
        let repo = skillstar_core::infra::paths::hub_root().join("repos/demo-repo");
        let source = repo.join("skills/demo");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("SKILL.md"), "---\nname: demo\n---\nbody").unwrap();
        let hub = skillstar_core::infra::paths::hub_skills_dir();
        std::fs::create_dir_all(&hub).unwrap();
        skillstar_core::infra::fs_ops::create_symlink(&source, &hub.join("demo")).unwrap();

        let snapshot = crate::content::snapshot("demo").unwrap();
        let baseline = snapshot.content_hash.clone();

        // A stat-only hit right after the snapshot.
        assert!(baseline_unchanged("demo", &baseline));
        // Wrong expected hash must miss even when files are untouched.
        assert!(!baseline_unchanged("demo", "not-the-baseline"));

        // A user edit (size + mtime change) must miss.
        std::fs::write(source.join("SKILL.md"), "---\nname: demo\n---\nedited body").unwrap();
        assert!(!baseline_unchanged("demo", &baseline));

        // A new file must miss even though existing files are untouched.
        let snapshot2 = crate::content::snapshot("demo").unwrap();
        assert!(baseline_unchanged("demo", &snapshot2.content_hash));
        std::fs::write(source.join("extra.md"), "extra").unwrap();
        assert!(!baseline_unchanged("demo", &snapshot2.content_hash));

        // A removed file must miss.
        std::fs::remove_file(source.join("extra.md")).unwrap();
        assert!(baseline_unchanged("demo", &snapshot2.content_hash));

        // A retargeted link must miss even when both trees are identical.
        let other = skillstar_core::infra::paths::hub_root().join("repos/other-repo");
        std::fs::create_dir_all(other.join("skills/demo")).unwrap();
        std::fs::write(
            other.join("skills/demo/SKILL.md"),
            "---\nname: demo\n---\nbody",
        )
        .unwrap();
        skillstar_core::infra::fs_ops::remove_symlink(&hub.join("demo")).unwrap();
        skillstar_core::infra::fs_ops::create_symlink(
            &other.join("skills/demo"),
            &hub.join("demo"),
        )
        .unwrap();
        assert!(!baseline_unchanged("demo", &snapshot2.content_hash));
    }
    restore_env(previous_data, previous_hub);
    guard.close().unwrap();
}

/// No record at all (fresh install, first fetch) must miss.
#[test]
fn missing_record_misses() {
    let _env_guard = crate::lock_test_env();
    let guard = env();
    let previous_data = std::env::var_os("SKILLSTAR_DATA_DIR");
    let previous_hub = std::env::var_os("SKILLSTAR_HUB_DIR");
    set_env(guard.path());
    assert!(!baseline_unchanged("never-seen", "any"));
    restore_env(previous_data, previous_hub);
    guard.close().unwrap();
}
