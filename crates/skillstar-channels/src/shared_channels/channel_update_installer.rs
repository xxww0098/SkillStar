use super::git_read::git_read_error;
use super::{
    CHANNEL_CONTENT_HASH_VERSION, ChannelSkillProvenance, ChannelSkillUpdateReceipt,
    ChannelSkillUpdateRequest, ChannelSubscribedSkill, ChannelSubscriptionUpdater,
    ChannelUpdateInspection, GitChannelSubscriptionInstaller, SharedChannelError,
    SharedChannelErrorCode,
};
use async_trait::async_trait;
use std::collections::BTreeMap;

#[async_trait]
impl ChannelSubscriptionUpdater for GitChannelSubscriptionInstaller {
    async fn inspect(
        &self,
        skill: &ChannelSubscribedSkill,
    ) -> Result<ChannelUpdateInspection, SharedChannelError> {
        let skill = skill.clone();
        tokio::task::spawn_blocking(move || inspect_blocking(&skill))
            .await
            .map_err(|_| update_error("The channel update inspection task stopped unexpectedly"))?
    }

    async fn apply(
        &self,
        request: ChannelSkillUpdateRequest,
    ) -> Result<ChannelSkillUpdateReceipt, SharedChannelError> {
        let git = self.git.clone();
        tokio::task::spawn_blocking(move || apply_blocking(&git, request))
            .await
            .map_err(|_| update_error("The channel update task stopped unexpectedly"))?
    }

    async fn verify(&self, receipt: &ChannelSkillUpdateReceipt) -> Result<(), SharedChannelError> {
        let receipt = receipt.clone();
        tokio::task::spawn_blocking(move || {
            let _guard = skillstar_skills::skill_update::acquire_update_transaction_lock()
                .map_err(|error| {
                    update_error(format!("Unable to lock channel verification: {error}"))
                })?;
            verify_exact_current(&receipt)
        })
        .await
        .map_err(|_| update_error("The channel update verification task stopped unexpectedly"))?
    }

    async fn verify_and_commit(
        &self,
        receipt: &ChannelSkillUpdateReceipt,
        commit: Box<dyn FnOnce() -> Result<(), SharedChannelError> + Send>,
    ) -> Result<(), SharedChannelError> {
        let receipt = receipt.clone();
        tokio::task::spawn_blocking(move || {
            let _guard = skillstar_skills::skill_update::acquire_update_transaction_lock()
                .map_err(|error| {
                    update_error(format!("Unable to lock channel metadata commit: {error}"))
                })?;
            verify_exact_current(&receipt)?;
            commit()
        })
        .await
        .map_err(|_| update_error("The channel metadata commit task stopped unexpectedly"))?
    }

    async fn rollback(
        &self,
        receipt: &ChannelSkillUpdateReceipt,
    ) -> Result<(), SharedChannelError> {
        let receipt = receipt.clone();
        tokio::task::spawn_blocking(move || {
            let _guard = skillstar_skills::skill_update::acquire_update_transaction_lock()
                .map_err(|error| {
                    update_error(format!("Unable to lock channel rollback: {error}"))
                })?;
            rollback_preserving_current(&receipt)
        })
        .await
        .map_err(|_| update_error("The channel update rollback task stopped unexpectedly"))?
    }
}

fn verify_exact_current(receipt: &ChannelSkillUpdateReceipt) -> Result<(), SharedChannelError> {
    if inspect_blocking(&receipt.installed)? != ChannelUpdateInspection::Clean {
        return Err(update_error(format!(
            "Skill '{}' changed after its channel update was staged",
            receipt.installed.id
        )));
    }
    // D-081: no shared checkout exists; the guarantee is the lock provenance
    // plus canonical content equal to the subscribed baseline.
    let entry = skillstar_skills::skill_lock::load()
        .skills
        .get(&receipt.installed.id)
        .cloned()
        .ok_or_else(|| update_error("The updated Skill provenance is missing"))?;
    validate_previous(&receipt.installed, &entry)
}

fn rollback_preserving_current(
    receipt: &ChannelSkillUpdateReceipt,
) -> Result<(), SharedChannelError> {
    let current = skillstar_skills::content::snapshot(&receipt.previous.id).map_err(|error| {
        update_error(format!(
            "Unable to inspect current content before channel rollback: {error}"
        ))
    })?;
    if current.content_hash == receipt.previous.baseline_hash {
        return rollback_exact(receipt);
    }
    match inspect_blocking(&receipt.installed)? {
        ChannelUpdateInspection::Clean => rollback_exact(receipt),
        ChannelUpdateInspection::Divergent {
            suggested_local_name,
            ..
        } => {
            skillstar_skills::local_skill::preserve_installed_copy(
                &receipt.installed.id,
                &suggested_local_name,
            )
            .map_err(|error| {
                update_error(format!(
                    "Unable to preserve content changed during channel rollback: {error:#}"
                ))
            })?;
            rollback_exact(receipt)
        }
    }
}

