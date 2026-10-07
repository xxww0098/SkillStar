//! Storage doctor: report everything, change only what SkillStar can prove it owns.

use std::path::{Path, PathBuf};

use ss_core::infra::{fs_ops, paths};

use super::scan::IssueKind;
use super::{ApplyOptions, RepairAction, RepairPlan, StepStatus, apply, apply_with, plan, scan};
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

fn record_lock(name: &str, source_type: SourceType, source_url: &str) {
    let now = "2026-01-01T00:00:00Z".to_string();
    crate::skill_lock::mutate(|lock| {
        lock.upsert(
            name,
            SkillLockEntry {
                source: format!("fixture/{name}"),
                source_type,
                source_url: source_url.to_string(),
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

fn touches(plan: &RepairPlan, path: &Path) -> bool {
    plan.steps.iter().any(|step| step.path == path)
}

fn write_lock_file(body: &[u8]) -> PathBuf {
    let path = crate::skill_lock::lock_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, body).unwrap();
    path
}

#[test]
fn reports_content_it_does_not_own_and_leaves_it_in_place() {
    let _sandbox = Sandbox::new();
    let hub = paths::hub_skills_dir();
    write_skill(&hub.join("orphan"), "orphan", "placed by hand");
    let bare = hub.join("bare");
    std::fs::create_dir_all(&bare).unwrap();

    let report = scan();
    assert!(report.issues.iter().any(|issue| matches!(
        issue.kind,
        IssueKind::CanonicalWithoutLock
    ) && issue.skill.as_deref() == Some("orphan")));
    assert!(
        report
            .issues
            .iter()
            .any(|issue| matches!(issue.kind, IssueKind::MissingSkillMd)
                && issue.skill.as_deref() == Some("bare"))
    );
    let repair = plan(&report);
    assert!(!touches(&repair, &hub.join("orphan")));
    assert!(!touches(&repair, &bare));
    assert!(repair.steps.is_empty(), "{repair:?}");

    let before = std::fs::read(hub.join("orphan/SKILL.md")).unwrap();
    apply(&repair).unwrap();
    assert_eq!(std::fs::read(hub.join("orphan/SKILL.md")).unwrap(), before);
    assert!(bare.is_dir());
    assert!(!hub.join("orphan").is_symlink());
}

#[test]
fn prunes_a_local_lock_entry_whose_folder_is_gone_and_is_idempotent() {
    let _sandbox = Sandbox::new();
    record_lock("ghost", SourceType::Local, "");
    let repair = plan(&scan());
    assert!(
        repair
            .steps
            .iter()
            .any(|step| matches!(step.action, RepairAction::PruneLockEntry)
                && step.skill.as_deref() == Some("ghost"))
    );

    let first = apply(&repair).unwrap();
    assert!(
        first
            .steps
            .iter()
            .any(|step| matches!(step.status, StepStatus::Applied))
    );
    assert!(!crate::skill_lock::load().skills.contains_key("ghost"));

    let second = apply(&repair).unwrap();
    assert_eq!(second.applied_count(), 0);
    assert!(
        second
            .steps
            .iter()
            .all(|step| matches!(step.status, StepStatus::AlreadyDone))
    );
}

#[test]
fn dry_run_does_not_fetch_a_missing_updatable_skill() {
    let _sandbox = Sandbox::new();
    record_lock(
        "remote",
        SourceType::Github,
        "https://github.com/acme/skills.git",
    );
    let repair = plan(&scan());
    assert!(repair.steps.iter().any(|step| {
        matches!(step.action, RepairAction::ReinstallFromLock { .. })
            && step.skill.as_deref() == Some("remote")
    }));
    let preview = apply_with(&repair, ApplyOptions { dry_run: true }).unwrap();
    assert_eq!(preview.would_apply_count(), 1);
    assert_eq!(preview.applied_count(), 0);
    assert!(crate::skill_lock::load().skills.contains_key("remote"));
    assert!(!paths::hub_skills_dir().join("remote").exists());
}

#[test]
fn refuses_to_rewrite_a_newer_or_corrupt_lock() {
    let _sandbox = Sandbox::new();
    let path = write_lock_file(br#"{"version":9,"skills":{}}"#);
    let original = std::fs::read(&path).unwrap();
    let report = scan();
    assert!(
        report
            .issues
            .iter()
            .any(|issue| matches!(issue.kind, IssueKind::LockTooNew { version: 9 }))
    );
    assert!(!report.lock_writable);
    let repair = plan(&report);
    assert!(repair.steps.is_empty(), "{repair:?}");
    assert_eq!(repair.untouched.len(), 1);
    apply(&repair).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), original);

    let corrupt = write_lock_file(b"{");
    let corrupt_bytes = std::fs::read(&corrupt).unwrap();
    let report = scan();
    assert!(
        report
            .issues
            .iter()
            .any(|issue| matches!(issue.kind, IssueKind::LockCorrupt { .. }))
    );
    let repair = plan(&report);
    assert!(repair.steps.is_empty(), "{repair:?}");
    apply(&repair).unwrap();
    assert_eq!(std::fs::read(&corrupt).unwrap(), corrupt_bytes);
}

#[test]
fn an_outdated_lock_is_reported_and_not_rewritten_by_repair() {
    let _sandbox = Sandbox::new();
    let path = write_lock_file(br#"{"version":2,"skills":{}}"#);
    let original = std::fs::read(&path).unwrap();
    let report = scan();
    assert!(
        report
            .issues
            .iter()
            .any(|issue| matches!(issue.kind, IssueKind::LockOutdated { version: 2 }))
    );
    let repair = plan(&report);
    assert!(repair.steps.is_empty(), "{repair:?}");
    apply(&repair).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), original);
}

#[test]
fn dry_run_then_apply_removes_staging_residue_once() {
    let _sandbox = Sandbox::new();
    let hub = paths::hub_skills_dir();
    std::fs::create_dir_all(hub.join("scratch")).unwrap();
    let residue = hub.join(".skillstar-stage-scratch");
    std::fs::create_dir_all(&residue).unwrap();
    std::fs::write(residue.join("partial"), b"x").unwrap();
    let repair = plan(&scan());
    assert!(
        repair.steps.iter().any(
            |step| matches!(step.action, RepairAction::RemoveOwnedResidue) && step.path == residue
        )
    );

    let preview = apply_with(&repair, ApplyOptions { dry_run: true }).unwrap();
    assert!(preview.would_apply_count() >= 1);
    assert_eq!(preview.applied_count(), 0);
    assert!(residue.join("partial").is_file());

    let done = apply(&repair).unwrap();
    assert!(done.applied_count() >= 1);
    assert!(!residue.exists());
    let again = apply(&repair).unwrap();
    assert_eq!(again.applied_count(), 0);
    assert!(
        again
            .steps
            .iter()
            .all(|step| matches!(step.status, StepStatus::AlreadyDone))
    );
}

#[test]
fn restores_a_backup_whose_result_never_landed() {
    let _sandbox = Sandbox::new();
    let hub = paths::hub_skills_dir();
    let backup = hub.join(".skillstar-backup-alpha");
    std::fs::create_dir_all(&backup).unwrap();
    std::fs::write(backup.join("SKILL.md"), b"restored").unwrap();
    let repair = plan(&scan());
    assert!(
        repair
            .steps
            .iter()
            .any(|step| matches!(step.action, RepairAction::RestoreBackup { .. }))
    );
    apply(&repair).unwrap();
    assert_eq!(
        std::fs::read(hub.join("alpha/SKILL.md")).unwrap(),
        b"restored"
    );
    assert!(!backup.exists());
}

#[cfg(unix)]
#[test]
fn agent_links_are_repaired_only_when_skillstar_owns_them() {
    let sandbox = Sandbox::new();
    sandbox.enable_agent("claude");
    let hub = paths::hub_skills_dir();
    write_skill(&hub.join("demo"), "demo", "canonical");
    write_skill(&hub.join("kept"), "kept", "canonical kept");
    record_lock("demo", SourceType::Local, "");
    record_lock("kept", SourceType::Local, "");

    let claude = sandbox.home().join(".claude/skills");
    std::fs::create_dir_all(&claude).unwrap();
    // Points at the local-skill path, which is absent, while the canonical
    // folder exists. A link to any other hub path is not SkillStar's.
    let missing_local = paths::local_skills_dir().join("demo");
    fs_ops::create_symlink(&missing_local, &claude.join("demo")).unwrap();
    std::os::unix::fs::symlink("loop", claude.join("loop")).unwrap();
    let outside = sandbox.home().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("secret"), b"mine").unwrap();
    fs_ops::create_symlink(&outside, &claude.join("foreign")).unwrap();
    write_skill(&claude.join("kept"), "kept", "the user's own copy");
    std::fs::write(claude.join("kept/NOTES.txt"), b"do not take this").unwrap();

    let report = scan();
    assert!(report.issues.iter().any(|issue| {
        matches!(issue.kind, IssueKind::BrokenLink { .. }) && issue.skill.as_deref() == Some("demo")
    }));
    assert!(
        report
            .issues
            .iter()
            .any(|issue| matches!(issue.kind, IssueKind::SelfLink)
                && issue.skill.as_deref() == Some("loop"))
    );
    assert!(report.issues.iter().any(|issue| {
        matches!(issue.kind, IssueKind::ForeignLink { .. })
            && issue.skill.as_deref() == Some("foreign")
    }));
    assert!(report.issues.iter().any(|issue| {
        matches!(issue.kind, IssueKind::UnmanagedDirectory)
            && issue.skill.as_deref() == Some("kept")
    }));

    let repair = plan(&report);
    assert!(touches(&repair, &claude.join("demo")));
    assert!(touches(&repair, &claude.join("loop")));
    assert!(!touches(&repair, &claude.join("foreign")));
    assert!(!touches(&repair, &claude.join("kept")));

    apply(&repair).unwrap();
    let relinked = fs_ops::read_link_resolved(&claude.join("demo")).unwrap();
    assert_eq!(
        std::fs::canonicalize(&relinked).unwrap(),
        std::fs::canonicalize(hub.join("demo")).unwrap()
    );
    assert!(!claude.join("loop").exists());
    assert!(fs_ops::is_link(&claude.join("foreign")));
    assert_eq!(std::fs::read(outside.join("secret")).unwrap(), b"mine");
    assert!(!fs_ops::is_link(&claude.join("kept")));
    assert_eq!(
        std::fs::read(claude.join("kept/NOTES.txt")).unwrap(),
        b"do not take this"
    );
    assert!(
        std::fs::read_to_string(claude.join("kept/SKILL.md"))
            .unwrap()
            .contains("the user's own copy")
    );
}

#[test]
fn refreshes_an_unedited_copy_and_keeps_one_edited_after_deploy() {
    let sandbox = Sandbox::new();
    sandbox.enable_agent("claude");
    let hub = paths::hub_skills_dir();
    let claude = sandbox.home().join(".claude/skills");
    std::fs::create_dir_all(&claude).unwrap();

    write_skill(&hub.join("fresh"), "fresh", "version one");
    write_skill(&hub.join("edited"), "edited", "version one");
    record_lock("fresh", SourceType::Local, "");
    record_lock("edited", SourceType::Local, "");
    crate::deployment::ownership::deploy_copy(&hub.join("fresh"), &claude.join("fresh"), "fresh")
        .unwrap();
    crate::deployment::ownership::deploy_copy(
        &hub.join("edited"),
        &claude.join("edited"),
        "edited",
    )
    .unwrap();
    write_skill(&hub.join("fresh"), "fresh", "version two");
    std::fs::write(claude.join("edited/SKILL.md"), "user edit\n").unwrap();

    let report = scan();
    assert!(
        report.issues.iter().any(|issue| {
            matches!(
                issue.kind,
                IssueKind::StaleCopy {
                    modified: false,
                    canonical_missing: false
                }
            ) && issue.skill.as_deref() == Some("fresh")
        }),
        "{report:?}"
    );
    assert!(
        report.issues.iter().any(|issue| {
            matches!(issue.kind, IssueKind::StaleCopy { modified: true, .. })
                && issue.skill.as_deref() == Some("edited")
        }),
        "{report:?}"
    );
    let repair = plan(&report);
    assert!(touches(&repair, &claude.join("fresh")));
    assert!(!touches(&repair, &claude.join("edited")));

    apply(&repair).unwrap();
    assert!(
        std::fs::read_to_string(claude.join("fresh/SKILL.md"))
            .unwrap()
            .contains("version two")
    );
    assert_eq!(
        std::fs::read_to_string(claude.join("edited/SKILL.md")).unwrap(),
        "user edit\n"
    );
    let again = apply(&plan(&scan())).unwrap();
    assert_eq!(again.applied_count(), 0);
}

#[cfg(unix)]
#[test]
fn resyncs_a_drifted_mirror_and_leaves_it_alone_the_second_time() {
    let sandbox = Sandbox::new();
    sandbox.enable_agent("antigravity");
    let hub = paths::hub_skills_dir();
    write_skill(&hub.join("demo"), "demo", "canonical");
    record_lock("demo", SourceType::Local, "");
    let agent = sandbox.home().join(".gemini/antigravity/skills");
    std::fs::create_dir_all(&agent).unwrap();
    fs_ops::create_symlink(&hub.join("demo"), &agent.join("demo")).unwrap();
    std::fs::create_dir_all(sandbox.home().join(".gemini/antigravity-cli")).unwrap();

    let report = scan();
    assert!(
        report
            .issues
            .iter()
            .any(|issue| matches!(issue.kind, IssueKind::MirrorDrift)
                && issue.agent_id.as_deref() == Some("antigravity")),
        "{report:?}"
    );
    let repair = plan(&report);
    assert!(
        repair
            .steps
            .iter()
            .any(|step| matches!(step.action, RepairAction::ResyncMirror))
    );
    apply(&repair).unwrap();
    let mirror = sandbox.home().join(".gemini/antigravity-cli/skills/demo");
    assert!(fs_ops::is_link(&mirror), "mirror link was not created");
    let again = apply(&plan(&scan())).unwrap();
    assert_eq!(again.applied_count(), 0);
    assert!(mirror.symlink_metadata().is_ok());
}

#[test]
fn repair_restores_a_removed_skill_after_the_sweep_that_runs_under_the_lock() {
    let _sandbox = Sandbox::new();
    let hub = paths::hub_skills_dir();
    write_skill(&hub.join("alpha"), "alpha", "come back");
    record_lock("alpha", SourceType::Local, "");
    let removed = hub.join(format!(".skillstar-remove-alpha-{}", "a".repeat(32)));
    std::fs::rename(hub.join("alpha"), &removed).unwrap();
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(31 * 60);
    crate::materialize::write_transient_born_at(&removed, old);
    let file = std::fs::File::open(&removed).unwrap();
    file.set_times(
        std::fs::FileTimes::new()
            .set_modified(std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(10)),
    )
    .unwrap();

    let repair = plan(&scan());
    assert!(
        repair.steps.iter().any(|step| {
            matches!(step.action, RepairAction::RestoreBackup { .. }) && step.path == removed
        }),
        "{repair:?}"
    );
    let _lock = crate::skill_update::acquire_update_transaction_lock().unwrap();
    crate::skill_update::sweep_stale_transients().unwrap();
    assert!(
        removed.join("SKILL.md").is_file(),
        "sweep must leave the only copy for doctor"
    );
    apply(&repair).unwrap();
    assert!(
        std::fs::read_to_string(hub.join("alpha/SKILL.md"))
            .unwrap()
            .contains("come back")
    );
    assert!(!removed.exists());
}

#[test]
fn vercel_raw_name_key_tracks_the_folder_that_exists() {
    let _sandbox = Sandbox::new();
    let hub = paths::hub_skills_dir();
    write_skill(&hub.join("my-skill"), "my-skill", "from git");
    let now = "2026-01-01T00:00:00Z".to_string();
    crate::skill_lock::mutate(|lock| {
        lock.skills.insert(
            "My Skill".into(),
            SkillLockEntry {
                source: "acme/skills".into(),
                source_type: SourceType::Github,
                source_url: "https://github.com/acme/skills.git".into(),
                git_ref: Some("main".into()),
                skill_path: None,
                skill_folder_hash: None,
                installed_at: now.clone(),
                updated_at: now.clone(),
                extra: Default::default(),
            },
        );
        lock.skills.insert(
            "Ghost Skill".into(),
            SkillLockEntry {
                source: "local/claude".into(),
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

    let report = scan();
    assert!(
        report.issues.iter().all(|issue| !matches!(
            issue.kind,
            IssueKind::LockWithoutCanonical { .. }
        ) || issue.skill.as_deref() != Some("My Skill")),
        "{report:?}"
    );
    assert!(
        !report.issues.iter().any(|issue| {
            matches!(issue.kind, IssueKind::LockWithoutCanonical { .. })
                && issue.path.ends_with("my-skill")
        }),
        "an existing folder is not missing: {report:?}"
    );
    assert!(
        !report.issues.iter().any(|issue| {
            matches!(issue.kind, IssueKind::CanonicalWithoutLock)
                && issue.skill.as_deref() == Some("my-skill")
        }),
        "{report:?}"
    );
    let ghost = report
        .issues
        .iter()
        .find(|issue| {
            matches!(issue.kind, IssueKind::LockWithoutCanonical { .. })
                && issue.skill.as_deref() == Some("ghost-skill")
        })
        .expect("missing folder is reported under the canonical directory");
    assert!(ghost.path.ends_with("ghost-skill"));
    assert!(!ghost.path.ends_with("Ghost Skill"));

    apply(&plan(&report)).unwrap();
    let lock = crate::skill_lock::load();
    assert!(lock.skills.contains_key("My Skill"));
    assert!(lock.keys_for_folder("ghost-skill").is_empty());
    assert!(!lock.skills.contains_key("Ghost Skill"));
}

#[test]
fn channel_retain_is_kept_while_the_subscription_still_names_the_old_release() {
    use crate::channels::shared_channels::{
        CHANNEL_SUBSCRIPTION_DESCRIPTOR_VERSION, CHANNEL_SUBSCRIPTION_STORE_VERSION,
        ChannelAutoUpdateState, ChannelReleaseTarget, ChannelSkillProvenance,
        ChannelSubscribedSkill, ChannelSubscription, ChannelSubscriptionRegistry,
        ChannelSubscriptionRemoteState, ChannelSubscriptionStore, DiskChannelSubscriptionRegistry,
    };

    let _sandbox = Sandbox::new();
    let hub = paths::hub_skills_dir();
    write_skill(&hub.join("writer"), "writer", "new bytes");
    let retain = hub.join(format!(".skillstar-retain-writer-{}", "b".repeat(32)));
    write_skill(&retain, "writer", "old bytes");
    let old_hash = crate::content::snapshot_path("writer", &retain)
        .unwrap()
        .content_hash;
    let git_ref = "a".repeat(40);
    let now = "2026-08-05T00:00:00Z".to_string();
    crate::skill_lock::mutate(|lock| {
        lock.upsert(
            "writer",
            SkillLockEntry {
                source: "acme/channel".into(),
                source_type: SourceType::Github,
                source_url: "https://github.com/acme/channel.git".into(),
                git_ref: Some(git_ref.clone()),
                skill_path: Some("skills/writer".into()),
                skill_folder_hash: None,
                installed_at: now.clone(),
                updated_at: now,
                extra: Default::default(),
            },
        );
    })
    .unwrap();
    DiskChannelSubscriptionRegistry
        .save(&ChannelSubscriptionStore {
            schema_version: CHANNEL_SUBSCRIPTION_STORE_VERSION,
            subscriptions: vec![ChannelSubscription {
                descriptor_version: CHANNEL_SUBSCRIPTION_DESCRIPTOR_VERSION,
                repository_id: 42,
                organization_id: 7,
                repository_url_aliases: Vec::new(),
                target: ChannelReleaseTarget {
                    revision: 1,
                    tag_name: "channel-v000001".into(),
                    commit_sha: git_ref.clone(),
                },
                skills: vec![ChannelSubscribedSkill {
                    id: "writer".into(),
                    content_root: "skills/writer".into(),
                    release_content_hash: old_hash.clone(),
                    release_content_hash_version: crate::content::SNAPSHOT_HASH_VERSION,
                    baseline_hash: old_hash,
                    baseline_hash_version: crate::content::SNAPSHOT_HASH_VERSION,
                    provenance: ChannelSkillProvenance {
                        repository_id: 42,
                        repository_url: "https://github.com/acme/channel.git".into(),
                        git_ref,
                        source_folder: "skills/writer".into(),
                    },
                }],
                known_skill_ids: vec!["writer".into()],
                pins: Vec::new(),
                last_update: None,
                auto_update: ChannelAutoUpdateState::default(),
                remote_state: ChannelSubscriptionRemoteState::default(),
                created_at: "2026-08-05T00:00:00Z".into(),
                updated_at: "2026-08-05T00:00:00Z".into(),
            }],
        })
        .unwrap();
    crate::materialize::write_transient_born_at(
        &retain,
        std::time::SystemTime::now() - std::time::Duration::from_secs(31 * 60),
    );

    let repair = plan(&scan());
    assert!(
        repair.steps.iter().all(|step| step.path != retain),
        "{repair:?}"
    );
    assert!(
        repair
            .untouched
            .iter()
            .any(|item| item.issue.path == retain),
        "{repair:?}"
    );
    crate::skill_update::sweep_stale_transients().unwrap();
    apply(&repair).unwrap();
    assert!(
        std::fs::read_to_string(retain.join("SKILL.md"))
            .unwrap()
            .contains("old bytes")
    );
    assert!(
        std::fs::read_to_string(hub.join("writer/SKILL.md"))
            .unwrap()
            .contains("new bytes")
    );
}
