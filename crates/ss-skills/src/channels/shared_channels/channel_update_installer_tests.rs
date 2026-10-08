use super::*;
use crate::channels::shared_channels::{
    CHANNEL_RELEASE_MANIFEST_VERSION, CHANNEL_SUBSCRIPTION_DESCRIPTOR_VERSION,
    CHANNEL_SUBSCRIPTION_STORE_VERSION, ChannelPublisherIdentity, ChannelReleaseManifest,
    ChannelReleaseSkill, ChannelReleaseTarget, ChannelSkillReleaseStatus, ChannelSubscription,
    ChannelSubscriptionRegistry, ChannelSubscriptionStore, DiskChannelSubscriptionRegistry,
    RemoteRepository, RepositoryPermissions,
};
use crate::git::transport::{GitOperationSession, GitTransportError, GitTransportErrorCode};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::path::Path;

#[test]
fn structured_git_transport_errors_survive_context_mapping() {
    for (git_code, expected) in [
        (
            GitTransportErrorCode::Network,
            SharedChannelErrorCode::Network,
        ),
        (
            GitTransportErrorCode::Unauthorized,
            SharedChannelErrorCode::AppRepositoryAccessRequired,
        ),
        (
            GitTransportErrorCode::UnsafeRemote,
            SharedChannelErrorCode::Integrity,
        ),
        (
            GitTransportErrorCode::Other,
            SharedChannelErrorCode::Protocol,
        ),
    ] {
        let error = anyhow::Error::new(GitTransportError {
            code: git_code,
            message: "transport failed".into(),
            session_id: "session".into(),
        })
        .context("fetching exact release");
        assert_eq!(
            git_read_error(error, "Unable to read channel update").code,
            expected
        );
    }
}

