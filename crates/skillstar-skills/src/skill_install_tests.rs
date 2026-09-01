use super::{derive_name_hint, find_target_skill, requested_skill_not_found_error};
use crate::repo_scanner::DiscoveredSkill;

fn discovered(id: &str) -> DiscoveredSkill {
    DiscoveredSkill {
        id: id.to_string(),
        folder_path: format!("skills/{id}"),
        description: String::new(),
        already_installed: false,
        frontmatter_issues: Vec::new(),
    }
}

#[test]
fn derive_name_hint_prefers_explicit_name() {
    let hint = derive_name_hint(
        "https://github.com/example/skills.git",
        Some("explicit-name"),
    );
    assert_eq!(hint, "explicit-name");
}

#[test]
fn derive_name_hint_falls_back_to_repo_tail() {
    let hint = derive_name_hint("https://github.com/example/awesome-skill.git", None);
    assert_eq!(hint, "awesome-skill");
}

#[test]
fn find_target_skill_prefers_requested_name_case_insensitive() {
    let skills = vec![discovered("frontend-ui"), discovered("security-review")];
    let target = find_target_skill(&skills, Some("FRONTEND-UI"), "unused-name-hint");
    assert_eq!(target.map(|skill| skill.id.as_str()), Some("frontend-ui"));
}

#[test]
fn find_target_skill_uses_single_skill_fallback() {
    let skills = vec![discovered("only-one")];
    let target = find_target_skill(&skills, None, "no-match-hint");
    assert_eq!(target.map(|skill| skill.id.as_str()), Some("only-one"));
}

#[test]
fn find_target_skill_rejects_different_single_skill_when_name_is_explicit() {
    let skills = vec![discovered("renamed-skill")];
    let target = find_target_skill(&skills, Some("removed-skill"), "removed-skill");
    assert!(target.is_none());
}

#[test]
fn requested_skill_not_found_error_names_missing_identity_and_possible_cause() {
    let error = requested_skill_not_found_error(&["removed-skill".to_string()]);
    assert!(error.contains("removed-skill"));
    assert!(error.contains("deleted or renamed"));
}

#[cfg(test)]
mod pipeline_local_source_tests {
    use super::super::install_skill;
    use std::ffi::OsString;
    use std::process::Command;

    struct Sandbox {
        previous: Vec<(&'static str, Option<OsString>)>,
        _temp: tempfile::TempDir,
        _guard: std::sync::MutexGuard<'static, ()>,
    }

    impl Sandbox {
        fn new() -> Self {
            let _guard = crate::lock_test_env();
            let temp = tempfile::tempdir().unwrap();
            let overrides = [
                ("SKILLSTAR_HUB_DIR", temp.path().join("hub")),
                ("SKILLSTAR_DATA_DIR", temp.path().join("data")),
                ("SKILLSTAR_TOOL_SYNC_HOME", temp.path().join("tool-home")),
                ("HOME", temp.path().join("home")),
                ("USERPROFILE", temp.path().join("home")),
            ];
            let previous = overrides
                .iter()
                .map(|(key, _)| (*key, std::env::var_os(key)))
                .collect();
            unsafe {
                for (key, value) in overrides {
                    std::env::set_var(key, value);
                }
            }
            Self {
                previous,
                _temp: temp,
                _guard,
            }
        }
    }

    impl Drop for Sandbox {
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

    fn init_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for args in [
            vec!["init", "--initial-branch=main"],
            vec!["config", "user.email", "test@example.com"],
            vec!["config", "user.name", "SkillStar Tests"],
        ] {
            let status = Command::new("git")
                .current_dir(dir.path())
                .args(&args)
                .status()
                .unwrap();
            assert!(status.success());
        }
        dir
    }

    /// Local path is step 1 of the same pipeline. An invalid root SKILL.md
    /// is still rejected and must not leave a hub entry.
    #[test]
    fn pipeline_rejects_invalid_root_skill_from_a_local_path() {
        let _sandbox = Sandbox::new();
        let repo = init_repo();
        std::fs::write(repo.path().join("SKILL.md"), "# No frontmatter\n").unwrap();
        let status = Command::new("git")
            .current_dir(repo.path())
            .args(["add", "."])
            .status()
            .unwrap();
        assert!(status.success());
        let status = Command::new("git")
            .current_dir(repo.path())
            .args(["commit", "-m", "init"])
            .status()
            .unwrap();
        assert!(status.success());

        let error = install_skill(
            repo.path().to_string_lossy().to_string(),
            Some("demo".into()),
        )
        .unwrap_err();
        assert!(
            error.contains("invalid") || error.contains("not installable"),
            "{error}"
        );
        assert!(error.contains("description"), "{error}");
        let hub = skillstar_core::infra::paths::hub_skills_dir();
        assert!(
            !hub.join("demo").symlink_metadata().is_ok(),
            "rejected clone must be cleaned up"
        );
    }

    #[test]
    fn pipeline_installs_a_valid_root_skill_from_a_local_path() {
        let _sandbox = Sandbox::new();
        let repo = init_repo();
        std::fs::write(
            repo.path().join("SKILL.md"),
            "---\nname: demo\ndescription: A valid root skill\n---\n\n# Demo\n",
        )
        .unwrap();
        let status = Command::new("git")
            .current_dir(repo.path())
            .args(["add", "."])
            .status()
            .unwrap();
        assert!(status.success());
        let status = Command::new("git")
            .current_dir(repo.path())
            .args(["commit", "-m", "init"])
            .status()
            .unwrap();
        assert!(status.success());

        let skill = install_skill(
            repo.path().to_string_lossy().to_string(),
            Some("demo".into()),
        )
        .expect("valid root skill installs through the same pipeline");
        assert_eq!(skill.name, "demo");
        let hub = skillstar_core::infra::paths::hub_skills_dir().join("demo");
        assert!(hub.join("SKILL.md").is_file());
    }

    #[test]
    fn pipeline_installs_a_harness_folder_from_a_local_pack_not_the_repo_root() {
        let _sandbox = Sandbox::new();
        let repo = init_repo();
        std::fs::write(
            repo.path().join("SKILL.md"),
            "---\nname: rust\ndescription: shim at pack root\n---\n\n# rust\n",
        )
        .unwrap();
        let cursor = repo.path().join(".cursor/skills/rust");
        std::fs::create_dir_all(&cursor).unwrap();
        std::fs::write(
            cursor.join("SKILL.md"),
            "---\nname: rust\ndescription: cursor copy\n---\n\n# rust\n",
        )
        .unwrap();
        let status = Command::new("git")
            .current_dir(repo.path())
            .args(["add", "."])
            .status()
            .unwrap();
        assert!(status.success());
        let status = Command::new("git")
            .current_dir(repo.path())
            .args(["commit", "-m", "init"])
            .status()
            .unwrap();
        assert!(status.success());

        let skill = install_skill(
            repo.path().to_string_lossy().to_string(),
            Some("rust".into()),
        )
        .expect("a harness pack must install through the same pipeline");
        assert_eq!(skill.name, "rust");
        let lock = crate::lockfile::Lockfile::load(&crate::lockfile::lockfile_path()).unwrap();
        let entry = lock
            .skills
            .iter()
            .find(|entry| entry.name == "rust")
            .expect("lock entry");
        assert_eq!(
            entry.source_folder.as_deref(),
            Some(".cursor/skills/rust"),
            "must hub-link the harness folder, not the repo root"
        );
        let hub = skillstar_core::infra::paths::hub_skills_dir().join("rust");
        assert!(hub.join("SKILL.md").is_file());
        assert!(!hub.join(".cursor").exists());
    }
}
