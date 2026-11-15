use super::*;
use crate::test_sandbox::Sandbox;
use std::path::PathBuf;

fn write_skill(dir: &Path, body: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("SKILL.md"), body).unwrap();
}

fn canonical(sandbox_body: &str) -> PathBuf {
    let dir = paths::agents_skills_root().join("foo");
    write_skill(&dir, sandbox_body);
    dir
}

#[test]
fn unmarked_copy_requires_an_equal_file_set_and_rejects_excluded_names() {
    let _sandbox = Sandbox::production();
    let source = canonical("same");
    let agent = _sandbox.home().join(".claude/skills");
    let plain = agent.join("foo");
    write_skill(&plain, "same");
    assert_eq!(owned_deployment(&plain, "foo"), Ownership::Copy);

    let extra = agent.join("extra");
    write_skill(&extra, "same");
    std::fs::write(extra.join("notes.md"), "mine").unwrap();
    assert_eq!(
        owned_deployment(&extra, "foo"),
        Ownership::Foreign,
        "an extra file is not the same tree"
    );

    let checkout = agent.join("checkout");
    write_skill(&checkout, "same");
    std::fs::create_dir(checkout.join(".git")).unwrap();
    assert_eq!(owned_deployment(&checkout, "foo"), Ownership::Foreign);
    assert_eq!(remove_owned(&checkout, "foo").unwrap(), Removal::Foreign);
    assert!(checkout.join(".git").is_dir(), "a git checkout must stay");
    assert!(checkout.join("SKILL.md").is_file());

    let _ = source;
}

#[test]
fn project_directories_without_a_marker_are_reported_and_kept() {
    let _sandbox = Sandbox::production();
    canonical("same");
    let project = _sandbox.home().join("proj/.claude/skills/foo");
    write_skill(&project, "same");
    assert_eq!(
        owned_project_deployment(&project, "foo"),
        Ownership::Foreign
    );
    assert_eq!(
        remove_project_owned(&project, "foo").unwrap(),
        Removal::Foreign
    );
    assert_eq!(
        std::fs::read_to_string(project.join("SKILL.md")).unwrap(),
        "same"
    );
}

#[test]
fn marker_content_hash_must_match_or_the_copy_is_foreign() {
    let _sandbox = Sandbox::production();
    let dir = _sandbox.home().join("agent/foo");
    write_skill(&dir, "deployed");
    mark_copy_for_test(&dir, "foo");
    assert_eq!(owned_deployment(&dir, "foo"), Ownership::Copy);

    std::fs::write(dir.join("SKILL.md"), "edited").unwrap();
    assert_eq!(owned_deployment(&dir, "foo"), Ownership::SkillStarModifiedCopy);
    assert_eq!(remove_owned(&dir, "foo").unwrap(), Removal::Foreign);
    assert!(dir.join("SKILL.md").is_file(), "an edited copy must stay");
}

fn assert_no_transients(target: &Path) {
    for entry in std::fs::read_dir(target.parent().unwrap()).unwrap() {
        assert!(
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(materialize::TRANSIENT_PREFIX),
            "staging and backup entries must be cleaned up"
        );
    }
}

#[test]
fn unknown_marker_versions_are_foreign_even_when_the_hash_matches() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("foo");
    write_skill(&target, "deployed");
    mark_copy_for_test(&target, "foo");
    let mut marker = read_marker(&target).unwrap();
    for version in [0, DEPLOY_MARKER_VERSION + 1, u32::MAX] {
        marker.version = version;
        std::fs::write(
            target.join(DEPLOY_MARKER),
            serde_json::to_vec(&marker).unwrap(),
        )
        .unwrap();
        assert_eq!(owned_deployment(&target, "foo"), Ownership::Foreign);
        assert_eq!(owned_project_deployment(&target, "foo"), Ownership::Foreign);
        assert!(marker_content_hash(&target).is_none());
        assert_eq!(remove_owned(&target, "foo").unwrap(), Removal::Foreign);
        assert_eq!(
            remove_project_owned(&target, "foo").unwrap(),
            Removal::Foreign
        );
    }
    assert_eq!(
        std::fs::read_to_string(target.join("SKILL.md")).unwrap(),
        "deployed"
    );
}

#[test]
fn deploy_copy_still_refuses_an_existing_owned_copy() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let target = temp.path().join("foo");
    write_skill(&source, "new");
    write_skill(&target, "old");
    mark_copy_for_test(&target, "foo");
    let marker = std::fs::read(target.join(DEPLOY_MARKER)).unwrap();
    let error = deploy_copy(&source, &target, "foo").unwrap_err();
    assert!(error.to_string().contains("Destination already exists"));
    assert_eq!(
        std::fs::read_to_string(target.join("SKILL.md")).unwrap(),
        "old"
    );
    assert_eq!(std::fs::read(target.join(DEPLOY_MARKER)).unwrap(), marker);
    assert_no_transients(&target);
}

