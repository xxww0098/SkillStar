use super::*;

struct TestEnv {
    previous: Vec<(&'static str, Option<std::ffi::OsString>)>,
}

impl TestEnv {
    fn new(root: &Path) -> Self {
        let values = [
            ("SKILLSTAR_DATA_DIR", root.join("data")),
            ("SKILLSTAR_HUB_DIR", root.join("hub")),
            ("SKILLSTAR_TOOL_SYNC_HOME", root.join("tool-home")),
            ("HOME", root.join("home")),
            ("USERPROFILE", root.join("home")),
        ];
        let previous = values
            .iter()
            .map(|(key, _)| (*key, std::env::var_os(key)))
            .collect();
        for (key, value) in values {
            unsafe { std::env::set_var(key, value) };
        }
        Self { previous }
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        for (key, value) in self.previous.drain(..) {
            unsafe {
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }
}

fn write_pack(repo: &Path, skill_path: &str) {
    std::fs::create_dir_all(repo.join("skills/demo")).unwrap();
    std::fs::write(
        repo.join("skills/demo/SKILL.md"),
        "---\nname: demo\ndescription: A demo skill for pack tests\n---\n\n# Demo\n",
    )
    .unwrap();
    std::fs::write(
        repo.join("skillpack.toml"),
        format!(
            "name = \"demo-pack\"\nversion = \"1.0.0\"\n\n[[skills]]\nname = \"demo\"\npath = {skill_path:?}\n"
        ),
    )
    .unwrap();
}

/// On non-Windows the interpreter is always `sh`, regardless of the script
/// extension (matches the historical behaviour). On Windows the extension
/// drives the choice so a pack that ships `post_install.ps1` actually runs
/// instead of failing on a missing `sh`.
#[test]
fn post_install_interpreter_matches_platform_and_extension() {
    #[cfg(not(windows))]
    {
        for ext in ["ps1", "bat", "cmd", "sh", ""] {
            assert_eq!(
                post_install_interpreter(ext),
                PostInstallInterpreter::Sh,
                "extension {ext:?} should map to sh on non-windows"
            );
        }
    }

    #[cfg(windows)]
    {
        assert_eq!(
            post_install_interpreter("ps1"),
            PostInstallInterpreter::PowerShell
        );
        assert_eq!(post_install_interpreter("bat"), PostInstallInterpreter::Cmd);
        assert_eq!(post_install_interpreter("cmd"), PostInstallInterpreter::Cmd);
        // Extensionless / `.sh` falls back to sh so Git Bash can run it.
        assert_eq!(post_install_interpreter("sh"), PostInstallInterpreter::Sh);
        assert_eq!(post_install_interpreter(""), PostInstallInterpreter::Sh);
    }
}

/// The chosen interpreter must advertise a concrete program name so
/// `command_with_path` has something to launch.
#[test]
fn every_interpreter_has_a_program() {
    for interp in [
        PostInstallInterpreter::Sh,
        PostInstallInterpreter::PowerShell,
        PostInstallInterpreter::Cmd,
    ] {
        assert!(!interp.program().is_empty());
    }
}

#[test]
fn detect_pack_rejects_paths_that_escape_the_repo() {
    let temp = tempfile::tempdir().unwrap();
    write_pack(temp.path(), "../outside");

    let error = detect_pack(temp.path()).unwrap_err().to_string();
    assert!(error.contains("invalid path"), "{error}");
}

#[test]
fn detect_pack_rejects_invalid_skill_names() {
    let temp = tempfile::tempdir().unwrap();
    write_pack(temp.path(), "skills/demo");
    let manifest = std::fs::read_to_string(temp.path().join("skillpack.toml")).unwrap();
    std::fs::write(
        temp.path().join("skillpack.toml"),
        manifest.replace("name = \"demo\"", "name = \"../demo\""),
    )
    .unwrap();

    let error = detect_pack(temp.path()).unwrap_err().to_string();
    assert!(error.contains("invalid Skill name"), "{error}");
}

#[test]
fn install_fails_closed_when_the_pack_registry_is_corrupt() {
    let _guard = crate::lock_test_env();
    let temp = tempfile::tempdir().unwrap();
    let _env = TestEnv::new(temp.path());
    let repo = temp.path().join("repo");
    write_pack(&repo, "skills/demo");
    let packs_path = skillstar_core::infra::paths::packs_path();
    std::fs::create_dir_all(packs_path.parent().unwrap()).unwrap();
    std::fs::write(&packs_path, b"{broken registry").unwrap();

    let error = install_pack(
        &repo,
        "example/demo-pack",
        "https://github.com/example/demo-pack",
    )
    .unwrap_err()
    .to_string();

    assert!(error.contains("Failed to parse pack registry"), "{error}");
    assert_eq!(std::fs::read(&packs_path).unwrap(), b"{broken registry");
    assert!(
        skillstar_core::infra::paths::hub_skills_dir()
            .join("demo")
            .symlink_metadata()
            .is_err()
    );
}

#[cfg(unix)]
#[test]
fn registry_save_failure_restores_previous_link_and_lockfile() {
    let _guard = crate::lock_test_env();
    let temp = tempfile::tempdir().unwrap();
    let _env = TestEnv::new(temp.path());
    let repo = temp.path().join("repo");
    write_pack(&repo, "skills/demo");

    let hub = skillstar_core::infra::paths::hub_skills_dir();
    std::fs::create_dir_all(&hub).unwrap();
    let previous_source = temp.path().join("previous-demo");
    std::fs::create_dir_all(&previous_source).unwrap();
    std::fs::write(previous_source.join("SKILL.md"), "# Previous\n").unwrap();
    skillstar_core::infra::fs_ops::create_symlink(&previous_source, &hub.join("demo")).unwrap();

    let lock_path = crate::lockfile::lockfile_path();
    let mut previous_lock = crate::lockfile::Lockfile::default();
    previous_lock.upsert(crate::lockfile::LockEntry {
        name: "demo".into(),
        git_url: "https://github.com/example/previous".into(),
        git_ref: None,
        tree_hash: "previous-tree".into(),
        content_hash: Some("previous-content".into()),
        content_hash_version: Some(crate::content::SNAPSHOT_HASH_VERSION),
        installed_at: "2026-01-01T00:00:00Z".into(),
        source_folder: None,
    });
    previous_lock.save(&lock_path).unwrap();
    let previous_lock_bytes = std::fs::read(&lock_path).unwrap();

    // Make the registry directory read-only after seeding a valid store.
    // The strict load succeeds, then the atomic store write fails after
    // the lockfile write and exercises the compensation path.
    let packs_path = skillstar_core::infra::paths::packs_path();
    let packs_parent = packs_path.parent().unwrap();
    std::fs::create_dir_all(packs_parent).unwrap();
    let state_dir = skillstar_core::infra::paths::state_dir();
    std::fs::create_dir_all(&state_dir).unwrap();
    std::fs::write(state_dir.join("skill-update.lock"), b"").unwrap();
    std::fs::write(&packs_path, b"{\"version\":1,\"packs\":[]}").unwrap();
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(packs_parent, std::fs::Permissions::from_mode(0o555)).unwrap();

    let result = install_pack(
        &repo,
        "example/demo-pack",
        "https://github.com/example/demo-pack",
    );
    std::fs::set_permissions(packs_parent, std::fs::Permissions::from_mode(0o755)).unwrap();
    let error = result.unwrap_err().to_string();
    assert!(error.contains("installation was rolled back"), "{error}");
    assert_eq!(
        std::fs::canonicalize(
            skillstar_core::infra::fs_ops::read_link_resolved(&hub.join("demo")).unwrap(),
        )
        .unwrap(),
        std::fs::canonicalize(&previous_source).unwrap()
    );
    assert_eq!(std::fs::read(&lock_path).unwrap(), previous_lock_bytes);
    assert_eq!(
        std::fs::read_to_string(hub.join("demo/SKILL.md")).unwrap(),
        "# Previous\n"
    );
}
