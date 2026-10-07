use super::*;
use crate::git::transport::{GitAuthMaterial, NoopGitProgressSink};
use crate::skill_lock::SourceType;

fn session() -> GitOperationSession {
    GitOperationSession::new(
        "update-test",
        GitAuthMaterial::missing(),
        Arc::new(NoopGitProgressSink),
    )
}

/// A lock entry modeling a git-backed install. Tests drive the clone
/// fallback with `file://` fixture repos while claiming GitHub provenance,
/// exactly what a real lock would record.
fn entry(url: &str, path: &str, hash: Option<String>) -> SkillLockEntry {
    SkillLockEntry {
        source: "o/r".into(),
        source_type: SourceType::Github,
        source_url: url.into(),
        git_ref: None,
        skill_path: if path.is_empty() {
            None
        } else {
            Some(path.into())
        },
        skill_folder_hash: hash,
        installed_at: String::new(),
        updated_at: String::new(),
        extra: Default::default(),
    }
}

/// A local git repo standing in for an upstream source, addressable via
/// `file://` so `fetch_source` can clone it.
struct UpstreamFixture {
    dir: tempfile::TempDir,
    url: String,
}

impl UpstreamFixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        Self::git(dir.path(), &["init", "--quiet"]);
        Self::commit(
            dir.path(),
            "skills/foo",
            "---\nname: foo\ndescription: v1\n---\n",
        );
        Self {
            url: git_upstream_url(dir.path()),
            dir,
        }
    }

    fn bump(&self) {
        Self::commit(
            self.dir.path(),
            "skills/foo",
            "---\nname: foo\ndescription: v2\n---\n",
        );
    }

    fn drop_skill(&self) {
        let remove = ss_core::infra::path_env::command_with_path("git")
            .args(["rm", "-rq", "skills/foo"])
            .current_dir(&self.dir)
            .status()
            .unwrap();
        assert!(remove.success());
        Self::git(
            self.dir.path(),
            &["commit", "--quiet", "-m", "drop", "--no-gpg-sign"],
        );
    }

    fn commit(dir: &std::path::Path, rel: &str, skill_md: &str) {
        let target = dir.join(rel);
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("SKILL.md"), skill_md).unwrap();
        Self::git(dir, &["add", "."]);
        Self::git(dir, &["commit", "--quiet", "-m", "init", "--no-gpg-sign"]);
    }

    fn git(dir: &std::path::Path, args: &[&str]) {
        let status = ss_core::infra::path_env::command_with_path("git")
            .args(args)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .env("GIT_COMMITTER_DATE", "1759500000 +0000")
            .env("GIT_AUTHOR_DATE", "1759500000 +0000")
            .current_dir(dir)
            .status()
            .unwrap();
        assert!(status.success(), "git {:?} failed", args);
    }
}

fn git_upstream_url(dir: &std::path::Path) -> String {
    crate::git::ops::local_file_url(dir)
}

#[tokio::test]
async fn check_reports_changed_and_removed_via_clone_fallback() {
    let _guard = crate::lock_test_env_async();
    let sandbox = tempfile::tempdir().unwrap();
    let previous = std::env::var_os("SKILLSTAR_DATA_DIR");
    unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", sandbox.path().join("data")) };

    let upstream = UpstreamFixture::new();
    // Clone once locally to compute the v1 hash the lock would store.
    let spec = Source::parse(&upstream.url).unwrap();
    let probe = fetch::fetch_source(&spec, &GitOperationSession::public()).unwrap();
    let v1 = fetch::folder_tree_hash(probe.dir(), Some("skills/foo")).unwrap();

    let entries = vec![(
        "foo".to_string(),
        entry(&upstream.url, "skills/foo", Some(v1.clone())),
    )];
    let verdicts = check_upstream(&entries, None, &session()).await;
    assert_eq!(verdicts["foo"], Upstream::Hash(v1.clone()));

    upstream.bump();
    let verdicts = check_upstream(&entries, None, &session()).await;
    assert_ne!(verdicts["foo"], Upstream::Hash(v1));

    upstream.drop_skill();
    let verdicts = check_upstream(&entries, None, &session()).await;
    assert_eq!(verdicts["foo"], Upstream::Removed);

    unsafe {
        match previous {
            Some(value) => std::env::set_var("SKILLSTAR_DATA_DIR", value),
            None => std::env::remove_var("SKILLSTAR_DATA_DIR"),
        }
    }
}