#[test]
fn refresh_project_copy_replaces_the_full_tree_and_preserves_copy_mode() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let target = temp.path().join("foo");
    write_skill(&source, "new");
    std::fs::create_dir(source.join("scripts")).unwrap();
    std::fs::write(source.join("scripts/run.sh"), "new script").unwrap();
    for excluded in [".git", ".skillstar"] {
        std::fs::create_dir(source.join(excluded)).unwrap();
        std::fs::write(source.join(excluded).join("private"), "not deployed").unwrap();
    }
    std::fs::write(source.join(DEPLOY_MARKER), "not the deployment marker").unwrap();
    write_skill(&target, "old");
    std::fs::write(target.join("obsolete.txt"), "old file").unwrap();
    mark_copy_for_test(&target, "foo");
    refresh_project_copy(&source, &target, "foo").unwrap();
    assert!(!fs_ops::is_link(&target), "refresh must keep a real copy");
    assert!(target.symlink_metadata().unwrap().is_dir());
    assert_eq!(
        std::fs::read_to_string(target.join("SKILL.md")).unwrap(),
        "new"
    );
    assert_eq!(
        std::fs::read_to_string(target.join("scripts/run.sh")).unwrap(),
        "new script"
    );
    assert!(!target.join("obsolete.txt").exists());
    assert!(!target.join(".git").exists());
    assert!(!target.join(".skillstar").exists());
    let marker = read_marker(&target).unwrap();
    assert_eq!(marker.version, DEPLOY_MARKER_VERSION);
    assert_eq!(marker.source, "foo");
    assert_eq!(marker.content_hash, dir_content_hash(&target).unwrap());
    assert_eq!(owned_project_deployment(&target, "foo"), Ownership::Copy);
    std::fs::write(source.join("SKILL.md"), "source changed again").unwrap();
    assert_eq!(
        std::fs::read_to_string(target.join("SKILL.md")).unwrap(),
        "new"
    );
    assert_no_transients(&target);
}

#[test]
fn refresh_project_copy_staging_failure_preserves_the_old_copy() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let target = temp.path().join("foo");
    write_skill(&source, "partly staged new content");
    let mut nested = source.clone();
    for _ in 0..=materialize::COPY_LIMITS.max_depth {
        nested = nested.join("nested");
        std::fs::create_dir(&nested).unwrap();
    }
    write_skill(&target, "old");
    std::fs::write(target.join("keep.txt"), "old file").unwrap();
    mark_copy_for_test(&target, "foo");
    let marker = std::fs::read(target.join(DEPLOY_MARKER)).unwrap();
    let error = refresh_project_copy(&source, &target, "foo").unwrap_err();
    assert!(format!("{error:#}").contains("too large to copy safely"));
    assert_eq!(
        std::fs::read_to_string(target.join("SKILL.md")).unwrap(),
        "old"
    );
    assert_eq!(
        std::fs::read_to_string(target.join("keep.txt")).unwrap(),
        "old file"
    );
    assert_eq!(std::fs::read(target.join(DEPLOY_MARKER)).unwrap(), marker);
    assert_eq!(owned_project_deployment(&target, "foo"), Ownership::Copy);
    assert_no_transients(&target);
}

#[test]
fn refresh_project_copy_refuses_missing_foreign_and_link_entries() {
    let sandbox = Sandbox::production();
    let source = canonical("new");
    let target = sandbox.home().join("proj/skills/foo");
    assert!(refresh_project_copy(&source, &target, "foo").is_err());
    assert!(!target.exists());
    write_skill(&target, "new");
    assert!(refresh_project_copy(&source, &target, "foo").is_err());
    mark_copy_for_test(&target, "bar");
    assert!(refresh_project_copy(&source, &target, "foo").is_err());
    mark_copy_for_test(&target, "foo");
    std::fs::write(target.join("SKILL.md"), "user edited").unwrap();
    assert!(refresh_project_copy(&source, &target, "foo").is_err());
    assert_eq!(
        std::fs::read_to_string(target.join("SKILL.md")).unwrap(),
        "user edited"
    );
    std::fs::remove_dir_all(&target).unwrap();
    fs_ops::create_symlink(&source, &target).unwrap();
    assert_eq!(
        owned_project_deployment(&target, "foo"),
        Ownership::Link { alive: true }
    );
    assert!(refresh_project_copy(&source, &target, "foo").is_err());
    assert!(fs_ops::is_link(&target));
    assert_no_transients(&target);
}