#[test]
fn exact_update_and_rollback_reconcile_hub_agent_project_provenance_and_state() {
    let _guard = crate::lock_test_env();
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let data = temp.path().join("data");
    let tool_home = temp.path().join("tool-home");
    let repository = temp.path().join("channel.git");
    let project = temp.path().join("project");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&project).unwrap();
    let previous_home = std::env::var_os("HOME");
    let previous_data = std::env::var_os("SKILLSTAR_DATA_DIR");
    let previous_tool_home = std::env::var_os("SKILLSTAR_TOOL_SYNC_HOME");
    set_env("HOME", &home);
    set_env("SKILLSTAR_DATA_DIR", &data);
    set_env("SKILLSTAR_TOOL_SYNC_HOME", &tool_home);
    crate::deployment::invalidate_profile_cache();
    crate::update_state::reset_for_test();

    let result = (|| {
        let skill_root = repository.join("skills/writer");
        fs::create_dir_all(&skill_root)?;
        fs::write(
            skill_root.join("SKILL.md"),
            "---\nname: writer\ndescription: Shared writer\n---\n# version one\n",
        )?;
        git(&repository, &["init", "-q"])?;
        git(&repository, &["config", "user.email", "test@example.com"])?;
        git(&repository, &["config", "user.name", "SkillStar Test"])?;
        git(&repository, &["add", "."])?;
        git(&repository, &["commit", "-qm", "release one"])?;
        let commit_one = git_output(&repository, &["rev-parse", "HEAD"])?;
        let hash_one = crate::content::snapshot_path("writer", &skill_root)?.content_hash;

        fs::write(
            skill_root.join("SKILL.md"),
            "---\nname: writer\ndescription: Shared writer\n---\n# version two\n",
        )?;
        git(&repository, &["add", "."])?;
        git(&repository, &["commit", "-qm", "release two"])?;
        let commit_two = git_output(&repository, &["rev-parse", "HEAD"])?;
        let hash_two = crate::content::snapshot_path("writer", &skill_root)?.content_hash;

        // D-081: fetch goes straight to the remote (file:// fixture) at the
        // pinned commit; the previous release is installed through the same
        // pipeline the subscription installer uses.
        // Canonicalize: macOS tempdirs live behind a /var -> /private/var symlink,
        // and the source parser canonicalizes local paths.
        let repository_url = format!("file://{}", std::fs::canonicalize(&repository)?.display());
        let spec_one =
            crate::source_resolver::Source::parse(&format!("{repository_url}#{commit_one}"))?;
        {
            let session = GitOperationSession::public();
            let checkout = crate::fetch::fetch_source(&spec_one, &session)?;
            crate::installer::install_units(
                checkout.dir(),
                &spec_one,
                &[crate::installer::InstallUnit {
                    id: "writer".into(),
                    folder_path: "skills/writer".into(),
                }],
            )?;
        }
        let _previous_lock_entry = lock_entry("writer")?;
        let previous = ChannelSubscribedSkill {
            id: "writer".into(),
            content_root: "skills/writer".into(),
            release_content_hash: hash_one.clone(),
            release_content_hash_version: CHANNEL_CONTENT_HASH_VERSION,
            baseline_hash: hash_one.clone(),
            baseline_hash_version: CHANNEL_CONTENT_HASH_VERSION,
            provenance: ChannelSkillProvenance {
                repository_id: 42,
                repository_url: repository_url.clone(),
                git_ref: commit_one.clone(),
                source_folder: "skills/writer".into(),
            },
        };
        fs::write(
            ss_core::infra::paths::agents_skill_dir("writer").join("SKILL.md"),
            "---\nname: writer\ndescription: Local writer notes\n---\n# local edits\n",
        )?;

        assert!(crate::agents::toggle_profile("codex")?);
        let agent_copy = home.join(".agents/skills/writer");
        fs::create_dir_all(&agent_copy)?;
        fs::write(agent_copy.join("SKILL.md"), "# stale agent copy\n")?;
        crate::deployment::ownership::mark_copy_for_test(&agent_copy, "writer");

        let project_entry = crate::projects::register_project(project.to_str().unwrap())?;
        let mut agents = HashMap::new();
        agents.insert("codex".to_string(), vec!["writer".to_string()]);
        let mut deploy_modes = HashMap::new();
        deploy_modes.insert(
            ".agents/skills".to_string(),
            crate::projects::ProjectDeployMode::Copy,
        );
        crate::projects::save_skills_list(
            &project_entry.name,
            &crate::projects::SkillsList {
                agents,
                deploy_modes,
                updated_at: chrono::Utc::now().to_rfc3339(),
            },
        )?;
        let project_copy = project.join(".agents/skills/writer");
        fs::create_dir_all(&project_copy)?;
        fs::write(project_copy.join("SKILL.md"), "# stale project copy\n")?;
        crate::deployment::ownership::mark_copy_for_test(&project_copy, "writer");

        crate::update_state::set("writer", true);
        // The channel already manages `writer`: the generic mutation gate
        // refuses it, and the channel upgrade must still go through.
        DiskChannelSubscriptionRegistry.save(&ChannelSubscriptionStore {
            schema_version: CHANNEL_SUBSCRIPTION_STORE_VERSION,
            subscriptions: vec![subscription(&previous)],
        })?;
        assert!(
            crate::skill_mutation::policy()
                .ensure_skill_mutation_allowed("writer")
                .is_err()
        );
        let mut update_repository = remote_repository();
        update_repository.clone_url = repository_url.clone();
        let request = ChannelSkillUpdateRequest {
            repository: update_repository,
            manifest: manifest(&commit_two, &hash_two),
            released: released_skill(&hash_two),
            installed: previous.clone(),
            resolution: Some(crate::skill_update::LocalDivergenceResolution::Preserve {
                local_name: "writer.local".into(),
            }),
        };
        let git_facade = crate::git_skill::GitSkillFacade::new(GitOperationSession::public());
        let installer = GitChannelSubscriptionInstaller::new(git_facade.clone());
        let receipt = block_on(ChannelSubscriptionUpdater::apply(&installer, request))?;
        let retained = receipt
            .retained
            .clone()
            .expect("the replaced content is retained");
        assert!(retained.path.is_dir());

        assert_eq!(receipt.installed.baseline_hash, hash_two);
        assert_eq!(receipt.installed.provenance.git_ref, commit_two);
        assert_content(&agent_copy, "# version two")?;
        assert_content(&project_copy, "# version two")?;
        assert_content(
            &ss_core::infra::paths::agents_skill_dir("writer"),
            "# version two",
        )?;
        assert_content(
            &ss_core::infra::paths::agents_skill_dir("writer.local"),
            "# local edits",
        )?;
        let updated_lock = lock_entry("writer")?;
        assert_eq!(updated_lock.git_ref.as_deref(), Some(commit_two.as_str()));
        assert_eq!(crate::content::snapshot("writer")?.content_hash, hash_two);
        assert_eq!(persisted_update_state("writer")?, Some(false));

        // Rollback restores the retained local copy: no fetch (the source is
        // gone), and exactly the content that was replaced comes back.
        let parked = temp.path().join("channel-parked.git");
        fs::rename(&repository, &parked)?;
        let rolled_back = block_on(ChannelSubscriptionUpdater::rollback(&installer, &receipt));
        fs::rename(&parked, &repository)?;
        rolled_back?;
        assert!(
            !retained.path.exists(),
            "the backup is consumed by the restore"
        );
        assert_content(&agent_copy, "# local edits")?;
        assert_content(&project_copy, "# local edits")?;
        assert_content(
            &ss_core::infra::paths::agents_skill_dir("writer"),
            "# local edits",
        )?;
        assert_content(
            &ss_core::infra::paths::agents_skill_dir("writer.local"),
            "# local edits",
        )?;
        let rolled_back_lock = lock_entry("writer")?;
        assert_eq!(
            rolled_back_lock.git_ref.as_deref(),
            Some(commit_one.as_str())
        );
        assert_eq!(
            crate::content::snapshot("writer")?.content_hash,
            retained.content_hash
        );
        assert_eq!(persisted_update_state("writer")?, Some(true));

        fs::write(
            ss_core::infra::paths::agents_skill_dir("writer").join("SKILL.md"),
            "---\nname: writer\ndescription: Shared writer\n---\n# version one\n",
        )?;
        assert_eq!(crate::content::snapshot("writer")?.content_hash, hash_one);

        let mut renamed_repository = remote_repository();
        renamed_repository.name = "renamed-channel".into();
        renamed_repository.html_url = "https://github.com/acme/renamed-channel".into();
        renamed_repository.clone_url = repository_url.clone();
        let renamed_receipt = apply_blocking(
            &git_facade,
            ChannelSkillUpdateRequest {
                repository: renamed_repository.clone(),
                manifest: manifest(&commit_two, &hash_two),
                released: released_skill(&hash_two),
                installed: previous.clone(),
                resolution: None,
            },
        )?;
        assert_eq!(
            renamed_receipt.installed.provenance.repository_id,
            renamed_repository.id
        );
        assert_eq!(
            renamed_receipt.installed.provenance.repository_url,
            renamed_repository.clone_url
        );
        assert_eq!(lock_entry("writer")?.source_url, repository_url);
        rollback_exact(&renamed_receipt)?;
        assert_eq!(lock_entry("writer")?.source_url, repository_url);

        // A manifest hash that does not match the fetched release content is
        // an integrity failure found before anything local is replaced: the
        // edited content, its provenance and the local-copy namespace stay
        // exactly as they were (no rollback, no `.local.N` duplicate).
        fs::write(
            ss_core::infra::paths::agents_skill_dir("writer").join("SKILL.md"),
            "---\nname: writer\ndescription: Local writer notes\n---\n# second edits\n",
        )?;
        let invalid_hash = format!("sha256:{}", "0".repeat(64));
        let mut invalid_repository = remote_repository();
        invalid_repository.clone_url = repository_url.clone();
        let error = apply_blocking(
            &git_facade,
            ChannelSkillUpdateRequest {
                repository: invalid_repository,
                manifest: manifest(&commit_two, &invalid_hash),
                released: released_skill(&invalid_hash),
                installed: previous,
                resolution: Some(crate::skill_update::LocalDivergenceResolution::Preserve {
                    local_name: "writer.local.2".into(),
                }),
            },
        )
        .unwrap_err();
        assert_eq!(error.code, SharedChannelErrorCode::Integrity);
        let intact_lock = lock_entry("writer")?;
        assert_eq!(intact_lock.git_ref.as_deref(), Some(commit_one.as_str()));
        assert_content(
            &ss_core::infra::paths::agents_skill_dir("writer"),
            "# second edits",
        )?;
        assert!(!ss_core::infra::paths::agents_skill_dir("writer.local.2").exists());
        let leftovers = fs::read_dir(ss_core::infra::paths::agents_skills_root())?
            .flatten()
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".skillstar-")
            })
            .count();
        assert_eq!(leftovers, 0, "no backup or staging folder is left behind");
        Ok::<(), anyhow::Error>(())
    })();

    restore_env("HOME", previous_home);
    restore_env("SKILLSTAR_DATA_DIR", previous_data);
    restore_env("SKILLSTAR_TOOL_SYNC_HOME", previous_tool_home);
    crate::deployment::invalidate_profile_cache();
    crate::update_state::reset_for_test();
    result.unwrap();
}