#[test]
fn apply_reinstalls_and_updates_lock_hash() {
    let _guard = crate::lock_test_env();
    let sandbox = tempfile::tempdir().unwrap();
    let previous_data = std::env::var_os("SKILLSTAR_DATA_DIR");
    let previous_hub = std::env::var_os("SKILLSTAR_HUB_DIR");
    unsafe {
        std::env::set_var("SKILLSTAR_DATA_DIR", sandbox.path().join("data"));
        std::env::set_var("SKILLSTAR_HUB_DIR", sandbox.path().join("hub"));
    }

    let upstream = UpstreamFixture::new();
    // Initial install through the real pipeline (v1).
    let spec = Source::parse(&upstream.url).unwrap();
    {
        let checkout = fetch::fetch_source(&spec, &session()).unwrap();
        installer::install_units(
            checkout.dir(),
            &spec,
            &[InstallUnit {
                id: "foo".into(),
                folder_path: "skills/foo".into(),
            }],
        )
        .unwrap();
    }
    // The file:// fixture drives the same clone path a GitHub source
    // takes; model the provenance a real GitHub install would record.
    skill_lock::mutate(|lock| {
        if let Some(entry) = lock.skills.get_mut("foo") {
            entry.source_type = SourceType::Github;
        }
    })
    .unwrap();
    let old_hash = skill_lock::load().skills["foo"].skill_folder_hash.clone();

    upstream.bump();
    let results = apply_updates(&["foo".to_string()], &session());
    assert!(
        matches!(&results[0].result, UpdateResult::Updated { .. }),
        "{:?}",
        results[0].result
    );
    let canonical = ss_core::infra::paths::agents_skill_dir("foo");
    let content = std::fs::read_to_string(canonical.join("SKILL.md")).unwrap();
    assert!(content.contains("v2"), "{content}");
    let new_hash = skill_lock::load().skills["foo"].skill_folder_hash.clone();
    assert_ne!(old_hash, new_hash);

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
}

#[test]
fn apply_reports_removed_upstream() {
    let _guard = crate::lock_test_env();
    let sandbox = tempfile::tempdir().unwrap();
    let previous_data = std::env::var_os("SKILLSTAR_DATA_DIR");
    unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", sandbox.path().join("data")) };

    let upstream = UpstreamFixture::new();
    let mut lock = skill_lock::SkillLock::default();
    lock.upsert(
        "foo",
        entry(&upstream.url, "skills/foo", Some("deadbeef".into())),
    );
    lock.save(&skill_lock::lock_path()).unwrap();

    upstream.drop_skill();
    let results = apply_updates(&["foo".to_string()], &session());
    assert_eq!(results[0].result, UpdateResult::Removed);

    unsafe {
        match previous_data {
            Some(value) => std::env::set_var("SKILLSTAR_DATA_DIR", value),
            None => std::env::remove_var("SKILLSTAR_DATA_DIR"),
        }
    }
}

