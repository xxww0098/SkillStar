use std::path::{Path, PathBuf};

use ss_core::infra::{fs_ops, paths};

use super::*;
use crate::skill_lock::SourceType;
use crate::test_sandbox::Sandbox;

fn write_skill(dir: &Path, name: &str, body: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: Intake fixture.\n---\n{body}\n"),
    )
    .unwrap();
}

fn claude_skills(sandbox: &Sandbox) -> PathBuf {
    sandbox.home().join(".claude/skills")
}

struct FailAfterCopy;

impl FailAfterCopy {
    fn arm() -> Self {
        unsafe { std::env::set_var("SKILLSTAR_INTAKE_FAIL", "copy") };
        Self
    }
}

impl Drop for FailAfterCopy {
    fn drop(&mut self) {
        unsafe { std::env::remove_var("SKILLSTAR_INTAKE_FAIL") };
    }
}

#[test]
fn production_layout_adopts_links_and_leaves_conflicts_git_and_canonical_agents() {
    let sandbox = Sandbox::production();
    sandbox.enable_agent("pi");
    sandbox.enable_agent("cline");
    sandbox.enable_agent("claude");

    let claude = claude_skills(&sandbox);
    let canonical = paths::hub_skills_dir();
    assert_eq!(
        canonical,
        sandbox.home().join(".skillstar/data/skills/installed")
    );
    for id in ["pi", "cline"] {
        let profile = crate::agents::list_profiles()
            .into_iter()
            .find(|profile| profile.id == id)
            .unwrap();
        assert!(
            !crate::deployment::targets_canonical_root(&profile.global_skills_dir),
            "{id}"
        );
    }

    write_skill(&canonical.join("canonical-only"), "canonical-only", "stays");
    write_skill(&claude.join("fresh"), "fresh", "new skill");
    std::fs::write(claude.join("fresh/notes.txt"), b"keep me").unwrap();
    write_skill(&canonical.join("twin"), "twin", "same bytes");
    write_skill(&claude.join("twin"), "twin", "same bytes");
    write_skill(&canonical.join("clash"), "clash", "canonical");
    write_skill(&claude.join("clash"), "clash", "agent edit");
    write_skill(&claude.join("repo"), "repo", "checkout");
    std::fs::create_dir_all(claude.join("repo/.git")).unwrap();
    std::fs::write(claude.join("repo/.git/HEAD"), b"ref: refs/heads/main\n").unwrap();

    let planned = plan(&scan());
    assert!(
        planned.steps.iter().any(|step| {
            step.skill == "fresh" && step.action == IntakeAction::Adopt && step.agent_id == "claude"
        }),
        "{planned:?}"
    );
    assert!(planned.steps.iter().any(|step| {
        step.skill == "twin" && step.action == IntakeAction::Relink && step.agent_id == "claude"
    }));
    assert!(
        planned
            .reported
            .iter()
            .any(|item| { item.skill == "clash" && item.reason.contains("Content conflict") })
    );
    assert!(
        planned
            .reported
            .iter()
            .any(|item| { item.skill == "repo" && item.reason.contains("Git working tree") })
    );
    assert!(
        planned
            .steps
            .iter()
            .all(|step| step.agent_id != "pi" && step.agent_id != "cline")
    );
    assert!(
        planned
            .reported
            .iter()
            .all(|item| item.agent_id != "pi" && item.agent_id != "cline")
    );

    // The storage doctor repairs only owned entries. It must not adopt.
    let health_plan = crate::health::plan(&crate::health::scan());
    assert!(
        health_plan
            .steps
            .iter()
            .all(|step| { !step.path.starts_with(&claude) })
    );
    crate::health::apply(&health_plan).unwrap();
    assert!(!fs_ops::is_link(&claude.join("fresh")));
    assert_eq!(
        std::fs::read(claude.join("fresh/notes.txt")).unwrap(),
        b"keep me"
    );

    let dry = apply_with(&plan(&scan()), IntakeApplyOptions { dry_run: true }).unwrap();
    assert_eq!(dry.applied_count(), 0);
    assert_eq!(dry.would_apply_count(), 2, "{dry:?}");
    assert!(dry.failed().next().is_none(), "{dry:?}");
    assert!(!fs_ops::is_link(&claude.join("fresh")));
    assert!(!paths::local_skills_dir().join("fresh").exists());
    assert!(claude.join("twin").is_dir() && !fs_ops::is_link(&claude.join("twin")));

    let outcome = apply(&plan(&scan())).unwrap();
    assert!(outcome.failed().next().is_none(), "{outcome:?}");
    assert_eq!(outcome.applied_count(), 2, "{outcome:?}");

    let fresh = claude.join("fresh");
    let fresh_link = std::fs::read_link(&fresh).unwrap();
    assert!(fresh_link.is_relative(), "{fresh_link:?}");
    let local = paths::local_skills_dir().join("fresh");
    let hub = paths::hub_skills_dir().join("fresh");
    assert!(!fs_ops::is_link(&local));
    assert!(fs_ops::is_link(&hub));
    assert_eq!(
        std::fs::canonicalize(&fresh).unwrap(),
        std::fs::canonicalize(&hub).unwrap()
    );
    assert_eq!(std::fs::read(fresh.join("notes.txt")).unwrap(), b"keep me");
    assert_eq!(std::fs::read(local.join("notes.txt")).unwrap(), b"keep me");
    let lock = crate::skill_lock::load();
    let entry = lock.skills.get("fresh").unwrap();
    assert_eq!(entry.source_type, SourceType::Local);
    assert_eq!(entry.source, "local/claude");
    assert!(entry.skill_folder_hash.is_none());

    let twin = claude.join("twin");
    let twin_link = std::fs::read_link(&twin).unwrap();
    assert!(twin_link.is_relative(), "{twin_link:?}");
    assert_eq!(
        std::fs::canonicalize(&twin).unwrap(),
        std::fs::canonicalize(&canonical.join("twin")).unwrap()
    );
    assert!(
        std::fs::read_to_string(canonical.join("twin/SKILL.md"))
            .unwrap()
            .contains("same bytes")
    );
    assert!(!paths::local_skills_dir().join("twin").exists());

    assert!(!fs_ops::is_link(&claude.join("clash")));
    assert!(
        std::fs::read_to_string(claude.join("clash/SKILL.md"))
            .unwrap()
            .contains("agent edit")
    );
    assert!(
        std::fs::read_to_string(canonical.join("clash/SKILL.md"))
            .unwrap()
            .contains("canonical")
    );
    assert!(!fs_ops::is_link(&claude.join("repo")));
    assert_eq!(
        std::fs::read(claude.join("repo/.git/HEAD")).unwrap(),
        b"ref: refs/heads/main\n"
    );
    assert!(!fs_ops::is_link(&canonical.join("canonical-only")));
    assert!(canonical.join("canonical-only/SKILL.md").is_file());

    let again = apply(&plan(&scan())).unwrap();
    assert_eq!(again.applied_count(), 0, "{again:?}");
    assert!(again.failed().next().is_none(), "{again:?}");
}