#[test]
fn refresh_project_copy_rechecks_ownership_after_staging() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let target = temp.path().join("foo");
    write_skill(&source, "new");
    write_skill(&target, "old");
    mark_copy_for_test(&target, "foo");
    let marker = std::fs::read(target.join(DEPLOY_MARKER)).unwrap();
    PROJECT_COPY_STAGED_PROBE.with(|cell| {
        cell.set(Some(|target| {
            assert_eq!(
                std::fs::read_to_string(target.join("SKILL.md")).unwrap(),
                "old"
            );
            let stage = std::fs::read_dir(target.parent().unwrap())
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .find(|path| {
                    path.file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with(".skillstar-stage-")
                })
                .expect("the complete replacement is staged before touching the live copy");
            assert_eq!(
                std::fs::read_to_string(stage.join("SKILL.md")).unwrap(),
                "new"
            );
            assert_eq!(owned_project_deployment(&stage, "foo"), Ownership::Copy);
            std::fs::write(target.join("SKILL.md"), "external edit").unwrap();
        }))
    });
    let error = refresh_project_copy(&source, &target, "foo").unwrap_err();
    assert!(error.to_string().contains("changed while staging"));
    assert_eq!(
        std::fs::read_to_string(target.join("SKILL.md")).unwrap(),
        "external edit"
    );
    assert_eq!(std::fs::read(target.join(DEPLOY_MARKER)).unwrap(), marker);
    assert_eq!(
        owned_project_deployment(&target, "foo"),
        Ownership::SkillStarModifiedCopy
    );
    assert_no_transients(&target);
}

#[test]
fn refresh_project_copy_rechecks_the_baseline_even_if_the_marker_is_updated() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let target = temp.path().join("foo");
    write_skill(&source, "new");
    write_skill(&target, "old");
    mark_copy_for_test(&target, "foo");
    PROJECT_COPY_STAGED_PROBE.with(|cell| {
        cell.set(Some(|target| {
            std::fs::write(target.join("SKILL.md"), "another deployment").unwrap();
            mark_copy_for_test(target, "foo");
            assert_eq!(owned_project_deployment(target, "foo"), Ownership::Copy);
        }))
    });
    let error = refresh_project_copy(&source, &target, "foo").unwrap_err();
    assert!(error.to_string().contains("changed while staging"));
    assert_eq!(
        std::fs::read_to_string(target.join("SKILL.md")).unwrap(),
        "another deployment"
    );
    assert_eq!(owned_project_deployment(&target, "foo"), Ownership::Copy);
    assert_no_transients(&target);
}

#[test]
fn replace_project_copy_explicitly_switches_to_a_link() {
    let sandbox = Sandbox::production();
    let source = canonical("new");
    let target = sandbox.home().join("proj/skills/foo");
    write_skill(&target, "old");
    std::fs::write(target.join("obsolete.txt"), "old file").unwrap();
    mark_copy_for_test(&target, "foo");
    let was_copy = replace_project_copy_with_link_or_copy(&source, &target, "foo").unwrap();
    assert!(!was_copy, "a supported link must replace the old copy");
    assert!(fs_ops::is_link(&target));
    assert_eq!(
        owned_project_deployment(&target, "foo"),
        Ownership::Link { alive: true }
    );
    assert_eq!(
        std::fs::canonicalize(&target).unwrap(),
        std::fs::canonicalize(&source).unwrap()
    );
    assert!(!target.join("obsolete.txt").exists());
    assert!(!target.join(DEPLOY_MARKER).exists());
    std::fs::write(source.join("SKILL.md"), "source changed again").unwrap();
    assert_eq!(
        std::fs::read_to_string(target.join("SKILL.md")).unwrap(),
        "source changed again"
    );
    assert_no_transients(&target);
}

#[test]
fn replace_project_copy_refuses_missing_and_foreign_entries() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let target = temp.path().join("foo");
    write_skill(&source, "new");
    assert!(replace_project_copy_with_link_or_copy(&source, &target, "foo").is_err());
    assert!(!target.exists());
    write_skill(&target, "new");
    assert!(replace_project_copy_with_link_or_copy(&source, &target, "foo").is_err());
    mark_copy_for_test(&target, "bar");
    assert!(replace_project_copy_with_link_or_copy(&source, &target, "foo").is_err());
    mark_copy_for_test(&target, "foo");
    std::fs::write(target.join("SKILL.md"), "user edited").unwrap();
    assert!(replace_project_copy_with_link_or_copy(&source, &target, "foo").is_err());
    assert_eq!(
        std::fs::read_to_string(target.join("SKILL.md")).unwrap(),
        "user edited"
    );
    assert!(!fs_ops::is_link(&target));
    assert_no_transients(&target);
}

