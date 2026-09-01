use super::*;

#[test]
fn baseline_failure_after_pull_rolls_back_to_the_previous_revision() {
    let _guard = crate::lock_test_env();
    let _hub = TestHub::new();
    let remote = committed_remote();

    crate::skill_install::install_skill(
        remote.path().to_string_lossy().into_owned(),
        Some("demo".to_string()),
    )
    .unwrap();
    let installed = skillstar_core::infra::paths::hub_skills_dir().join("demo");
    std::fs::create_dir_all(remote.path().join("assets")).unwrap();
    std::fs::write(
        remote.path().join("assets/oversized.bin"),
        vec![0; content::SnapshotLimits::default().max_file_bytes as usize + 1],
    )
    .unwrap();
    run_git(remote.path(), &["add", "."]);
    run_git(remote.path(), &["commit", "-m", "oversized"]);

    let report = update_skills(&["demo".to_string()]);

    assert!(report.updated.is_empty());
    assert_eq!(report.failed.len(), 1);
    assert!(!installed.join("assets/oversized.bin").exists());
    assert_eq!(
        std::fs::read_to_string(installed.join("scripts/run.sh")).unwrap(),
        "echo remote-v1\n"
    );
}

#[cfg(unix)]
#[test]
fn deployment_failure_is_reported_without_destroying_the_old_copy() {
    use std::os::unix::fs::PermissionsExt as _;

    let _guard = crate::lock_test_env();
    let _hub = TestHub::new();
    let remote = committed_remote();
    crate::skill_install::install_skill(
        remote.path().to_string_lossy().into_owned(),
        Some("demo".to_string()),
    )
    .unwrap();

    let agent_dir = _hub._temp.path().join("agent-skills");
    let deployed = agent_dir.join("demo");
    std::fs::create_dir_all(deployed.join("scripts")).unwrap();
    std::fs::write(
        deployed.join("SKILL.md"),
        "---\ndescription: deployed v1\n---\n",
    )
    .unwrap();
    std::fs::write(deployed.join("scripts/run.sh"), "echo deployed-v1\n").unwrap();
    skillstar_agents::add_custom_profile(skillstar_agents::CustomProfileDef {
        id: "update-failure-agent".to_string(),
        display_name: "Update Failure Agent".to_string(),
        global_skills_dir: agent_dir.to_string_lossy().into_owned(),
        project_skills_rel: String::new(),
        icon_data_uri: None,
    })
    .unwrap();
    assert!(skillstar_agents::toggle_profile("update-failure-agent").unwrap());
    crate::deployment::invalidate_profile_cache();

    std::fs::write(remote.path().join("scripts/run.sh"), "echo remote-v2\n").unwrap();
    run_git(remote.path(), &["add", "."]);
    run_git(remote.path(), &["commit", "-m", "v2"]);

    std::fs::set_permissions(&agent_dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    let report = update_skills(&["demo".to_string()]);
    std::fs::set_permissions(&agent_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    crate::deployment::invalidate_profile_cache();

    assert_eq!(report.updated.len(), 1);
    assert!(
        report.updated[0]
            .agent_link_failures
            .iter()
            .any(|failure| failure.contains("Update Failure Agent"))
    );
    assert_eq!(
        std::fs::read_to_string(deployed.join("scripts/run.sh")).unwrap(),
        "echo deployed-v1\n",
        "a failed staged resync must preserve the previous copy"
    );
}

#[test]
fn blocked_update_suggests_a_non_destructive_local_copy_name() {
    let _guard = crate::lock_test_env();
    let _hub = TestHub::new();
    let remote = committed_remote();

    crate::skill_install::install_skill(
        remote.path().to_string_lossy().into_owned(),
        Some("demo".to_string()),
    )
    .unwrap();
    crate::local_skill::create("demo.local", None).unwrap();
    let installed = skillstar_core::infra::paths::hub_skills_dir().join("demo");
    std::fs::write(installed.join("scripts/run.sh"), "echo local-change\n").unwrap();

    let report = update_skills(&["demo".to_string()]);

    assert_eq!(report.blocked[0].suggested_local_name, "demo.local.2");
    assert!(crate::local_skill::is_local_skill("demo.local"));
}

#[test]
fn editable_local_copy_name_cannot_escape_the_managed_local_root() {
    let _guard = crate::lock_test_env();
    let _hub = TestHub::new();
    let remote = committed_remote();

    crate::skill_install::install_skill(
        remote.path().to_string_lossy().into_owned(),
        Some("demo".to_string()),
    )
    .unwrap();
    let installed = skillstar_core::infra::paths::hub_skills_dir().join("demo");
    std::fs::write(installed.join("scripts/run.sh"), "echo local-change\n").unwrap();

    let error = resolve_skill_update(
        "demo",
        LocalDivergenceResolution::Preserve {
            local_name: "../escape".to_string(),
        },
    )
    .unwrap_err();

    assert!(error.to_string().contains("Invalid local Skill name"));
    assert_eq!(
        std::fs::read_to_string(installed.join("scripts/run.sh")).unwrap(),
        "echo local-change\n"
    );
}

#[test]
fn divergence_in_a_repo_sibling_blocks_the_shared_checkout_before_reset() {
    let _guard = crate::lock_test_env();
    let _hub = TestHub::new();
    let remote = committed_multi_skill_remote();
    let repos = skillstar_core::infra::paths::repos_cache_dir();
    std::fs::create_dir_all(&repos).unwrap();
    let cache = repos.join("multi");
    run_git(
        &repos,
        &[
            "clone",
            remote.path().to_str().unwrap(),
            cache.to_str().unwrap(),
        ],
    );
    let targets = ["alpha", "beta"].map(|name| crate::repo_scanner::SkillInstallTarget {
        id: name.to_string(),
        folder_path: format!("skills/{name}"),
    });
    crate::repo_scanner::install_from_repo_at(
        &cache,
        &remote.path().to_string_lossy(),
        None,
        &targets,
    )
    .unwrap();

    let skills = skillstar_core::infra::paths::hub_skills_dir();
    std::fs::write(
        skills.join("alpha/SKILL.md"),
        "---\nname: alpha\ndescription: locally edited\n---\n",
    )
    .unwrap();
    std::fs::write(
        remote.path().join("skills/beta/SKILL.md"),
        "---\nname: beta\ndescription: beta v2\n---\n",
    )
    .unwrap();
    run_git(remote.path(), &["add", "."]);
    run_git(remote.path(), &["commit", "-m", "v2"]);

    let report = update_skills(&["beta".to_string()]);

    assert!(report.updated.is_empty());
    assert_eq!(
        report
            .blocked
            .iter()
            .map(|blocked| blocked.name.as_str())
            .collect::<Vec<_>>(),
        vec!["alpha"]
    );
    assert!(
        std::fs::read_to_string(skills.join("beta/SKILL.md"))
            .unwrap()
            .contains("beta v1"),
        "a clean sibling cannot move when that would overwrite a divergent shared checkout"
    );
}

#[test]
fn shared_checkout_waits_until_every_divergent_skill_is_resolved() {
    let _guard = crate::lock_test_env();
    let _hub = TestHub::new();
    let remote = committed_multi_skill_remote();
    let repos = skillstar_core::infra::paths::repos_cache_dir();
    std::fs::create_dir_all(&repos).unwrap();
    let cache = repos.join("multi");
    run_git(
        &repos,
        &[
            "clone",
            remote.path().to_str().unwrap(),
            cache.to_str().unwrap(),
        ],
    );
    let targets = ["alpha", "beta"].map(|name| crate::repo_scanner::SkillInstallTarget {
        id: name.to_string(),
        folder_path: format!("skills/{name}"),
    });
    crate::repo_scanner::install_from_repo_at(
        &cache,
        &remote.path().to_string_lossy(),
        None,
        &targets,
    )
    .unwrap();

    let skills = skillstar_core::infra::paths::hub_skills_dir();
    for name in ["alpha", "beta"] {
        std::fs::write(
            skills.join(name).join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {name} local\n---\n"),
        )
        .unwrap();
    }
    std::fs::write(
        remote.path().join("skills/beta/SKILL.md"),
        "---\nname: beta\ndescription: beta v2\n---\n",
    )
    .unwrap();
    run_git(remote.path(), &["add", "."]);
    run_git(remote.path(), &["commit", "-m", "v2"]);

    let report = update_skills(&["beta".to_string()]);
    assert_eq!(report.blocked.len(), 2);

    let first = resolve_skill_update(
        "alpha",
        LocalDivergenceResolution::Preserve {
            local_name: "alpha.local".to_string(),
        },
    )
    .unwrap();
    assert!(first.update.is_none());
    assert_eq!(
        first
            .remaining_blocked
            .iter()
            .map(|blocked| blocked.name.as_str())
            .collect::<Vec<_>>(),
        vec!["beta"]
    );
    assert!(
        std::fs::read_to_string(skills.join("beta/SKILL.md"))
            .unwrap()
            .contains("beta local")
    );

    let second = resolve_skill_update("beta", LocalDivergenceResolution::Discard).unwrap();
    assert!(second.update.is_some());
    assert!(second.remaining_blocked.is_empty());
    assert!(
        std::fs::read_to_string(skills.join("beta/SKILL.md"))
            .unwrap()
            .contains("beta v2")
    );
}

#[test]
fn checkout_root_skill_cannot_discard_an_unresolved_nested_skill() {
    let _guard = crate::lock_test_env();
    let _hub = TestHub::new();
    let remote = committed_root_and_nested_remote();
    let repos = skillstar_core::infra::paths::repos_cache_dir();
    std::fs::create_dir_all(&repos).unwrap();
    let cache = repos.join("root-and-nested");
    run_git(
        &repos,
        &[
            "clone",
            remote.path().to_str().unwrap(),
            cache.to_str().unwrap(),
        ],
    );
    let targets = [
        crate::repo_scanner::SkillInstallTarget {
            id: "root".to_string(),
            folder_path: String::new(),
        },
        crate::repo_scanner::SkillInstallTarget {
            id: "beta".to_string(),
            folder_path: "skills/beta".to_string(),
        },
    ];
    crate::repo_scanner::install_from_repo_at(
        &cache,
        &remote.path().to_string_lossy(),
        None,
        &targets,
    )
    .unwrap();

    let skills = skillstar_core::infra::paths::hub_skills_dir();
    std::fs::write(
        skills.join("root/SKILL.md"),
        "---\nname: root\ndescription: root local\n---\n",
    )
    .unwrap();
    std::fs::write(
        skills.join("beta/SKILL.md"),
        "---\nname: beta\ndescription: beta local\n---\n",
    )
    .unwrap();

    let report = update_skills(&["root".to_string()]);
    assert_eq!(
        report
            .blocked
            .iter()
            .map(|blocked| blocked.name.as_str())
            .collect::<Vec<_>>(),
        vec!["beta", "root"],
        "nested Skills must be resolved before a checkout-root Skill"
    );

    let error = resolve_skill_update("root", LocalDivergenceResolution::Discard).unwrap_err();
    assert!(error.to_string().contains("Resolve nested Skill changes"));
    assert!(
        std::fs::read_to_string(skills.join("beta/SKILL.md"))
            .unwrap()
            .contains("beta local")
    );

    let nested = resolve_skill_update("beta", LocalDivergenceResolution::Discard).unwrap();
    assert!(nested.update.is_none());
    assert_eq!(nested.remaining_blocked[0].name, "root");
    let root = resolve_skill_update("root", LocalDivergenceResolution::Discard).unwrap();
    assert!(root.update.is_some());
    assert!(root.remaining_blocked.is_empty());
}

#[test]
fn stale_missing_source_folder_cannot_discard_the_whole_shared_checkout() {
    let _guard = crate::lock_test_env();
    let _hub = TestHub::new();
    let remote = committed_multi_skill_remote();
    let repos = skillstar_core::infra::paths::repos_cache_dir();
    std::fs::create_dir_all(&repos).unwrap();
    let cache = repos.join("stale-source-folder");
    run_git(
        &repos,
        &[
            "clone",
            remote.path().to_str().unwrap(),
            cache.to_str().unwrap(),
        ],
    );
    let targets = [
        crate::repo_scanner::SkillInstallTarget {
            id: "alpha".to_string(),
            folder_path: "skills/alpha".to_string(),
        },
        crate::repo_scanner::SkillInstallTarget {
            id: "beta".to_string(),
            folder_path: "skills/beta".to_string(),
        },
    ];
    crate::repo_scanner::install_from_repo_at(
        &cache,
        &remote.path().to_string_lossy(),
        None,
        &targets,
    )
    .unwrap();

    let lock_path = lockfile::lockfile_path();
    let mut lockfile = lockfile::Lockfile::load(&lock_path).unwrap();
    lockfile
        .skills
        .iter_mut()
        .find(|entry| entry.name == "beta")
        .unwrap()
        .source_folder = None;
    lockfile.save(&lock_path).unwrap();

    let skills = skillstar_core::infra::paths::hub_skills_dir();
    std::fs::write(
        skills.join("alpha/SKILL.md"),
        "---\nname: alpha\ndescription: alpha local\n---\n",
    )
    .unwrap();
    std::fs::write(
        skills.join("beta/SKILL.md"),
        "---\nname: beta\ndescription: beta local\n---\n",
    )
    .unwrap();

    let result = resolve_skill_update("beta", LocalDivergenceResolution::Discard).unwrap();
    assert!(result.update.is_none());
    assert!(
        result
            .remaining_blocked
            .iter()
            .any(|item| item.name == "alpha")
    );
    assert!(
        std::fs::read_to_string(skills.join("alpha/SKILL.md"))
            .unwrap()
            .contains("alpha local"),
        "the physical nested path, not stale lock metadata, scopes discard"
    );
}

#[cfg(unix)]
#[test]
fn physical_checkout_comparison_accepts_a_symlinked_path_prefix() {
    let physical = tempfile::tempdir().unwrap();
    let checkout = physical.path().join("checkout");
    std::fs::create_dir_all(&checkout).unwrap();
    let alias_parent = tempfile::tempdir().unwrap();
    let alias = alias_parent.path().join("checkout-alias");
    std::os::unix::fs::symlink(&checkout, &alias).unwrap();

    assert!(same_physical_path(&checkout, &alias));
    assert_eq!(
        skillstar_core::infra::fs_ops::canonicalize_existing_prefix(&alias.join("missing/nested")),
        std::fs::canonicalize(&checkout)
            .unwrap()
            .join("missing/nested")
    );
}