/// Production layout: upstream renames the frontmatter `name`. The update
/// must not install a second identity nor overwrite the locked folder.
#[test]
fn upstream_rename_reports_identity_change_without_installing() {
    let _sandbox = crate::test_sandbox::Sandbox::production();
    let upstream = UpstreamFixture::new();
    let spec = Source::parse(&upstream.url).unwrap();
    {
        let checkout = fetch::fetch_source(&spec, &session()).unwrap();
        installer::install_units(
            checkout.dir(),
            &spec,
            &[InstallUnit {
                id: "foo".into(),
                folder_path: "skills/foo".into(),
            }],
        )
        .unwrap();
    }
    skill_lock::mutate(|lock| {
        if let Some(entry) = lock.skills.get_mut("foo") {
            entry.source_type = SourceType::Github;
        }
    })
    .unwrap();

    UpstreamFixture::commit(
        upstream.dir.path(),
        "skills/foo",
        "---\nname: foo-renamed\ndescription: v2\n---\n",
    );
    crate::update_state::reset_for_test();
    let report = crate::git_skill::GitSkillFacade::new(session()).update_skills(&["foo".into()]);
    assert_eq!(report.identity_changed.len(), 1, "{report:?}");
    assert_eq!(report.identity_changed[0].upstream_name, "foo-renamed");
    assert!(report.updated.is_empty(), "{report:?}");
    assert_eq!(
        crate::update_state::upstream_change("foo"),
        Some(crate::update_state::UpstreamChange::IdentityChanged {
            upstream_name: "foo-renamed".into()
        })
    );
    let snapshot = std::fs::read_to_string(
        ss_core::infra::paths::state_dir().join("skill_update_states.json"),
    )
    .unwrap();
    assert!(
        snapshot.contains("identity_changed") && snapshot.contains("foo-renamed"),
        "{snapshot}"
    );
    let canonical = ss_core::infra::paths::agents_skill_dir("foo");
    let content = std::fs::read_to_string(canonical.join("SKILL.md")).unwrap();
    assert!(content.contains("v1"), "{content}");
    assert!(!ss_core::infra::paths::agents_skill_dir("foo-renamed").exists());
    assert_eq!(
        skill_lock::load().skills.keys().collect::<Vec<_>>(),
        ["foo"]
    );
}

#[test]
fn missing_lock_entry_fails_with_reason() {
    let results = apply_updates(&["nope".to_string()], &session());
    assert!(matches!(&results[0].result, UpdateResult::Failed(reason) if reason.contains("nope")));
}

/// The background monitor must reuse the manual pair end to end: an
/// unchanged upstream is a no-op, a changed one is reinstalled, and the
/// applied update leaves no badge behind.
#[tokio::test]
async fn auto_update_checks_then_applies_changed_skills() {
    let _guard = crate::lock_test_env_async();
    let sandbox = tempfile::tempdir().unwrap();
    let previous_data = std::env::var_os("SKILLSTAR_DATA_DIR");
    let previous_hub = std::env::var_os("SKILLSTAR_HUB_DIR");
    unsafe {
        std::env::set_var("SKILLSTAR_DATA_DIR", sandbox.path().join("data"));
        std::env::set_var("SKILLSTAR_HUB_DIR", sandbox.path().join("hub"));
    }
    crate::update_state::reset_for_test();

    let upstream = UpstreamFixture::new();
    let spec = Source::parse(&upstream.url).unwrap();
    {
        let checkout = fetch::fetch_source(&spec, &session()).unwrap();
        installer::install_units(
            checkout.dir(),
            &spec,
            &[InstallUnit {
                id: "foo".into(),
                folder_path: "skills/foo".into(),
            }],
        )
        .unwrap();
    }
    // The file:// fixture drives the same clone path a GitHub source
    // takes; model the provenance a real GitHub install would record.
    skill_lock::mutate(|lock| {
        if let Some(entry) = lock.skills.get_mut("foo") {
            entry.source_type = SourceType::Github;
        }
    })
    .unwrap();

    let report = auto_update_locked_skills(&session()).await;
    assert_eq!(report.checked, 1);
    assert!(report.updated.is_empty(), "{report:?}");
    assert!(report.error.is_none(), "{report:?}");

    upstream.bump();
    let report = auto_update_locked_skills(&session()).await;
    assert_eq!(report.checked, 1);
    assert_eq!(report.updated, vec!["foo".to_string()], "{report:?}");
    assert!(report.failed.is_empty(), "{report:?}");
    assert!(report.error.is_none(), "{report:?}");

    let canonical = ss_core::infra::paths::agents_skill_dir("foo");
    let content = std::fs::read_to_string(canonical.join("SKILL.md")).unwrap();
    assert!(content.contains("v2"), "{content}");
    assert_eq!(
        crate::update_state::get("foo"),
        Some(false),
        "an applied update is authoritative and clears the badge"
    );

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
}