/// A receipt with no retained copy must not refetch the private repository.
/// The installed files stay exactly as they were.
#[test]
fn rollback_without_a_backup_leaves_the_skill_and_does_not_fetch() {
    let _sandbox = crate::test_sandbox::Sandbox::new();
    let dir = ss_core::infra::paths::agents_skill_dir("writer");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("SKILL.md"),
        "---\nname: writer\ndescription: d\n---\n# kept\n",
    )
    .unwrap();
    let previous = ChannelSubscribedSkill {
        id: "writer".into(),
        content_root: "skills/writer".into(),
        release_content_hash: "sha256:old".into(),
        release_content_hash_version: CHANNEL_CONTENT_HASH_VERSION,
        baseline_hash: "sha256:old".into(),
        baseline_hash_version: CHANNEL_CONTENT_HASH_VERSION,
        provenance: ChannelSkillProvenance {
            repository_id: 42,
            repository_url: "https://github.com/acme/private-channel.git".into(),
            git_ref: "a".repeat(40),
            source_folder: "skills/writer".into(),
        },
    };
    let error = rollback_exact(&ChannelSkillUpdateReceipt {
        installed: previous.clone(),
        previous,
        previous_lock_entry: crate::skill_lock::SkillLockEntry {
            source: "acme/private-channel".into(),
            source_type: crate::skill_lock::SourceType::Github,
            source_url: "https://github.com/acme/private-channel.git".into(),
            git_ref: Some("a".repeat(40)),
            skill_path: Some("skills/writer".into()),
            skill_folder_hash: None,
            installed_at: String::new(),
            updated_at: String::new(),
            extra: Default::default(),
        },
        previous_update_available: None,
        update_state_revision_after_apply: None,
        retained: None,
    })
    .unwrap_err();
    assert!(
        error.message.contains("no local channel-update backup"),
        "{error:?}"
    );
    assert_eq!(
        fs::read_to_string(dir.join("SKILL.md")).unwrap(),
        "---\nname: writer\ndescription: d\n---\n# kept\n"
    );
}