#[test]
fn replace_project_copy_rechecks_ownership_after_staging_a_link() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let target = temp.path().join("foo");
    write_skill(&source, "new");
    write_skill(&target, "old");
    mark_copy_for_test(&target, "foo");
    let marker = std::fs::read(target.join(DEPLOY_MARKER)).unwrap();
    PROJECT_COPY_STAGED_PROBE.with(|cell| {
        cell.set(Some(|target| {
            assert!(!fs_ops::is_link(target));
            assert_eq!(
                std::fs::read_to_string(target.join("SKILL.md")).unwrap(),
                "old"
            );
            let staging = std::fs::read_dir(target.parent().unwrap())
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .find(|path| {
                    path.file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with(".skillstar-stage-")
                })
                .expect("replacement is staged before the live copy changes");
            assert_eq!(
                std::fs::read_to_string(staging.join("SKILL.md")).unwrap(),
                "new"
            );
            std::fs::write(target.join("SKILL.md"), "external edit").unwrap();
        }))
    });
    let error = replace_project_copy_with_link_or_copy(&source, &target, "foo").unwrap_err();
    assert!(error.to_string().contains("changed while staging"));
    assert!(!fs_ops::is_link(&target));
    assert_eq!(
        std::fs::read_to_string(target.join("SKILL.md")).unwrap(),
        "external edit"
    );
    assert_eq!(std::fs::read(target.join(DEPLOY_MARKER)).unwrap(), marker);
    assert_eq!(
        owned_project_deployment(&target, "foo"),
        Ownership::SkillStarModifiedCopy
    );
    assert_no_transients(&target);
}

#[test]
fn replace_project_copy_rechecks_the_baseline_after_another_marked_copy() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let target = temp.path().join("foo");
    write_skill(&source, "new");
    write_skill(&target, "old");
    mark_copy_for_test(&target, "foo");
    PROJECT_COPY_STAGED_PROBE.with(|cell| {
        cell.set(Some(|target| {
            std::fs::write(target.join("SKILL.md"), "another deployment").unwrap();
            mark_copy_for_test(target, "foo");
            assert_eq!(owned_project_deployment(target, "foo"), Ownership::Copy);
        }))
    });
    let error = replace_project_copy_with_link_or_copy(&source, &target, "foo").unwrap_err();
    assert!(error.to_string().contains("changed while staging"));
    assert!(!fs_ops::is_link(&target));
    assert_eq!(
        std::fs::read_to_string(target.join("SKILL.md")).unwrap(),
        "another deployment"
    );
    assert_eq!(owned_project_deployment(&target, "foo"), Ownership::Copy);
    assert_no_transients(&target);
}

#[cfg(unix)]
#[test]
fn replace_project_copy_staging_failure_preserves_the_old_copy() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let temp = tempfile::tempdir().unwrap();
    // A NUL makes symlink creation and the copy fallback fail without relying
    // on permissions (which are different for root and normal test runners).
    let source = temp
        .path()
        .join(OsString::from_vec(b"invalid\0source".to_vec()));
    let target = temp.path().join("foo");
    write_skill(&target, "old");
    std::fs::write(target.join("keep.txt"), "old file").unwrap();
    mark_copy_for_test(&target, "foo");
    let marker = std::fs::read(target.join(DEPLOY_MARKER)).unwrap();
    let error = replace_project_copy_with_link_or_copy(&source, &target, "foo").unwrap_err();
    assert!(error.to_string().contains("Failed to stage"));
    assert!(!fs_ops::is_link(&target));
    assert_eq!(
        std::fs::read_to_string(target.join("SKILL.md")).unwrap(),
        "old"
    );
    assert_eq!(
        std::fs::read_to_string(target.join("keep.txt")).unwrap(),
        "old file"
    );
    assert_eq!(std::fs::read(target.join(DEPLOY_MARKER)).unwrap(), marker);
    assert_eq!(owned_project_deployment(&target, "foo"), Ownership::Copy);
    assert_no_transients(&target);
}

