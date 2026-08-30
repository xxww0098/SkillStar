use super::*;
use std::ffi::OsStr;
use std::fs;

fn set_env<K: AsRef<OsStr>, V: AsRef<OsStr>>(key: K, value: V) {
    unsafe { std::env::set_var(key, value) }
}

fn remove_env<K: AsRef<OsStr>>(key: K) {
    unsafe { std::env::remove_var(key) }
}

fn make_skill_dir(root: &Path, name: &str) -> std::path::PathBuf {
    let dir = root.join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("SKILL.md"), "# test skill\n").unwrap();
    dir
}

#[test]
fn project_only_agent_is_rejected_by_global_deployment_guard() {
    let profiles = vec![agent_profile::AgentProfile {
        id: "eve".to_string(),
        display_name: "Eve".to_string(),
        icon: "lobe:eve".to_string(),
        global_skills_dir: std::path::PathBuf::new(),
        project_skills_rel: "agent/skills".to_string(),
        installed: true,
        enabled: true,
        synced_count: 0,
    }];

    let error = require_global_profile(&profiles, "eve").unwrap_err();
    assert!(error.to_string().contains("does not support global skills"));
}

#[test]
fn batch_link_requires_enabled_agent_and_skips_missing_skills_without_creating_agent_dir()
-> Result<()> {
    let _guard = crate::lock_test_env();
    invalidate_profile_cache();

    let tmp = tempfile::tempdir()?;
    let home = tmp.path().join("home");
    fs::create_dir_all(&home)?;

    let previous_home = std::env::var_os("HOME");
    let previous_data_dir = std::env::var_os("SKILLSTAR_DATA_DIR");
    let previous_claude_config_dir = std::env::var_os("CLAUDE_CONFIG_DIR");
    set_env("HOME", &home);
    set_env("SKILLSTAR_DATA_DIR", home.join(".skillstar"));
    remove_env("CLAUDE_CONFIG_DIR");
    #[cfg(windows)]
    let previous_userprofile = std::env::var_os("USERPROFILE");
    #[cfg(windows)]
    set_env("USERPROFILE", &home);

    let result = (|| -> Result<()> {
        let missing = vec!["missing-skill".to_string()];

        let error = batch_link_skills_to_agent(&missing, "claude").unwrap_err();
        assert!(error.to_string().contains("is not enabled"));

        let hub_skill = skillstar_core::infra::paths::hub_skills_dir().join("demo-skill");
        fs::create_dir_all(&hub_skill)?;
        fs::write(hub_skill.join("SKILL.md"), "# demo\n")?;
        let error = toggle_skill_for_agent("demo-skill", "claude", true).unwrap_err();
        assert!(error.to_string().contains("is not enabled"));
        assert!(
            !home.join(".claude").exists(),
            "inactive single or batch requests must not provision the Agent config root"
        );

        assert!(skillstar_agents::toggle_profile("claude")?);
        invalidate_profile_cache();
        let linked = batch_link_skills_to_agent(&missing, "claude")?;
        assert_eq!(linked, 0);
        assert!(
            !home.join(".claude").exists(),
            "skipping missing skills must not create the agent config root"
        );
        Ok(())
    })();

    match previous_home {
        Some(value) => set_env("HOME", value),
        None => remove_env("HOME"),
    }
    match previous_data_dir {
        Some(value) => set_env("SKILLSTAR_DATA_DIR", value),
        None => remove_env("SKILLSTAR_DATA_DIR"),
    }
    match previous_claude_config_dir {
        Some(value) => set_env("CLAUDE_CONFIG_DIR", value),
        None => remove_env("CLAUDE_CONFIG_DIR"),
    }
    #[cfg(windows)]
    match previous_userprofile {
        Some(value) => set_env("USERPROFILE", value),
        None => remove_env("USERPROFILE"),
    }
    invalidate_profile_cache();

    result
}

