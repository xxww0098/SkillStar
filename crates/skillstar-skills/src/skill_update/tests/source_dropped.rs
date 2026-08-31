//! What happens when the source stops shipping an installed Skill.
//!
//! These cover the state that has no clean upstream to go back to: the update
//! must stop *before* the content is gone, and the exits it offers must be the
//! ones that actually work.

use super::*;

/// Install `alpha` and `beta` from one shared checkout and return its cache path.
fn install_shared_checkout(remote: &Path) -> std::path::PathBuf {
    let repos = skillstar_core::infra::paths::repos_cache_dir();
    std::fs::create_dir_all(&repos).unwrap();
    let cache = repos.join("multi");
    run_git(
        &repos,
        &["clone", remote.to_str().unwrap(), cache.to_str().unwrap()],
    );
    let targets = ["alpha", "beta"].map(|name| crate::repo_scanner::SkillInstallTarget {
        id: name.to_string(),
        folder_path: format!("skills/{name}"),
    });
    crate::repo_scanner::install_from_repo_at(&cache, &remote.to_string_lossy(), None, &targets)
        .unwrap();
    cache
}

/// Drop `skills/alpha` upstream and move `beta` forward, so an update has both
/// a removal to report and a real change to apply once it is resolved.
fn drop_alpha_upstream(remote: &Path) {
    std::fs::remove_dir_all(remote.join("skills/alpha")).unwrap();
    std::fs::write(
        remote.join("skills/beta/SKILL.md"),
        "---\nname: beta\ndescription: beta v2\n---\n",
    )
    .unwrap();
    run_git(remote, &["add", "-A"]);
    run_git(remote, &["commit", "-m", "drop alpha"]);
}

#[test]
fn a_dropped_skill_stops_the_update_with_its_content_still_on_disk() {
    let _guard = crate::lock_test_env();
    let _hub = TestHub::new();
    let remote = committed_multi_skill_remote();
    install_shared_checkout(remote.path());
    drop_alpha_upstream(remote.path());

    let report = update_skills(&["beta".to_string()]);

    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert!(report.updated.is_empty());
    assert_eq!(
        report
            .blocked
            .iter()
            .map(|blocked| (blocked.name.as_str(), blocked.reason))
            .collect::<Vec<_>>(),
        vec![("alpha", LocalDivergenceReason::SourceRemoved)]
    );

    let skills = skillstar_core::infra::paths::hub_skills_dir();
    assert!(
        std::fs::read_to_string(skills.join("alpha/SKILL.md"))
            .unwrap()
            .contains("alpha v1"),
        "the rollback must put the dropped Skill's content back so it can still be kept"
    );
    assert!(
        std::fs::read_to_string(skills.join("beta/SKILL.md"))
            .unwrap()
            .contains("beta v1"),
        "the rest of the checkout cannot move while a removal is unanswered"
    );
}

#[test]
fn removing_a_dropped_skill_lets_its_checkout_finish_updating() {
    let _guard = crate::lock_test_env();
    let _hub = TestHub::new();
    let remote = committed_multi_skill_remote();
    install_shared_checkout(remote.path());
    drop_alpha_upstream(remote.path());
    assert_eq!(update_skills(&["beta".to_string()]).blocked.len(), 1);

    let resolved = resolve_skill_update("alpha", LocalDivergenceResolution::Uninstall).unwrap();

    assert_eq!(resolved.uninstalled, vec!["alpha".to_string()]);
    assert!(resolved.local_copy.is_none());
    assert!(resolved.remaining_blocked.is_empty());
    assert_eq!(
        resolved
            .update
            .expect("beta must pull once alpha is gone")
            .skill
            .name,
        "beta"
    );

    let skills = skillstar_core::infra::paths::hub_skills_dir();
    assert!(
        skills.join("alpha").symlink_metadata().is_err(),
        "the hub link must not survive as a dangling entry"
    );
    let lockfile = crate::lockfile::Lockfile::load(&crate::lockfile::lockfile_path()).unwrap();
    assert!(!lockfile.skills.iter().any(|entry| entry.name == "alpha"));
    assert!(
        std::fs::read_to_string(skills.join("beta/SKILL.md"))
            .unwrap()
            .contains("beta v2")
    );
}

