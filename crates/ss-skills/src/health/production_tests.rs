//! Production layout: only `HOME` is sandboxed. The canonical root is under
//! `~/.skillstar`, so Pi and Cline's `~/.agents/skills` is an ordinary Agent
//! directory.

use std::path::Path;

use ss_core::infra::paths;

use super::scan::IssueKind;
use super::{ApplyOptions, RepairAction, StepStatus, apply, apply_with, plan, scan};
use crate::skill_lock::{SkillLockEntry, SourceType};
use crate::test_sandbox::Sandbox;

fn write_skill(dir: &Path, name: &str, body: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: Health fixture.\n---\n{body}\n"),
    )
    .unwrap();
}

fn record_lock(name: &str) {
    let now = "2026-01-01T00:00:00Z".to_string();
    crate::skill_lock::mutate(|lock| {
        lock.upsert(
            name,
            SkillLockEntry {
                source: format!("fixture/{name}"),
                source_type: SourceType::Local,
                source_url: String::new(),
                git_ref: None,
                skill_path: None,
                skill_folder_hash: None,
                installed_at: now.clone(),
                updated_at: now,
                extra: Default::default(),
            },
        );
    })
    .unwrap();
}

#[test]
fn pi_and_cline_are_ordinary_agents_and_unmarked_directories_stay() {
    let sandbox = Sandbox::production();
    sandbox.enable_agent("pi");
    sandbox.enable_agent("cline");
    sandbox.enable_agent("claude");

    let root = paths::hub_skills_dir();
    assert_eq!(
        root,
        sandbox.home().join(".skillstar/data/skills/installed")
    );
    let profiles = crate::agents::list_profiles();
    for id in ["pi", "cline"] {
        let profile = profiles.iter().find(|profile| profile.id == id).unwrap();
        assert!(
            !crate::deployment::targets_canonical_root(&profile.global_skills_dir),
            "{id} reads ~/.agents/skills, which is not the SkillStar store"
        );
    }
    let visited = super::scan::agent_dirs();
    assert!(
        visited
            .iter()
            .any(|(id, dir)| { (id == "pi" || id == "cline") && dir.ends_with(".agents/skills") }),
        "Pi and Cline share an ordinary Agent directory: {visited:?}"
    );
    assert!(
        visited
            .iter()
            .all(|(_, dir)| !crate::deployment::targets_canonical_root(dir)),
        "repair must not walk the canonical root as an Agent directory: {visited:?}"
    );
    assert!(visited.iter().any(|(id, _)| id == "claude"));

    write_skill(&root.join("demo"), "demo", "installed");
    record_lock("demo");
    let claude = sandbox.home().join(".claude/skills");
    write_skill(&claude.join("demo"), "demo", "the user's own skill");
    std::fs::write(claude.join("demo/NOTES.txt"), b"keep").unwrap();
    // Same bytes as canonical: still the user's directory, not a deployment to delete.
    write_skill(&claude.join("twin"), "twin", "installed");
    write_skill(&root.join("twin"), "twin", "installed");
    record_lock("twin");

    let report = scan();
    assert!(
        report.issues.iter().any(|issue| {
            matches!(issue.kind, IssueKind::UnmanagedDirectory)
                && issue.skill.as_deref() == Some("demo")
        }),
        "{report:?}"
    );
    let repair = plan(&report);
    assert!(
        repair
            .steps
            .iter()
            .all(|step| step.path != claude.join("demo")
                && step.path != claude.join("twin")
                && step.path != root.join("demo"))
    );
    apply(&repair).unwrap();

    assert!(root.join("demo").is_dir());
    assert!(!root.join("demo").is_symlink());
    assert!(!claude.join("demo").is_symlink());
    assert_eq!(
        std::fs::read(claude.join("demo/NOTES.txt")).unwrap(),
        b"keep"
    );
    assert!(
        std::fs::read_to_string(claude.join("demo/SKILL.md"))
            .unwrap()
            .contains("the user's own skill")
    );
    assert!(!claude.join("twin").is_symlink());
    assert!(claude.join("twin/SKILL.md").is_file());
}

#[test]
fn legacy_cleanup_records_reinstalls_before_deleting_the_lock() {
    let sandbox = Sandbox::production();
    let legacy = paths::legacy_hub_root();
    assert!(
        legacy.starts_with(sandbox.home()),
        "legacy hub escaped the sandboxed home: {}",
        legacy.display()
    );
    std::fs::create_dir_all(&legacy).unwrap();
    let lock = legacy.join("lock.json");
    std::fs::write(
        &lock,
        br#"{"skills":[{"name":"old-skill","gitUrl":"https://github.com/acme/old.git","gitRef":"main","sourceFolder":"skills/old-skill"}]}"#,
    )
    .unwrap();

    let repair = plan(&scan());
    assert!(
        repair.steps.iter().any(
            |step| matches!(step.action, RepairAction::RerunLegacyCleanup) && step.path == lock
        )
    );
    let preview = apply_with(&repair, ApplyOptions { dry_run: true }).unwrap();
    assert!(
        preview
            .steps
            .iter()
            .any(|step| matches!(step.status, StepStatus::WouldApply))
    );
    assert!(lock.is_file(), "dry-run deleted the legacy lock");

    apply(&repair).unwrap();
    assert!(!lock.exists());
    let pending = crate::legacy_cleanup::pending_reinstalls();
    assert!(
        pending.iter().any(|entry| {
            entry.name == "old-skill" && entry.git_url == "https://github.com/acme/old.git"
        }),
        "{pending:?}"
    );
    assert!(paths::state_dir().join("agents-migration-done").is_file());

    let again = plan(&scan());
    assert!(
        again.steps.iter().any(|step| {
            matches!(step.action, RepairAction::ReinstallLegacy { .. })
                && step.skill.as_deref() == Some("old-skill")
        }),
        "{again:?}"
    );
    apply_with(&again, ApplyOptions { dry_run: true }).unwrap();
    assert!(
        !paths::hub_skills_dir().join("old-skill").exists(),
        "dry-run fetched the legacy skill"
    );
    assert!(
        crate::legacy_cleanup::pending_reinstalls()
            .iter()
            .any(|entry| entry.name == "old-skill")
    );
}
