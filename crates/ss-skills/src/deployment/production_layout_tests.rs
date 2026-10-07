//! Production layout: only `HOME` is sandboxed, so the canonical root is
//! `$HOME/.skillstar/data/skills/installed`.

use super::*;
use crate::test_sandbox::Sandbox;
use ss_core::infra::paths;

fn install(sandbox: &Sandbox, name: &str) -> std::path::PathBuf {
    let checkout = sandbox.root().join(format!("checkout-{name}"));
    let dir = checkout.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: d\n---\n"),
    )
    .unwrap();
    crate::installer::install_units(
        &checkout,
        &crate::source_resolver::Source {
            repo_url: "file:///fixture".into(),
            short: "local/fixture".into(),
            git_ref: None,
            subpath: None,
            skill_filter: None,
        },
        &[crate::installer::InstallUnit {
            id: name.into(),
            folder_path: name.into(),
        }],
    )
    .unwrap();
    paths::agents_skill_dir(name)
}

fn skip_code(outcome: ToggleSkillOutcome) -> String {
    match outcome {
        ToggleSkillOutcome::Skipped { code, .. } => code,
        ToggleSkillOutcome::Applied => "applied".into(),
    }
}

/// Installing a Skill writes the SkillStar data root, not the shared
/// `~/.agents/skills` directory Pi and Cline read. Linking Pi adds a link
/// there; unlinking removes that link and leaves the installed copy.
#[test]
fn install_stays_in_skillstar_and_unlinking_pi_keeps_the_copy() {
    let sandbox = Sandbox::production();
    let canonical_root = paths::agents_skills_root();
    assert_eq!(
        canonical_root,
        sandbox.home().join(".skillstar/data/skills/installed")
    );
    assert!(!crate::deployment::targets_canonical_root(
        &sandbox.home().join(".agents/skills")
    ));
    sandbox.enable_agent("pi");
    let canonical = install(&sandbox, "foo");

    assert!(canonical.join("SKILL.md").is_file());
    assert!(!sandbox.home().join(".agents/skills/foo").exists());
    assert_eq!(
        toggle_skill_for_agent("foo", "pi", true).unwrap(),
        ToggleSkillOutcome::Applied
    );
    assert!(ss_core::infra::fs_ops::is_link(
        &sandbox.home().join(".agents/skills/foo")
    ));
    assert_eq!(
        toggle_skill_for_agent("foo", "pi", false).unwrap(),
        ToggleSkillOutcome::Applied
    );
    assert!(
        sandbox
            .home()
            .join(".agents/skills/foo")
            .symlink_metadata()
            .is_err()
    );
    assert!(canonical.join("SKILL.md").is_file());
}

/// P0 regression: a folder the user made in an Agent directory under the
/// same name as an installed Skill is never overwritten or deleted.
#[test]
fn user_folder_with_the_same_name_survives_link_unlink_and_uninstall() {
    let sandbox = Sandbox::production();
    sandbox.enable_agent("claude");
    install(&sandbox, "foo");
    let own = sandbox.home().join(".claude/skills/foo");
    std::fs::create_dir_all(&own).unwrap();
    std::fs::write(
        own.join("SKILL.md"),
        "---\nname: foo\ndescription: mine\n---\n",
    )
    .unwrap();

    assert_eq!(
        skip_code(toggle_skill_for_agent("foo", "claude", true).unwrap()),
        SKIP_UNMANAGED_REAL_DIRECTORY
    );
    assert_eq!(
        batch_link_skills_to_agent(&["foo".to_string()], "claude").unwrap(),
        0
    );
    assert_eq!(
        skip_code(toggle_skill_for_agent("foo", "claude", false).unwrap()),
        SKIP_UNMANAGED_REAL_DIRECTORY
    );
    assert!(resync_existing_links("foo").unwrap().linked_to.is_empty());
    crate::skill_install::uninstall_skill("foo").unwrap();

    assert!(!paths::agents_skill_dir("foo").exists());
    let content = std::fs::read_to_string(own.join("SKILL.md")).unwrap();
    assert!(content.contains("mine"), "{content}");
}

/// A copy deployment carries a marker, so it is recognised and removed while
/// a later user edit of the canonical Skill does not orphan it.
#[test]
fn marked_copy_deployments_are_owned_and_removed_on_unlink() {
    let sandbox = Sandbox::production();
    sandbox.enable_agent("claude");
    install(&sandbox, "foo");
    batch_deploy_skills_to_agents(
        &["foo".to_string()],
        &["claude".to_string()],
        crate::projects::ProjectDeployMode::Copy,
    )
    .unwrap();
    let copy = sandbox.home().join(".claude/skills/foo");
    assert!(copy.join(ownership::DEPLOY_MARKER).is_file());
    std::fs::write(paths::agents_skill_dir("foo").join("extra.md"), "drift").unwrap();
    assert_eq!(owned_deployment(&copy, "foo"), Ownership::Copy);
    assert_eq!(owned_deployment(&copy, "bar"), Ownership::Foreign);

    assert_eq!(
        toggle_skill_for_agent("foo", "claude", false).unwrap(),
        ToggleSkillOutcome::Applied
    );
    assert!(copy.symlink_metadata().is_err());
}