#[test]
fn content_hash_length_prefixes_stop_boundary_collisions_and_streams() {
    let temp = tempfile::tempdir().unwrap();
    let left = temp.path().join("left");
    let right = temp.path().join("right");
    std::fs::create_dir_all(left.join("ab")).unwrap();
    std::fs::write(left.join("ab").join("c"), "c").unwrap();
    // The colliding shape is path "ab" + body "c" versus path "a" + body "bc"
    // when the two are concatenated without lengths. Here the files themselves
    // are `ab` (contents `c`) and `a` (contents `bc`).
    std::fs::create_dir(&right).unwrap();
    std::fs::write(left.join("ab-file"), "c").unwrap();
    // Rebuild left as a single file named "ab".
    let _ = std::fs::remove_dir_all(&left);
    std::fs::create_dir(&left).unwrap();
    std::fs::write(left.join("ab"), "c").unwrap();
    std::fs::write(right.join("a"), "bc").unwrap();
    let left_hash = dir_content_hash(&left).unwrap();
    let right_hash = dir_content_hash(&right).unwrap();
    assert_ne!(left_hash, right_hash);

    let big = temp.path().join("big");
    std::fs::create_dir(&big).unwrap();
    std::fs::write(big.join("blob"), vec![b'x'; 70 * 1024]).unwrap();
    assert_eq!(
        dir_content_hash(&big).unwrap(),
        dir_content_hash(&big).unwrap()
    );
}

#[test]
fn canonical_root_includes_directories_inside_it() {
    let _sandbox = Sandbox::production();
    let root = paths::agents_skills_root();
    assert!(targets_canonical_root(&root));
    assert!(targets_canonical_root(&root.join("foo")));
    assert!(!targets_canonical_root(&PathBuf::new()));
    assert!(!targets_canonical_root(
        &_sandbox.home().join(".claude/skills")
    ));
}

#[test]
fn deployment_links_must_be_the_skill_directory_itself() {
    let _sandbox = Sandbox::production();
    let source = canonical("body");
    let temp = tempfile::tempdir().unwrap();

    let exact = temp.path().join("exact");
    fs_ops::create_symlink(&source, &exact).unwrap();
    assert_eq!(
        owned_deployment(&exact, "foo"),
        Ownership::Link { alive: true }
    );

    let scripts = source.join("scripts");
    std::fs::create_dir(&scripts).unwrap();
    let nested = temp.path().join("nested");
    fs_ops::create_symlink(&scripts, &nested).unwrap();
    assert_eq!(owned_deployment(&nested, "foo"), Ownership::Foreign);

    let other = paths::agents_skills_root().join("bar");
    write_skill(&other, "other");
    let sibling = temp.path().join("sibling");
    fs_ops::create_symlink(&other, &sibling).unwrap();
    assert_eq!(owned_deployment(&sibling, "foo"), Ownership::Foreign);

    let legacy = paths::legacy_hub_root().join("skills").join("foo");
    write_skill(&legacy, "legacy");
    let legacy_link = temp.path().join("legacy");
    fs_ops::create_symlink(&legacy, &legacy_link).unwrap();
    assert_eq!(
        owned_deployment(&legacy_link, "foo"),
        Ownership::Link { alive: true }
    );

    let hub_elsewhere = paths::legacy_hub_root().join("foo");
    write_skill(&hub_elsewhere, "nope");
    let elsewhere = temp.path().join("elsewhere");
    fs_ops::create_symlink(&hub_elsewhere, &elsewhere).unwrap();
    assert_eq!(owned_deployment(&elsewhere, "foo"), Ownership::Foreign);

    let local = paths::local_skills_dir().join("foo");
    write_skill(&local, "local");
    let local_link = temp.path().join("local");
    fs_ops::create_symlink(&local, &local_link).unwrap();
    assert_eq!(
        owned_deployment(&local_link, "foo"),
        Ownership::Link { alive: true }
    );
}

#[test]
fn a_dangling_link_to_the_removed_agents_directory_is_still_ours() {
    let sandbox = Sandbox::production();
    let legacy = paths::legacy_agents_skills_root().join("foo");
    let link = sandbox.home().join("mirror/foo");
    std::fs::create_dir_all(link.parent().unwrap()).unwrap();
    fs_ops::create_symlink(&legacy, &link).unwrap();
    assert_eq!(
        owned_deployment(&link, "foo"),
        Ownership::Link { alive: false }
    );

    // Relative text with `..` is the same path once joined to the link dir.
    #[cfg(unix)]
    {
        let relative = sandbox.home().join(".gemini/antigravity-ide/skills/foo");
        std::fs::create_dir_all(relative.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink("../../../.agents/skills/foo", &relative).unwrap();
        assert_eq!(
            owned_deployment(&relative, "foo"),
            Ownership::Link { alive: false }
        );
    }

    write_skill(&legacy, "left behind");
    assert_eq!(owned_deployment(&link, "foo"), Ownership::Foreign);
}