#[test]
fn batch_global_deploy_honors_explicit_copy_mode() -> Result<()> {
    let _guard = crate::lock_test_env();
    invalidate_profile_cache();

    let tmp = tempfile::tempdir()?;
    let home = tmp.path().join("home");
    fs::create_dir_all(&home)?;

    let previous_home = std::env::var_os("HOME");
    let previous_data_dir = std::env::var_os("SKILLSTAR_DATA_DIR");
    // The codex profile resolves its skills dir from CODEX_HOME first, so a
    // CODEX_HOME leaking in from the ambient environment would redirect the
    // deploy away from `home/.codex`. Sandbox it like HOME.
    let previous_codex_home = std::env::var_os("CODEX_HOME");
    set_env("HOME", &home);
    set_env("SKILLSTAR_DATA_DIR", home.join(".skillstar"));
    remove_env("CODEX_HOME");
    #[cfg(windows)]
    let previous_userprofile = std::env::var_os("USERPROFILE");
    #[cfg(windows)]
    set_env("USERPROFILE", &home);

    let result = (|| -> Result<()> {
        invalidate_profile_cache();
        let hub_skill = skillstar_core::infra::paths::hub_skills_dir().join("demo-skill");
        fs::create_dir_all(&hub_skill)?;
        fs::write(hub_skill.join("SKILL.md"), "# original\n")?;

        let deployed = batch_deploy_skills_to_agents(
            &["demo-skill".to_string()],
            &["codex".to_string()],
            crate::projects::ProjectDeployMode::Copy,
        )?;
        assert_eq!(deployed, 1);

        let target = home.join(".codex/skills/demo-skill");
        assert!(target.join("SKILL.md").is_file());
        assert!(!skillstar_core::infra::fs_ops::is_link(&target));

        fs::write(hub_skill.join("SKILL.md"), "# changed\n")?;
        assert_eq!(fs::read_to_string(target.join("SKILL.md"))?, "# original\n");
        Ok(())
    })();

    match previous_home {
        Some(value) => set_env("HOME", value),
        None => remove_env("HOME"),
    }
    match previous_data_dir {
        Some(value) => set_env("SKILLSTAR_DATA_DIR", value),
        None => remove_env("SKILLSTAR_DATA_DIR"),
    }
    match previous_codex_home {
        Some(value) => set_env("CODEX_HOME", value),
        None => remove_env("CODEX_HOME"),
    }
    #[cfg(windows)]
    match previous_userprofile {
        Some(value) => set_env("USERPROFILE", value),
        None => remove_env("USERPROFILE"),
    }
    invalidate_profile_cache();

    result
}

#[test]
fn swap_refreshes_an_existing_symlink() {
    let tmp = tempfile::tempdir().unwrap();
    let skill = make_skill_dir(tmp.path(), "hub-skill");
    let agent_dir = tmp.path().join("agent");
    fs::create_dir_all(&agent_dir).unwrap();
    let target = agent_dir.join("hub-skill");
    skillstar_core::infra::fs_ops::create_symlink(&skill, &target).unwrap();

    let was_copy = swap_in_fresh_deploy(&skill, &target).unwrap();

    assert!(!was_copy);
    assert!(skillstar_core::infra::fs_ops::is_link(&target));
    assert!(target.join("SKILL.md").exists());
    assert!(
        resync_staging_path(&target).symlink_metadata().is_err(),
        "staging entry must not be left behind"
    );
}

#[test]
fn swap_refreshes_a_stale_copy_deployment() {
    let tmp = tempfile::tempdir().unwrap();
    let skill = make_skill_dir(tmp.path(), "hub-skill");
    fs::write(skill.join("SKILL.md"), "# fresh content\n").unwrap();

    let agent_dir = tmp.path().join("agent");
    fs::create_dir_all(&agent_dir).unwrap();
    let target = agent_dir.join("hub-skill");
    // Simulate an old copy deployment with stale content.
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join("SKILL.md"), "# stale content\n").unwrap();

    swap_in_fresh_deploy(&skill, &target).unwrap();

    let refreshed = fs::read_to_string(
        skillstar_core::infra::fs_ops::read_link_resolved(&target)
            .map(|p| p.join("SKILL.md"))
            .unwrap_or_else(|_| target.join("SKILL.md")),
    )
    .unwrap();
    assert!(refreshed.contains("fresh content"));
}

#[cfg(unix)]
#[test]
fn swap_keeps_old_link_when_staging_fails() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempfile::tempdir().unwrap();
    let skill = make_skill_dir(tmp.path(), "hub-skill");
    let agent_dir = tmp.path().join("agent");
    fs::create_dir_all(&agent_dir).unwrap();
    let target = agent_dir.join("hub-skill");
    skillstar_core::infra::fs_ops::create_symlink(&skill, &target).unwrap();

    // Make the agent dir read-only so staging creation fails.
    fs::set_permissions(&agent_dir, fs::Permissions::from_mode(0o555)).unwrap();
    let result = swap_in_fresh_deploy(&skill, &target);
    fs::set_permissions(&agent_dir, fs::Permissions::from_mode(0o755)).unwrap();

    assert!(result.is_err());
    assert!(
        skillstar_core::infra::fs_ops::is_link(&target),
        "the pre-existing link must survive a failed resync"
    );
}

