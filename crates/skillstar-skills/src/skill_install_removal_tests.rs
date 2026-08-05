use std::ffi::OsString;
use std::path::{Path, PathBuf};

struct RemovalSandbox {
    previous: Vec<(&'static str, Option<OsString>)>,
    _temp: tempfile::TempDir,
}

impl RemovalSandbox {
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

    fn hub(&self) -> PathBuf {
        skillstar_core::infra::paths::hub_skills_dir()
    }
}

impl Drop for RemovalSandbox {
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

fn write_installed_skill(root: &Path) -> String {
    std::fs::create_dir_all(root).unwrap();
    std::fs::write(
        root.join("SKILL.md"),
        "---\nname: writer\ndescription: Writer\n---\n# Writer\n",
    )
    .unwrap();
    let hash = crate::content::snapshot_path("writer", root)
        .unwrap()
        .content_hash;
    let mut lockfile = crate::lockfile::Lockfile::default();
    lockfile.upsert(crate::lockfile::LockEntry {
        name: "writer".into(),
        git_url: "https://github.com/acme/channel.git".into(),
        git_ref: Some("a".repeat(40)),
        tree_hash: "tree".into(),
        content_hash: Some(hash.clone()),
        content_hash_version: Some(crate::content::SNAPSHOT_HASH_VERSION),
        installed_at: chrono::Utc::now().to_rfc3339(),
        source_folder: Some("skills/writer".into()),
    });
    lockfile.save(&crate::lockfile::lockfile_path()).unwrap();
    hash
}

#[test]
fn hub_removal_restores_content_and_lockfile_when_metadata_commit_fails() {
    let _guard = crate::lock_test_env();
    let sandbox = RemovalSandbox::new();
    let skill = sandbox.hub().join("writer");
    write_installed_skill(&skill);

    let failure = crate::skill_install::uninstall_hub_skill_with_commit("writer", || {
        Err("subscription save failed")
    })
    .unwrap_err();

    assert!(!failure.committed);
    assert!(skill.join("SKILL.md").is_file());
    assert!(
        crate::lockfile::Lockfile::load(&crate::lockfile::lockfile_path())
            .unwrap()
            .skills
            .iter()
            .any(|entry| entry.name == "writer")
    );
}

#[tokio::test]
async fn channel_conversion_removes_its_safety_copy_when_metadata_commit_rolls_back() {
    use crate::shared_channels::{
        ChannelRemovedSkillHandler, ChannelSkillProvenance, ChannelSubscribedSkill,
        GitChannelSubscriptionInstaller, SharedChannelError, SharedChannelErrorCode,
    };

    let _guard = crate::lock_test_env();
    let sandbox = RemovalSandbox::new();
    let skill = sandbox.hub().join("writer");
    let hash = write_installed_skill(&skill);
    let installer =
        GitChannelSubscriptionInstaller::new(crate::git_skill::GitSkillFacade::from_keyring());
    let subscribed = ChannelSubscribedSkill {
        id: "writer".into(),
        content_root: "skills/writer".into(),
        release_content_hash: hash.clone(),
        release_content_hash_version: crate::content::SNAPSHOT_HASH_VERSION,
        baseline_hash: hash,
        baseline_hash_version: crate::content::SNAPSHOT_HASH_VERSION,
        provenance: ChannelSkillProvenance {
            repository_id: 42,
            repository_url: "https://github.com/acme/channel.git".into(),
            git_ref: "a".repeat(40),
            source_folder: "skills/writer".into(),
        },
    };

    let error = installer
        .convert_to_local_and_commit(
            &subscribed,
            "writer.local",
            Box::new(|| {
                Err(SharedChannelError::new(
                    SharedChannelErrorCode::Storage,
                    "subscription save failed",
                ))
            }),
        )
        .await
        .unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::Storage);
    assert!(skill.join("SKILL.md").is_file());
    assert!(
        !skillstar_core::infra::paths::local_skills_dir()
            .join("writer.local")
            .exists()
    );
}
