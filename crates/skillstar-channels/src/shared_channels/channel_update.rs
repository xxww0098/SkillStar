use super::subscription::release_target;
use super::{
    ChannelReleaseManifest, ChannelReleaseSkill, ChannelSubscribedSkill, ChannelSubscription,
    ChannelSubscriptionFacade, ChannelSubscriptionGateway, ChannelSubscriptionInstaller,
    ChannelSubscriptionRegistry, SHARED_CHANNEL_MUTATION_GATE, SharedChannelError,
    SharedChannelErrorCode, SharedChannelRegistry,
};
use chrono::Utc;
use skillstar_skills::skill_update::LocalDivergenceReason;
use std::collections::{BTreeMap, BTreeSet};

pub use super::channel_update_types::*;

impl<G, C, S, I> ChannelSubscriptionFacade<G, C, S, I>
where
    G: ChannelSubscriptionGateway,
    C: SharedChannelRegistry,
    S: ChannelSubscriptionRegistry,
    I: ChannelSubscriptionInstaller + ChannelSubscriptionUpdater,
{
    pub fn update_state(
        &self,
        repository_id: u64,
    ) -> Result<Option<ChannelUpdateSnapshot>, SharedChannelError> {
        Ok(self
            .subscriptions
            .load_mutable()?
            .subscriptions
            .into_iter()
            .find(|subscription| subscription.repository_id == repository_id)
            .and_then(|subscription| subscription.last_update))
    }

    pub async fn check_update(
        &self,
        repository_id: u64,
    ) -> Result<ChannelUpdateSnapshot, SharedChannelError> {
        self.check_update_inner(repository_id, false).await
    }

    /// Background variant of [`Self::check_update`] that never degrades
    /// `remote_state`.
    ///
    /// `remote_state` is what `ensure_remote_access` consults to freeze the
    /// user's manual apply/install/rollback actions, so a transient network
    /// error during an unattended probe must not set it — that would let a
    /// background task lock the user out of a channel for up to an hour.
    /// Probe failures are still recorded in the snapshot's `check_error`, which
    /// is reporting rather than gating. A *successful* probe still clears the
    /// state: proving access can only unblock the user.
    pub async fn probe_update(
        &self,
        repository_id: u64,
    ) -> Result<ChannelUpdateSnapshot, SharedChannelError> {
        self.check_update_inner(repository_id, true).await
    }

    async fn check_update_inner(
        &self,
        repository_id: u64,
        probe_only: bool,
    ) -> Result<ChannelUpdateSnapshot, SharedChannelError> {
        let mark_failure = |subscription: &mut _, error: &SharedChannelError| -> bool {
            if probe_only {
                return false;
            }
            super::subscription_remote::mark_remote_failure(subscription, error)
        };
        let _mutation_guard = SHARED_CHANNEL_MUTATION_GATE.lock().await;
        let _registry_lease = self.channels.acquire_mutation_lease().await?;
        let _subscription_lease = self.subscriptions.acquire_mutation_lease().await?;
        let mut store = self.subscriptions.load_mutable()?;
        let index = subscription_index(&store.subscriptions, repository_id)?;
        let previous = store.subscriptions[index].last_update.clone();
        let remote = async {
            let channel = self.active_channel(repository_id)?;
            let repository = self.validated_repository(&channel).await?;
            let manifest = self.latest_manifest(&repository, &channel).await?;
            Ok::<_, SharedChannelError>((channel, repository, manifest))
        }
        .await;
        let (channel, repository, manifest) = match remote {
            Ok(value) => value,
            Err(error) => {
                let remote_state_changed = mark_failure(&mut store.subscriptions[index], &error);
                if let Some(mut snapshot) = previous {
                    snapshot.checked_at = Utc::now().to_rfc3339();
                    snapshot.check_error = Some(error.message);
                    snapshot.check_error_code = Some(error.code);
                    store.subscriptions[index].last_update = Some(snapshot.clone());
                    store.subscriptions[index].updated_at = snapshot.checked_at.clone();
                    self.subscriptions.save(&store)?;
                    return Ok(snapshot);
                }
                if remote_state_changed {
                    self.subscriptions.save(&store)?;
                }
                return Err(error);
            }
        };
        if let Err(error) = validate_manifest_progress(&store.subscriptions[index], &manifest) {
            mark_failure(&mut store.subscriptions[index], &error);
            if let Some(mut snapshot) = previous {
                snapshot.checked_at = Utc::now().to_rfc3339();
                snapshot.check_error = Some(error.message);
                snapshot.check_error_code = Some(error.code);
                store.subscriptions[index].last_update = Some(snapshot.clone());
                store.subscriptions[index].updated_at = snapshot.checked_at.clone();
                self.subscriptions.save(&store)?;
                return Ok(snapshot);
            }
            self.subscriptions.save(&store)?;
            return Err(error);
        }
        if let Err(error) = self
            .installer
            .verify_release_content(&repository, &manifest)
            .await
        {
            mark_failure(&mut store.subscriptions[index], &error);
            if let Some(mut snapshot) = previous {
                snapshot.checked_at = Utc::now().to_rfc3339();
                snapshot.check_error = Some(error.message);
                snapshot.check_error_code = Some(error.code);
                store.subscriptions[index].last_update = Some(snapshot.clone());
                store.subscriptions[index].updated_at = snapshot.checked_at.clone();
                self.subscriptions.save(&store)?;
                return Ok(snapshot);
            }
            self.subscriptions.save(&store)?;
            return Err(error);
        }
        let snapshot = self
            .build_update_snapshot(&store.subscriptions[index], &manifest)
            .await;
        let snapshot = match snapshot {
            Ok(snapshot) => snapshot,
            Err(error) => {
                let remote_state_changed = mark_failure(&mut store.subscriptions[index], &error);
                if let Some(mut snapshot) = previous {
                    snapshot.checked_at = Utc::now().to_rfc3339();
                    snapshot.check_error = Some(error.message);
                    snapshot.check_error_code = Some(error.code);
                    store.subscriptions[index].last_update = Some(snapshot.clone());
                    store.subscriptions[index].updated_at = snapshot.checked_at.clone();
                    self.subscriptions.save(&store)?;
                    return Ok(snapshot);
                }
                if remote_state_changed {
                    self.subscriptions.save(&store)?;
                }
                return Err(error);
            }
        };
        self.persist_refreshed_channel(&channel, &repository)?;
        store = self.subscriptions.load_mutable()?;
        let index = subscription_index(&store.subscriptions, repository_id)?;
        super::subscription_remote::mark_remote_active(&mut store.subscriptions[index]);
        store.subscriptions[index].last_update = Some(snapshot.clone());
        store.subscriptions[index].updated_at = Utc::now().to_rfc3339();
        self.subscriptions.save(&store)?;
        Ok(snapshot)
    }

    pub async fn apply_update(
        &self,
        request: ApplyChannelUpdateRequest,
    ) -> Result<ApplyChannelUpdateResult, SharedChannelError> {
        self.apply_update_selected(request, None, true, None).await
    }

    pub(super) async fn apply_update_selected(
        &self,
        request: ApplyChannelUpdateRequest,
        allowed_skill_ids: Option<&BTreeSet<String>>,
        acknowledge_release: bool,
        auto_claim_started_at: Option<&str>,
    ) -> Result<ApplyChannelUpdateResult, SharedChannelError> {
        validate_resolutions(&request.resolutions)?;
        let _mutation_guard = SHARED_CHANNEL_MUTATION_GATE.lock().await;
        let _registry_lease = self.channels.acquire_mutation_lease().await?;
        let _subscription_lease = self.subscriptions.acquire_mutation_lease().await?;
        let mut store = self.subscriptions.load_mutable()?;
        let index = subscription_index(&store.subscriptions, request.repository_id)?;
        super::subscription_remote::ensure_remote_access(&store.subscriptions[index])?;
        if let Some(expected_started_at) = auto_claim_started_at {
            let claim = store.subscriptions[index].auto_update.last_run.as_ref();
            if !store.subscriptions[index].auto_update.enabled
                || !claim.is_some_and(|run| {
                    run.status == super::ChannelAutoUpdateRunStatus::Checking
                        && run.started_at == expected_started_at
                })
            {
                return Err(SharedChannelError::new(
                    SharedChannelErrorCode::Cancelled,
                    "Protected automatic upgrade ownership changed before this update could apply",
                ));
            }
        }
        let pinned = store.subscriptions[index]
            .pins
            .iter()
            .map(|pin| pin.skill_id.to_ascii_lowercase())
            .collect::<BTreeSet<_>>();
        let remote = async {
            let channel = self.active_channel(request.repository_id)?;
            let repository = self.validated_repository(&channel).await?;
            let manifest = self.latest_manifest(&repository, &channel).await?;
            Ok::<_, SharedChannelError>((channel, repository, manifest))
        }
        .await;
        let (channel, repository, manifest) = match remote {
            Ok(value) => value,
            Err(error) => {
                super::subscription_remote::persist_remote_failure(
                    &self.subscriptions,
                    &mut store,
                    index,
                    &error,
                )?;
                return Err(error);
            }
        };
        if let Err(error) = validate_manifest_progress(&store.subscriptions[index], &manifest) {
            super::subscription_remote::persist_remote_failure(
                &self.subscriptions,
                &mut store,
                index,
                &error,
            )?;
            return Err(error);
        }
        if release_target(&manifest) != request.target {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::ReleaseConflict,
                "The channel release changed after review; check updates again before applying",
            ));
        }
        if let Err(error) = self
            .installer
            .verify_release_content(&repository, &manifest)
            .await
        {
            super::subscription_remote::persist_remote_failure(
                &self.subscriptions,
                &mut store,
                index,
                &error,
            )?;
            return Err(error);
        }
        let resolutions = request
            .resolutions
            .into_iter()
            .map(|item| (item.skill_id.to_ascii_lowercase(), item.resolution))
            .collect::<BTreeMap<_, _>>();
        let initial = self
            .build_update_snapshot(&store.subscriptions[index], &manifest)
            .await;
        let initial = match initial {
            Ok(initial) => initial,
            Err(error) => {
                super::subscription_remote::persist_remote_failure(
                    &self.subscriptions,
                    &mut store,
                    index,
                    &error,
                )?;
                return Err(error);
            }
        };
        let initial_items = initial
            .items
            .iter()
            .cloned()
            .map(|item| (item.id.to_ascii_lowercase(), item))
            .collect::<BTreeMap<_, _>>();
        let blocked = initial
            .items
            .iter()
            .filter(|item| {
                item.selected
                    && item.change == ChannelUpdateChange::Updated
                    && item.state == ChannelUpdateItemState::Blocked
            })
            .map(|item| item.id.to_ascii_lowercase())
            .collect::<BTreeSet<_>>();
        if resolutions
            .keys()
            .any(|skill_id| !blocked.contains(skill_id))
        {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::SubscriptionSelectionInvalid,
                "Channel update resolutions only apply to currently blocked Skills",
            ));
        }
        self.persist_refreshed_channel(&channel, &repository)?;
        store = self.subscriptions.load_mutable()?;
        let index = subscription_index(&store.subscriptions, request.repository_id)?;
        let released = current_release_skills(&manifest);
        let mut receipts = Vec::new();
        let mut applied = BTreeSet::new();
        let mut failures = BTreeMap::new();
        let subscription_before_apply = store.subscriptions[index].clone();

        for item in &initial.items {
            if item.change != ChannelUpdateChange::Updated
                || !item.selected
                || !matches!(
                    item.state,
                    ChannelUpdateItemState::Available | ChannelUpdateItemState::Blocked
                )
            {
                continue;
            }
            let key = item.id.to_ascii_lowercase();
            if pinned.contains(&key)
                || allowed_skill_ids.is_some_and(|allowed| !allowed.contains(&key))
            {
                continue;
            }
            let Some(installed) = store.subscriptions[index]
                .skills
                .iter()
                .find(|skill| skill.id.eq_ignore_ascii_case(&item.id))
                .cloned()
            else {
                failures.insert(
                    key,
                    SharedChannelError::new(
                        SharedChannelErrorCode::SubscriptionUpdateFailed,
                        "The subscribed Skill record is missing",
                    ),
                );
                continue;
            };
            let Some(target) = released.get(&key).cloned() else {
                failures.insert(
                    key,
                    SharedChannelError::new(
                        SharedChannelErrorCode::Integrity,
                        "The target release Skill is missing",
                    ),
                );
                continue;
            };
            if item.state == ChannelUpdateItemState::Blocked && !resolutions.contains_key(&key) {
                continue;
            }
            match self
                .installer
                .apply(ChannelSkillUpdateRequest {
                    repository: repository.clone(),
                    manifest: manifest.clone(),
                    released: target.clone(),
                    installed,
                    resolution: (item.state == ChannelUpdateItemState::Blocked)
                        .then(|| resolutions.get(&key).cloned())
                        .flatten(),
                })
                .await
            {
                Ok(receipt) => {
                    if let Some(skill) = store.subscriptions[index]
                        .skills
                        .iter_mut()
                        .find(|skill| skill.id.eq_ignore_ascii_case(&item.id))
                    {
                        *skill = receipt.installed.clone();
                    }
                    applied.insert(key);
                    receipts.push(receipt);
                }
                Err(error) => {
                    if super::subscription_remote::mark_remote_failure(
                        &mut store.subscriptions[index],
                        &error,
                    ) {
                        let rollback_failures = self.rollback_applied(&receipts).await;
                        store.subscriptions[index] = subscription_before_apply;
                        super::subscription_remote::mark_remote_failure(
                            &mut store.subscriptions[index],
                            &error,
                        );
                        self.subscriptions.save(&store)?;
                        return Err(with_rollback_failures(error, rollback_failures));
                    }
                    failures.insert(key, error);
                }
            }
        }

        let mut verified_receipts = Vec::with_capacity(receipts.len());
        for receipt in &receipts {
            match self.installer.verify(receipt).await {
                Ok(()) => verified_receipts.push(receipt.clone()),
                Err(error) => {
                    if let Err(rollback) =
                        ChannelSubscriptionUpdater::rollback(&self.installer, receipt).await
                    {
                        let mut rollback_failures = self.rollback_applied(&receipts).await;
                        rollback_failures
                            .push(format!("{}: {}", receipt.previous.id, rollback.message));
                        return Err(with_rollback_failures(error, rollback_failures));
                    }
                    if let Some(skill) = store.subscriptions[index]
                        .skills
                        .iter_mut()
                        .find(|skill| skill.id.eq_ignore_ascii_case(&receipt.previous.id))
                    {
                        *skill = receipt.previous.clone();
                    }
                    let key = receipt.previous.id.to_ascii_lowercase();
                    applied.remove(&key);
                    failures.insert(key, error);
                }
            }
        }
        receipts = verified_receipts;

        let mut snapshot = match self
            .build_update_snapshot(&store.subscriptions[index], &manifest)
            .await
        {
            Ok(snapshot) => snapshot,
            Err(error) => {
                let rollback_failures = self.rollback_applied(&receipts).await;
                if super::subscription_remote::mark_remote_failure(
                    &mut store.subscriptions[index],
                    &error,
                ) {
                    store.subscriptions[index] = subscription_before_apply.clone();
                    super::subscription_remote::mark_remote_failure(
                        &mut store.subscriptions[index],
                        &error,
                    );
                    self.subscriptions.save(&store)?;
                }
                return Err(with_rollback_failures(error, rollback_failures));
            }
        };
        for item in &mut snapshot.items {
            let key = item.id.to_ascii_lowercase();
            if applied.contains(&key) {
                if let Some(initial) = initial_items.get(&key) {
                    item.change = initial.change;
                    item.from_content_hash = initial.from_content_hash.clone();
                    item.to_content_hash = initial.to_content_hash.clone();
                }
                item.state = ChannelUpdateItemState::Applied;
                item.block_reason = None;
                item.suggested_local_name = None;
                item.error = None;
            } else if let Some(error) = failures.get(&key) {
                if let Some(initial) = initial_items.get(&key) {
                    item.change = initial.change;
                    item.from_content_hash = initial.from_content_hash.clone();
                    item.to_content_hash = initial.to_content_hash.clone();
                    item.block_reason = initial.block_reason;
                    item.suggested_local_name = initial.suggested_local_name.clone();
                }
                item.state = ChannelUpdateItemState::Failed;
                item.error = Some(error.message.clone());
                item.error_code = Some(error.code);
            }
        }
        if acknowledge_release && all_selected_at_target(&snapshot.items) {
            store.subscriptions[index].target = release_target(&manifest);
            store.subscriptions[index].known_skill_ids = current_release_skills(&manifest)
                .into_values()
                .map(|skill| skill.id)
                .collect();
            snapshot.acknowledgement_required = false;
            snapshot
                .items
                .retain(|item| item.state != ChannelUpdateItemState::Notification);
        }
        snapshot.status = derive_status(
            &snapshot.items,
            subscription_has_advanced_skill(&store.subscriptions[index], &manifest),
            snapshot.acknowledgement_required,
        );
        store.subscriptions[index].last_update = Some(snapshot.clone());
        store.subscriptions[index].updated_at = Utc::now().to_rfc3339();
        if let Err(error) = self.subscriptions.save(&store) {
            let rollback_failures = self.rollback_applied(&receipts).await;
            return Err(with_rollback_failures(error, rollback_failures));
        }

        Ok(ApplyChannelUpdateResult {
            snapshot,
            applied_skill_ids: applied.into_iter().collect(),
        })
    }

    async fn rollback_applied(&self, receipts: &[ChannelSkillUpdateReceipt]) -> Vec<String> {
        let mut failures = Vec::new();
        for receipt in receipts.iter().rev() {
            if let Err(rollback) =
                ChannelSubscriptionUpdater::rollback(&self.installer, receipt).await
            {
                failures.push(format!("{}: {}", receipt.previous.id, rollback.message));
            }
        }
        failures
    }

    pub(super) async fn build_update_snapshot(
        &self,
        subscription: &ChannelSubscription,
        manifest: &ChannelReleaseManifest,
    ) -> Result<ChannelUpdateSnapshot, SharedChannelError> {
        let released = current_release_skills(manifest);
        let known = subscription
            .known_skill_ids
            .iter()
            .map(|id| id.to_ascii_lowercase())
            .collect::<BTreeSet<_>>();
        let mut tracked = BTreeSet::new();
        let mut items = Vec::new();
        for installed in &subscription.skills {
            let key = installed.id.to_ascii_lowercase();
            tracked.insert(key.clone());
            if has_pending_removal(subscription, &installed.id) {
                items.push(removed_item(installed));
                continue;
            }
            let Some(target) = released.get(&key) else {
                items.push(removed_item(installed));
                continue;
            };
            if target.id != installed.id {
                return Err(SharedChannelError::new(
                    SharedChannelErrorCode::Integrity,
                    format!(
                        "Channel Skill identity casing changed from '{}' to '{}'",
                        installed.id, target.id
                    ),
                ));
            }
            let inspection = match self.installer.inspect(installed).await {
                Ok(inspection) => inspection,
                Err(error) => ChannelUpdateInspection::Divergent {
                    reason: LocalDivergenceReason::SnapshotFailed,
                    suggested_local_name: skillstar_skills::skill_update::suggested_local_name(
                        &installed.id,
                    ),
                    error: Some(error.message),
                },
            };
            if installed.release_content_hash == target.content_hash
                && installed.content_root == target.content_root
            {
                items.push(current_item(installed, target, inspection));
                continue;
            }
            items.push(update_item(installed, target, inspection));
        }
        for (key, skill) in released {
            if tracked.contains(&key)
                || known.contains(&key)
                || manifest.revision <= subscription.target.revision
            {
                continue;
            }
            items.push(ChannelUpdateItem {
                id: skill.id.clone(),
                change: ChannelUpdateChange::Added,
                state: ChannelUpdateItemState::Notification,
                selected: false,
                from_content_hash: None,
                to_content_hash: Some(skill.content_hash.clone()),
                block_reason: None,
                suggested_local_name: None,
                error: None,
                pinned_target: None,
                error_code: None,
            });
        }
        items.sort_by(|left, right| left.id.cmp(&right.id));
        for item in &mut items {
            item.pinned_target = subscription
                .pins
                .iter()
                .find(|pin| pin.skill_id.eq_ignore_ascii_case(&item.id))
                .map(|pin| pin.target.clone());
        }
        let has_advanced = subscription_has_advanced_skill(subscription, manifest);
        let acknowledgement_required = manifest.revision > subscription.target.revision;
        Ok(ChannelUpdateSnapshot {
            target: release_target(manifest),
            title: manifest.title.clone(),
            notes: manifest.notes.clone(),
            publisher: manifest.publisher.clone(),
            published_at: manifest.published_at.clone(),
            checked_at: Utc::now().to_rfc3339(),
            status: derive_status(&items, has_advanced, acknowledgement_required),
            acknowledgement_required,
            items,
            check_error: None,
            check_error_code: None,
        })
    }
}

