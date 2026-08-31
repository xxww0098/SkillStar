use super::*;
use std::fs;
use std::process::Command;

fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn manifest(name: &str, description: &str, body: &str) -> String {
    format!("---\nname: {name}\ndescription: {description}\n---\n# {name}\n\n{body}")
}

fn long_body(prefix: &str) -> String {
    (1..=12)
        .map(|step| format!("{prefix} step {step}: do the thing carefully and check it.\n"))
        .collect()
}

fn write_skill(repo: &Path, folder: &str, content: &str) {
    let dir = repo.join(folder);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("SKILL.md"), content).unwrap();
}

fn lock_entry(name: &str, folder: &str) -> LockEntry {
    LockEntry {
        name: name.into(),
        git_url: "https://github.com/acme/demo.git".into(),
        git_ref: None,
        tree_hash: "tree".into(),
        content_hash: None,
        content_hash_version: None,
        installed_at: "2026-08-21T00:00:00Z".into(),
        source_folder: Some(folder.into()),
    }
}

#[test]
fn a_folder_gone_at_the_tracked_ref_is_removed_and_its_move_is_found() {
    let _guard = crate::lock_test_env();
    let temp = tempfile::tempdir().unwrap();
    let previous_data = std::env::var_os("SKILLSTAR_DATA_DIR");
    let previous_hub = std::env::var_os("SKILLSTAR_HUB_DIR");
    unsafe {
        std::env::set_var("SKILLSTAR_DATA_DIR", temp.path().join("data"));
        std::env::set_var("SKILLSTAR_HUB_DIR", temp.path().join("hub"));
    }
    crate::update_state::reset_for_test();

    let result = (|| -> anyhow::Result<()> {
        let remote = temp.path().join("remote");
        fs::create_dir_all(&remote)?;
        git(&remote, &["init", "-q", "--initial-branch=main"]);
        git(&remote, &["config", "user.email", "test@example.com"]);
        git(&remote, &["config", "user.name", "SkillStar Tests"]);
        write_skill(
            &remote,
            "skills/one",
            &manifest("one", "First skill", &long_body("one")),
        );
        write_skill(
            &remote,
            "skills/keep",
            &manifest("keep", "Kept skill", &long_body("keep")),
        );
        write_skill(
            &remote,
            "skills/three",
            &manifest("three", "Third", &long_body("three")),
        );
        git(&remote, &["add", "."]);
        git(&remote, &["commit", "-q", "-m", "initial"]);

        let cache = skillstar_core::infra::paths::repos_cache_dir();
        fs::create_dir_all(&cache)?;
        let repo_dir = cache.join("acme--demo");
        git(
            &cache,
            &[
                "clone",
                "-q",
                remote.to_str().unwrap(),
                repo_dir.to_str().unwrap(),
            ],
        );

        let hub = skillstar_core::infra::paths::hub_skills_dir();
        fs::create_dir_all(&hub)?;
        let mut lock = lockfile::Lockfile::default();
        for (name, folder) in [
            ("one", "skills/one"),
            ("keep", "skills/keep"),
            ("three", "skills/three"),
        ] {
            skillstar_core::infra::fs_ops::create_symlink(&repo_dir.join(folder), &hub.join(name))?;
            lock.upsert(lock_entry(name, folder));
        }
        lock.save(&lockfile::lockfile_path())?;

        let session = GitOperationSession::public();
        let none = HashSet::new();
        let check = |name: &str, failed: &HashSet<PathBuf>| {
            check_upstream_status(&hub.join(name), failed, None, &session)
        };

        assert_eq!(check("one", &none), Some(UpstreamStatus::Current));

        // Upstream: `one` moves into a bucket, gets a new name and a small
        // edit; `keep` is deleted outright; `three` is rewritten under a
        // new folder but keeps its frontmatter name.
        // `git mv` needs the destination parent to exist already.
        fs::create_dir_all(remote.join("skills/engineering"))?;
        fs::create_dir_all(remote.join("skills/misc"))?;
        git(
            &remote,
            &["mv", "skills/one", "skills/engineering/one-spec"],
        );
        fs::write(
            remote.join("skills/engineering/one-spec/SKILL.md"),
            manifest("one-spec", "First skill", &long_body("one")),
        )?;
        git(&remote, &["rm", "-r", "-q", "skills/keep"]);
        git(&remote, &["mv", "skills/three", "skills/misc/three-new"]);
        fs::write(
            remote.join("skills/misc/three-new/SKILL.md"),
            manifest(
                "three",
                "Third, rewritten",
                "Completely different text now.\n",
            ),
        )?;
        git(&remote, &["add", "-A"]);
        git(&remote, &["commit", "-q", "-m", "reshuffle"]);

        // Nothing fetched yet: the tracked ref has not moved.
        assert_eq!(check("one", &none), Some(UpstreamStatus::Current));

        git(&repo_dir, &["fetch", "-q"]);

        match check("one", &none) {
            Some(UpstreamStatus::Removed(UpstreamChange::Removed {
                suggested_local_name,
                successor: Some(successor),
            })) => {
                assert_eq!(suggested_local_name, "one.local");
                assert_eq!(successor.folder_path, "skills/engineering/one-spec");
                assert_eq!(successor.skill_id, "one-spec");
                assert_eq!(successor.description, "First skill");
                let similarity = successor.similarity.expect("git scored the rename");
                assert!((50..100).contains(&similarity), "similarity {similarity}");
            }
            other => panic!("expected a rename with a successor, got {other:?}"),
        }
        match check("keep", &none) {
            Some(UpstreamStatus::Removed(UpstreamChange::Removed {
                successor: None, ..
            })) => {}
            other => panic!("expected a plain removal, got {other:?}"),
        }
        match check("three", &none) {
            Some(UpstreamStatus::Removed(UpstreamChange::Removed {
                successor: Some(successor),
                ..
            })) => {
                assert_eq!(successor.folder_path, "skills/misc/three-new");
                assert_eq!(successor.similarity, None, "matched by frontmatter name");
            }
            other => panic!("expected a name-matched successor, got {other:?}"),
        }

        // A failed fetch for this checkout keeps the verdict unknown.
        let failed: HashSet<PathBuf> = [repo_link::repo_root_of(&hub.join("one")).unwrap()]
            .into_iter()
            .collect();
        assert_eq!(check("one", &failed), None);

        // The checkout was only read, never moved.
        assert!(repo_dir.join("skills/one/SKILL.md").exists());
        assert!(!repo_dir.join("skills/engineering").exists());
        Ok(())
    })();

    crate::update_state::reset_for_test();
    unsafe {
        match previous_data {
            Some(value) => std::env::set_var("SKILLSTAR_DATA_DIR", value),
            None => std::env::remove_var("SKILLSTAR_DATA_DIR"),
        }
        match previous_hub {
            Some(value) => std::env::set_var("SKILLSTAR_HUB_DIR", value),
            None => std::env::remove_var("SKILLSTAR_HUB_DIR"),
        }
    }
    result.unwrap();
}