fn inspect_blocking(
    skill: &ChannelSubscribedSkill,
) -> Result<ChannelUpdateInspection, SharedChannelError> {
    // D-081: channel-managed skills keep their own baseline in the
    // subscription store; divergence is the canonical content drifting away
    // from it. There is no shared checkout to inspect.
    let snapshot = skillstar_skills::content::snapshot(&skill.id).map_err(|error| {
        update_error(format!(
            "Unable to capture the installed channel Skill '{}': {error}",
            skill.id
        ))
    })?;
    if snapshot.content_hash != skill.baseline_hash
        || skill.baseline_hash_version != CHANNEL_CONTENT_HASH_VERSION
    {
        return Ok(ChannelUpdateInspection::Divergent {
            reason: skillstar_skills::skill_update::LocalDivergenceReason::ContentChanged,
            suggested_local_name: skillstar_skills::skill_update::divergence::suggested_local_name(&skill.id),
            error: None,
        });
    }
    Ok(ChannelUpdateInspection::Clean)
}

fn apply_blocking(
    git: &skillstar_skills::git_skill::GitSkillFacade,
    request: ChannelSkillUpdateRequest,
) -> Result<ChannelSkillUpdateReceipt, SharedChannelError> {
    let _guard = skillstar_skills::skill_update::acquire_update_transaction_lock()
        .map_err(|error| update_error(format!("Unable to lock channel update: {error}")))?;
    let inspection = inspect_blocking(&request.installed)?;
    if request.installed.provenance.repository_id != request.repository.id {
        return Err(content_integrity_error());
    }
    if inspection != ChannelUpdateInspection::Clean && request.resolution.is_none() {
        return Err(update_error(format!(
            "Skill '{}' changed while waiting to update",
            request.installed.id
        )));
    }
    let previous_lock_entry = skillstar_skills::skill_lock::load()
        .skills
        .get(&request.installed.id)
        .cloned()
        .ok_or_else(|| update_error("The installed Skill provenance is missing"))?;
    validate_previous(&request.installed, &previous_lock_entry)?;
    let mut receipt = ChannelSkillUpdateReceipt {
        previous: request.installed.clone(),
        installed: request.installed.clone(),
        previous_lock_entry,
        previous_update_available: skillstar_skills::update_state::get(&request.installed.id),
        update_state_revision_after_apply: None,
    };
    let mut resolved_divergence = false;
    if inspection != ChannelUpdateInspection::Clean {
        let resolution = request
            .resolution
            .clone()
            .expect("resolution checked above");
        if let skillstar_skills::skill_update::LocalDivergenceResolution::Preserve { local_name } =
            &resolution
        {
            skillstar_skills::local_skill::preserve_installed_copy(
                &request.installed.id,
                local_name,
            )
            .map_err(|error| {
                update_error(format!(
                    "Unable to preserve local changes for '{}': {error:#}",
                    request.installed.id
                ))
            })?;
        }
        // D-081: with real canonical copies there is no checkout to clean.
        // Preserving copies the current content to the chosen local name;
        // either resolution delegates the overwrite to the staged install
        // below.
        resolved_divergence = true;
    }

    let source = format!(
        "{}#{}",
        request.repository.clone_url, request.manifest.commit_sha
    );
    let fetched = git
        .fetch_repo_scanned(&source, true)
        .map_err(|error| {
            rollback_if_resolved(
                &receipt,
                resolved_divergence,
                git_read_error(error, "Unable to read channel update"),
            )
        })?;
    let repo_dir = fetched.dir.clone();
    let discovered = fetched
        .skills
        .into_iter()
        .map(|skill| (skill.id.to_ascii_lowercase(), skill))
        .collect::<BTreeMap<_, _>>();
    let target = discovered
        .get(&request.released.id.to_ascii_lowercase())
        .ok_or_else(|| {
            rollback_if_resolved(&receipt, resolved_divergence, content_integrity_error())
        })?;
    if target.folder_path != request.released.content_root
        || request.released.content_hash_version != CHANNEL_CONTENT_HASH_VERSION
    {
        return Err(rollback_if_resolved(
            &receipt,
            resolved_divergence,
            content_integrity_error(),
        ));
    }
    let target_root = if request.released.content_root.is_empty() {
        repo_dir.clone()
    } else {
        repo_dir.join(&request.released.content_root)
    };
    let target_snapshot =
        skillstar_skills::content::snapshot_path(&request.released.id, &target_root).map_err(
            |_| rollback_if_resolved(&receipt, resolved_divergence, content_integrity_error()),
        )?;
    if target_snapshot.content_hash != request.released.content_hash {
        return Err(rollback_if_resolved(
            &receipt,
            resolved_divergence,
            content_integrity_error(),
        ));
    }
    if !resolved_divergence
        && !matches!(inspect_blocking(&request.installed), Ok(ChannelUpdateInspection::Clean))
    {
        return Err(update_error(format!(
            "Skill '{}' changed while fetching the reviewed release and was not updated",
            request.installed.id
        )));
    }
    let target = skillstar_skills::repo_scanner::SkillInstallTarget {
        id: request.released.id.clone(),
        folder_path: request.released.content_root.clone(),
        pinned: false,
    };
    if let Err(error) = git.replace_verified_channel_checkout(
        &repo_dir,
        &request.repository.clone_url,
        &request.installed.provenance.repository_url,
        &request.manifest.commit_sha,
        &[target],
    ) {
        return Err(rollback_after_apply_failure(
            &receipt,
            update_error(format!(
                "Unable to stage channel Skill update '{}': {error:#}",
                request.installed.id
            )),
        ));
    }
    receipt.installed = match subscribed_skill_from_lock(&request) {
        Ok(installed) => installed,
        Err(error) => return Err(rollback_after_apply_failure(&receipt, error)),
    };
    if let Err(error) = reconcile(&request.installed.id) {
        return Err(rollback_after_apply_failure(&receipt, error));
    }
    receipt.update_state_revision_after_apply = Some(skillstar_skills::update_state::set_stamped(
        &request.installed.id,
        false,
    ));
    Ok(receipt)
}

