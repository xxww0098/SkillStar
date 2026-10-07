use super::*;

#[test]
fn canonical_names_follow_vercel_mapping_and_refuse_lossy_identities() {
    assert_eq!(canonical_skill_name("../etc/passwd").unwrap(), "etc-passwd");
    assert_eq!(canonical_skill_name("My Skill").unwrap(), "my-skill");
    assert_eq!(canonical_skill_name("a//b").unwrap(), "a-b");
    assert_eq!(canonical_skill_name("foo_bar").unwrap(), "foo_bar");
    assert_eq!(canonical_skill_name("foo-bar").unwrap(), "foo-bar");
    assert_eq!(canonical_skill_name("v1.2").unwrap(), "v1.2");
    assert_eq!(canonical_skill_name(&"x".repeat(300)).unwrap().len(), 255);
    assert!(canonical_skill_name("---").is_err());
    assert!(canonical_skill_name("..").is_err());
    assert!(canonical_skill_name("数据分析").is_err());
    assert!(canonical_skill_name("数据-tool").is_err());
    assert!(canonical_skill_name("con").is_err());
}

#[cfg(unix)]
#[test]
fn confined_copy_skips_escaping_links_and_follows_internal_ones() {
    let temp = tempfile::tempdir().unwrap();
    let secret = temp.path().join("outside/id_rsa");
    std::fs::create_dir_all(secret.parent().unwrap()).unwrap();
    std::fs::write(&secret, "PRIVATE KEY").unwrap();

    let checkout = temp.path().join("checkout");
    let skill = checkout.join("skills/foo");
    std::fs::create_dir_all(checkout.join("shared/assets")).unwrap();
    std::fs::write(checkout.join("shared/assets/logo.txt"), "logo").unwrap();
    std::fs::write(checkout.join("shared/common.md"), "common").unwrap();
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(skill.join("SKILL.md"), "---\nname: foo\n---\n").unwrap();
    std::os::unix::fs::symlink("../../../outside/id_rsa", skill.join("stolen")).unwrap();
    std::os::unix::fs::symlink(&secret, skill.join("stolen-abs")).unwrap();
    std::os::unix::fs::symlink("../../outside", skill.join("stolen-dir")).unwrap();
    std::os::unix::fs::symlink("../../shared/common.md", skill.join("common.md")).unwrap();
    std::os::unix::fs::symlink("../../shared/assets", skill.join("assets")).unwrap();
    std::os::unix::fs::symlink("missing", skill.join("broken")).unwrap();
    std::os::unix::fs::symlink(".", skill.join("loop")).unwrap();

    let dest = temp.path().join("dest");
    copy_confined(&skill, &dest, &checkout, &[]).unwrap();

    assert!(!dest.join("stolen").exists());
    assert!(!dest.join("stolen-abs").exists());
    assert!(!dest.join("stolen-dir").exists());
    assert!(!dest.join("broken").exists());
    assert!(!dest.join("loop").exists());
    assert_eq!(
        std::fs::read_to_string(dest.join("common.md")).unwrap(),
        "common"
    );
    assert_eq!(
        std::fs::read_to_string(dest.join("assets/logo.txt")).unwrap(),
        "logo"
    );
    assert!(!dest.join("assets").is_symlink(), "copies are real files");
}

#[test]
fn staged_replace_restores_the_previous_entry_when_dropped() {
    let temp = tempfile::tempdir().unwrap();
    let dest = temp.path().join("foo");
    std::fs::create_dir_all(&dest).unwrap();
    std::fs::write(dest.join("SKILL.md"), "old").unwrap();

    let mut staged = StagedReplace::stage(&dest, |staging| {
        std::fs::create_dir_all(staging)?;
        std::fs::write(staging.join("SKILL.md"), "new")?;
        Ok(())
    })
    .unwrap();
    staged.swap().unwrap();
    assert_eq!(
        std::fs::read_to_string(dest.join("SKILL.md")).unwrap(),
        "new"
    );
    drop(staged);

    assert_eq!(
        std::fs::read_to_string(dest.join("SKILL.md")).unwrap(),
        "old"
    );
    let leftovers: Vec<_> = std::fs::read_dir(temp.path())
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name())
        .filter(|name| name != "foo")
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn staged_replace_commit_keeps_new_content_without_leftovers() {
    let temp = tempfile::tempdir().unwrap();
    let dest = temp.path().join("foo");
    std::fs::create_dir_all(&dest).unwrap();
    std::fs::write(dest.join("SKILL.md"), "old").unwrap();
    let mut staged = StagedReplace::stage(&dest, |staging| {
        std::fs::create_dir_all(staging)?;
        std::fs::write(staging.join("SKILL.md"), "new")?;
        Ok(())
    })
    .unwrap();
    staged.swap().unwrap();
    staged.commit();
    assert_eq!(
        std::fs::read_to_string(dest.join("SKILL.md")).unwrap(),
        "new"
    );
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
}

