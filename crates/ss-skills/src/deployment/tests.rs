use super::*;
use crate::test_sandbox::Sandbox;
use std::fs;

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
    let sandbox = Sandbox::new();
    let home = sandbox.home();
    let missing = vec!["missing-skill".to_string()];

    let error = batch_link_skills_to_agent(&missing, "claude").unwrap_err();
    assert!(error.to_string().contains("is not enabled"));

    let hub_skill = ss_core::infra::paths::hub_skills_dir().join("demo-skill");
    fs::create_dir_all(&hub_skill)?;
    fs::write(hub_skill.join("SKILL.md"), "# demo\n")?;
    let error = toggle_skill_for_agent("demo-skill", "claude", true).unwrap_err();
    assert!(error.to_string().contains("is not enabled"));
    assert!(
        !home.join(".claude").exists(),
        "inactive single or batch requests must not provision the Agent config root"
    );

    assert!(crate::agents::toggle_profile("claude")?);
    invalidate_profile_cache();
    let linked = batch_link_skills_to_agent(&missing, "claude")?;
    assert_eq!(linked, 0);
    assert!(
        !home.join(".claude").exists(),
        "skipping missing skills must not create the agent config root"
    );
    Ok(())
}

#[test]
fn batch_global_deploy_honors_explicit_copy_mode() -> Result<()> {
    let sandbox = Sandbox::new();
    let home = sandbox.home();
    let hub_skill = ss_core::infra::paths::hub_skills_dir().join("demo-skill");
    fs::create_dir_all(&hub_skill)?;
    fs::write(hub_skill.join("SKILL.md"), "# original\n")?;

    let deployed = batch_deploy_skills_to_agents(
        &["demo-skill".to_string()],
        &["codex".to_string()],
        crate::projects::ProjectDeployMode::Copy,
    )?;
    assert_eq!(deployed, 1);

    let target = home.join(".agents/skills/demo-skill");
    assert!(target.join("SKILL.md").is_file());
    assert!(!ss_core::infra::fs_ops::is_link(&target));

    fs::write(hub_skill.join("SKILL.md"), "# changed\n")?;
    assert_eq!(fs::read_to_string(target.join("SKILL.md"))?, "# original\n");
    Ok(())
}

#[test]
fn batch_deploy_rewrites_a_stale_link_and_leaves_other_agents_pinned() -> Result<()> {
    let sandbox = Sandbox::new();
    let home = sandbox.home();
    // A SkillStar link left pointing at an older payload in local storage.
    let cursor_payload = make_skill_dir(&ss_core::infra::paths::local_skills_dir(), "rust");
    fs::write(cursor_payload.join("payload.txt"), "cursor rust")?;
    let dsh_payload = make_skill_dir(sandbox.root(), "dsh-payload");
    fs::write(dsh_payload.join("payload.txt"), "dsh rust")?;

    let hub = ss_core::infra::paths::hub_skills_dir();
    fs::create_dir_all(&hub)?;
    let hub_skill = hub.join("rust");
    ss_core::infra::fs_ops::create_symlink(&dsh_payload, &hub_skill)?;

    let cursor_target = home.join(".cursor/skills/rust");
    fs::create_dir_all(cursor_target.parent().unwrap())?;
    ss_core::infra::fs_ops::create_symlink(&cursor_payload, &cursor_target)?;
    let dsh_target = home.join(".dsh/skills/rust");
    fs::create_dir_all(dsh_target.parent().unwrap())?;
    ss_core::infra::fs_ops::create_symlink(&cursor_payload, &dsh_target)?;

    let deployed = batch_deploy_skills_to_agents(
        &["rust".to_string()],
        &["deepseek".to_string()],
        crate::projects::ProjectDeployMode::Symlink,
    )?;
    assert_eq!(deployed, 1, "stale dsh link must count as a new deploy");
    assert_eq!(
        fs::read_to_string(dsh_target.join("payload.txt"))?,
        "dsh rust"
    );
    assert_eq!(
        fs::read_to_string(cursor_target.join("payload.txt"))?,
        "cursor rust"
    );

    let again = batch_deploy_skills_to_agents(
        &["rust".to_string()],
        &["deepseek".to_string()],
        crate::projects::ProjectDeployMode::Symlink,
    )?;
    assert_eq!(again, 0, "correct payload must stay idempotent");
    Ok(())
}