fn subscribed_skill_from_lock(
    request: &ChannelSkillUpdateRequest,
) -> Result<ChannelSubscribedSkill, SharedChannelError> {
    let lock = skillstar_skills::skill_lock::load();
    let entry = lock
        .skills
        .get(&request.released.id)
        .ok_or_else(content_integrity_error)?;
    // D-081: the channel baseline is the canonical content snapshot, which
    // the staged install just verified against the release hash.
    let baseline_hash = skillstar_skills::content::snapshot(&request.released.id)
        .map_err(|_| content_integrity_error())?
        .content_hash;
    if !skillstar_skills::source_resolver::same_remote_url(
        &entry.source_url,
        &request.repository.clone_url,
    ) || entry.git_ref.as_deref() != Some(request.manifest.commit_sha.as_str())
        || entry.skill_path.as_deref().unwrap_or_default() != request.released.content_root
        || request.released.content_hash_version != CHANNEL_CONTENT_HASH_VERSION
        || baseline_hash != request.released.content_hash
    {
        return Err(content_integrity_error());
    }
    Ok(ChannelSubscribedSkill {
        id: request.released.id.clone(),
        content_root: request.released.content_root.clone(),
        release_content_hash: request.released.content_hash.clone(),
        release_content_hash_version: request.released.content_hash_version,
        baseline_hash,
        baseline_hash_version: request.released.content_hash_version,
        provenance: ChannelSkillProvenance {
            repository_id: request.repository.id,
            repository_url: entry.source_url.clone(),
            git_ref: request.manifest.commit_sha.clone(),
            source_folder: request.released.content_root.clone(),
        },
    })
}

fn validate_previous(
    skill: &ChannelSubscribedSkill,
    entry: &skillstar_skills::skill_lock::SkillLockEntry,
) -> Result<(), SharedChannelError> {
    if !skillstar_skills::source_resolver::same_remote_url(
        &entry.source_url,
        &skill.provenance.repository_url,
    ) || entry.git_ref.as_deref() != Some(skill.provenance.git_ref.as_str())
        || entry.skill_path.as_deref().unwrap_or_default() != skill.content_root
    {
        return Err(update_error(format!(
            "Skill '{}' provenance changed after the channel update check",
            skill.id
        )));
    }
    Ok(())
}

