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
    assert_eq!(owned_deployment(&dir, "foo"), Ownership::Foreign);
    assert_eq!(remove_owned(&dir, "foo").unwrap(), Removal::Foreign);
    assert!(dir.join("SKILL.md").is_file(), "an edited copy must stay");
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