pub(super) fn has_pending_removal(subscription: &ChannelSubscription, skill_id: &str) -> bool {
    subscription.last_update.as_ref().is_some_and(|snapshot| {
        snapshot.items.iter().any(|item| {
            item.id.eq_ignore_ascii_case(skill_id)
                && item.change == ChannelUpdateChange::Removed
                && (item.state == ChannelUpdateItemState::RemovedFromChannel
                    || (item.state == ChannelUpdateItemState::Blocked
                        && item.block_reason == Some(ChannelUpdateBlockReason::RemovedUpstream)))
        })
    })
}

fn removed_item(installed: &ChannelSubscribedSkill) -> ChannelUpdateItem {
    ChannelUpdateItem {
        id: installed.id.clone(),
        change: ChannelUpdateChange::Removed,
        state: ChannelUpdateItemState::RemovedFromChannel,
        selected: true,
        from_content_hash: Some(installed.release_content_hash.clone()),
        to_content_hash: None,
        block_reason: Some(ChannelUpdateBlockReason::RemovedUpstream),
        suggested_local_name: Some(skillstar_skills::skill_update::suggested_local_name(
            &installed.id,
        )),
        error: None,
        pinned_target: None,
        error_code: None,
    }
}

fn with_rollback_failures(
    original: SharedChannelError,
    failures: Vec<String>,
) -> SharedChannelError {
    if failures.is_empty() {
        original
    } else {
        SharedChannelError::new(
            original.code,
            format!(
                "{}; updated Skills also could not be rolled back: {}",
                original.message,
                failures.join(", ")
            ),
        )
    }
}