fn rollback_if_resolved(
    receipt: &ChannelSkillUpdateReceipt,
    resolved_divergence: bool,
    mut error: SharedChannelError,
) -> SharedChannelError {
    if !resolved_divergence {
        return error;
    }
    match inspect_blocking(&receipt.previous) {
        Ok(ChannelUpdateInspection::Clean) => rollback_after_apply_failure(receipt, error),
        Ok(ChannelUpdateInspection::Divergent {
            suggested_local_name,
            ..
        }) => match skillstar_skills::local_skill::preserve_installed_copy(
            &receipt.previous.id,
            &suggested_local_name,
        ) {
            Ok(_) => {
                error.message = format!(
                    "{}; edits made during the channel fetch were preserved as '{}'",
                    error.message, suggested_local_name
                );
                rollback_after_apply_failure(receipt, error)
            }
            Err(preserve) => update_error(format!(
                "{}; rollback was skipped to avoid discarding edits made during the channel fetch, and those edits could not be copied: {preserve:#}",
                error.message
            )),
        },
        Err(inspect) => update_error(format!(
            "{}; rollback was skipped because newly changed local content could not be verified safely: {}",
            error.message, inspect.message
        )),
    }
}

fn rollback_after_apply_failure(
    receipt: &ChannelSkillUpdateReceipt,
    original: SharedChannelError,
) -> SharedChannelError {
    match rollback_preserving_current(receipt) {
        Ok(()) => original,
        Err(rollback) => update_error(format!(
            "{}; rollback is incomplete and manual cleanup may be required: {}",
            original.message, rollback.message
        )),
    }
}

fn rollback_exact(receipt: &ChannelSkillUpdateReceipt) -> Result<(), SharedChannelError> {
    // D-081: there is no retained checkout — the previous release is fetched
    // at its pinned commit and overwrite-installed, then the previous lock
    // entry is restored.
    let spec = skillstar_skills::source_resolver::Source::parse(&format!(
        "{}#{}",
        receipt.previous.provenance.repository_url, receipt.previous.provenance.git_ref
    ))
    .map_err(|error| update_error(format!("Unable to resolve rollback source: {error}")))?;
    let session = skillstar_skills::git::transport::GitOperationSession::public();
    let checkout = skillstar_skills::fetch::fetch_source(&spec, &session)
        .map_err(|error| update_error(format!("Unable to fetch the rollback release: {error:#}")))?;
    skillstar_skills::installer::install_units(
        checkout.dir(),
        &spec,
        &[skillstar_skills::installer::InstallUnit {
            id: receipt.previous.id.clone(),
            folder_path: receipt.previous.content_root.clone(),
        }],
    )
    .map_err(|error| update_error(format!("Unable to restore previous Skill content: {error:#}")))?;
    skillstar_skills::skill_lock::mutate(|lock| {
        lock.upsert(&receipt.previous.id, receipt.previous_lock_entry.clone());
    })
    .map_err(|error| update_error(format!("Unable to restore rollback provenance: {error}")))?;
    skillstar_skills::installed_skill::invalidate_cache();
    let snapshot = skillstar_skills::content::snapshot(&receipt.previous.id)
        .map_err(|error| update_error(format!("Unable to verify restored Skill: {error}")))?;
    if snapshot.content_hash != receipt.previous.baseline_hash {
        return Err(update_error(
            "Restored Skill content does not match its previous baseline",
        ));
    }
    if let Some(revision) = receipt.update_state_revision_after_apply {
        skillstar_skills::update_state::restore_if_revision(
            &receipt.previous.id,
            revision,
            receipt.previous_update_available,
        );
    }
    reconcile(&receipt.previous.id)?;
    Ok(())
}

fn reconcile(skill_id: &str) -> Result<(), SharedChannelError> {
    let mut failures = Vec::new();
    match skillstar_skills::deployment::resync_existing_links(skill_id) {
        Ok(report) => failures.extend(report.failures),
        Err(error) => failures.push(format!("Agent reconciliation: {error:#}")),
    }
    let project =
        skillstar_skills::projects::cascade_skill_update_to_projects(&[skill_id.to_string()]);
    failures.extend(
        project
            .failures
            .into_iter()
            .map(|failure| format!("Project {failure}")),
    );
    if failures.is_empty() {
        Ok(())
    } else {
        Err(update_error(format!(
            "Skill '{skill_id}' could not be reconciled everywhere: {}",
            failures.join(", ")
        )))
    }
}

fn content_integrity_error() -> SharedChannelError {
    SharedChannelError::new(
        SharedChannelErrorCode::Integrity,
        "The channel update content does not match the published manifest",
    )
}

fn update_error(message: impl Into<String>) -> SharedChannelError {
    SharedChannelError::new(SharedChannelErrorCode::SubscriptionUpdateFailed, message)
}

#[cfg(all(test, not(windows)))]
#[path = "channel_update_installer_tests.rs"]
mod tests;