#[test]
fn toggle_skips_an_unmanaged_real_directory_without_overwriting_it() -> Result<()> {
    let _guard = crate::lock_test_env();
    invalidate_profile_cache();

    let tmp = tempfile::tempdir()?;
    let home = tmp.path().join("home");
    fs::create_dir_all(&home)?;

    let previous_home = std::env::var_os("HOME");
    let previous_data_dir = std::env::var_os("SKILLSTAR_DATA_DIR");
    let previous_claude_config_dir = std::env::var_os("CLAUDE_CONFIG_DIR");
    set_env("HOME", &home);
    set_env("SKILLSTAR_DATA_DIR", home.join(".skillstar"));
    remove_env("CLAUDE_CONFIG_DIR");
    #[cfg(windows)]
    let previous_userprofile = std::env::var_os("USERPROFILE");
    #[cfg(windows)]
    set_env("USERPROFILE", &home);

    let result = (|| -> Result<()> {
        assert!(skillstar_agents::toggle_profile("claude")?);
        invalidate_profile_cache();

        let hub_skill = skillstar_core::infra::paths::hub_skills_dir().join("research");
        fs::create_dir_all(&hub_skill)?;
        fs::write(hub_skill.join("SKILL.md"), "# hub research\n")?;

        let occupied = home.join(".claude/skills/research");
        fs::create_dir_all(&occupied)?;
        fs::write(occupied.join("DESCRIPTION.md"), "agent-owned category\n")?;

        let outcome = toggle_skill_for_agent("research", "claude", true)?;
        match outcome {
            ToggleSkillOutcome::Skipped { code, path, reason } => {
                assert_eq!(code, SKIP_UNMANAGED_REAL_DIRECTORY);
                assert_eq!(path, occupied.display().to_string());
                assert!(reason.contains("unmanaged real directory"));
            }
            ToggleSkillOutcome::Applied => panic!("must not replace an unmanaged directory"),
        }
        assert!(
            occupied.join("DESCRIPTION.md").is_file(),
            "the occupied directory must be left in place"
        );
        assert!(!skillstar_core::infra::fs_ops::is_link(&occupied));
        Ok(())
    })();

    match previous_home {
        Some(value) => set_env("HOME", value),
        None => remove_env("HOME"),
    }
    match previous_data_dir {
        Some(value) => set_env("SKILLSTAR_DATA_DIR", value),
        None => remove_env("SKILLSTAR_DATA_DIR"),
    }
    match previous_claude_config_dir {
        Some(value) => set_env("CLAUDE_CONFIG_DIR", value),
        None => remove_env("CLAUDE_CONFIG_DIR"),
    }
    #[cfg(windows)]
    match previous_userprofile {
        Some(value) => set_env("USERPROFILE", value),
        None => remove_env("USERPROFILE"),
    }
    invalidate_profile_cache();

    result
}

/// "Unlink all" sweeps a whole directory rather than a path the user named, so
/// it must leave anything SkillStar did not deploy in place. That currently
/// holds because `fs_ops::remove_link_or_copy` refuses to delete a directory
/// without `SKILL.md`; this locks the end-to-end behaviour so the sweep cannot
/// grow its own deletion path and quietly lose the guarantee.
#[test]
fn unlink_all_leaves_unmanaged_entries_in_place() -> Result<()> {
    let _guard = crate::lock_test_env();
    invalidate_profile_cache();

    let tmp = tempfile::tempdir()?;
    let home = tmp.path().join("home");
    fs::create_dir_all(&home)?;

    let previous_home = std::env::var_os("HOME");
    let previous_data_dir = std::env::var_os("SKILLSTAR_DATA_DIR");
    let previous_claude_config_dir = std::env::var_os("CLAUDE_CONFIG_DIR");
    set_env("HOME", &home);
    set_env("SKILLSTAR_DATA_DIR", home.join(".skillstar"));
    remove_env("CLAUDE_CONFIG_DIR");
    #[cfg(windows)]
    let previous_userprofile = std::env::var_os("USERPROFILE");
    #[cfg(windows)]
    set_env("USERPROFILE", &home);

    let result = (|| -> Result<()> {
        invalidate_profile_cache();
        make_skill_dir(
            &skillstar_core::infra::paths::hub_skills_dir(),
            "demo-skill",
        );
        let deployed = batch_deploy_skills_to_agents(
            &["demo-skill".to_string()],
            &["claude".to_string()],
            crate::projects::ProjectDeployMode::Symlink,
        )?;
        assert_eq!(deployed, 1);

        // Things SkillStar did not put there: a loose file, and a real
        // directory that is not a skill (no SKILL.md).
        let skills_dir = home.join(".claude/skills");
        fs::write(skills_dir.join("notes.md"), "user notes\n")?;
        let scratch = skills_dir.join("scratch");
        fs::create_dir_all(&scratch)?;
        fs::write(scratch.join("todo.txt"), "not a skill\n")?;

        let removed = unlink_all_skills_from_agent("claude")?;
        assert_eq!(removed, 1, "only the managed deployment should be removed");
        assert!(
            skills_dir.join("demo-skill").symlink_metadata().is_err(),
            "expected the managed deployment to be gone"
        );
        assert!(
            skills_dir.join("notes.md").is_file(),
            "expected an unmanaged file to survive unlink-all"
        );
        assert!(
            scratch.join("todo.txt").is_file(),
            "expected an unmanaged directory to survive unlink-all"
        );
        Ok(())
    })();

    match previous_home {
        Some(value) => set_env("HOME", value),
        None => remove_env("HOME"),
    }
    match previous_data_dir {
        Some(value) => set_env("SKILLSTAR_DATA_DIR", value),
        None => remove_env("SKILLSTAR_DATA_DIR"),
    }
    match previous_claude_config_dir {
        Some(value) => set_env("CLAUDE_CONFIG_DIR", value),
        None => remove_env("CLAUDE_CONFIG_DIR"),
    }
    #[cfg(windows)]
    match previous_userprofile {
        Some(value) => set_env("USERPROFILE", value),
        None => remove_env("USERPROFILE"),
    }
    invalidate_profile_cache();

    result
}