#[test]
fn copy_failure_restores_the_agent_directory() {
    let sandbox = Sandbox::production();
    let claude = claude_skills(&sandbox);
    write_skill(&claude.join("fresh"), "fresh", "must survive");
    std::fs::write(claude.join("fresh/notes.txt"), b"original").unwrap();
    let _fail = FailAfterCopy::arm();
    let outcome = apply(&plan(&scan())).unwrap();
    assert!(outcome.failed().next().is_some(), "{outcome:?}");
    assert!(!fs_ops::is_link(&claude.join("fresh")));
    assert_eq!(
        std::fs::read(claude.join("fresh/notes.txt")).unwrap(),
        b"original"
    );
    assert!(!paths::local_skills_dir().join("fresh").exists());
    assert!(!paths::hub_skills_dir().join("fresh").exists());
}

#[test]
fn uninstall_drops_the_adoption_lock_so_the_folder_can_be_adopted_again() {
    let sandbox = Sandbox::production();
    sandbox.enable_agent("claude");
    let claude = claude_skills(&sandbox);
    write_skill(&claude.join("fresh"), "fresh", "again");
    apply(&plan(&scan())).unwrap();
    assert!(
        crate::skill_lock::load()
            .entry_for_folder("fresh")
            .is_some()
    );

    crate::skill_install::uninstall_skill("fresh").unwrap();
    let lock = crate::skill_lock::load();
    assert!(
        lock.keys_for_folder("fresh").is_empty(),
        "uninstall must drop every lock key for the folder: {:?}",
        lock.skills.keys().collect::<Vec<_>>()
    );

    write_skill(&claude.join("fresh"), "fresh", "again");
    let outcome = apply(&plan(&scan())).unwrap();
    assert!(outcome.failed().next().is_none(), "{outcome:?}");
    assert!(outcome.applied_count() >= 1, "{outcome:?}");
    assert_eq!(
        crate::skill_lock::load()
            .entry_for_folder("fresh")
            .map(|(_, entry)| entry.source_type),
        Some(SourceType::Local),
        "hub exists={} local exists={} outcome={outcome:?}",
        paths::hub_skills_dir()
            .join("fresh")
            .symlink_metadata()
            .is_ok(),
        paths::local_skills_dir()
            .join("fresh")
            .symlink_metadata()
            .is_ok(),
    );
}