#[test]
fn failed_fill_leaves_target_untouched() {
    let temp = tempfile::tempdir().unwrap();
    let dest = temp.path().join("foo");
    std::fs::create_dir_all(&dest).unwrap();
    std::fs::write(dest.join("SKILL.md"), "old").unwrap();
    let error = StagedReplace::stage(&dest, |staging| {
        std::fs::create_dir_all(staging)?;
        bail!("copy failed")
    });
    assert!(error.is_err());
    assert_eq!(
        std::fs::read_to_string(dest.join("SKILL.md")).unwrap(),
        "old"
    );
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
}

#[cfg(unix)]
#[test]
fn confined_copy_does_not_amplify_directory_links_and_keeps_real_directories() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("skill");
    std::fs::create_dir_all(root.join("real")).unwrap();
    std::fs::write(root.join("real/SKILL.md"), "body").unwrap();
    // Sorted before `real`, so the link is walked first.
    symlink("real", root.join("alink")).unwrap();
    let mut prev = root.join("real");
    for level in 0..12 {
        let dir = root.join(format!("l{level:02}"));
        std::fs::create_dir_all(&dir).unwrap();
        symlink(&prev, dir.join("a")).unwrap();
        symlink(&prev, dir.join("b")).unwrap();
        prev = dir;
    }

    let dest = temp.path().join("dest");
    copy_confined(&root, &dest, &root, &[".git"]).unwrap();
    assert_eq!(
        std::fs::read_to_string(dest.join("real/SKILL.md")).unwrap(),
        "body"
    );
    assert_eq!(
        std::fs::read_to_string(dest.join("alink/SKILL.md")).unwrap(),
        "body",
        "a link sorted before its target must not hide the real directory"
    );
    assert!(
        count_files(&dest) < 20,
        "directory links must not multiply the tree"
    );
}

#[cfg(unix)]
#[test]
fn confined_copy_skips_a_link_whose_resolved_path_enters_an_excluded_directory() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let checkout = temp.path().join("checkout");
    let skill = checkout.join("skills/foo");
    std::fs::create_dir_all(checkout.join(".git")).unwrap();
    std::fs::write(checkout.join(".git/config"), "secret-git").unwrap();
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(skill.join("SKILL.md"), "ok").unwrap();
    symlink("../../.git", skill.join("meta")).unwrap();

    let dest = temp.path().join("dest");
    copy_confined(&skill, &dest, &checkout, &[".git"]).unwrap();
    assert_eq!(
        std::fs::read_to_string(dest.join("SKILL.md")).unwrap(),
        "ok"
    );
    assert!(!dest.join("meta").exists());
    assert!(!tree_contains(&dest, "secret-git"));
}