#[test]
fn swap_refreshes_an_existing_symlink() {
    let tmp = tempfile::tempdir().unwrap();
    let skill = make_skill_dir(tmp.path(), "hub-skill");
    let agent_dir = tmp.path().join("agent");
    fs::create_dir_all(&agent_dir).unwrap();
    let target = agent_dir.join("hub-skill");
    ss_core::infra::fs_ops::create_symlink(&skill, &target).unwrap();

    let was_copy = batch::swap_in_fresh_deploy(&skill, &target, "hub-skill").unwrap();

    assert!(!was_copy);
    assert!(ss_core::infra::fs_ops::is_link(&target));
    assert!(target.join("SKILL.md").exists());
    let leftovers: Vec<_> = fs::read_dir(&agent_dir)
        .unwrap()
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with('.'))
        .collect();
    assert!(
        leftovers.is_empty(),
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

    batch::swap_in_fresh_deploy(&skill, &target, "hub-skill").unwrap();

    let refreshed = fs::read_to_string(
        ss_core::infra::fs_ops::read_link_resolved(&target)
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
    ss_core::infra::fs_ops::create_symlink(&skill, &target).unwrap();

    // Make the agent dir read-only so staging creation fails.
    fs::set_permissions(&agent_dir, fs::Permissions::from_mode(0o555)).unwrap();
    let result = batch::swap_in_fresh_deploy(&skill, &target, "hub-skill");
    fs::set_permissions(&agent_dir, fs::Permissions::from_mode(0o755)).unwrap();

    assert!(result.is_err());
    assert!(
        ss_core::infra::fs_ops::is_link(&target),
        "the pre-existing link must survive a failed resync"
    );
}

#[test]
fn toggle_skips_an_unmanaged_real_directory_without_overwriting_it() -> Result<()> {
    let sandbox = Sandbox::new();
    let home = sandbox.home();
    assert!(crate::agents::toggle_profile("claude")?);
    invalidate_profile_cache();

    let hub_skill = ss_core::infra::paths::hub_skills_dir().join("research");
    fs::create_dir_all(&hub_skill)?;
    fs::write(hub_skill.join("SKILL.md"), "# hub research\n")?;

    let occupied = home.join(".claude/skills/research");
    fs::create_dir_all(&occupied)?;
    fs::write(occupied.join("DESCRIPTION.md"), "agent-owned category\n")?;

    let outcome = toggle_skill_for_agent("research", "claude", true)?;
    match outcome {
        ToggleSkillOutcome::Skipped { code, path, reason } => {
            assert_eq!(code, SKIP_UNMANAGED_REAL_DIRECTORY);
            assert_eq!(
                Path::new(&path).components().collect::<Vec<_>>(),
                occupied.components().collect::<Vec<_>>(),
                "{path} vs {}",
                occupied.display()
            );
            assert!(reason.contains("not managed by SkillStar"));
        }
        ToggleSkillOutcome::Applied => panic!("must not replace an unmanaged directory"),
    }
    assert!(
        occupied.join("DESCRIPTION.md").is_file(),
        "the occupied directory must be left in place"
    );
    assert!(!ss_core::infra::fs_ops::is_link(&occupied));
    Ok(())
}

/// "Unlink all" sweeps a whole directory rather than a path the user named, so
/// it must leave anything SkillStar did not deploy in place — including a
/// user folder that happens to carry a `SKILL.md`.
#[test]
fn unlink_all_leaves_unmanaged_entries_in_place() -> Result<()> {
    let sandbox = Sandbox::new();
    let home = sandbox.home();
    make_skill_dir(&ss_core::infra::paths::hub_skills_dir(), "demo-skill");
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
    let own = make_skill_dir(&skills_dir, "my-own-skill");

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
    assert!(
        own.join("SKILL.md").is_file(),
        "a user's own Skill folder is not a SkillStar deployment"
    );
    Ok(())
}

#[test]
fn batch_deploy_dedups_agents_that_share_one_physical_directory() -> Result<()> {
    let sandbox = Sandbox::new();
    let real = sandbox.root().join("agent-skills");
    fs::create_dir_all(&real)?;
    let indirect = real.join("..").join("agent-skills");
    crate::agents::add_custom_profile(crate::agents::CustomProfileDef {
        id: "custom_real".into(),
        display_name: "Real".into(),
        global_skills_dir: real.display().to_string(),
        project_skills_rel: ".real/skills".into(),
        icon_data_uri: None,
    })?;
    crate::agents::add_custom_profile(crate::agents::CustomProfileDef {
        id: "custom_via".into(),
        display_name: "Via".into(),
        global_skills_dir: indirect.display().to_string(),
        project_skills_rel: ".via/skills".into(),
        icon_data_uri: None,
    })?;
    invalidate_profile_cache();

    let error = batch_deploy_skills_to_agents(
        &["..".to_string()],
        &["custom_real".to_string(), "custom_via".to_string()],
        crate::projects::ProjectDeployMode::Symlink,
    )
    .unwrap_err();
    let message = error.to_string();
    assert_eq!(
        message.matches("invalid skill name").count(),
        1,
        "one physical directory is deployed once: {message}"
    );
    assert!(message.contains("custom_real"), "{message}");
    assert!(!message.contains("custom_via"), "{message}");
    Ok(())
}