/// The upgrade fetch is a remote git call and must not hold the skill
/// transaction. The probe is written from the clone path, before replacement.
#[test]
fn channel_update_fetches_outside_the_transaction_lock() {
    let sandbox = crate::test_sandbox::Sandbox::new();
    let release = installed_writer(sandbox.root()).unwrap();
    let probe = crate::fetch::RemoteGitLockProbe::arm(sandbox.root().join("update-lock-probe"));
    let git_facade = crate::git_skill::GitSkillFacade::new(GitOperationSession::public());
    let receipt = apply_blocking(
        &git_facade,
        update_request(&release, &release.hash_two, None),
    )
    .unwrap();
    assert_eq!(
        probe.recorded_depth().as_deref(),
        Some("0"),
        "channel update must clone while the skill transaction depth is 0"
    );
    assert_eq!(receipt.installed.provenance.git_ref, release.commit_two);
    assert_content(
        &ss_core::infra::paths::agents_skill_dir("writer"),
        "# version two",
    )
    .unwrap();
}

/// A hash mismatch is found after the fetch and before any local replacement.
/// The error is the integrity failure itself: no rollback, and canonical
/// content plus the lock stay byte-for-byte as they were.
#[test]
fn channel_update_rejects_a_bad_release_without_rolling_back() {
    let sandbox = crate::test_sandbox::Sandbox::new();
    let release = installed_writer(sandbox.root()).unwrap();
    let canonical = ss_core::infra::paths::agents_skill_dir("writer");
    fs::write(
        canonical.join("SKILL.md"),
        "---\nname: writer\ndescription: Local writer notes\n---\n# local edits\n",
    )
    .unwrap();
    let before = fs::read(canonical.join("SKILL.md")).unwrap();
    let before_lock = lock_entry("writer").unwrap();
    let probe = crate::fetch::RemoteGitLockProbe::arm(sandbox.root().join("update-lock-probe"));
    let invalid_hash = format!("sha256:{}", "0".repeat(64));
    let git_facade = crate::git_skill::GitSkillFacade::new(GitOperationSession::public());
    let error = apply_blocking(
        &git_facade,
        update_request(
            &release,
            &invalid_hash,
            Some(crate::skill_update::LocalDivergenceResolution::Preserve {
                local_name: "writer.local".into(),
            }),
        ),
    )
    .unwrap_err();
    assert_eq!(error.code, SharedChannelErrorCode::Integrity);
    assert_eq!(
        error.message,
        "The channel update content does not match the published manifest"
    );
    assert_eq!(
        probe.recorded_depth().as_deref(),
        Some("0"),
        "the rejected fetch still runs outside the skill transaction"
    );
    assert_eq!(fs::read(canonical.join("SKILL.md")).unwrap(), before);
    assert_eq!(lock_entry("writer").unwrap(), before_lock);
    assert!(
        !ss_core::infra::paths::agents_skill_dir("writer.local").exists(),
        "a pre-replacement failure must not keep a local copy"
    );
    let leftovers = fs::read_dir(ss_core::infra::paths::agents_skills_root())
        .unwrap()
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(".skillstar-")
        })
        .count();
    assert_eq!(leftovers, 0, "no backup or staging folder is left behind");
}

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future)
}

