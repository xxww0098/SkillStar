use super::find_target_skill;
use crate::repo_scanner::DiscoveredSkill;

fn discovered(id: &str) -> DiscoveredSkill {
    DiscoveredSkill {
        id: id.to_string(),
        folder_path: format!("skills/{id}"),
        description: String::new(),
        already_installed: false,
        installable: true,
        frontmatter_issues: Vec::new(),
    }
}

#[test]
fn find_target_skill_prefers_requested_name_case_insensitive() {
    let skills = vec![discovered("frontend-ui"), discovered("security-review")];
    let target = find_target_skill(&skills, Some("FRONTEND-UI"), "unused-name-hint");
    assert_eq!(target.map(|skill| skill.id.as_str()), Ok("frontend-ui"));
}

#[test]
fn find_target_skill_uses_single_skill_fallback() {
    let skills = vec![discovered("only-one")];
    let target = find_target_skill(&skills, None, "no-match-hint");
    assert_eq!(target.map(|skill| skill.id.as_str()), Ok("only-one"));
}

#[test]
fn find_target_skill_rejects_different_single_skill_when_name_is_explicit() {
    let skills = vec![discovered("renamed-skill")];
    let target = find_target_skill(&skills, Some("removed-skill"), "removed-skill");
    assert!(target.is_err());
    let reason = target.unwrap_err();
    assert!(reason.contains("removed-skill"), "{reason}");
    assert!(reason.contains("renamed-skill"), "{reason}");
}

#[test]
fn find_target_skill_rejects_uninstallable_skill_with_reason() {
    let mut blocked = discovered("blocked-skill");
    blocked.installable = false;
    blocked.frontmatter_issues = vec!["missing_description".to_string()];
    let skills = vec![blocked];
    let target = find_target_skill(&skills, Some("blocked-skill"), "blocked-skill");
    let reason = target.unwrap_err();
    assert!(reason.contains("cannot be installed"), "{reason}");
    assert!(reason.contains("missing_description"), "{reason}");
}

#[cfg(test)]
mod pipeline_local_source_tests {
    use super::super::install_skill;
    use std::ffi::OsString;

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

    fn canonical(name: &str) -> std::path::PathBuf {
        ss_core::infra::paths::agents_skill_dir(name)
    }

    /// Local path is step 1 of the same pipeline (borrowed in place, D-081).
    /// An invalid root SKILL.md is still rejected and must leave nothing behind.
    #[test]
    fn pipeline_rejects_invalid_root_skill_from_a_local_path() {
        let _sandbox = Sandbox::new();
        let repo = tempfile::tempdir().unwrap();
        std::fs::write(repo.path().join("SKILL.md"), "# No frontmatter\n").unwrap();

        // The invalid root SKILL.md never surfaces as an installable skill,
        // so the requested identity fails closed ("not found") and nothing
        // lands in canonical. The frontmatter gate itself is pinned by the
        // installer tests.
        let error = install_skill(
            repo.path().to_string_lossy().to_string(),
            Some("demo".into()),
        )
        .unwrap_err();
        assert!(error.contains("demo"), "{error}");
        assert!(
            !canonical("demo").symlink_metadata().is_ok(),
            "rejected install must leave nothing behind"
        );
    }

    #[test]
    fn pipeline_installs_a_valid_root_skill_from_a_local_path() {
        let _sandbox = Sandbox::new();
        let repo = tempfile::tempdir().unwrap();
        std::fs::write(
            repo.path().join("SKILL.md"),
            "---\nname: demo\ndescription: A valid root skill\n---\n\n# Demo\n",
        )
        .unwrap();

        let skill = install_skill(
            repo.path().to_string_lossy().to_string(),
            Some("demo".into()),
        )
        .expect("valid root skill installs through the same pipeline");
        assert_eq!(skill.name, "demo");
        assert!(canonical("demo").join("SKILL.md").is_file());

        let lock = crate::skill_lock::load();
        assert_eq!(lock.skills["demo"].skill_path, None, "root skill");
        assert_eq!(
            lock.skills["demo"].source_type,
            crate::skill_lock::SourceType::Local
        );
    }

    /// vercel parity: the repo-root SKILL.md IS the install unit in
    /// root-first mode; harness copies no longer win (D-075 table removed).
    #[test]
    fn pipeline_installs_the_root_skill_of_a_local_pack() {
        let _sandbox = Sandbox::new();
        let repo = tempfile::tempdir().unwrap();
        std::fs::write(
            repo.path().join("SKILL.md"),
            "---\nname: rust\ndescription: pack root\n---\n\n# rust\n",
        )
        .unwrap();
        let cursor = repo.path().join(".cursor/skills/rust");
        std::fs::create_dir_all(&cursor).unwrap();
        std::fs::write(
            cursor.join("SKILL.md"),
            "---\nname: rust\ndescription: cursor copy\n---\n\n# rust\n",
        )
        .unwrap();

        let skill = install_skill(
            repo.path().to_string_lossy().to_string(),
            Some("rust".into()),
        )
        .expect("a pack installs through the same pipeline");
        assert_eq!(skill.name, "rust");
        let lock = crate::skill_lock::load();
        assert_eq!(
            lock.skills["rust"].skill_path, None,
            "root-first: the repo root is the install unit"
        );
        let content = std::fs::read_to_string(canonical("rust").join("SKILL.md")).unwrap();
        assert!(content.contains("pack root"), "{content}");
    }
}
