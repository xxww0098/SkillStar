use super::*;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

struct InstallSandbox {
    previous: Vec<(&'static str, Option<OsString>)>,
    _temp: tempfile::TempDir,
}

impl InstallSandbox {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let overrides = [
            ("SKILLSTAR_HUB_DIR", temp.path().join("hub")),
            ("SKILLSTAR_DATA_DIR", temp.path().join("data")),
            ("SKILLSTAR_TOOL_SYNC_HOME", temp.path().join("tool-home")),
            ("HOME", temp.path().join("home")),
            ("USERPROFILE", temp.path().join("home")),
        ];
        let previous = overrides
            .iter()
            .map(|(key, _)| (*key, std::env::var_os(key)))
            .collect();
        unsafe {
            for (key, value) in overrides {
                std::env::set_var(key, value);
            }
        }
        Self {
            previous,
            _temp: temp,
        }
    }
}

impl Drop for InstallSandbox {
    fn drop(&mut self) {
        unsafe {
            for (key, previous) in self.previous.drain(..).rev() {
                match previous {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }
}

fn git(repo: &Path, args: &[&str]) -> String {
    let output = ss_core::infra::path_env::command_with_path("git")
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

fn install_fixture() -> (InstallSandbox, ChannelInstallReceipt, PathBuf, PathBuf) {
    let sandbox = InstallSandbox::new();
    let repo = ss_core::infra::paths::repos_cache_dir().join("acme--channel");
    let source = repo.join("skills/writer");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(
        source.join("SKILL.md"),
        "---\nname: writer\ndescription: Writer\n---\n# Writer\n",
    )
    .unwrap();
    git(&repo, &["init"]);
    git(&repo, &["config", "user.email", "tests@skillstar.local"]);
    git(&repo, &["config", "user.name", "SkillStar Tests"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "initial"]);
    let head = git(&repo, &["rev-parse", "HEAD"]);

    // D-081: installed channel skills are real canonical copies recorded in
    // the vercel lock — no hub symlink into a checkout.
    let hub_skill = ss_core::infra::paths::agents_skill_dir("writer");
    std::fs::create_dir_all(&hub_skill).unwrap();
    std::fs::copy(source.join("SKILL.md"), hub_skill.join("SKILL.md")).unwrap();
    let hash = crate::content::snapshot("writer").unwrap().content_hash;
    let mut lock = crate::skill_lock::SkillLock::default();
    lock.upsert(
        "writer",
        crate::skill_lock::SkillLockEntry {
            source: "acme/channel".into(),
            source_type: crate::skill_lock::SourceType::Github,
            source_url: "https://github.com/acme/channel.git".into(),
            git_ref: Some(head.clone()),
            skill_path: Some("skills/writer".into()),
            skill_folder_hash: None,
            installed_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            extra: Default::default(),
        },
    );
    lock.save(&crate::skill_lock::lock_path()).unwrap();

    let receipt = ChannelInstallReceipt {
        skills: vec![ChannelSubscribedSkill {
            id: "writer".into(),
            content_root: "skills/writer".into(),
            release_content_hash: hash.clone(),
            release_content_hash_version: CHANNEL_CONTENT_HASH_VERSION,
            baseline_hash: hash,
            baseline_hash_version: CHANNEL_CONTENT_HASH_VERSION,
            provenance: ChannelSkillProvenance {
                repository_id: 42,
                repository_url: "https://github.com/acme/channel.git".into(),
                git_ref: head,
                source_folder: "skills/writer".into(),
            },
        }],
        newly_installed_skill_ids: vec!["writer".into()],
    };
    (sandbox, receipt, repo, hub_skill)
}

#[tokio::test]
async fn metadata_failure_preserves_content_edited_during_commit() {
    let _guard = crate::lock_test_env_async();
    let (_sandbox, receipt, _repo, hub_skill) = install_fixture();
    let installer =
        GitChannelSubscriptionInstaller::new(crate::git_skill::GitSkillFacade::from_file_store());
    let edit_path = hub_skill.join("SKILL.md");

    let error = installer
        .verify_and_commit_install(
            &receipt,
            Box::new(move || {
                std::fs::write(
                    edit_path,
                    "---\nname: writer\ndescription: Writer\n---\n# Locally edited\n",
                )
                .unwrap();
                Err(SharedChannelError::new(
                    SharedChannelErrorCode::Storage,
                    "subscription save failed",
                ))
            }),
        )
        .await
        .unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::Storage);
    assert!(hub_skill.join("SKILL.md").is_file());
    assert!(
        std::fs::read_to_string(hub_skill.join("SKILL.md"))
            .unwrap()
            .contains("Locally edited")
    );
    assert!(crate::skill_lock::load().skills.contains_key("writer"));
}

#[tokio::test]
async fn metadata_failure_rolls_back_unchanged_install_without_relocking() {
    let _guard = crate::lock_test_env_async();
    let (_sandbox, receipt, _repo, hub_skill) = install_fixture();
    let installer =
        GitChannelSubscriptionInstaller::new(crate::git_skill::GitSkillFacade::from_file_store());

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        installer.verify_and_commit_install(
            &receipt,
            Box::new(|| {
                Err(SharedChannelError::new(
                    SharedChannelErrorCode::Storage,
                    "subscription save failed",
                ))
            }),
        ),
    )
    .await
    .expect("rollback must not deadlock on the update transaction mutex")
    .unwrap_err();

    assert_eq!(result.code, SharedChannelErrorCode::Storage);
    assert!(!hub_skill.exists());
    assert!(!crate::skill_lock::load().skills.contains_key("writer"));
}

/// D-081: there is no shared checkout to move; the equivalent invariant is
/// that the canonical content changed between install and commit.
#[tokio::test]
async fn final_verification_rejects_edited_canonical_content() {
    let _guard = crate::lock_test_env_async();
    let (_sandbox, receipt, _repo, hub_skill) = install_fixture();
    std::fs::write(
        hub_skill.join("SKILL.md"),
        "---\nname: writer\ndescription: Writer\n---\n# Edited before commit\n",
    )
    .unwrap();
    let committed = Arc::new(AtomicBool::new(false));
    let commit_called = committed.clone();
    let installer =
        GitChannelSubscriptionInstaller::new(crate::git_skill::GitSkillFacade::from_file_store());

    let error = installer
        .verify_and_commit_install(
            &receipt,
            Box::new(move || {
                commit_called.store(true, Ordering::SeqCst);
                Ok(())
            }),
        )
        .await
        .unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::Integrity);
    assert!(!committed.load(Ordering::SeqCst));
    assert!(hub_skill.join("SKILL.md").is_file());
}

#[tokio::test]
async fn production_installer_verifies_the_exact_release_checkout() {
    let _guard = crate::lock_test_env_async();
    let (_sandbox, _receipt, _installed_repo, _hub_skill) = install_fixture();
    let origin = ss_core::infra::paths::hub_root().join("verifier-origin");
    let skill_root = origin.join("skills/writer");
    std::fs::create_dir_all(&skill_root).unwrap();
    std::fs::write(
        skill_root.join("SKILL.md"),
        "---\nname: writer\ndescription: Writer\n---\n# Verified\n",
    )
    .unwrap();
    git(&origin, &["init", "-q"]);
    git(&origin, &["config", "user.email", "tests@skillstar.local"]);
    git(&origin, &["config", "user.name", "SkillStar Tests"]);
    git(&origin, &["config", "core.autocrlf", "false"]);
    git(&origin, &["config", "core.eol", "lf"]);
    std::fs::write(origin.join(".gitattributes"), "* -text\n").unwrap();
    git(&origin, &["add", "."]);
    git(&origin, &["commit", "-m", "verified release"]);
    let commit = git(&origin, &["rev-parse", "HEAD"]);
    let hash = crate::content::snapshot_path("writer", &skill_root)
        .unwrap()
        .content_hash;
    // D-081: verification fetches an isolated temp clone; seeding a
    // persistent cache entry is no longer part of the contract.

    let repository = RemoteRepository {
        id: 42,
        owner_id: 7,
        owner_login: "acme".into(),
        owner_type: "Organization".into(),
        name: "channel".into(),
        default_branch: "main".into(),
        html_url: "https://github.com/acme/channel".into(),
        clone_url: format!(
            "file://{}",
            std::fs::canonicalize(&origin).unwrap().display()
        ),
        private: true,
        permissions: super::RepositoryPermissions {
            admin: false,
            maintain: false,
            push: false,
            pull: true,
        },
    };
    let manifest = ChannelReleaseManifest {
        schema_version: CHANNEL_RELEASE_MANIFEST_VERSION,
        repository_id: 42,
        organization_id: 7,
        revision: 1,
        tag_name: super::revision_tag(1),
        commit_sha: commit,
        publisher: super::ChannelPublisherIdentity {
            id: 9,
            login: "alice".into(),
        },
        published_at: "2026-08-05T00:00:00Z".into(),
        title: "Release one".into(),
        notes: "Verified".into(),
        skills: vec![super::ChannelReleaseSkill {
            id: "writer".into(),
            content_root: "skills/writer".into(),
            content_hash: hash,
            content_hash_version: CHANNEL_CONTENT_HASH_VERSION,
            status: super::ChannelSkillReleaseStatus::Added,
        }],
    };
    let installer =
        GitChannelSubscriptionInstaller::new(crate::git_skill::GitSkillFacade::from_file_store());

    installer
        .verify_release_content(&repository, &manifest)
        .await
        .unwrap();
}

/// The install fetch is a remote git call and must not hold the skill
/// transaction. The probe is written from the clone path.
#[tokio::test]
async fn channel_install_fetches_outside_the_transaction_lock() {
    let sandbox = crate::test_sandbox::Sandbox::new();
    let probe = crate::fetch::RemoteGitLockProbe::arm(sandbox.root().join("channel-lock-probe"));
    let origin = sandbox.root().join("origin");
    let skill_root = origin.join("skills/writer");
    std::fs::create_dir_all(&skill_root).unwrap();
    std::fs::write(
        skill_root.join("SKILL.md"),
        "---\nname: writer\ndescription: Writer\n---\n# released\n",
    )
    .unwrap();
    git(&origin, &["init", "-q"]);
    git(&origin, &["config", "user.email", "tests@skillstar.local"]);
    git(&origin, &["config", "user.name", "SkillStar Tests"]);
    git(&origin, &["add", "."]);
    git(&origin, &["commit", "-qm", "release"]);
    let commit = git(&origin, &["rev-parse", "HEAD"]);
    let hash = crate::content::snapshot_path("writer", &skill_root)
        .unwrap()
        .content_hash;
    let clone_url = crate::git::ops::local_file_url(&std::fs::canonicalize(&origin).unwrap());

    let request = ChannelInstallRequest {
        repository: RemoteRepository {
            id: 42,
            owner_id: 7,
            owner_login: "acme".into(),
            owner_type: "Organization".into(),
            name: "channel".into(),
            default_branch: "main".into(),
            html_url: "https://github.com/acme/channel".into(),
            clone_url,
            private: true,
            permissions: super::RepositoryPermissions {
                admin: false,
                maintain: false,
                push: false,
                pull: true,
            },
        },
        manifest: ChannelReleaseManifest {
            schema_version: CHANNEL_RELEASE_MANIFEST_VERSION,
            repository_id: 42,
            organization_id: 7,
            revision: 1,
            tag_name: super::revision_tag(1),
            commit_sha: commit,
            publisher: super::ChannelPublisherIdentity {
                id: 9,
                login: "alice".into(),
            },
            published_at: "2026-08-05T00:00:00Z".into(),
            title: "Release".into(),
            notes: String::new(),
            skills: vec![super::ChannelReleaseSkill {
                id: "writer".into(),
                content_root: "skills/writer".into(),
                content_hash: hash,
                content_hash_version: CHANNEL_CONTENT_HASH_VERSION,
                status: super::ChannelSkillReleaseStatus::Added,
            }],
        },
        selected_skill_ids: vec!["writer".into()],
    };
    let installer = GitChannelSubscriptionInstaller::new(crate::git_skill::GitSkillFacade::new(
        crate::git::transport::GitOperationSession::public(),
    ));
    installer.install(request).await.unwrap();
    assert_eq!(
        probe.recorded_depth().as_deref(),
        Some("0"),
        "channel fetch must run after the skill transaction is released"
    );
}

/// Release-content verification clones the pinned commit. That clone is a
/// remote git call and must not hold the skill transaction: the check never
/// writes installed skills.
#[tokio::test]
async fn channel_release_verification_fetches_outside_the_transaction_lock() {
    let sandbox = crate::test_sandbox::Sandbox::new();
    let probe = crate::fetch::RemoteGitLockProbe::arm(sandbox.root().join("verify-lock-probe"));
    let origin = sandbox.root().join("origin");
    let skill_root = origin.join("skills/writer");
    std::fs::create_dir_all(&skill_root).unwrap();
    std::fs::write(
        skill_root.join("SKILL.md"),
        "---\nname: writer\ndescription: Writer\n---\n# released\n",
    )
    .unwrap();
    git(&origin, &["init", "-q"]);
    git(&origin, &["config", "user.email", "tests@skillstar.local"]);
    git(&origin, &["config", "user.name", "SkillStar Tests"]);
    git(&origin, &["add", "."]);
    git(&origin, &["commit", "-qm", "release"]);
    let commit = git(&origin, &["rev-parse", "HEAD"]);
    let hash = crate::content::snapshot_path("writer", &skill_root)
        .unwrap()
        .content_hash;
    let clone_url = crate::git::ops::local_file_url(&std::fs::canonicalize(&origin).unwrap());

    let repository = RemoteRepository {
        id: 42,
        owner_id: 7,
        owner_login: "acme".into(),
        owner_type: "Organization".into(),
        name: "channel".into(),
        default_branch: "main".into(),
        html_url: "https://github.com/acme/channel".into(),
        clone_url,
        private: true,
        permissions: super::RepositoryPermissions {
            admin: false,
            maintain: false,
            push: false,
            pull: true,
        },
    };
    let manifest = ChannelReleaseManifest {
        schema_version: CHANNEL_RELEASE_MANIFEST_VERSION,
        repository_id: 42,
        organization_id: 7,
        revision: 1,
        tag_name: super::revision_tag(1),
        commit_sha: commit,
        publisher: super::ChannelPublisherIdentity {
            id: 9,
            login: "alice".into(),
        },
        published_at: "2026-08-05T00:00:00Z".into(),
        title: "Release".into(),
        notes: String::new(),
        skills: vec![super::ChannelReleaseSkill {
            id: "writer".into(),
            content_root: "skills/writer".into(),
            content_hash: hash,
            content_hash_version: CHANNEL_CONTENT_HASH_VERSION,
            status: super::ChannelSkillReleaseStatus::Added,
        }],
    };
    let installer = GitChannelSubscriptionInstaller::new(crate::git_skill::GitSkillFacade::new(
        crate::git::transport::GitOperationSession::public(),
    ));
    installer
        .verify_release_content(&repository, &manifest)
        .await
        .unwrap();
    assert_eq!(
        probe.recorded_depth().as_deref(),
        Some("0"),
        "channel content verification must clone while the skill transaction depth is 0"
    );
}

#[tokio::test]
async fn channel_rollback_does_not_fetch() {
    let sandbox = crate::test_sandbox::Sandbox::new();
    let probe = crate::fetch::RemoteGitLockProbe::arm(sandbox.root().join("rollback-lock-probe"));
    let installer = GitChannelSubscriptionInstaller::new(crate::git_skill::GitSkillFacade::new(
        crate::git::transport::GitOperationSession::public(),
    ));
    let receipt = ChannelInstallReceipt {
        skills: Vec::new(),
        newly_installed_skill_ids: vec!["missing-skill".into()],
    };
    let _ = ChannelSubscriptionInstaller::rollback(&installer, &receipt).await;
    assert!(
        probe.recorded_depth().is_none(),
        "rollback restores local folders and must not clone"
    );
}
