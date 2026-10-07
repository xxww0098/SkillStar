use crate::skill_lock::SourceType;
use crate::test_sandbox::Sandbox;

use super::*;

fn unit(id: &str, folder: &str) -> InstallUnit {
    InstallUnit {
        id: id.to_string(),
        folder_path: folder.to_string(),
    }
}

fn spec(url: &str) -> Source {
    Source {
        repo_url: url.to_string(),
        short: "owner/repo".to_string(),
        git_ref: None,
        subpath: None,
        skill_filter: None,
    }
}

fn make_skill(dir: &Path, name: &str, description: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n"),
    )
    .unwrap();
}

#[test]
fn install_copies_to_canonical_and_writes_lock() {
    let _sandbox = Sandbox::new();
    let checkout = tempfile::tempdir().unwrap();
    make_skill(&checkout.path().join("skills/foo"), "foo", "does things");

    let installed = install_units(
        checkout.path(),
        &spec("https://github.com/o/r.git"),
        &[unit("foo", "skills/foo")],
    )
    .unwrap();
    assert_eq!(installed, vec!["foo".to_string()]);

    let canonical = paths::agents_skill_dir("foo");
    assert!(canonical.join("SKILL.md").is_file());
    let lock = skill_lock::load();
    let entry = &lock.skills["foo"];
    assert_eq!(entry.source_url, "https://github.com/o/r.git");
    assert_eq!(entry.skill_path.as_deref(), Some("skills/foo"));
    assert_eq!(entry.source_type, SourceType::Github);
    let leftovers: Vec<_> = std::fs::read_dir(paths::agents_skills_root())
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name())
        .filter(|name| name.to_string_lossy().starts_with('.'))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn same_name_from_other_source_overwrites_provenance() {
    let _sandbox = Sandbox::new();
    let first = tempfile::tempdir().unwrap();
    make_skill(&first.path().join("skills/foo"), "foo", "from repo A");
    install_units(
        first.path(),
        &spec("https://github.com/a/one.git"),
        &[unit("foo", "skills/foo")],
    )
    .unwrap();

    let second = tempfile::tempdir().unwrap();
    make_skill(&second.path().join("elsewhere/foo"), "foo", "from repo B");
    install_units(
        second.path(),
        &spec("https://github.com/b/two.git"),
        &[unit("foo", "elsewhere/foo")],
    )
    .unwrap();

    let lock = skill_lock::load();
    assert_eq!(lock.skills.len(), 1);
    assert_eq!(
        lock.skills["foo"].source_url,
        "https://github.com/b/two.git"
    );
    let content = std::fs::read_to_string(paths::agents_skill_dir("foo").join("SKILL.md")).unwrap();
    assert!(content.contains("from repo B"), "{content}");
}

#[test]
fn invalid_frontmatter_fails_whole_batch_closed() {
    let _sandbox = Sandbox::new();
    let checkout = tempfile::tempdir().unwrap();
    make_skill(&checkout.path().join("skills/good"), "good", "fine");
    let bad = checkout.path().join("skills/bad");
    std::fs::create_dir_all(&bad).unwrap();
    std::fs::write(bad.join("SKILL.md"), "---\nname: bad\n---\n").unwrap();

    let error = install_units(
        checkout.path(),
        &spec("https://github.com/o/r.git"),
        &[unit("good", "skills/good"), unit("bad", "skills/bad")],
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("bad"), "{error}");
    assert!(
        !paths::agents_skill_dir("good").exists(),
        "fail-closed batch must not install anything"
    );
    assert!(skill_lock::load().skills.is_empty());
}

#[test]
fn colliding_or_unmappable_identities_fail_the_batch_closed() {
    let _sandbox = Sandbox::new();
    let checkout = tempfile::tempdir().unwrap();
    make_skill(&checkout.path().join("a"), "My Skill", "a");
    make_skill(&checkout.path().join("b"), "my-skill", "b");
    make_skill(&checkout.path().join("c"), "数据分析", "c");

    let error = install_units(
        checkout.path(),
        &spec("https://github.com/o/r.git"),
        &[unit("My Skill", "a"), unit("my-skill", "b")],
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("both install as 'my-skill'"), "{error}");

    let error = install_units(
        checkout.path(),
        &spec("https://github.com/o/r.git"),
        &[unit("数据分析", "c")],
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("non-ASCII"), "{error}");
    assert!(installed_names().is_empty());
    assert!(skill_lock::load().skills.is_empty());
}