fn install_foo(upstream: &UpstreamFixture) {
    let spec = Source::parse(&upstream.url).unwrap();
    let checkout = fetch::fetch_source(&spec, &session()).unwrap();
    installer::install_units(
        checkout.dir(),
        &spec,
        &[InstallUnit {
            id: "foo".into(),
            folder_path: "skills/foo".into(),
        }],
    )
    .unwrap();
    // The file:// fixture drives the same clone path a GitHub source takes;
    // model the provenance a real GitHub install would record.
    skill_lock::mutate(|lock| {
        if let Some(entry) = lock.skills.get_mut("foo") {
            entry.source_type = SourceType::Github;
        }
    })
    .unwrap();
}

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future)
}

/// The background monitor must not overwrite a Skill edited after install;
/// the skip is visible in the update projection, and a manual update still
/// applies (and re-baselines) it.
#[test]
fn auto_update_keeps_locally_edited_skills() {
    let _sandbox = crate::test_sandbox::Sandbox::new();
    let upstream = UpstreamFixture::new();
    install_foo(&upstream);
    let canonical = ss_core::infra::paths::agents_skill_dir("foo");
    std::fs::write(canonical.join("NOTES.md"), "mine").unwrap();

    upstream.bump();
    let report = block_on(auto_update_locked_skills(&session()));
    assert!(report.updated.is_empty(), "{report:?}");
    assert_eq!(report.kept_local, vec!["foo".to_string()]);
    assert_eq!(
        std::fs::read_to_string(canonical.join("NOTES.md")).unwrap(),
        "mine"
    );
    assert_eq!(crate::update_state::get("foo"), Some(true));
    assert_eq!(
        crate::update_state::upstream_change("foo"),
        Some(crate::update_state::UpstreamChange::LocalChanges {
            baseline_missing: false
        })
    );

    let manual = crate::git_skill::GitSkillFacade::new(session()).update_skills(&["foo".into()]);
    assert_eq!(manual.updated.len(), 1, "{manual:?}");
    assert_eq!(
        crate::install_baseline::local_content("foo"),
        crate::install_baseline::LocalContent::Unchanged
    );
}

/// Removal found by a check or by an update attempt must reach the projection
/// the list reads, not only the return value.
#[test]
fn removed_upstream_is_persisted_by_check_and_by_update() {
    let _sandbox = crate::test_sandbox::Sandbox::new();
    let upstream = UpstreamFixture::new();
    install_foo(&upstream);
    upstream.drop_skill();

    block_on(crate::installed_skill::refresh_skill_updates_in_session(
        &session(),
    ))
    .unwrap();
    assert!(matches!(
        crate::update_state::upstream_change("foo"),
        Some(crate::update_state::UpstreamChange::Removed { .. })
    ));

    crate::update_state::reset_for_test();
    let report = crate::git_skill::GitSkillFacade::new(session()).update_skills(&["foo".into()]);
    assert_eq!(report.skipped, vec!["foo".to_string()]);
    let listed = block_on(crate::installed_skill::list_installed_skills()).unwrap();
    let foo = listed.iter().find(|skill| skill.name == "foo").unwrap();
    assert!(matches!(
        foo.upstream_change,
        Some(crate::update_state::UpstreamChange::Removed { .. })
    ));
}

/// Local creations are not failures of a batch update.
#[test]
fn local_sources_are_reported_as_not_updatable() {
    let _sandbox = crate::test_sandbox::Sandbox::new();
    let mut local = entry("", "", None);
    local.source_type = SourceType::Local;
    let mut lock = skill_lock::SkillLock::default();
    lock.upsert("mine", local);
    lock.save(&skill_lock::lock_path()).unwrap();

    let report = crate::git_skill::GitSkillFacade::new(session()).update_skills(&["mine".into()]);
    assert_eq!(report.not_updatable, vec!["mine".to_string()]);
    assert!(report.failed.is_empty(), "{:?}", report.failed);
}