#[test]
fn keeping_a_dropped_skill_turns_it_into_an_independent_local_copy() {
    let _guard = crate::lock_test_env();
    let _hub = TestHub::new();
    let remote = committed_multi_skill_remote();
    install_shared_checkout(remote.path());
    drop_alpha_upstream(remote.path());
    assert_eq!(update_skills(&["beta".to_string()]).blocked.len(), 1);

    let resolved = resolve_skill_update(
        "alpha",
        LocalDivergenceResolution::Preserve {
            local_name: "alpha.local".to_string(),
        },
    )
    .unwrap();

    assert_eq!(
        resolved
            .local_copy
            .as_ref()
            .map(|skill| skill.name.as_str()),
        Some("alpha.local")
    );
    assert_eq!(resolved.uninstalled, vec!["alpha".to_string()]);
    assert_eq!(
        resolved.update.expect("beta must still pull").skill.name,
        "beta"
    );

    let skills = skillstar_core::infra::paths::hub_skills_dir();
    assert!(
        std::fs::read_to_string(skills.join("alpha.local/SKILL.md"))
            .unwrap()
            .contains("alpha v1"),
        "the copy must carry the content the source dropped"
    );
    assert!(skills.join("alpha").symlink_metadata().is_err());
}

#[test]
fn a_dropped_skill_cannot_be_discarded_into_existence() {
    let _guard = crate::lock_test_env();
    let _hub = TestHub::new();
    let remote = committed_multi_skill_remote();
    install_shared_checkout(remote.path());
    drop_alpha_upstream(remote.path());
    assert_eq!(update_skills(&["beta".to_string()]).blocked.len(), 1);

    let error = resolve_skill_update("alpha", LocalDivergenceResolution::Discard)
        .expect_err("there is no upstream state left to restore");

    assert!(
        format!("{error:#}").contains("dropped by its source"),
        "{error:#}"
    );
    let skills = skillstar_core::infra::paths::hub_skills_dir();
    assert!(
        skills.join("alpha/SKILL.md").exists(),
        "a rejected resolution must not remove anything"
    );
}

#[test]
fn a_skill_the_checkout_has_not_materialized_is_not_a_user_decision() {
    let _guard = crate::lock_test_env();
    let _hub = TestHub::new();
    let remote = committed_multi_skill_remote();
    let cache = install_shared_checkout(remote.path());
    std::fs::write(
        remote.path().join("skills/beta/SKILL.md"),
        "---\nname: beta\ndescription: beta v2\n---\n",
    )
    .unwrap();
    run_git(remote.path(), &["add", "-A"]);
    run_git(remote.path(), &["commit", "-m", "beta v2"]);
    // The source still ships alpha; only the working tree lacks it, exactly as
    // a sparse checkout that has not materialized the directory.
    std::fs::remove_dir_all(cache.join("skills/alpha")).unwrap();

    let report = update_skills(&["beta".to_string()]);

    assert!(report.blocked.is_empty(), "{:?}", report.blocked);
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    let skills = skillstar_core::infra::paths::hub_skills_dir();
    assert!(
        std::fs::read_to_string(skills.join("alpha/SKILL.md"))
            .unwrap()
            .contains("alpha v1"),
        "the pull must simply put the unmaterialized Skill back"
    );
    assert!(
        std::fs::read_to_string(skills.join("beta/SKILL.md"))
            .unwrap()
            .contains("beta v2")
    );
}

