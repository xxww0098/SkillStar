use super::*;
use crate::shared_channels::{
    CHANNEL_RELEASE_MANIFEST_VERSION, ChannelPublisherIdentity, ChannelReleaseManifest,
    ChannelReleaseSkill, ChannelSkillReleaseStatus, RemoteRepository, RepositoryPermissions,
};
use skillstar_skills::git::transport::{
    GitOperationSession, GitTransportError, GitTransportErrorCode,
};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;

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
    let previous_codex = std::env::var_os("CODEX_HOME");
    let previous_tool_home = std::env::var_os("SKILLSTAR_TOOL_SYNC_HOME");
    set_env("HOME", &home);
    set_env("SKILLSTAR_DATA_DIR", &data);
    set_env("SKILLSTAR_TOOL_SYNC_HOME", &tool_home);
    remove_env("CODEX_HOME");
    skillstar_skills::deployment::invalidate_profile_cache();
    skillstar_skills::update_state::reset_for_test();

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
        let hash_one =
            skillstar_skills::content::snapshot_path("writer", &skill_root)?.content_hash;

        fs::write(
            skill_root.join("SKILL.md"),
            "---\nname: writer\ndescription: Shared writer\n---\n# version two\n",
        )?;
        git(&repository, &["add", "."])?;
        git(&repository, &["commit", "-qm", "release two"])?;
        let commit_two = git_output(&repository, &["rev-parse", "HEAD"])?;
        let hash_two =
            skillstar_skills::content::snapshot_path("writer", &skill_root)?.content_hash;

        let cache_one = exact_cache(&commit_one);
        let cache_two = exact_cache(&commit_two);
        fs::create_dir_all(cache_one.parent().unwrap())?;
        git_clone(&repository, &cache_one)?;
        git(&cache_one, &["checkout", "-q", &commit_one])?;
        git_clone(&repository, &cache_two)?;
        git(&cache_two, &["checkout", "-q", &commit_two])?;

        let repository_url = "https://github.com/acme/channel.git";
        skillstar_skills::repo_scanner::install_from_repo_at(
            &cache_one,
            repository_url,
            Some(&commit_one),
            &[skillstar_skills::repo_scanner::SkillInstallTarget {
                id: "writer".into(),
                folder_path: "skills/writer".into(),
            }],
        )?;
        let previous_lock_entry = lock_entry("writer")?;
        let previous = ChannelSubscribedSkill {
            id: "writer".into(),
            content_root: "skills/writer".into(),
            release_content_hash: hash_one.clone(),
            release_content_hash_version: CHANNEL_CONTENT_HASH_VERSION,
            baseline_hash: previous_lock_entry.content_hash.clone().unwrap(),
            baseline_hash_version: previous_lock_entry.content_hash_version.unwrap(),
            provenance: ChannelSkillProvenance {
                repository_id: 42,
                repository_url: repository_url.into(),
                git_ref: commit_one.clone(),
                source_folder: "skills/writer".into(),
            },
        };
        fs::write(
            skillstar_core::infra::paths::hub_skills_dir().join("writer/SKILL.md"),
            "---\nname: writer\ndescription: Local writer notes\n---\n# local edits\n",
        )?;

        assert!(skillstar_agents::toggle_profile("codex")?);
        let agent_copy = home.join(".codex/skills/writer");
        fs::create_dir_all(&agent_copy)?;
        fs::write(agent_copy.join("SKILL.md"), "# stale agent copy\n")?;

        let project_entry =
            skillstar_skills::projects::register_project(project.to_str().unwrap())?;
        let mut agents = HashMap::new();
        agents.insert("codex".to_string(), vec!["writer".to_string()]);
        let mut deploy_modes = HashMap::new();
        deploy_modes.insert(
            ".agents/skills".to_string(),
            skillstar_skills::projects::ProjectDeployMode::Copy,
        );
        skillstar_skills::projects::save_skills_list(
            &project_entry.name,
            &skillstar_skills::projects::SkillsList {
                agents,
                deploy_modes,
                updated_at: chrono::Utc::now().to_rfc3339(),
            },
        )?;
        let project_copy = project.join(".agents/skills/writer");
        fs::create_dir_all(&project_copy)?;
        fs::write(project_copy.join("SKILL.md"), "# stale project copy\n")?;

        skillstar_skills::update_state::set("writer", true);
        let request = ChannelSkillUpdateRequest {
            repository: remote_repository(),
            manifest: manifest(&commit_two, &hash_two),
            released: released_skill(&hash_two),
            installed: previous.clone(),
            resolution: Some(
                skillstar_skills::skill_update::LocalDivergenceResolution::Preserve {
                    local_name: "writer.local".into(),
                },
            ),
        };
        let git_facade =
            skillstar_skills::git_skill::GitSkillFacade::new(GitOperationSession::public());
        let receipt = apply_blocking(&git_facade, request)?;

        assert_eq!(receipt.installed.baseline_hash, hash_two);
        assert_eq!(receipt.installed.provenance.git_ref, commit_two);
        assert_content(&agent_copy, "# version two")?;
        assert_content(&project_copy, "# version two")?;
        assert_content(
            &skillstar_core::infra::paths::hub_skills_dir().join("writer"),
            "# version two",
        )?;
        assert_content(
            &skillstar_core::infra::paths::hub_skills_dir().join("writer.local"),
            "# local edits",
        )?;
        let updated_lock = lock_entry("writer")?;
        assert_eq!(updated_lock.git_ref.as_deref(), Some(commit_two.as_str()));
        assert_eq!(
            updated_lock.content_hash.as_deref(),
            Some(hash_two.as_str())
        );
        assert_eq!(persisted_update_state("writer")?, Some(false));

        rollback_exact(&receipt)?;
        assert_content(&agent_copy, "# version one")?;
        assert_content(&project_copy, "# version one")?;
        assert_content(
            &skillstar_core::infra::paths::hub_skills_dir().join("writer"),
            "# version one",
        )?;
        assert_content(
            &skillstar_core::infra::paths::hub_skills_dir().join("writer.local"),
            "# local edits",
        )?;
        let rolled_back_lock = lock_entry("writer")?;
        assert_eq!(
            rolled_back_lock.git_ref.as_deref(),
            Some(commit_one.as_str())
        );
        assert_eq!(
            rolled_back_lock.content_hash.as_deref(),
            Some(hash_one.as_str())
        );
        assert_eq!(persisted_update_state("writer")?, Some(true));

        let renamed_cache = exact_cache_for("acme/renamed-channel", &commit_two);
        git_clone(&repository, &renamed_cache)?;
        git(&renamed_cache, &["checkout", "-q", &commit_two])?;
        let mut renamed_repository = remote_repository();
        renamed_repository.name = "renamed-channel".into();
        renamed_repository.html_url = "https://github.com/acme/renamed-channel".into();
        renamed_repository.clone_url = "https://github.com/acme/renamed-channel.git".into();
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
        assert_eq!(
            lock_entry("writer")?.git_url,
            "https://github.com/acme/renamed-channel.git"
        );
        rollback_exact(&renamed_receipt)?;
        assert_eq!(lock_entry("writer")?.git_url, repository_url);

        let lock_path = skillstar_skills::lockfile::lockfile_path();
        let mut missing_baseline = skillstar_skills::lockfile::Lockfile::load(&lock_path)?;
        let entry = missing_baseline
            .skills
            .iter_mut()
            .find(|entry| entry.name == "writer")
            .unwrap();
        entry.content_hash = None;
        entry.content_hash_version = None;
        missing_baseline.save(&lock_path)?;
        let invalid_hash = format!("sha256:{}", "0".repeat(64));
        let error = apply_blocking(
            &git_facade,
            ChannelSkillUpdateRequest {
                repository: remote_repository(),
                manifest: manifest(&commit_two, &invalid_hash),
                released: released_skill(&invalid_hash),
                installed: previous,
                resolution: Some(
                    skillstar_skills::skill_update::LocalDivergenceResolution::Discard,
                ),
            },
        )
        .unwrap_err();
        assert_eq!(error.code, SharedChannelErrorCode::Integrity);
        let repaired_lock = lock_entry("writer")?;
        assert_eq!(
            repaired_lock.content_hash.as_deref(),
            Some(hash_one.as_str())
        );
        assert_eq!(
            repaired_lock.content_hash_version,
            Some(CHANNEL_CONTENT_HASH_VERSION)
        );
        assert_content(
            &skillstar_core::infra::paths::hub_skills_dir().join("writer"),
            "# version one",
        )?;
        Ok::<(), anyhow::Error>(())
    })();

    restore_env("HOME", previous_home);
    restore_env("SKILLSTAR_DATA_DIR", previous_data);
    restore_env("CODEX_HOME", previous_codex);
    restore_env("SKILLSTAR_TOOL_SYNC_HOME", previous_tool_home);
    skillstar_skills::deployment::invalidate_profile_cache();
    skillstar_skills::update_state::reset_for_test();
    result.unwrap();
}