fn current_release_skills(
    manifest: &ChannelReleaseManifest,
) -> BTreeMap<String, ChannelReleaseSkill> {
    manifest
        .skills
        .iter()
        .filter(|skill| skill.status != super::ChannelSkillReleaseStatus::Removed)
        .cloned()
        .map(|skill| (skill.id.to_ascii_lowercase(), skill))
        .collect()
}

fn current_item(
    installed: &ChannelSubscribedSkill,
    target: &ChannelReleaseSkill,
    inspection: ChannelUpdateInspection,
) -> ChannelUpdateItem {
    let (state, block_reason, suggested_local_name, error) = match inspection {
        ChannelUpdateInspection::Clean => (ChannelUpdateItemState::Current, None, None, None),
        ChannelUpdateInspection::Divergent {
            reason,
            suggested_local_name,
            error,
        } => (
            ChannelUpdateItemState::Blocked,
            Some(map_divergence_reason(reason)),
            Some(suggested_local_name),
            error,
        ),
    };
    ChannelUpdateItem {
        id: installed.id.clone(),
        change: ChannelUpdateChange::Unchanged,
        state,
        selected: true,
        from_content_hash: Some(installed.release_content_hash.clone()),
        to_content_hash: Some(target.content_hash.clone()),
        block_reason,
        suggested_local_name,
        error,
        pinned_target: None,
        error_code: None,
    }
}

