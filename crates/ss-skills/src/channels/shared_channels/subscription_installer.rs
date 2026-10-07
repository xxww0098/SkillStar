use super::git_read::git_read_error;
use super::{
    CHANNEL_CONTENT_HASH_VERSION, ChannelInstallReceipt, ChannelInstallRequest,
    ChannelReleaseManifest, ChannelSkillProvenance, ChannelSubscribedSkill,
    ChannelSubscriptionInstaller, RemoteRepository, SharedChannelError, SharedChannelErrorCode,
};
use crate::git_skill::GitSkillFacade;
use async_trait::async_trait;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
pub struct GitChannelSubscriptionInstaller {
    pub(super) git: GitSkillFacade,
}

impl GitChannelSubscriptionInstaller {
    pub fn new(git: GitSkillFacade) -> Self {
        Self { git }
    }
}

#[async_trait]
impl ChannelSubscriptionInstaller for GitChannelSubscriptionInstaller {
    async fn verify_release_content(
        &self,
        repository: &RemoteRepository,
        manifest: &ChannelReleaseManifest,
    ) -> Result<(), SharedChannelError> {
        let git = self.git.clone();
        let repository = repository.clone();
        let manifest = manifest.clone();
        tokio::task::spawn_blocking(move || {
            super::release_content_verifier::verify_release_content_blocking(
                &git,
                &repository,
                &manifest,
            )
        })
        .await
        .map_err(|_| {
            SharedChannelError::new(
                SharedChannelErrorCode::SubscriptionUpdateFailed,
                "The channel release verification task stopped unexpectedly",
            )
        })?
    }

    async fn install(
        &self,
        request: ChannelInstallRequest,
    ) -> Result<ChannelInstallReceipt, SharedChannelError> {
        let git = self.git.clone();
        // Fetch and content checks run inside `install_blocking` before it
        // takes the transaction. The lock covers only the local write.
        tokio::task::spawn_blocking(move || install_blocking(&git, request))
            .await
            .map_err(|_| install_error("The channel installation task stopped unexpectedly"))?
    }

    async fn rollback(&self, receipt: &ChannelInstallReceipt) -> Result<(), SharedChannelError> {
        let names = receipt.newly_installed_skill_ids.clone();
        tokio::task::spawn_blocking(move || {
            let _guard =
                crate::skill_update::acquire_update_transaction_lock().map_err(|error| {
                    install_error(format!(
                        "Unable to lock channel installation rollback: {error}"
                    ))
                })?;
            // Local restore of what this install added. Do not re-fetch the
            // release: an anonymous session cannot see a private channel, and
            // the receipt already names the folders to put back.
            rollback_new_installs(&names)
        })
        .await
        .map_err(|_| install_error("The channel installation rollback task stopped unexpectedly"))?
    }

    async fn verify_and_commit_install(
        &self,
        receipt: &ChannelInstallReceipt,
        commit: Box<dyn FnOnce() -> Result<(), SharedChannelError> + Send>,
    ) -> Result<(), SharedChannelError> {
        let receipt = receipt.clone();
        tokio::task::spawn_blocking(move || {
            let _guard =
                crate::skill_update::acquire_update_transaction_lock().map_err(|error| {
                    install_error(format!(
                        "Unable to lock channel installation commit: {error}"
                    ))
                })?;
            if let Err(error) = verify_install_receipt(&receipt) {
                return Err(with_install_rollback(
                    error,
                    rollback_install_receipt_preserving_changes(&receipt),
                ));
            }
            if let Err(error) = commit() {
                return Err(with_install_rollback(
                    error,
                    rollback_install_receipt_preserving_changes(&receipt),
                ));
            }
            Ok(())
        })
        .await
        .map_err(|_| install_error("The channel installation commit task stopped unexpectedly"))?
    }
}

