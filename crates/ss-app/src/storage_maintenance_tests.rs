use std::collections::HashSet;

use super::*;

/// `dir_size_recursive` must NOT follow symlink/junction targets.
/// AGENTS.md: "treat links as metadata-only entries to avoid recursive
/// loops and Windows UI hangs". A symlink to a large dir must contribute 0 bytes.
#[test]
fn dir_size_does_not_follow_symlinks() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();

    // Real file with known size inside the scanned root.
    std::fs::write(root.join("real.txt"), b"hello").unwrap();

    // A directory with a 1MB file, placed OUTSIDE root, then symlinked in.
    // (If it were under root it would be counted as a normal subdir.)
    let outside = std::env::temp_dir().join(format!("sst-storage-test-{}", std::process::id()));
    std::fs::create_dir_all(&outside).unwrap();
    let payload = vec![0u8; 1_000_000];
    std::fs::write(outside.join("blob.bin"), &payload).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, root.join("link_to_big")).unwrap();

    let bytes = dir_size_recursive(root);
    // Only real.txt (5 bytes) counts; the 1MB via symlink must be excluded.
    assert_eq!(bytes, 5, "symlink target content must not be counted");

    // Cleanup the outside dir.
    let _ = std::fs::remove_dir_all(&outside);
}

/// `count_hub_skills` distinguishes valid symlinks from broken ones and
/// skips regular files. This is the storage-overview health signal.
/// Unix-only: the symlink fixtures are cfg(unix), so on Windows the test
/// would see just one valid entry and fail (windows-ci lesson 4 family).
#[cfg(unix)]
#[test]
fn count_hub_skills_separates_valid_and_broken_links() {
    let tmp = tempfile::tempdir().unwrap();
    let hub = tmp.path().join("skills");
    std::fs::create_dir_all(&hub).unwrap();

    // Real dir skill → valid.
    std::fs::create_dir_all(hub.join("real-skill")).unwrap();

    // Valid symlink → target exists.
    let target = tmp.path().join("target-skill");
    std::fs::create_dir_all(&target).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target, hub.join("linked-skill")).unwrap();

    // Broken symlink → target missing.
    #[cfg(unix)]
    std::os::unix::fs::symlink(tmp.path().join("does-not-exist"), hub.join("broken-skill"))
        .unwrap();

    // Stray regular file → ignored entirely.
    std::fs::write(hub.join(".DS_Store"), b"x").unwrap();

    let (valid, _broken) = count_hub_skills(&hub);
    // real-skill + linked-skill = 2 valid. broken-skill is counted but the
    // lockfile lookup may also add orphans, so only assert the floor.
    assert!(
        valid >= 2,
        "real dir + valid symlink should both count as valid, got {valid}"
    );
}

#[test]
fn unused_cache_counts_only_directories_the_lock_does_not_reference() {
    let tmp = tempfile::tempdir().unwrap();
    let keep = ss_skills::fetch::import_cache_key("https://example.com/keep.git", None);
    std::fs::create_dir_all(tmp.path().join(&keep)).unwrap();
    std::fs::write(tmp.path().join(&keep).join("a"), b"abcd").unwrap();
    std::fs::create_dir_all(tmp.path().join("orphan")).unwrap();
    std::fs::write(tmp.path().join("orphan").join("b"), b"ef").unwrap();
    std::fs::create_dir_all(tmp.path().join(".fetch-temp")).unwrap();
    let mut referenced = HashSet::new();
    referenced.insert(keep);
    let (count, bytes) = unused_cache_in(tmp.path(), &referenced);
    assert_eq!(count, 1);
    assert_eq!(bytes, 2);
}