/// Skills sharing a source are refreshed from one checkout.
#[test]
fn skills_from_one_source_update_together() {
    let _sandbox = crate::test_sandbox::Sandbox::new();
    let upstream = UpstreamFixture::new();
    UpstreamFixture::commit(
        upstream.dir.path(),
        "skills/bar",
        "---\nname: bar\ndescription: v1\n---\n",
    );
    let spec = Source::parse(&upstream.url).unwrap();
    let checkout = fetch::fetch_source(&spec, &session()).unwrap();
    installer::install_units(
        checkout.dir(),
        &spec,
        &[
            InstallUnit {
                id: "foo".into(),
                folder_path: "skills/foo".into(),
            },
            InstallUnit {
                id: "bar".into(),
                folder_path: "skills/bar".into(),
            },
        ],
    )
    .unwrap();
    drop(checkout);
    skill_lock::mutate(|lock| {
        for entry in lock.skills.values_mut() {
            entry.source_type = SourceType::Github;
        }
    })
    .unwrap();
    upstream.bump();

    let results = apply_updates(&["foo".to_string(), "bar".to_string()], &session());
    assert_eq!(results[0].name, "foo");
    assert!(
        matches!(results[0].result, UpdateResult::Updated { .. }),
        "{results:?}"
    );
    // `bar` did not change upstream; it is reinstalled from the same checkout.
    assert!(
        matches!(results[1].result, UpdateResult::Updated { .. }),
        "{results:?}"
    );
}

/// A vercel lock keys the entry by the raw frontmatter name. Updating the
/// canonical folder must find that entry, keep unknown fields and the first
/// `installedAt`, and write the pin back as `ref` (not the old `gitRef` name).
#[test]
fn directory_update_keeps_extra_fields_from_a_vercel_key() {
    let sandbox = crate::test_sandbox::Sandbox::new();
    let repo = sandbox.root().join("origin");
    let skill = repo.join("skills/my-skill");
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(
        skill.join("SKILL.md"),
        "---\nname: My Skill\ndescription: v1\n---\n",
    )
    .unwrap();
    git_in(&repo, &["init", "-q"]);
    git_in(&repo, &["add", "."]);
    git_in(&repo, &["commit", "-q", "-m", "v1", "--no-gpg-sign"]);
    let branch = git_in(&repo, &["branch", "--show-current"]);
    let url = crate::git::ops::local_file_url(&repo);

    let lock_path = skill_lock::lock_path();
    std::fs::create_dir_all(lock_path.parent().unwrap()).unwrap();
    let lock_body = format!(
        r#"{{"version":3,"skills":{{"My Skill":{{"source":"local/demo","sourceType":"github","sourceUrl":{url:?},"gitRef":{branch:?},"skillPath":"skills/my-skill","skillFolderHash":"deadbeef","installedAt":"2020-01-01T00:00:00Z","updatedAt":"2020-01-01T00:00:00Z","pluginName":"plug"}}}}}}"#
    );
    std::fs::write(&lock_path, lock_body).unwrap();

    std::fs::write(
        skill.join("SKILL.md"),
        "---\nname: My Skill\ndescription: v2\n---\n",
    )
    .unwrap();
    git_in(&repo, &["add", "."]);
    git_in(&repo, &["commit", "-q", "-m", "v2", "--no-gpg-sign"]);

    let results = apply_updates(&["my-skill".to_string()], &session());
    assert!(
        matches!(results[0].result, UpdateResult::Updated { .. }),
        "{:?}",
        results[0].result
    );
    let canonical = ss_core::infra::paths::agents_skill_dir("my-skill");
    let content = std::fs::read_to_string(canonical.join("SKILL.md")).unwrap();
    assert!(content.contains("v2"), "{content}");

    let written: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(lock_path).unwrap()).unwrap();
    assert!(written["skills"].get("My Skill").is_none());
    let entry = &written["skills"]["my-skill"];
    assert_eq!(entry["pluginName"], "plug");
    assert_eq!(entry["installedAt"], "2020-01-01T00:00:00Z");
    assert_eq!(entry["ref"], branch);
    assert!(entry.get("gitRef").is_none(), "{entry}");
}

fn git_in(dir: &std::path::Path, args: &[&str]) -> String {
    let output = ss_core::infra::path_env::command_with_path("git")
        .args(args)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}