fn install_blocking(
    git: &GitSkillFacade,
    request: ChannelInstallRequest,
) -> Result<ChannelInstallReceipt, SharedChannelError> {
    if request.selected_skill_ids.is_empty() {
        return Ok(ChannelInstallReceipt {
            skills: Vec::new(),
            newly_installed_skill_ids: Vec::new(),
        });
    }
    let source = format!(
        "{}#{}",
        request.repository.clone_url, request.manifest.commit_sha
    );
    // Network and the release-content check stay outside the transaction.
    // Only the canonical write below takes it.
    let fetched = git
        .fetch_repo_scanned(&source, true)
        .map_err(|error| git_read_error(error, "Unable to read the selected channel release"))?;
    validate_fetched_release(&request, &fetched)?;

    let _guard = crate::skill_update::acquire_update_transaction_lock()
        .map_err(|error| install_error(format!("Unable to lock channel installation: {error}")))?;
    commit_fetched_install(git, &request, &fetched)
}

fn validate_fetched_release(
    request: &ChannelInstallRequest,
    fetched: &crate::skill_install::FetchedScan,
) -> Result<(), SharedChannelError> {
    let discovered = fetched
        .skills
        .iter()
        .map(|skill| (skill.id.to_ascii_lowercase(), skill))
        .collect::<BTreeMap<_, _>>();
    let released = selected_release_skills(request);
    for id in &request.selected_skill_ids {
        let key = id.to_ascii_lowercase();
        let released_skill = released.get(&key).ok_or_else(content_integrity_error)?;
        let discovered_skill = discovered.get(&key).ok_or_else(content_integrity_error)?;
        if discovered_skill.folder_path != released_skill.content_root
            || released_skill.content_hash_version != CHANNEL_CONTENT_HASH_VERSION
        {
            return Err(content_integrity_error());
        }
        let root = if released_skill.content_root.is_empty() {
            fetched.dir.clone()
        } else {
            fetched.dir.join(&released_skill.content_root)
        };
        let snapshot = crate::content::snapshot_path(&released_skill.id, &root)
            .map_err(|_| content_integrity_error())?;
        if snapshot.content_hash != released_skill.content_hash {
            return Err(content_integrity_error());
        }
    }
    Ok(())
}

fn selected_release_skills(
    request: &ChannelInstallRequest,
) -> BTreeMap<String, &super::ChannelReleaseSkill> {
    request
        .manifest
        .skills
        .iter()
        .filter(|skill| {
            request
                .selected_skill_ids
                .iter()
                .any(|selected| selected.eq_ignore_ascii_case(&skill.id))
        })
        .map(|skill| (skill.id.to_ascii_lowercase(), skill))
        .collect()
}