/// The persisted subscription for `skill`. The store only accepts GitHub
/// URLs, so it records the production route while the fixture fetches over
/// `file://`; ownership is keyed by Skill id and repository id.
fn subscription(skill: &ChannelSubscribedSkill) -> ChannelSubscription {
    let mut skill = skill.clone();
    skill.provenance.repository_url = remote_repository().clone_url;
    ChannelSubscription {
        descriptor_version: CHANNEL_SUBSCRIPTION_DESCRIPTOR_VERSION,
        repository_id: 42,
        organization_id: 7,
        repository_url_aliases: Vec::new(),
        target: ChannelReleaseTarget {
            revision: 1,
            tag_name: "channel-v000001".into(),
            commit_sha: skill.provenance.git_ref.clone(),
        },
        known_skill_ids: vec![skill.id.clone()],
        skills: vec![skill],
        pins: Vec::new(),
        last_update: None,
        auto_update: Default::default(),
        remote_state: Default::default(),
        created_at: "2026-08-05T00:00:00Z".into(),
        updated_at: "2026-08-05T00:00:00Z".into(),
    }
}

/// Two commits of `writer` on a local `file://` origin, with the first
/// already installed. Later tests arm the lock probe only around `apply`.
struct WriterRelease {
    clone_url: String,
    previous: ChannelSubscribedSkill,
    commit_two: String,
    hash_two: String,
}

fn installed_writer(root: &Path) -> anyhow::Result<WriterRelease> {
    let origin = root.join("origin");
    let skill_root = origin.join("skills/writer");
    fs::create_dir_all(&skill_root)?;
    fs::write(
        skill_root.join("SKILL.md"),
        "---\nname: writer\ndescription: Shared writer\n---\n# version one\n",
    )?;
    git(&origin, &["init", "-q"])?;
    git(&origin, &["config", "user.email", "tests@skillstar.local"])?;
    git(&origin, &["config", "user.name", "SkillStar Tests"])?;
    git(&origin, &["add", "."])?;
    git(&origin, &["commit", "-qm", "release one"])?;
    let commit_one = git_output(&origin, &["rev-parse", "HEAD"])?;
    let hash_one = crate::content::snapshot_path("writer", &skill_root)?.content_hash;
    let clone_url = crate::git::ops::local_file_url(&fs::canonicalize(&origin)?);
    let spec = crate::source_resolver::Source::parse(&format!("{clone_url}#{commit_one}"))?;
    let session = GitOperationSession::public();
    let checkout = crate::fetch::fetch_source(&spec, &session)?;
    crate::installer::install_units(
        checkout.dir(),
        &spec,
        &[crate::installer::InstallUnit {
            id: "writer".into(),
            folder_path: "skills/writer".into(),
        }],
    )?;

    fs::write(
        skill_root.join("SKILL.md"),
        "---\nname: writer\ndescription: Shared writer\n---\n# version two\n",
    )?;
    git(&origin, &["add", "."])?;
    git(&origin, &["commit", "-qm", "release two"])?;
    Ok(WriterRelease {
        previous: ChannelSubscribedSkill {
            id: "writer".into(),
            content_root: "skills/writer".into(),
            release_content_hash: hash_one.clone(),
            release_content_hash_version: CHANNEL_CONTENT_HASH_VERSION,
            baseline_hash: hash_one,
            baseline_hash_version: CHANNEL_CONTENT_HASH_VERSION,
            provenance: ChannelSkillProvenance {
                repository_id: 42,
                repository_url: clone_url.clone(),
                git_ref: commit_one,
                source_folder: "skills/writer".into(),
            },
        },
        clone_url,
        commit_two: git_output(&origin, &["rev-parse", "HEAD"])?,
        hash_two: crate::content::snapshot_path("writer", &skill_root)?.content_hash,
    })
}

