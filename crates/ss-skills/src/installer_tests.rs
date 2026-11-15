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
        well_known: false,
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
fn same_name_from_other_source_coexists_with_owner_suffix() {
    let _sandbox = Sandbox::new();
    let first = tempfile::tempdir().unwrap();
    make_skill(&first.path().join("skills/foo"), "foo", "from repo A");
    let first_installed = install_units(
        first.path(),
        &spec("https://github.com/a/one.git"),
        &[unit("foo", "skills/foo")],
    )
    .unwrap();
    assert_eq!(first_installed, vec!["foo".to_string()]);

    let second = tempfile::tempdir().unwrap();
    make_skill(&second.path().join("elsewhere/foo"), "foo", "from repo B");
    let second_installed = install_units(
        second.path(),
        &spec("https://github.com/b/two.git"),
        &[unit("foo", "elsewhere/foo")],
    )
    .unwrap();
    assert_eq!(second_installed, vec!["foo-b".to_string()]);

    let lock = skill_lock::load();
    assert_eq!(lock.skills.len(), 2);
    assert_eq!(
        lock.skills["foo"].source_url,
        "https://github.com/a/one.git"
    );
    assert_eq!(
        lock.skills["foo-b"].source_url,
        "https://github.com/b/two.git"
    );
    assert_eq!(
        lock.skills["foo-b"]
            .extra
            .get("called")
            .and_then(|v| v.as_str()),
        Some("foo")
    );
    assert_eq!(crate::library::called_name("foo-b").as_deref(), Some("foo"));

    let content_a =
        std::fs::read_to_string(paths::agents_skill_dir("foo").join("SKILL.md")).unwrap();
    assert!(content_a.contains("from repo A"), "{content_a}");
    let content_b =
        std::fs::read_to_string(paths::agents_skill_dir("foo-b").join("SKILL.md")).unwrap();
    assert!(content_b.contains("from repo B"), "{content_b}");
    assert!(
        content_b.contains("name: foo-b"),
        "coexisting skill must rewrite frontmatter name: {content_b}"
    );
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
fn reinstall_keeps_chinese_when_only_the_skill_body_changes() {
    let _sandbox = Sandbox::new();
    let checkout = tempfile::tempdir().unwrap();
    let source = checkout.path().join("skills/foo");
    make_skill(&source, "foo", "Build desktop apps.");
    let units = [unit("foo", "skills/foo")];
    let origin = spec("https://github.com/o/r.git");
    install_units(checkout.path(), &origin, &units).unwrap();
    ss_core::translation::remember("Build desktop apps.", "zh-CN", "构建桌面应用。");
    ss_core::translation::set_description_choice("foo", true).unwrap();

    std::fs::write(
        source.join("SKILL.md"),
        "---\nname: foo\ndescription: '  Build desktop apps.  '\n---\n\nUpdated instructions.\n",
    )
    .unwrap();
    install_units(checkout.path(), &origin, &units).unwrap();

    assert_eq!(
        ss_core::translation::load_config()
            .unwrap()
            .description_choices
            .get("foo"),
        Some(&true),
        "a body-only update must keep the chosen Chinese description"
    );
    assert_eq!(
        ss_core::translation::lookup("Build desktop apps.", "zh-CN").as_deref(),
        Some("构建桌面应用。")
    );
}

#[test]
fn reinstall_shows_original_when_the_description_changes() {
    let _sandbox = Sandbox::new();
    let checkout = tempfile::tempdir().unwrap();
    let source = checkout.path().join("skills/foo");
    make_skill(&source, "foo", "Build desktop apps.");
    let units = [unit("foo", "skills/foo")];
    let origin = spec("https://github.com/o/r.git");
    install_units(checkout.path(), &origin, &units).unwrap();
    let config = ss_core::translation::TranslationConfig {
        translate_descriptions: true,
        ..Default::default()
    };
    ss_core::translation::save_config(&config).unwrap();
    ss_core::translation::set_description_choice("foo", true).unwrap();
    ss_core::translation::set_description_choice("untouched", true).unwrap();
    ss_core::translation::remember("Build desktop apps.", "zh-CN", "构建桌面应用。");
    ss_core::translation::remember("Build mobile apps.", "zh-CN", "构建移动应用。");

    make_skill(&source, "foo", "Build mobile apps.");
    install_units(checkout.path(), &origin, &units).unwrap();

    let config = ss_core::translation::load_config().unwrap();
    assert_eq!(
        config.description_choices.get("foo"),
        Some(&false),
        "a changed description must show English even with automatic translation on"
    );
    assert_eq!(config.description_choices.get("untouched"), Some(&true));
    assert!(config.translate_descriptions);
    assert_eq!(
        ss_core::translation::lookup("Build desktop apps.", "zh-CN").as_deref(),
        Some("构建桌面应用。"),
        "changing the choice must not delete reusable translations"
    );
}

#[test]
fn colliding_or_unmappable_identities_fail_the_batch_closed() {
    let _sandbox = Sandbox::new();
    let checkout = tempfile::tempdir().unwrap();
    make_skill(&checkout.path().join("a"), "My Skill", "a");
    make_skill(&checkout.path().join("b"), "my-skill", "b");
    make_skill(&checkout.path().join("数据分析"), "数据分析", "c");

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
        &[unit("数据分析", "数据分析")],
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
    ss_core::translation::set_description_choice("foo", true).unwrap();
    ss_core::translation::remember("old", "zh-CN", "旧描述");

    std::fs::write(skill_lock::lock_path(), "{\"version\":9,\"skills\":{}}").unwrap();
    let second = tempfile::tempdir().unwrap();
    make_skill(&second.path().join("foo"), "foo", "new");
    assert!(install_units(second.path(), &spec("file:///a"), &[unit("foo", "foo")]).is_err());

    let content = std::fs::read_to_string(paths::agents_skill_dir("foo").join("SKILL.md")).unwrap();
    assert!(content.contains("old"), "{content}");
    assert_eq!(installed_names(), vec!["foo".to_string()]);
    assert_eq!(
        ss_core::translation::load_config()
            .unwrap()
            .description_choices
            .get("foo"),
        Some(&true),
        "a rolled-back install must preserve the previous language choice"
    );
    assert_eq!(
        ss_core::translation::lookup("old", "zh-CN").as_deref(),
        Some("旧描述")
    );
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

#[test]
fn non_ascii_fallback_folder_move_backup_and_runtime_carry_work() {
    let _sandbox = Sandbox::new();
    let checkout = tempfile::tempdir().unwrap();
    make_skill(
        &checkout.path().join("skills/data-analysis"),
        "数据分析",
        "cjk title",
    );

    let installed = install_units(
        checkout.path(),
        &spec("https://github.com/acme/tools.git"),
        &[unit("数据分析", "skills/data-analysis")],
    )
    .unwrap();
    assert_eq!(installed, vec!["data-analysis".to_string()]);
    assert_eq!(
        crate::library::called_name("data-analysis").as_deref(),
        Some("数据分析")
    );
    let skill_md =
        std::fs::read_to_string(paths::agents_skill_dir("data-analysis").join("SKILL.md")).unwrap();
    assert!(skill_md.contains("name: data-analysis"), "{skill_md}");

    // Seed runtime .venv and modify canonical SKILL.md locally.
    let canon = paths::agents_skill_dir("data-analysis");
    std::fs::create_dir_all(canon.join(".venv/bin")).unwrap();
    std::fs::write(canon.join(".venv/bin/python"), "venv-ok").unwrap();
    std::fs::write(
        canon.join("SKILL.md"),
        "---\nname: data-analysis\ndescription: cjk title\n---\nlocal edit\n",
    )
    .unwrap();

    // Same-source reinstall after folder move folds skill_path, archives local
    // edit to backups/, and carries .venv forward.
    let moved = tempfile::tempdir().unwrap();
    make_skill(
        &moved.path().join("packages/data-analysis"),
        "数据分析",
        "cjk title v2",
    );
    let reinstalled = install_units(
        moved.path(),
        &spec("https://github.com/acme/tools.git"),
        &[unit("数据分析", "packages/data-analysis")],
    )
    .unwrap();
    assert_eq!(reinstalled, vec!["data-analysis".to_string()]);
    assert_eq!(
        skill_lock::load().skills["data-analysis"]
            .skill_path
            .as_deref(),
        Some("packages/data-analysis")
    );
    assert!(canon.join(".venv/bin/python").is_file());
    let backups = crate::backup::list_backups().unwrap();
    assert!(
        backups
            .iter()
            .any(|b| b.skill_name == "data-analysis"
                && b.reason == crate::backup::BackupReason::UpdateOverwrite),
        "{backups:?}"
    );
}