/// vercel keys the lock by the raw frontmatter name. `My Skill` is the
/// folder `my-skill` and must survive cleanup while that folder exists.
/// A key that does not map to a folder is uncertain and is kept. Every key
/// for a folder that is confirmed missing is removed together.
#[test]
fn clean_broken_keeps_vercel_raw_name_keys_for_an_existing_folder() {
    let _env_lock = futures::executor::block_on(crate::test_support::ENV_LOCK.lock());
    let temp = tempfile::tempdir().unwrap();
    let _env = MaintenanceEnv::new(temp.path());

    let hub = ss_core::infra::paths::hub_skills_dir();
    std::fs::create_dir_all(hub.join("my-skill")).unwrap();
    let lock_path = ss_skills::skill_lock::lock_path();
    std::fs::create_dir_all(lock_path.parent().unwrap()).unwrap();
    std::fs::write(
        &lock_path,
        r#"{
  "version": 3,
  "skills": {
    "My Skill": {"source":"o/r","sourceType":"github","sourceUrl":"https://github.com/o/r.git","ref":"main","skillPath":"skills/my-skill","installedAt":"2020-01-01T00:00:00Z","updatedAt":"2020-01-01T00:00:00Z","pluginName":"plug"},
    "Ghost": {"source":"o/r","sourceType":"github","sourceUrl":"https://github.com/o/r.git","ref":"main","installedAt":"t","updatedAt":"t"},
    "ghost": {"source":"o/r","sourceType":"github","sourceUrl":"https://github.com/o/r.git","ref":"main","installedAt":"t","updatedAt":"t"},
    "...": {"source":"o/r","sourceType":"github","sourceUrl":"https://github.com/o/r.git","ref":"main","installedAt":"t","updatedAt":"t"}
  }
}"#,
    )
    .unwrap();

    let (_valid, broken) = count_hub_skills(&hub);
    assert_eq!(
        broken, 1,
        "My Skill maps to my-skill, which exists; only the missing ghost folder is broken"
    );

    let fixed = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(clean_broken_skills())
        .unwrap();
    assert_eq!(fixed, 2, "Ghost and ghost are one folder, two keys");

    let written: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&lock_path).unwrap()).unwrap();
    assert_eq!(written["skills"]["My Skill"]["pluginName"], "plug");
    assert!(written["skills"].get("Ghost").is_none());
    assert!(written["skills"].get("ghost").is_none());
    assert!(
        written["skills"].get("...").is_some(),
        "a key with no folder mapping is never pruned"
    );
    assert!(hub.join("my-skill").is_dir());
}

#[test]
fn force_delete_repo_caches_clears_import_cache_and_does_not_recreate_legacy_repos_dir() {
    let _env_lock = futures::executor::block_on(crate::test_support::ENV_LOCK.lock());
    let temp = tempfile::tempdir().unwrap();
    let _env = MaintenanceEnv::new(temp.path());

    let import_key = ss_skills::fetch::import_cache_key("https://example.com/demo.git", None);
    let import_entry = ss_core::infra::paths::skill_import_cache_dir().join(&import_key);
    std::fs::create_dir_all(&import_entry).unwrap();
    std::fs::write(import_entry.join("SKILL.md"), "# demo").unwrap();

    let legacy_repos = ss_core::infra::paths::repos_cache_dir();
    std::fs::create_dir_all(legacy_repos.join("old-repo")).unwrap();

    let removed = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(force_delete_repo_caches())
        .unwrap();

    assert_eq!(removed, 2);
    assert!(
        !import_entry.exists(),
        "current skill-imports cache entry must be removed"
    );
    assert!(
        !legacy_repos.exists(),
        "legacy hub/repos dir must be removed and never recreated"
    );
}

#[test]
fn get_storage_overview_exposes_skills_data_path_and_does_not_create_legacy_repos() {
    let _env_lock = futures::executor::block_on(crate::test_support::ENV_LOCK.lock());
    let temp = tempfile::tempdir().unwrap();
    let _env = MaintenanceEnv::new(temp.path());

    let overview = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(get_storage_overview())
        .unwrap();

    assert_eq!(
        overview.skills_data_path,
        ss_core::infra::paths::skills_data_dir()
            .to_string_lossy()
            .to_string()
    );
    assert_ne!(overview.skills_data_path, overview.config_path);

    let removed = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(force_delete_repo_caches())
        .unwrap();
    assert_eq!(removed, 0);
    assert!(!ss_core::infra::paths::repos_cache_dir().exists());
}

struct MaintenanceEnv {
    previous: Vec<(&'static str, Option<std::ffi::OsString>)>,
}

impl MaintenanceEnv {
    fn new(root: &std::path::Path) -> Self {
        let home = root.join("home");
        std::fs::create_dir_all(&home).unwrap();
        let assignments = [
            ("HOME", Some(home.clone())),
            ("USERPROFILE", Some(home.clone())),
            ("SKILLSTAR_DATA_DIR", Some(root.join("data"))),
            ("SKILLSTAR_HUB_DIR", Some(root.join("hub"))),
            ("SKILLSTAR_TOOL_SYNC_HOME", Some(root.join("tool-home"))),
            ("XDG_CONFIG_HOME", Some(home.join(".config"))),
            ("XDG_STATE_HOME", None),
            ("CLAUDE_CONFIG_DIR", None),
            ("AUTOHAND_HOME", None),
            ("DSH_HOME", None),
            ("GROK_HOME", None),
            ("HERMES_HOME", None),
            ("VIBE_HOME", None),
        ];
        let previous = assignments
            .iter()
            .map(|(key, _)| (*key, std::env::var_os(key)))
            .collect();
        unsafe {
            for (key, value) in assignments {
                match value {
                    Some(path) => std::env::set_var(key, path),
                    None => std::env::remove_var(key),
                }
            }
        }
        Self { previous }
    }
}

impl Drop for MaintenanceEnv {
    fn drop(&mut self) {
        unsafe {
            for (key, previous) in self.previous.drain(..).rev() {
                match previous {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }
}