fn update_item(
    installed: &ChannelSubscribedSkill,
    target: &ChannelReleaseSkill,
    inspection: ChannelUpdateInspection,
) -> ChannelUpdateItem {
    let (state, block_reason, suggested_local_name, error) = match inspection {
        ChannelUpdateInspection::Clean => (ChannelUpdateItemState::Available, None, None, None),
        ChannelUpdateInspection::Divergent {
            reason,
            suggested_local_name,
            error,
        } => (
            ChannelUpdateItemState::Blocked,
            Some(map_divergence_reason(reason)),
            Some(suggested_local_name),
            error,
        ),
    };
    ChannelUpdateItem {
        id: installed.id.clone(),
        change: ChannelUpdateChange::Updated,
        state,
        selected: true,
        from_content_hash: Some(installed.release_content_hash.clone()),
        to_content_hash: Some(target.content_hash.clone()),
        block_reason,
        suggested_local_name,
        error,
        pinned_target: None,
        error_code: None,
    }
}

fn map_divergence_reason(reason: LocalDivergenceReason) -> ChannelUpdateBlockReason {
    match reason {
        LocalDivergenceReason::ContentChanged => ChannelUpdateBlockReason::LocalContentChanged,
        LocalDivergenceReason::BaselineMissing => ChannelUpdateBlockReason::BaselineMissing,
        LocalDivergenceReason::SnapshotFailed => ChannelUpdateBlockReason::SnapshotFailed,
        // A channel Skill whose content vanished is reported through the
        // channel's own removed-upstream flow, not the generic updater's.
        LocalDivergenceReason::SourceRemoved | LocalDivergenceReason::SourceMissing => {
            ChannelUpdateBlockReason::RemovedUpstream
        }
    }
}