#[test]
fn vercel_raw_name_key_occupies_the_folder_and_stays_updatable() {
    let sandbox = Sandbox::production();
    sandbox.enable_agent("claude");
    let claude = claude_skills(&sandbox);
    write_skill(&claude.join("my-skill"), "my-skill", "agent copy");
    let now = "2026-01-01T00:00:00Z".to_string();
    crate::skill_lock::mutate(|lock| {
        lock.skills.insert(
            "My Skill".into(),
            crate::skill_lock::SkillLockEntry {
                source: "acme/skills".into(),
                source_type: SourceType::Github,
                source_url: "https://github.com/acme/skills.git".into(),
                git_ref: Some("main".into()),
                skill_path: None,
                skill_folder_hash: None,
                installed_at: now.clone(),
                updated_at: now,
                extra: Default::default(),
            },
        );
    })
    .unwrap();

    let planned = plan(&scan());
    assert!(
        planned
            .steps
            .iter()
            .all(|step| step.action != IntakeAction::Adopt || step.skill != "my-skill"),
        "{planned:?}"
    );
    assert!(
        planned
            .reported
            .iter()
            .any(|item| item.skill == "my-skill" && item.reason.contains("install lock")),
        "{planned:?}"
    );

    crate::skill_lock::mutate(|lock| {
        lock.upsert(
            "my-skill",
            crate::skill_lock::SkillLockEntry {
                source: "local/claude".into(),
                source_type: SourceType::Local,
                source_url: String::new(),
                git_ref: None,
                skill_path: None,
                skill_folder_hash: None,
                installed_at: "2026-02-01T00:00:00Z".into(),
                updated_at: "2026-02-01T00:00:00Z".into(),
                extra: Default::default(),
            },
        );
    })
    .unwrap();
    let lock = crate::skill_lock::load();
    let (key, entry) = lock.entry_for_folder("my-skill").unwrap();
    assert_eq!(key, "My Skill");
    assert_eq!(entry.source_type, SourceType::Github);
    assert_eq!(entry.source, "acme/skills");
}

#[test]
fn extra_file_in_the_backup_is_restored_instead_of_committed() {
    let sandbox = Sandbox::production();
    sandbox.enable_agent("claude");
    let agent = claude_skills(&sandbox).join("fresh");
    write_skill(&agent, "fresh", "verified");
    std::fs::write(agent.join("extra.txt"), b"race").unwrap();
    let verified = paths::local_skills_dir().join("fresh-verified");
    write_skill(&verified, "fresh", "verified");

    let mut staged = crate::materialize::StagedReplace::stage(&agent, |staging| {
        std::fs::create_dir_all(staging)?;
        std::fs::write(staging.join("SKILL.md"), b"replacement")?;
        Ok(())
    })
    .unwrap();
    staged.swap().unwrap();
    let error = super::apply::commit_unless_backup_drifted(staged, &verified).unwrap_err();
    assert!(error.to_string().contains("Content conflict"), "{error:#}");
    assert_eq!(std::fs::read(agent.join("extra.txt")).unwrap(), b"race");
    assert!(
        std::fs::read_to_string(agent.join("SKILL.md"))
            .unwrap()
            .contains("verified")
    );
    assert!(!fs_ops::is_link(&agent));
    assert!(agent.is_dir());
}