#[test]
fn a_dropped_skill_whose_content_is_already_gone_can_only_be_removed() {
    let _guard = crate::lock_test_env();
    let _hub = TestHub::new();
    let remote = committed_multi_skill_remote();
    let cache = install_shared_checkout(remote.path());
    std::fs::remove_dir_all(remote.path().join("skills/alpha")).unwrap();
    run_git(remote.path(), &["add", "-A"]);
    run_git(remote.path(), &["commit", "-m", "drop alpha"]);
    // The damage an older update left behind: the checkout is already past the
    // removal, so the hub link dangles and no revision can bring it back.
    run_git(&cache, &["fetch", "origin"]);
    run_git(&cache, &["reset", "--hard", "origin/HEAD"]);

    let report = update_skills(&["beta".to_string()]);

    assert_eq!(
        report
            .blocked
            .iter()
            .map(|blocked| (blocked.name.as_str(), blocked.reason))
            .collect::<Vec<_>>(),
        vec![("alpha", LocalDivergenceReason::SourceMissing)]
    );

    let error = resolve_skill_update(
        "alpha",
        LocalDivergenceResolution::Preserve {
            local_name: "alpha.local".to_string(),
        },
    )
    .expect_err("there is no content left to copy");
    assert!(format!("{error:#}").contains("already gone"), "{error:#}");

    let resolved = resolve_skill_update("alpha", LocalDivergenceResolution::Uninstall).unwrap();
    assert_eq!(resolved.uninstalled, vec!["alpha".to_string()]);
    assert!(
        resolved.update.is_some(),
        "removing the dead entry must let its checkout update again"
    );
    let skills = skillstar_core::infra::paths::hub_skills_dir();
    assert!(skills.join("alpha").symlink_metadata().is_err());
    let lockfile = crate::lockfile::Lockfile::load(&crate::lockfile::lockfile_path()).unwrap();
    assert!(!lockfile.skills.iter().any(|entry| entry.name == "alpha"));
}

#[test]
fn adding_a_duplicate_provider_path_does_not_look_like_source_removal() {
    let _guard = crate::lock_test_env();
    let _hub = TestHub::new();
    let remote = tempfile::tempdir().unwrap();
    run_git(remote.path(), &["init", "--initial-branch=main"]);
    run_git(remote.path(), &["config", "user.email", "test@example.com"]);
    run_git(remote.path(), &["config", "user.name", "SkillStar Tests"]);
    pin_lf_repo(remote.path());

    let original = remote.path().join(".agents/skills/impeccable");
    std::fs::create_dir_all(&original).unwrap();
    std::fs::write(
        original.join("SKILL.md"),
        "---\nname: impeccable\ndescription: impeccable v1\n---\n",
    )
    .unwrap();
    run_git(remote.path(), &["add", "."]);
    run_git(remote.path(), &["commit", "-m", "v1"]);

    let repos = skillstar_core::infra::paths::repos_cache_dir();
    std::fs::create_dir_all(&repos).unwrap();
    let cache = repos.join("impeccable");
    run_git(
        &repos,
        &[
            "clone",
            remote.path().to_str().unwrap(),
            cache.to_str().unwrap(),
        ],
    );
    run_git(&cache, &["sparse-checkout", "init", "--cone"]);
    run_git(
        &cache,
        &["sparse-checkout", "set", ".agents/skills/impeccable"],
    );
    crate::repo_scanner::install_from_repo_at(
        &cache,
        &remote.path().to_string_lossy(),
        None,
        &[crate::repo_scanner::SkillInstallTarget {
            id: "impeccable".to_string(),
            folder_path: ".agents/skills/impeccable".to_string(),
        }],
    )
    .unwrap();

    std::fs::write(
        original.join("SKILL.md"),
        "---\nname: impeccable\ndescription: impeccable v2\n---\n",
    )
    .unwrap();
    let duplicate = remote.path().join(".agent/skills/impeccable");
    std::fs::create_dir_all(&duplicate).unwrap();
    std::fs::write(
        duplicate.join("SKILL.md"),
        "---\nname: impeccable\ndescription: generated provider copy\n---\n",
    )
    .unwrap();
    run_git(remote.path(), &["add", "."]);
    run_git(remote.path(), &["commit", "-m", "add provider copy"]);

    let report = update_skills(&["impeccable".to_string()]);
    if !report.blocked.is_empty() {
        let error = resolve_skill_update("impeccable", LocalDivergenceResolution::Uninstall)
            .expect_err("the false source-removal dialog currently has no working exit");
        panic!(
            "the tracked source still contains the installed path, but update reported {:?} and removal failed: {error:#}",
            report.blocked
        );
    }

    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert_eq!(report.updated.len(), 1);
    assert!(
        std::fs::read_to_string(
            skillstar_core::infra::paths::hub_skills_dir().join("impeccable/SKILL.md")
        )
        .unwrap()
        .contains("impeccable v2")
    );
}