fn exact_cache(commit: &str) -> std::path::PathBuf {
    exact_cache_for("acme/channel", commit)
}

fn exact_cache_for(source: &str, commit: &str) -> std::path::PathBuf {
    skillstar_core::infra::paths::repos_cache_dir().join(format!(
        "{}--ref--{}",
        skillstar_skills::source_resolver::cache_dir_name(source),
        skillstar_skills::source_resolver::cache_dir_name(commit)
    ))
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

fn lock_entry(name: &str) -> anyhow::Result<skillstar_skills::lockfile::LockEntry> {
    skillstar_skills::lockfile::Lockfile::load(&skillstar_skills::lockfile::lockfile_path())?
        .skills
        .into_iter()
        .find(|entry| entry.name == name)
        .ok_or_else(|| anyhow::anyhow!("missing lock entry for {name}"))
}

fn persisted_update_state(name: &str) -> anyhow::Result<Option<bool>> {
    // On disk each name maps to `{ update_available, upstream_change? }`
    // (see `skillstar_skills::update_state`); only the badge matters here.
    let path = skillstar_core::infra::paths::state_dir().join("skill_update_states.json");
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
    let output = skillstar_core::infra::path_env::command_with_path("git")
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
    let output = skillstar_core::infra::path_env::command_with_path("git")
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

fn git_clone(source: &Path, destination: &Path) -> anyhow::Result<()> {
    let output = skillstar_core::infra::path_env::command_with_path("git")
        .args(["clone", "-q"])
        .arg(source)
        .arg(destination)
        .output()?;
    anyhow::ensure!(
        output.status.success(),
        "git clone failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
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