fn update_request(
    release: &WriterRelease,
    content_hash: &str,
    resolution: Option<crate::skill_update::LocalDivergenceResolution>,
) -> ChannelSkillUpdateRequest {
    let mut repository = remote_repository();
    repository.clone_url = release.clone_url.clone();
    ChannelSkillUpdateRequest {
        repository,
        manifest: manifest(&release.commit_two, content_hash),
        released: released_skill(content_hash),
        installed: release.previous.clone(),
        resolution,
    }
}

fn remote_repository() -> RemoteRepository {
    RemoteRepository {
        id: 42,
        owner_id: 7,
        owner_login: "acme".into(),
        owner_type: "Organization".into(),
        name: "channel".into(),
        default_branch: "main".into(),
        html_url: "https://github.com/acme/channel".into(),
        clone_url: "https://github.com/acme/channel.git".into(),
        private: true,
        permissions: RepositoryPermissions {
            admin: false,
            maintain: false,
            push: false,
            pull: true,
        },
    }
}

fn released_skill(content_hash: &str) -> ChannelReleaseSkill {
    ChannelReleaseSkill {
        id: "writer".into(),
        content_root: "skills/writer".into(),
        content_hash: content_hash.into(),
        content_hash_version: CHANNEL_CONTENT_HASH_VERSION,
        status: ChannelSkillReleaseStatus::Updated,
    }
}

fn manifest(commit: &str, content_hash: &str) -> ChannelReleaseManifest {
    ChannelReleaseManifest {
        schema_version: CHANNEL_RELEASE_MANIFEST_VERSION,
        repository_id: 42,
        organization_id: 7,
        revision: 2,
        tag_name: "channel-v000002".into(),
        commit_sha: commit.into(),
        publisher: ChannelPublisherIdentity {
            id: 9,
            login: "alice".into(),
        },
        published_at: "2026-08-05T01:00:00Z".into(),
        title: "Release two".into(),
        notes: "Upgrade writer".into(),
        skills: vec![released_skill(content_hash)],
    }
}

fn lock_entry(name: &str) -> anyhow::Result<crate::skill_lock::SkillLockEntry> {
    crate::skill_lock::load()
        .skills
        .get(name)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("missing lock entry for {name}"))
}

fn persisted_update_state(name: &str) -> anyhow::Result<Option<bool>> {
    // On disk each name maps to `{ update_available, upstream_change? }`
    // (see `crate::update_state`); only the badge matters here.
    let path = ss_core::infra::paths::state_dir().join("skill_update_states.json");
    let states =
        serde_json::from_str::<HashMap<String, serde_json::Value>>(&fs::read_to_string(path)?)?;
    Ok(states
        .get(name)
        .and_then(|state| state.get("update_available"))
        .and_then(serde_json::Value::as_bool))
}

fn assert_content(path: &Path, expected: &str) -> anyhow::Result<()> {
    let content = fs::read_to_string(path.join("SKILL.md"))?;
    anyhow::ensure!(content.contains(expected), "unexpected content: {content}");
    Ok(())
}

fn git(repository: &Path, args: &[&str]) -> anyhow::Result<()> {
    let output = ss_core::infra::path_env::command_with_path("git")
        .current_dir(repository)
        .args(args)
        .output()?;
    anyhow::ensure!(
        output.status.success(),
        "git failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

fn git_output(repository: &Path, args: &[&str]) -> anyhow::Result<String> {
    let output = ss_core::infra::path_env::command_with_path("git")
        .current_dir(repository)
        .args(args)
        .output()?;
    anyhow::ensure!(
        output.status.success(),
        "git failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

fn set_env<K: AsRef<OsStr>, V: AsRef<OsStr>>(key: K, value: V) {
    unsafe { std::env::set_var(key, value) }
}

fn remove_env<K: AsRef<OsStr>>(key: K) {
    unsafe { std::env::remove_var(key) }
}

fn restore_env<K: AsRef<OsStr>>(key: K, value: Option<std::ffi::OsString>) {
    match value {
        Some(value) => set_env(key, value),
        None => remove_env(key),
    }
}