fn derive_status(
    items: &[ChannelUpdateItem],
    has_advanced: bool,
    acknowledgement_required: bool,
) -> ChannelUpdateStatus {
    let has_available = items
        .iter()
        .any(|item| item.state == ChannelUpdateItemState::Available);
    let has_pending = items.iter().any(|item| {
        matches!(
            item.state,
            ChannelUpdateItemState::Available
                | ChannelUpdateItemState::Blocked
                | ChannelUpdateItemState::Failed
                | ChannelUpdateItemState::RemovedFromChannel
        )
    });
    let has_notification = items
        .iter()
        .any(|item| item.state == ChannelUpdateItemState::Notification);
    if !has_pending && !has_notification && !acknowledgement_required {
        ChannelUpdateStatus::UpToDate
    } else if has_pending && has_advanced {
        ChannelUpdateStatus::PartiallyUpgraded
    } else if has_pending && !has_available {
        ChannelUpdateStatus::Blocked
    } else {
        ChannelUpdateStatus::UpdateAvailable
    }
}

pub(super) fn refresh_snapshot_status(snapshot: &mut ChannelUpdateSnapshot) {
    let has_advanced = snapshot.items.iter().any(|item| {
        matches!(
            item.state,
            ChannelUpdateItemState::Current | ChannelUpdateItemState::Applied
        )
    });
    snapshot.status = derive_status(
        &snapshot.items,
        has_advanced,
        snapshot.acknowledgement_required,
    );
}