/// P0 regression: a Skill folder carrying a relative link out of the
/// checkout (`../../../.ssh/id_rsa`) must not copy the target into canonical.
#[cfg(unix)]
#[test]
fn relative_symlink_escaping_the_checkout_is_not_copied() {
    let sandbox = Sandbox::new();
    let secret = sandbox.home().join(".ssh/id_rsa");
    std::fs::create_dir_all(secret.parent().unwrap()).unwrap();
    std::fs::write(&secret, "PRIVATE KEY").unwrap();
    let checkout = sandbox.root().join("checkout");
    let skill = checkout.join("skills/foo");
    make_skill(&skill, "foo", "d");
    std::fs::create_dir_all(checkout.join("shared")).unwrap();
    std::fs::write(checkout.join("shared/ref.md"), "shared").unwrap();
    let rel = pathdiff(&secret, &skill);
    std::os::unix::fs::symlink(&rel, skill.join("key")).unwrap();
    std::os::unix::fs::symlink("../../shared", skill.join("shared")).unwrap();
    std::os::unix::fs::symlink("nowhere", skill.join("broken")).unwrap();

    install_units(
        &checkout,
        &spec("file:///nowhere"),
        &[unit("foo", "skills/foo")],
    )
    .unwrap();

    let dest = paths::agents_skill_dir("foo");
    assert!(
        dest.join("key").symlink_metadata().is_err(),
        "escaping link copied"
    );
    assert!(dest.join("broken").symlink_metadata().is_err());
    assert_eq!(
        std::fs::read_to_string(dest.join("shared/ref.md")).unwrap(),
        "shared",
        "in-checkout directory links are copied as real files"
    );
}

#[cfg(unix)]
fn pathdiff(target: &Path, from: &Path) -> std::path::PathBuf {
    let target = std::fs::canonicalize(target).unwrap();
    let from = std::fs::canonicalize(from).unwrap();
    let common = target
        .components()
        .zip(from.components())
        .take_while(|(a, b)| a == b)
        .count();
    let mut rel = std::path::PathBuf::new();
    for _ in from.components().skip(common) {
        rel.push("..");
    }
    for part in target.components().skip(common) {
        rel.push(part);
    }
    rel
}

#[test]
fn failed_lock_write_restores_the_previous_folder() {
    let _sandbox = Sandbox::new();
    let first = tempfile::tempdir().unwrap();
    make_skill(&first.path().join("foo"), "foo", "old");
    install_units(first.path(), &spec("file:///a"), &[unit("foo", "foo")]).unwrap();

    std::fs::write(skill_lock::lock_path(), "{\"version\":9,\"skills\":{}}").unwrap();
    let second = tempfile::tempdir().unwrap();
    make_skill(&second.path().join("foo"), "foo", "new");
    assert!(install_units(second.path(), &spec("file:///b"), &[unit("foo", "foo")]).is_err());

    let content = std::fs::read_to_string(paths::agents_skill_dir("foo").join("SKILL.md")).unwrap();
    assert!(content.contains("old"), "{content}");
    assert_eq!(installed_names(), vec!["foo".to_string()]);
}

#[test]
fn copy_excludes_git_and_caches() {
    let _sandbox = Sandbox::new();
    let checkout = tempfile::tempdir().unwrap();
    let skill = checkout.path().join("skills/foo");
    make_skill(&skill, "foo", "d");
    std::fs::create_dir_all(skill.join(".git")).unwrap();
    std::fs::write(skill.join(".git/HEAD"), "ref").unwrap();
    std::fs::create_dir_all(skill.join("__pycache__")).unwrap();
    std::fs::write(skill.join("__pycache__/x.pyc"), "bin").unwrap();
    std::fs::write(skill.join("metadata.json"), "{}").unwrap();
    std::fs::write(skill.join("helper.sh"), "#!/bin/sh\n").unwrap();

    install_units(
        checkout.path(),
        &spec("file:///nowhere"),
        &[unit("foo", "skills/foo")],
    )
    .unwrap();
    let dest = paths::agents_skill_dir("foo");
    assert!(dest.join("SKILL.md").is_file());
    assert!(dest.join("helper.sh").is_file());
    assert!(!dest.join(".git").exists());
    assert!(!dest.join("__pycache__").exists());
    assert!(!dest.join("metadata.json").exists());
}

#[test]
fn uninstall_removes_canonical_and_lock_entry() {
    let _sandbox = Sandbox::new();
    let checkout = tempfile::tempdir().unwrap();
    make_skill(&checkout.path().join("skills/foo"), "foo", "d");
    install_units(
        checkout.path(),
        &spec("https://github.com/o/r.git"),
        &[unit("foo", "skills/foo")],
    )
    .unwrap();

    uninstall_canonical("foo").unwrap();
    assert!(!paths::agents_skill_dir("foo").exists());
    assert!(skill_lock::load().skills.is_empty());
    // Idempotent.
    uninstall_canonical("foo").unwrap();
    assert!(uninstall_canonical("../foo").is_err());
}

#[test]
fn local_source_records_no_folder_hash() {
    let _sandbox = Sandbox::new();
    let checkout = tempfile::tempdir().unwrap();
    make_skill(&checkout.path().join("skills/foo"), "foo", "d");

    install_units(
        checkout.path(),
        &spec(&format!("file://{}", checkout.path().display())),
        &[unit("foo", "skills/foo")],
    )
    .unwrap();
    let lock = skill_lock::load();
    assert_eq!(lock.skills["foo"].source_type, SourceType::Local);
    assert_eq!(lock.skills["foo"].skill_folder_hash, None);
}
