use super::*;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
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
    let output = Command::new("git")
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
    let repo = skillstar_core::infra::paths::repos_cache_dir().join("acme--channel");
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

    let hub_skill = skillstar_core::infra::paths::hub_skills_dir().join("writer");
    std::fs::create_dir_all(hub_skill.parent().unwrap()).unwrap();
    skillstar_core::infra::fs_ops::create_symlink(&source, &hub_skill).unwrap();
    let hash = skillstar_skills::content::snapshot("writer").unwrap().content_hash;
    let mut lockfile = skillstar_skills::lockfile::Lockfile::default();
    lockfile.upsert(skillstar_skills::lockfile::LockEntry {
        name: "writer".into(),
        git_url: "https://github.com/acme/channel.git".into(),
        git_ref: Some(head.clone()),
        tree_hash: "tree".into(),
        content_hash: Some(hash.clone()),
        content_hash_version: Some(CHANNEL_CONTENT_HASH_VERSION),
        installed_at: chrono::Utc::now().to_rfc3339(),
        source_folder: Some("skills/writer".into()),
    });
    lockfile.save(&skillstar_skills::lockfile::lockfile_path()).unwrap();

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
    let _guard = crate::lock_test_env_async().await;
    let (_sandbox, receipt, _repo, hub_skill) = install_fixture();
    let installer =
        GitChannelSubscriptionInstaller::new(skillstar_skills::git_skill::GitSkillFacade::from_keyring());
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
    assert!(
        skillstar_skills::lockfile::Lockfile::load(&skillstar_skills::lockfile::lockfile_path())
            .unwrap()
            .skills
            .iter()
            .any(|entry| entry.name == "writer")
    );
}

#[tokio::test]
async fn metadata_failure_rolls_back_unchanged_install_without_relocking() {
    let _guard = crate::lock_test_env_async().await;
    let (_sandbox, receipt, _repo, hub_skill) = install_fixture();
    let installer =
        GitChannelSubscriptionInstaller::new(skillstar_skills::git_skill::GitSkillFacade::from_keyring());

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
    assert!(
        skillstar_skills::lockfile::Lockfile::load(&skillstar_skills::lockfile::lockfile_path())
            .unwrap()
            .skills
            .iter()
            .all(|entry| entry.name != "writer")
    );
}

#[tokio::test]
async fn final_verification_rejects_a_checkout_that_moved_to_another_commit() {
    let _guard = crate::lock_test_env_async().await;
    let (_sandbox, receipt, repo, hub_skill) = install_fixture();
    std::fs::write(repo.join("README.md"), "moved without changing writer\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "move head"]);
    let committed = Arc::new(AtomicBool::new(false));
    let commit_called = committed.clone();
    let installer =
        GitChannelSubscriptionInstaller::new(skillstar_skills::git_skill::GitSkillFacade::from_keyring());

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
    let _guard = crate::lock_test_env_async().await;
    let (_sandbox, _receipt, _installed_repo, _hub_skill) = install_fixture();
    let origin = skillstar_core::infra::paths::hub_root().join("verifier-origin");
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
    git(&origin, &["add", "."]);
    git(&origin, &["commit", "-m", "verified release"]);
    let commit = git(&origin, &["rev-parse", "HEAD"]);
    let hash = skillstar_skills::content::snapshot_path("writer", &skill_root)
        .unwrap()
        .content_hash;
    let cache = skillstar_core::infra::paths::repos_cache_dir().join(format!(
        "{}--ref--{}",
        skillstar_skills::repo_scanner::cache_dir_name(&format!(
            "channel-verify-42-{}",
            skillstar_skills::repo_scanner::cache_dir_name("https://github.com/acme/channel.git")
        )),
        skillstar_skills::repo_scanner::cache_dir_name(&commit)
    ));
    std::fs::create_dir_all(cache.parent().unwrap()).unwrap();
    let clone = Command::new("git")
        .args(["clone", "-q"])
        .arg(&origin)
        .arg(&cache)
        .status()
        .unwrap();
    assert!(clone.success());

    let repository = RemoteRepository {
        id: 42,
        owner_id: 7,
        owner_login: "acme".into(),
        owner_type: "Organization".into(),
        name: "channel".into(),
        default_branch: "main".into(),
        html_url: "https://github.com/acme/channel".into(),
        clone_url: "https://github.com/acme/channel.git".into(),
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
        GitChannelSubscriptionInstaller::new(skillstar_skills::git_skill::GitSkillFacade::from_keyring());

    installer
        .verify_release_content(&repository, &manifest)
        .await
        .unwrap();
}
