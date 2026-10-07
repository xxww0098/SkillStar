use super::git_read::git_read_error;
use super::{
    CHANNEL_CONTENT_HASH_VERSION, ChannelSkillProvenance, ChannelSkillUpdateReceipt,
    ChannelSkillUpdateRequest, ChannelSubscribedSkill, ChannelSubscriptionUpdater,
    ChannelUpdateInspection, GitChannelSubscriptionInstaller, RetainedChannelContent,
    SharedChannelError, SharedChannelErrorCode,
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
            let _guard =
                crate::skill_update::acquire_update_transaction_lock().map_err(|error| {
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
            let _guard =
                crate::skill_update::acquire_update_transaction_lock().map_err(|error| {
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
            let _guard =
                crate::skill_update::acquire_update_transaction_lock().map_err(|error| {
                    update_error(format!("Unable to lock channel rollback: {error}"))
                })?;
            rollback_preserving_current(&receipt)
        })
        .await
        .map_err(|_| update_error("The channel update rollback task stopped unexpectedly"))?
    }

    async fn finalize(&self, receipt: &ChannelSkillUpdateReceipt) {
        let receipt = receipt.clone();
        let _ = tokio::task::spawn_blocking(move || release_retained(&receipt)).await;
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
    let entry = crate::skill_lock::load()
        .skills
        .get(&receipt.installed.id)
        .cloned()
        .ok_or_else(|| update_error("The updated Skill provenance is missing"))?;
    validate_previous(&receipt.installed, &entry)
}

fn rollback_preserving_current(
    receipt: &ChannelSkillUpdateReceipt,
) -> Result<(), SharedChannelError> {
    let current = crate::content::snapshot(&receipt.previous.id).map_err(|error| {
        update_error(format!(
            "Unable to inspect current content before channel rollback: {error}"
        ))
    })?;
    let restores_to = receipt
        .retained
        .as_ref()
        .map_or(&receipt.previous.baseline_hash, |retained| {
            &retained.content_hash
        });
    // Content that is neither what was there before nor what the update
    // installed was edited after the apply; keep it before restoring.
    if current.content_hash != *restores_to
        && let ChannelUpdateInspection::Divergent {
            suggested_local_name,
            ..
        } = inspect_blocking(&receipt.installed)?
    {
        crate::local_skill::preserve_installed_copy(&receipt.installed.id, &suggested_local_name)
            .map_err(|error| {
            update_error(format!(
                "Unable to preserve content changed during channel rollback: {error:#}"
            ))
        })?;
    }
    rollback_exact(receipt)
}

fn inspect_blocking(
    skill: &ChannelSubscribedSkill,
) -> Result<ChannelUpdateInspection, SharedChannelError> {
    // D-081: channel-managed skills keep their own baseline in the
    // subscription store; divergence is the canonical content drifting away
    // from it. There is no shared checkout to inspect.
    let snapshot = crate::content::snapshot(&skill.id).map_err(|error| {
        update_error(format!(
            "Unable to capture the installed channel Skill '{}': {error}",
            skill.id
        ))
    })?;
    if snapshot.content_hash != skill.baseline_hash
        || skill.baseline_hash_version != CHANNEL_CONTENT_HASH_VERSION
    {
        return Ok(ChannelUpdateInspection::Divergent {
            reason: crate::skill_update::LocalDivergenceReason::ContentChanged,
            suggested_local_name: crate::skill_update::divergence::suggested_local_name(&skill.id),
            error: None,
        });
    }
    Ok(ChannelUpdateInspection::Clean)
}

fn apply_blocking(
    git: &crate::git_skill::GitSkillFacade,
    request: ChannelSkillUpdateRequest,
) -> Result<ChannelSkillUpdateReceipt, SharedChannelError> {
    if request.installed.provenance.repository_id != request.repository.id {
        return Err(content_integrity_error());
    }
    // Local reads only. An already-divergent Skill, or a lock that no longer
    // matches, fails before the network and before any canonical write.
    ensure_installed_ready(&request)?;

    let source = format!(
        "{}#{}",
        request.repository.clone_url, request.manifest.commit_sha
    );
    // Fetch and the release-content check stay outside the transaction.
    // Only the canonical replacement below takes it. A failure here returns
    // directly: nothing local has been replaced, so there is no rollback.
    let fetched = git
        .fetch_repo_scanned(&source, true)
        .map_err(|error| git_read_error(error, "Unable to read channel update"))?;
    verify_fetched_update(&request, &fetched)?;

    let _guard = crate::skill_update::acquire_update_transaction_lock()
        .map_err(|error| update_error(format!("Unable to lock channel update: {error}")))?;
    replace_fetched_channel_update(git, request, &fetched)
}

fn ensure_installed_ready(request: &ChannelSkillUpdateRequest) -> Result<(), SharedChannelError> {
    let inspection = inspect_blocking(&request.installed)?;
    if inspection != ChannelUpdateInspection::Clean && request.resolution.is_none() {
        return Err(update_error(format!(
            "Skill '{}' changed while waiting to update",
            request.installed.id
        )));
    }
    let previous_lock_entry = crate::skill_lock::load()
        .skills
        .get(&request.installed.id)
        .cloned()
        .ok_or_else(|| update_error("The installed Skill provenance is missing"))?;
    validate_previous(&request.installed, &previous_lock_entry)
}

fn verify_fetched_update(
    request: &ChannelSkillUpdateRequest,
    fetched: &crate::skill_install::FetchedScan,
) -> Result<(), SharedChannelError> {
    let discovered = fetched
        .skills
        .iter()
        .map(|skill| (skill.id.to_ascii_lowercase(), skill))
        .collect::<BTreeMap<_, _>>();
    let target = discovered
        .get(&request.released.id.to_ascii_lowercase())
        .ok_or_else(content_integrity_error)?;
    if target.folder_path != request.released.content_root
        || request.released.content_hash_version != CHANNEL_CONTENT_HASH_VERSION
    {
        return Err(content_integrity_error());
    }
    let target_root = if request.released.content_root.is_empty() {
        fetched.dir.clone()
    } else {
        fetched.dir.join(&request.released.content_root)
    };
    let target_snapshot = crate::content::snapshot_path(&request.released.id, &target_root)
        .map_err(|_| content_integrity_error())?;
    if target_snapshot.content_hash != request.released.content_hash {
        return Err(content_integrity_error());
    }
    Ok(())
}

fn replace_fetched_channel_update(
    git: &crate::git_skill::GitSkillFacade,
    request: ChannelSkillUpdateRequest,
    fetched: &crate::skill_install::FetchedScan,
) -> Result<ChannelSkillUpdateReceipt, SharedChannelError> {
    // The fetch ran without the lock. Re-read the lock and the canonical
    // content, then replace. A mismatch here is still before replacement.
    let previous_lock_entry = crate::skill_lock::load()
        .skills
        .get(&request.installed.id)
        .cloned()
        .ok_or_else(|| update_error("The installed Skill provenance is missing"))?;
    validate_previous(&request.installed, &previous_lock_entry)?;

    // Edits made while fetching are only overwritten with an explicit choice.
    let resolution = match inspect_blocking(&request.installed)? {
        ChannelUpdateInspection::Clean => None,
        ChannelUpdateInspection::Divergent { .. } => match &request.resolution {
            Some(resolution) => Some(resolution.clone()),
            None => {
                return Err(update_error(format!(
                    "Skill '{}' changed while fetching the reviewed release and was not updated",
                    request.installed.id
                )));
            }
        },
    };
    let preserved = match resolution {
        Some(crate::skill_update::LocalDivergenceResolution::Preserve { local_name }) => {
            crate::local_skill::preserve_installed_copy(&request.installed.id, &local_name)
                .map_err(|error| {
                    update_error(format!(
                        "Unable to preserve local changes for '{}': {error:#}",
                        request.installed.id
                    ))
                })?;
            Some(local_name)
        }
        _ => None,
    };
    let retained = match retain_canonical(&request.installed.id) {
        Ok(retained) => retained,
        Err(error) => return Err(drop_preserved(preserved.as_deref(), error)),
    };
    let mut receipt = ChannelSkillUpdateReceipt {
        previous: request.installed.clone(),
        installed: request.installed.clone(),
        previous_lock_entry,
        previous_update_available: crate::update_state::get(&request.installed.id),
        update_state_revision_after_apply: None,
        retained: Some(retained),
    };

    let target = crate::repo_scanner::SkillInstallTarget {
        id: request.released.id.clone(),
        folder_path: request.released.content_root.clone(),
        pinned: false,
    };
    if let Err(error) = git.install_verified_channel_checkout(
        &fetched.dir,
        &request.repository.clone_url,
        &request.manifest.commit_sha,
        &[target],
        &super::ChannelInstallAuthority::for_repository(request.repository.id),
    ) {
        let error = update_error(format!(
            "Unable to stage channel Skill update '{}': {error:#}",
            request.installed.id
        ));
        // The staged installer restores the old folder on its own failures;
        // only a failure after the lock was rewritten needs a rollback.
        let replaced = crate::skill_lock::load().skills.get(&request.installed.id)
            != Some(&receipt.previous_lock_entry);
        if !replaced {
            release_retained(&receipt);
            return Err(drop_preserved(preserved.as_deref(), error));
        }
        return Err(abandon(&receipt, preserved.as_deref(), error));
    }
    receipt.installed = match subscribed_skill_from_lock(&request) {
        Ok(installed) => installed,
        Err(error) => return Err(abandon(&receipt, preserved.as_deref(), error)),
    };
    if let Err(error) = reconcile(&request.installed.id) {
        return Err(abandon(&receipt, preserved.as_deref(), error));
    }
    receipt.update_state_revision_after_apply = Some(crate::update_state::set_stamped(
        &request.installed.id,
        false,
    ));
    Ok(receipt)
}

fn retain_canonical(skill_id: &str) -> Result<RetainedChannelContent, SharedChannelError> {
    let content_hash = crate::content::snapshot(skill_id)
        .map_err(|error| {
            update_error(format!(
                "Unable to capture '{skill_id}' before its channel update: {error}"
            ))
        })?
        .content_hash;
    let path = crate::materialize::retain_copy(&ss_core::infra::paths::agents_skill_dir(skill_id))
        .map_err(|error| update_error(format!("Unable to back up '{skill_id}': {error:#}")))?;
    Ok(RetainedChannelContent { path, content_hash })
}

fn release_retained(receipt: &ChannelSkillUpdateReceipt) {
    if let Some(retained) = &receipt.retained
        && let Err(error) = crate::materialize::remove_entry(&retained.path)
    {
        tracing::warn!(target: "shared_channels", path = %retained.path.display(), "failed to remove a channel update backup: {error:#}");
    }
}

/// The user's "keep a local copy" choice only made sense together with the
/// replacement; once the original content is back, the copy is a duplicate.
fn drop_preserved(local_name: Option<&str>, mut error: SharedChannelError) -> SharedChannelError {
    if let Some(local_name) = local_name
        && let Err(cleanup) = crate::local_skill::delete(local_name)
    {
        error.message = format!(
            "{}; the local copy '{local_name}' was kept because removing it failed: {cleanup:#}",
            error.message
        );
    }
    error
}

fn abandon(
    receipt: &ChannelSkillUpdateReceipt,
    preserved: Option<&str>,
    original: SharedChannelError,
) -> SharedChannelError {
    // Still inside the apply transaction: what is in place now is what this
    // apply wrote, so it is restored from the local backup without a fetch.
    match rollback_exact(receipt) {
        Ok(()) => drop_preserved(preserved, original),
        Err(rollback) => update_error(format!(
            "{}; rollback is incomplete and manual cleanup may be required: {}",
            original.message, rollback.message
        )),
    }
}

fn subscribed_skill_from_lock(
    request: &ChannelSkillUpdateRequest,
) -> Result<ChannelSubscribedSkill, SharedChannelError> {
    let lock = crate::skill_lock::load();
    let entry = lock
        .skills
        .get(&request.released.id)
        .ok_or_else(content_integrity_error)?;
    // D-081: the channel baseline is the canonical content snapshot, which
    // the staged install just verified against the release hash.
    let baseline_hash = crate::content::snapshot(&request.released.id)
        .map_err(|_| content_integrity_error())?
        .content_hash;
    if !crate::source_resolver::same_remote_url(&entry.source_url, &request.repository.clone_url)
        || entry.git_ref.as_deref() != Some(request.manifest.commit_sha.as_str())
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
    entry: &crate::skill_lock::SkillLockEntry,
) -> Result<(), SharedChannelError> {
    if !crate::source_resolver::same_remote_url(&entry.source_url, &skill.provenance.repository_url)
        || entry.git_ref.as_deref() != Some(skill.provenance.git_ref.as_str())
        || entry.skill_path.as_deref().unwrap_or_default() != skill.content_root
    {
        return Err(update_error(format!(
            "Skill '{}' provenance changed after the channel update check",
            skill.id
        )));
    }
    Ok(())
}

fn rollback_exact(receipt: &ChannelSkillUpdateReceipt) -> Result<(), SharedChannelError> {
    let skill_id = &receipt.previous.id;
    // Local restore only. Refetching the previous release would use whatever
    // session the caller happened to hold — including an anonymous one that
    // cannot read a private channel repository — and would be wrong offline.
    let Some(retained) = &receipt.retained else {
        return Err(update_error(format!(
            "Skill '{skill_id}' has no local channel-update backup to restore; it was left unchanged"
        )));
    };
    crate::materialize::restore_retained(
        &ss_core::infra::paths::agents_skill_dir(skill_id),
        &retained.path,
    )
    .map_err(|error| {
        update_error(format!(
            "Unable to restore previous Skill content: {error:#}"
        ))
    })?;
    let expected = retained.content_hash.clone();
    crate::skill_lock::mutate(|lock| {
        lock.upsert(skill_id, receipt.previous_lock_entry.clone());
    })
    .map_err(|error| update_error(format!("Unable to restore rollback provenance: {error}")))?;
    crate::installed_skill::invalidate_cache();
    let snapshot = crate::content::snapshot(skill_id)
        .map_err(|error| update_error(format!("Unable to verify restored Skill: {error}")))?;
    if snapshot.content_hash != expected {
        return Err(update_error(
            "Restored Skill content does not match its previous content",
        ));
    }
    if let Some(revision) = receipt.update_state_revision_after_apply {
        crate::update_state::restore_if_revision(
            skill_id,
            revision,
            receipt.previous_update_available,
        );
    }
    reconcile(skill_id)?;
    Ok(())
}

fn reconcile(skill_id: &str) -> Result<(), SharedChannelError> {
    let mut failures = Vec::new();
    match crate::deployment::resync_existing_links(skill_id) {
        Ok(report) => failures.extend(report.failures),
        Err(error) => failures.push(format!("Agent reconciliation: {error:#}")),
    }
    let project = crate::projects::cascade_skill_update_to_projects(&[skill_id.to_string()]);
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