fn subscription_has_advanced_skill(
    subscription: &ChannelSubscription,
    manifest: &ChannelReleaseManifest,
) -> bool {
    subscription.target.revision < manifest.revision
        && subscription
            .skills
            .iter()
            .any(|skill| skill.provenance.git_ref == manifest.commit_sha)
}

pub(super) fn validate_manifest_progress(
    subscription: &ChannelSubscription,
    manifest: &ChannelReleaseManifest,
) -> Result<(), SharedChannelError> {
    let target = release_target(manifest);
    let observed = subscription
        .last_update
        .as_ref()
        .map(|snapshot| &snapshot.target)
        .filter(|target| target.revision > subscription.target.revision)
        .unwrap_or(&subscription.target);
    if target.revision < observed.revision
        || (target.revision == observed.revision && target != *observed)
    {
        return Err(SharedChannelError::new(
            SharedChannelErrorCode::ReleaseConflict,
            "The latest visible channel release is older than or conflicts with the subscribed target; refusing to downgrade",
        ));
    }
    Ok(())
}

fn all_selected_at_target(items: &[ChannelUpdateItem]) -> bool {
    items.iter().filter(|item| item.selected).all(|item| {
        matches!(
            item.state,
            ChannelUpdateItemState::Current | ChannelUpdateItemState::Applied
        )
    })
}

fn subscription_index(
    subscriptions: &[ChannelSubscription],
    repository_id: u64,
) -> Result<usize, SharedChannelError> {
    subscriptions
        .iter()
        .position(|subscription| subscription.repository_id == repository_id)
        .ok_or_else(|| {
            SharedChannelError::new(
                SharedChannelErrorCode::SubscriptionNotFound,
                "This channel has not been subscribed on this device",
            )
        })
}

fn validate_resolutions(
    resolutions: &[ChannelSkillUpdateResolution],
) -> Result<(), SharedChannelError> {
    let mut seen = BTreeSet::new();
    for item in resolutions {
        skillstar_skills::content::validate_skill_name(&item.skill_id).map_err(|_| {
            SharedChannelError::new(
                SharedChannelErrorCode::SubscriptionSelectionInvalid,
                "Channel update resolutions contain an invalid Skill identity",
            )
        })?;
        if !seen.insert(item.skill_id.to_ascii_lowercase()) {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::SubscriptionSelectionInvalid,
                "Channel update resolutions contain duplicate Skill identities",
            ));
        }
    }
    Ok(())
}