#[test]
fn confined_copy_fails_closed_past_file_byte_and_depth_limits() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("skill");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("a.txt"), "aa").unwrap();
    std::fs::write(root.join("b.txt"), "bb").unwrap();
    let error = copy_confined_with_limits(
        &root,
        &temp.path().join("dest-files"),
        &root,
        &[],
        CopyLimits {
            max_files: 1,
            max_bytes: 1024,
            max_depth: 8,
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("too large"), "{error}");

    let wide = temp.path().join("wide");
    std::fs::create_dir_all(&wide).unwrap();
    std::fs::write(wide.join("blob"), "0123456789").unwrap();
    let error = copy_confined_with_limits(
        &wide,
        &temp.path().join("dest-bytes"),
        &wide,
        &[],
        CopyLimits {
            max_files: 10,
            max_bytes: 4,
            max_depth: 8,
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("too large"), "{error}");

    let deep_root = temp.path().join("deep");
    let mut deep = deep_root.clone();
    for name in ["a", "b", "c"] {
        deep = deep.join(name);
        std::fs::create_dir_all(&deep).unwrap();
    }
    std::fs::write(deep.join("SKILL.md"), "x").unwrap();
    let error = copy_confined_with_limits(
        &deep_root,
        &temp.path().join("dest-depth"),
        &deep_root,
        &[],
        CopyLimits {
            max_files: 10,
            max_bytes: 1024,
            max_depth: 1,
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("too large"), "{error}");
}

#[test]
fn sweep_keeps_a_backup_whose_target_is_gone_and_drops_an_old_stage() {
    let _sandbox = crate::test_sandbox::Sandbox::new();
    let root = ss_core::infra::paths::agents_skills_root();
    std::fs::create_dir_all(&root).unwrap();
    let skill = root.join("kept");
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(skill.join("SKILL.md"), "stay").unwrap();
    let marker = root.join(".skillstar-deploy.json");
    std::fs::write(&marker, "{}").unwrap();

    let present = root.join("present");
    std::fs::create_dir_all(&present).unwrap();
    std::fs::write(present.join("SKILL.md"), "live").unwrap();
    let old_stage = root.join(".skillstar-stage-present");
    std::fs::create_dir_all(&old_stage).unwrap();
    std::fs::write(old_stage.join("partial"), "old").unwrap();
    write_transient_born_at(
        &old_stage,
        std::time::SystemTime::now() - std::time::Duration::from_secs(31 * 60),
    );

    let fresh = root.join(".skillstar-stage-present-fresh");
    std::fs::create_dir_all(&fresh).unwrap();
    note_transient_born(&fresh);

    // Rename preserves mtime. This backup's directory clock is 31 minutes
    // old and its born stamp is too; the destination is gone, so it stays.
    let old_backup = root.join(".skillstar-backup-missing");
    std::fs::create_dir_all(&old_backup).unwrap();
    std::fs::write(old_backup.join("SKILL.md"), "only copy").unwrap();
    backdate(&old_backup);
    write_transient_born_at(
        &old_backup,
        std::time::SystemTime::now() - std::time::Duration::from_secs(31 * 60),
    );

    let swept = crate::skill_update::sweep_stale_transients().unwrap();
    assert!(
        swept.removed >= 1,
        "the old stage beside a live skill should go"
    );
    assert!(!old_stage.exists(), "stage older than 30 minutes is swept");
    assert!(
        old_backup.join("SKILL.md").is_file(),
        "backup of a missing target survives an old mtime"
    );
    assert!(fresh.is_dir(), "a younger leftover stays");
    assert!(skill.join("SKILL.md").is_file());
    assert!(present.join("SKILL.md").is_file());
    assert!(marker.is_file(), "a deploy marker is not staging residue");
}

#[test]
fn rename_retries_a_transient_failure_and_stops_when_the_source_is_missing() {
    let mut tries = 0;
    retry_attempts(&[0, 0, 0], || {
        tries += 1;
        if tries < 3 {
            Err(std::io::Error::other("busy"))
        } else {
            Ok(())
        }
    })
    .unwrap();
    assert_eq!(tries, 3);

    let error = retry_attempts(&[0, 0], || {
        Err(std::io::Error::new(std::io::ErrorKind::NotFound, "gone"))
    })
    .unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
}

fn backdate(path: &std::path::Path) {
    let file = std::fs::File::open(path).unwrap();
    let times = std::fs::FileTimes::new()
        .set_modified(std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(10));
    file.set_times(times).unwrap();
}

fn count_files(dir: &std::path::Path) -> u64 {
    let mut count = 0;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && !path.is_symlink() {
            count += count_files(&path);
        } else if path.is_file() {
            count += 1;
        }
    }
    count
}

fn tree_contains(dir: &std::path::Path, needle: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && !path.is_symlink() && tree_contains(&path, needle) {
            return true;
        }
        if path.is_file()
            && std::fs::read_to_string(&path)
                .ok()
                .is_some_and(|text| text.contains(needle))
        {
            return true;
        }
    }
    false
}