fn commit_fetched_install(
    git: &GitSkillFacade,
    request: &ChannelInstallRequest,
    fetched: &crate::skill_install::FetchedScan,
) -> Result<ChannelInstallReceipt, SharedChannelError> {
    let repo_dir = fetched.dir.clone();
    let released = selected_release_skills(request);
    let existing_lock = crate::skill_lock::load();
    let hub_skills_dir = ss_core::infra::paths::hub_skills_dir();
    let mut targets = Vec::new();

    for id in &request.selected_skill_ids {
        let key = id.to_ascii_lowercase();
        let released_skill = released.get(&key).ok_or_else(content_integrity_error)?;
        if ss_core::infra::paths::local_skills_dir()
            .join(&released_skill.id)
            .symlink_metadata()
            .is_ok()
        {
            return Err(selection_conflict(&released_skill.id));
        }
        if hub_skills_dir
            .join(&released_skill.id)
            .symlink_metadata()
            .is_ok()
        {
            subscribed_skill_from_lock(request, released_skill, &existing_lock)?;
        } else {
            targets.push(crate::repo_scanner::SkillInstallTarget {
                id: released_skill.id.clone(),
                folder_path: released_skill.content_root.clone(),
                pinned: false,
            });
        }
    }

    let target_ids = targets
        .iter()
        .map(|target| target.id.clone())
        .collect::<Vec<_>>();
    let newly_installed_skill_ids = if targets.is_empty() {
        Vec::new()
    } else {
        match git.install_verified_channel_checkout(
            &repo_dir,
            &request.repository.clone_url,
            &request.manifest.commit_sha,
            &targets,
            &super::ChannelInstallAuthority::for_repository(request.repository.id),
        ) {
            Ok(installed) => installed,
            Err(error) => {
                return Err(rollback_after_failure(
                    &target_ids,
                    install_error(format!(
                        "Unable to install the selected channel Skills: {error}"
                    )),
                ));
            }
        }
    };
    let expected_new = targets
        .iter()
        .map(|target| target.id.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    let actual_new = newly_installed_skill_ids
        .iter()
        .map(|id| id.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    if actual_new != expected_new {
        return Err(rollback_after_failure(
            &newly_installed_skill_ids,
            install_error(
                "One or more selected channel Skills could not be staged without replacing local content",
            ),
        ));
    }
    let result = installed_receipt(&request, &newly_installed_skill_ids);
    let receipt = match result {
        Ok(receipt) => receipt,
        Err(error) => {
            return Err(rollback_after_failure(&newly_installed_skill_ids, error));
        }
    };

    let mut deployment_failures = Vec::new();
    for skill in &receipt.skills {
        match crate::deployment::resync_existing_links(&skill.id) {
            Ok(report) => deployment_failures.extend(report.failures),
            Err(error) => deployment_failures.push(format!("{}: {error:#}", skill.id)),
        }
    }
    let canonical_ids = receipt
        .skills
        .iter()
        .map(|skill| skill.id.clone())
        .collect::<Vec<_>>();
    let project_summary = crate::projects::cascade_skill_update_to_projects(&canonical_ids);
    deployment_failures.extend(
        project_summary
            .failures
            .into_iter()
            .map(|failure| format!("Project {failure}")),
    );
    if !deployment_failures.is_empty() {
        return Err(rollback_after_failure(
            &newly_installed_skill_ids,
            install_error(format!(
                "Installed channel Skills could not be reconciled to every Agent or Project: {}",
                deployment_failures.join(", ")
            )),
        ));
    }
    Ok(receipt)
}

fn installed_receipt(
    request: &ChannelInstallRequest,
    newly_installed_skill_ids: &[String],
) -> Result<ChannelInstallReceipt, SharedChannelError> {
    let lockfile = crate::skill_lock::load();
    let released = request
        .manifest
        .skills
        .iter()
        .map(|skill| (skill.id.to_ascii_lowercase(), skill))
        .collect::<BTreeMap<_, _>>();
    let mut skills = Vec::with_capacity(request.selected_skill_ids.len());
    let mut seen = BTreeSet::new();
    for id in &request.selected_skill_ids {
        let key = id.to_ascii_lowercase();
        if !seen.insert(key.clone()) {
            return Err(content_integrity_error());
        }
        let release = released.get(&key).ok_or_else(content_integrity_error)?;
        skills.push(subscribed_skill_from_lock(request, release, &lockfile)?);
    }
    skills.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(ChannelInstallReceipt {
        skills,
        newly_installed_skill_ids: newly_installed_skill_ids.to_vec(),
    })
}

fn verify_install_receipt(receipt: &ChannelInstallReceipt) -> Result<(), SharedChannelError> {
    let lockfile = crate::skill_lock::load();
    for skill in &receipt.skills {
        verify_installed_skill(skill, &lockfile)?;
    }
    Ok(())
}

fn verify_installed_skill(
    skill: &ChannelSubscribedSkill,
    lock: &crate::skill_lock::SkillLock,
) -> Result<(), SharedChannelError> {
    // D-081: canonical copies carry no checkout; the guarantee is the lock's
    // provenance plus the canonical content hash equaling the release hash.
    let entry = lock
        .skills
        .get(&skill.id)
        .ok_or_else(content_integrity_error)?;
    let current = crate::content::snapshot(&skill.id).map_err(|_| content_integrity_error())?;
    if current.content_hash != skill.release_content_hash
        || skill.baseline_hash != skill.release_content_hash
        || entry.git_ref.as_deref() != Some(skill.provenance.git_ref.as_str())
        || entry.skill_path.as_deref().unwrap_or_default() != skill.provenance.source_folder
        || !crate::source_resolver::same_remote_url(
            &entry.source_url,
            &skill.provenance.repository_url,
        )
    {
        return Err(content_integrity_error());
    }
    Ok(())
}

fn rollback_install_receipt_preserving_changes(
    receipt: &ChannelInstallReceipt,
) -> Result<(), SharedChannelError> {
    let lockfile = crate::skill_lock::load();
    let mut failures = Vec::new();
    for name in receipt.newly_installed_skill_ids.iter().rev() {
        let Some(skill) = receipt
            .skills
            .iter()
            .find(|skill| skill.id.eq_ignore_ascii_case(name))
        else {
            failures.push(format!("{name}: staged receipt is missing"));
            continue;
        };
        if verify_installed_skill(skill, &lockfile).is_err() {
            failures.push(format!("{name}: current content changed and was preserved"));
            continue;
        }
        if let Err(error) = crate::skill_install::uninstall_skill_locked_unchecked(name) {
            failures.push(format!("{name}: {error}"));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(install_error(failures.join(", ")))
    }
}

fn with_install_rollback(
    error: SharedChannelError,
    rollback: Result<(), SharedChannelError>,
) -> SharedChannelError {
    match rollback {
        Ok(()) => error,
        Err(rollback) => SharedChannelError::new(
            error.code,
            format!(
                "{}; the staged channel Skill could not be rolled back: {}",
                error.message, rollback.message
            ),
        ),
    }
}

fn subscribed_skill_from_lock(
    request: &ChannelInstallRequest,
    release: &super::ChannelReleaseSkill,
    lock: &crate::skill_lock::SkillLock,
) -> Result<ChannelSubscribedSkill, SharedChannelError> {
    let entry = lock
        .skills
        .get(&release.id)
        .ok_or_else(content_integrity_error)?;
    if !crate::source_resolver::same_remote_url(&entry.source_url, &request.repository.clone_url)
        || entry.git_ref.as_deref() != Some(request.manifest.commit_sha.as_str())
        || entry.skill_path.as_deref().unwrap_or_default() != release.content_root
    {
        return Err(selection_conflict(&release.id));
    }
    // D-081: the install lock carries no content baseline; the channel's own
    // baseline is the canonical content snapshot, which must still equal the
    // reviewed release hash.
    let baseline_hash = crate::content::snapshot(&release.id)
        .map_err(|_| content_integrity_error())?
        .content_hash;
    if release.content_hash_version != CHANNEL_CONTENT_HASH_VERSION
        || baseline_hash != release.content_hash
    {
        return Err(SharedChannelError::new(
            SharedChannelErrorCode::SubscriptionSelectionInvalid,
            format!(
                "Skill '{}' differs from the reviewed channel release and cannot be adopted",
                release.id
            ),
        ));
    }
    Ok(ChannelSubscribedSkill {
        id: release.id.clone(),
        content_root: release.content_root.clone(),
        release_content_hash: release.content_hash.clone(),
        release_content_hash_version: release.content_hash_version,
        baseline_hash,
        baseline_hash_version: release.content_hash_version,
        provenance: ChannelSkillProvenance {
            repository_id: request.repository.id,
            repository_url: entry.source_url.clone(),
            git_ref: request.manifest.commit_sha.clone(),
            source_folder: release.content_root.clone(),
        },
    })
}

fn selection_conflict(id: &str) -> SharedChannelError {
    SharedChannelError::new(
        SharedChannelErrorCode::SubscriptionSelectionInvalid,
        format!("Skill '{id}' is already installed from another source or channel release"),
    )
}

fn rollback_new_installs(names: &[String]) -> Result<(), SharedChannelError> {
    let mut failures = Vec::new();
    for name in names.iter().rev() {
        if let Err(error) = crate::skill_install::uninstall_skill_locked_unchecked(name) {
            failures.push(format!("{name}: {error}"));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(install_error(format!(
            "Unable to roll back installed channel Skills: {}",
            failures.join(", ")
        )))
    }
}

fn rollback_after_failure(names: &[String], original: SharedChannelError) -> SharedChannelError {
    match rollback_new_installs(names) {
        Ok(()) => original,
        Err(rollback) => install_error(format!(
            "{}; rollback is incomplete and manual cleanup may be required: {}",
            original.message, rollback.message
        )),
    }
}

fn content_integrity_error() -> SharedChannelError {
    SharedChannelError::new(
        SharedChannelErrorCode::Integrity,
        "The selected channel Skill content does not match the published manifest",
    )
}

fn install_error(message: impl Into<String>) -> SharedChannelError {
    SharedChannelError::new(SharedChannelErrorCode::SubscriptionInstallFailed, message)
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;
    use crate::channels::shared_channels::{
        CHANNEL_RELEASE_MANIFEST_VERSION, ChannelPublisherIdentity, ChannelReleaseManifest,
        ChannelReleaseSkill, ChannelSkillReleaseStatus, RemoteRepository, RepositoryPermissions,
    };
    use crate::git::transport::GitOperationSession;
    use std::collections::HashMap;
    use std::ffi::OsStr;
    use std::fs;

    #[test]
    fn rollback_failure_is_never_hidden_by_the_original_error() {
        let error = rollback_after_failure(&["../invalid".into()], content_integrity_error());

        assert_eq!(
            error.code,
            SharedChannelErrorCode::SubscriptionInstallFailed
        );
        assert!(error.message.contains("rollback is incomplete"));
        assert!(error.message.contains("Invalid Skill name"));
    }

    #[test]
    fn exact_release_install_refreshes_existing_agent_and_project_copies() {
        let _guard = crate::lock_test_env();
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let data = temp.path().join("data");
        let repository = temp.path().join("channel.git");
        let project = temp.path().join("project");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&project).unwrap();
        let previous_home = std::env::var_os("HOME");
        let previous_data = std::env::var_os("SKILLSTAR_DATA_DIR");
        let previous_codex = std::env::var_os("CODEX_HOME");
        let previous_hub = std::env::var_os("SKILLSTAR_HUB_DIR");
        set_env("HOME", &home);
        set_env("SKILLSTAR_DATA_DIR", &data);
        // The merged skills test binary may inherit an outer Hub override.
        set_env("SKILLSTAR_HUB_DIR", temp.path().join("hub"));
        remove_env("CODEX_HOME");
        crate::deployment::invalidate_profile_cache();

        let result = (|| {
            fs::create_dir_all(&repository)?;
            fs::write(
                repository.join("SKILL.md"),
                "---\nname: root-skill\ndescription: Root Skill\n---\n# root\n",
            )?;
            fs::create_dir_all(repository.join("skills/writer"))?;
            fs::write(
                repository.join("skills/writer/SKILL.md"),
                "---\nname: writer\ndescription: Shared writer\n---\n# released\n",
            )?;
            git(&repository, &["init", "-q"])?;
            git(&repository, &["config", "user.email", "test@example.com"])?;
            git(&repository, &["config", "user.name", "SkillStar Test"])?;
            git(&repository, &["add", "."])?;
            git(&repository, &["commit", "-qm", "release"])?;
            let commit = git_output(&repository, &["rev-parse", "HEAD"])?;
            let release_hash =
                crate::content::snapshot_path("writer", &repository.join("skills/writer"))?
                    .content_hash;
            assert!(crate::agents::toggle_profile("codex")?);
            let agent_copy = home.join(".codex/skills/writer");
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

            let request = ChannelInstallRequest {
                repository: RemoteRepository {
                    id: 42,
                    owner_id: 7,
                    owner_login: "acme".into(),
                    owner_type: "Organization".into(),
                    name: "channel".into(),
                    default_branch: "main".into(),
                    html_url: "https://github.com/acme/channel".into(),
                    // D-081 fetches the remote directly (no repo cache), so
                    // the fixture serves the release over a file:// remote.
                    clone_url: format!("file://{}", repository.display()),
                    private: true,
                    permissions: RepositoryPermissions {
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
                    tag_name: "channel-v000001".into(),
                    commit_sha: commit.clone(),
                    publisher: ChannelPublisherIdentity {
                        id: 9,
                        login: "alice".into(),
                    },
                    published_at: "2026-08-05T00:00:00Z".into(),
                    title: "Release".into(),
                    notes: String::new(),
                    skills: vec![ChannelReleaseSkill {
                        id: "writer".into(),
                        content_root: "skills/writer".into(),
                        content_hash: release_hash,
                        content_hash_version: CHANNEL_CONTENT_HASH_VERSION,
                        status: ChannelSkillReleaseStatus::Added,
                    }],
                },
                selected_skill_ids: vec!["writer".into()],
            };
            let git = GitSkillFacade::new(GitOperationSession::public());
            let receipt = install_blocking(&git, request)?;

            assert_eq!(receipt.skills.len(), 1);
            assert_eq!(receipt.newly_installed_skill_ids, vec!["writer"]);
            assert_eq!(
                receipt.skills[0].baseline_hash,
                receipt.skills[0].release_content_hash
            );
            assert!(fs::read_to_string(agent_copy.join("SKILL.md"))?.contains("# released"));
            assert!(fs::read_to_string(project_copy.join("SKILL.md"))?.contains("# released"));
            Ok::<(), anyhow::Error>(())
        })();

        match previous_home {
            Some(value) => set_env("HOME", value),
            None => remove_env("HOME"),
        }
        match previous_data {
            Some(value) => set_env("SKILLSTAR_DATA_DIR", value),
            None => remove_env("SKILLSTAR_DATA_DIR"),
        }
        match previous_hub {
            Some(value) => set_env("SKILLSTAR_HUB_DIR", value),
            None => remove_env("SKILLSTAR_HUB_DIR"),
        }
        match previous_codex {
            Some(value) => set_env("CODEX_HOME", value),
            None => remove_env("CODEX_HOME"),
        }
        crate::deployment::invalidate_profile_cache();
        result.unwrap();
    }

    fn git(repository: &std::path::Path, args: &[&str]) -> anyhow::Result<()> {
        let output = ss_core::infra::path_env::command_with_path("git")
            .current_dir(repository)
            .args(args)
            .output()?;
        if output.status.success() {
            Ok(())
        } else {
            anyhow::bail!("git failed: {}", String::from_utf8_lossy(&output.stderr))
        }
    }

    fn git_output(repository: &std::path::Path, args: &[&str]) -> anyhow::Result<String> {
        let output = ss_core::infra::path_env::command_with_path("git")
            .current_dir(repository)
            .args(args)
            .output()?;
        if output.status.success() {
            Ok(String::from_utf8(output.stdout)?.trim().to_string())
        } else {
            anyhow::bail!("git failed: {}", String::from_utf8_lossy(&output.stderr))
        }
    }

    fn set_env<K: AsRef<OsStr>, V: AsRef<OsStr>>(key: K, value: V) {
        unsafe { std::env::set_var(key, value) }
    }

    fn remove_env<K: AsRef<OsStr>>(key: K) {
        unsafe { std::env::remove_var(key) }
    }
}
