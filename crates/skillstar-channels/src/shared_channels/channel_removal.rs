use super::subscription::{release_target, validate_receipt};
use super::{
    ChannelInstallReceipt, ChannelInstallRequest, ChannelReleaseManifest,
    ChannelSkillReleaseStatus, ChannelSubscribedSkill, ChannelSubscription,
    ChannelSubscriptionFacade, ChannelSubscriptionGateway, ChannelSubscriptionInstaller,
    ChannelSubscriptionRegistry, ChannelSubscriptionUpdater, ChannelUpdateItemState,
    ChannelUpdateSnapshot, SHARED_CHANNEL_MUTATION_GATE, SharedChannelError,
    SharedChannelErrorCode, SharedChannelRegistry,
};
use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConvertRemovedChannelSkillRequest {
    pub repository_id: u64,
    pub skill_id: String,
    pub local_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HandleRemovedChannelSkillResult {
    pub skill_id: String,
    pub local_name: Option<String>,
    pub snapshot: ChannelUpdateSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InstallChannelSkillResult {
    pub subscription: ChannelSubscription,
    pub snapshot: ChannelUpdateSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HandleRevokedChannelSkillResult {
    pub skill_id: String,
    pub local_name: Option<String>,
    pub subscription: ChannelSubscription,
}

#[async_trait]
pub trait ChannelRemovedSkillHandler: Send + Sync {
    async fn uninstall_and_commit(
        &self,
        skill: &ChannelSubscribedSkill,
        commit: Box<dyn FnOnce() -> Result<(), SharedChannelError> + Send>,
    ) -> Result<(), SharedChannelError>;

    async fn convert_to_local_and_commit(
        &self,
        skill: &ChannelSubscribedSkill,
        local_name: &str,
        commit: Box<dyn FnOnce() -> Result<(), SharedChannelError> + Send>,
    ) -> Result<(), SharedChannelError>;
}

enum RemovedSkillAction {
    Uninstall,
    ConvertToLocal(String),
}

impl<G, C, S, I> ChannelSubscriptionFacade<G, C, S, I>
where
    G: ChannelSubscriptionGateway,
    C: SharedChannelRegistry,
    S: ChannelSubscriptionRegistry + Clone + 'static,
    I: ChannelSubscriptionInstaller + ChannelSubscriptionUpdater + ChannelRemovedSkillHandler,
{
    pub async fn uninstall_removed_skill(
        &self,
        repository_id: u64,
        skill_id: &str,
    ) -> Result<HandleRemovedChannelSkillResult, SharedChannelError> {
        self.handle_removed_skill(repository_id, skill_id, RemovedSkillAction::Uninstall)
            .await
    }

    pub async fn convert_removed_skill_to_local(
        &self,
        request: ConvertRemovedChannelSkillRequest,
    ) -> Result<HandleRemovedChannelSkillResult, SharedChannelError> {
        validate_local_copy_name(&request.skill_id, &request.local_name)?;
        self.handle_removed_skill(
            request.repository_id,
            &request.skill_id,
            RemovedSkillAction::ConvertToLocal(request.local_name),
        )
        .await
    }

    pub async fn uninstall_revoked_skill(
        &self,
        repository_id: u64,
        skill_id: &str,
    ) -> Result<HandleRevokedChannelSkillResult, SharedChannelError> {
        self.handle_revoked_skill(repository_id, skill_id, RemovedSkillAction::Uninstall)
            .await
    }

    pub async fn convert_revoked_skill_to_local(
        &self,
        request: ConvertRemovedChannelSkillRequest,
    ) -> Result<HandleRevokedChannelSkillResult, SharedChannelError> {
        validate_local_copy_name(&request.skill_id, &request.local_name)?;
        self.handle_revoked_skill(
            request.repository_id,
            &request.skill_id,
            RemovedSkillAction::ConvertToLocal(request.local_name),
        )
        .await
    }

    pub async fn install_channel_skill(
        &self,
        repository_id: u64,
        skill_id: &str,
    ) -> Result<InstallChannelSkillResult, SharedChannelError> {
        validate_skill_id(skill_id)?;
        let _mutation_guard = SHARED_CHANNEL_MUTATION_GATE.lock().await;
        let _registry_lease = self.channels.acquire_mutation_lease().await?;
        let _subscription_lease = self.subscriptions.acquire_mutation_lease().await?;
        let mut store = self.subscriptions.load_mutable()?;
        let index = subscription_index(&store.subscriptions, repository_id)?;
        super::subscription_remote::ensure_remote_access(&store.subscriptions[index])?;
        if store.subscriptions[index]
            .skills
            .iter()
            .any(|skill| skill.id.eq_ignore_ascii_case(skill_id))
        {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::SubscriptionSelectionInvalid,
                "This Skill is already tracked by the channel subscription",
            ));
        }
        let remote = async {
            let channel = self.active_channel(repository_id)?;
            let repository = self.validated_repository(&channel).await?;
            let manifest = self.latest_manifest(&repository, &channel).await?;
            self.installer
                .verify_release_content(&repository, &manifest)
                .await?;
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
        store = self.subscriptions.load_mutable()?;
        let index = subscription_index(&store.subscriptions, repository_id)?;
        if let Err(error) = super::channel_update::validate_manifest_progress(
            &store.subscriptions[index],
            &manifest,
        ) {
            super::subscription_remote::persist_remote_failure(
                &self.subscriptions,
                &mut store,
                index,
                &error,
            )?;
            return Err(error);
        }
        let released = active_skill(&manifest, skill_id).ok_or_else(|| {
            SharedChannelError::new(
                SharedChannelErrorCode::ReleaseConflict,
                "The latest channel release no longer contains this Skill",
            )
        })?;
        if released.id != skill_id {
            let error = SharedChannelError::new(
                SharedChannelErrorCode::Integrity,
                "The reintroduced channel Skill changed identity casing",
            );
            super::subscription_remote::persist_remote_failure(
                &self.subscriptions,
                &mut store,
                index,
                &error,
            )?;
            return Err(error);
        }
        self.persist_refreshed_channel(&channel, &repository)?;
        store = self.subscriptions.load_mutable()?;
        let index = subscription_index(&store.subscriptions, repository_id)?;
        let receipt = self
            .installer
            .install(ChannelInstallRequest {
                repository: repository.clone(),
                manifest: manifest.clone(),
                selected_skill_ids: vec![released.id.clone()],
            })
            .await;
        let receipt = match receipt {
            Ok(receipt) => receipt,
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
        if let Err(error) = validate_receipt(&receipt, &repository, &manifest, &[released]) {
            let error = rollback_install(&self.installer, &receipt, error).await;
            if let Err(persist_error) = super::subscription_remote::persist_remote_failure(
                &self.subscriptions,
                &mut store,
                index,
                &error,
            ) {
                return Err(SharedChannelError::new(
                    error.code,
                    format!(
                        "{}; the staged install was rolled back, but its frozen remote state could not be saved: {}",
                        error.message, persist_error.message
                    ),
                ));
            }
            return Err(error);
        }
        let mut next_subscription = store.subscriptions[index].clone();
        next_subscription.skills.extend(receipt.skills.clone());
        next_subscription
            .skills
            .sort_by(|left, right| left.id.cmp(&right.id));
        next_subscription
            .known_skill_ids
            .retain(|id| !id.eq_ignore_ascii_case(skill_id));
        next_subscription.known_skill_ids.push(released.id.clone());
        next_subscription.known_skill_ids.sort();
        let snapshot = match self
            .refresh_after_selection_change(&mut next_subscription, &manifest)
            .await
        {
            Ok(snapshot) => snapshot,
            Err(error) => {
                let error = rollback_install(&self.installer, &receipt, error).await;
                if let Err(persist_error) = super::subscription_remote::persist_remote_failure(
                    &self.subscriptions,
                    &mut store,
                    index,
                    &error,
                ) {
                    return Err(SharedChannelError::new(
                        error.code,
                        format!(
                            "{}; the staged install was rolled back, but its frozen remote state could not be saved: {}",
                            error.message, persist_error.message
                        ),
                    ));
                }
                return Err(error);
            }
        };
        let mut next_store = store.clone();
        next_store.subscriptions[index] = next_subscription;
        let subscriptions = self.subscriptions.clone();
        let committed_store = next_store.clone();
        self.installer
            .verify_and_commit_install(
                &receipt,
                Box::new(move || subscriptions.save(&committed_store)),
            )
            .await?;
        Ok(InstallChannelSkillResult {
            subscription: next_store.subscriptions[index].clone(),
            snapshot,
        })
    }

    async fn handle_removed_skill(
        &self,
        repository_id: u64,
        skill_id: &str,
        action: RemovedSkillAction,
    ) -> Result<HandleRemovedChannelSkillResult, SharedChannelError> {
        validate_skill_id(skill_id)?;
        let _mutation_guard = SHARED_CHANNEL_MUTATION_GATE.lock().await;
        let _registry_lease = self.channels.acquire_mutation_lease().await?;
        let _subscription_lease = self.subscriptions.acquire_mutation_lease().await?;
        let mut store = self.subscriptions.load_mutable()?;
        let index = subscription_index(&store.subscriptions, repository_id)?;
        super::subscription_remote::ensure_remote_access(&store.subscriptions[index])?;
        let installed = store.subscriptions[index]
            .skills
            .iter()
            .find(|skill| skill.id.eq_ignore_ascii_case(skill_id))
            .cloned()
            .ok_or_else(|| {
                SharedChannelError::new(
                    SharedChannelErrorCode::SubscriptionSelectionInvalid,
                    "This Skill is no longer tracked by the channel subscription",
                )
            })?;
        let remote = async {
            let channel = self.active_channel(repository_id)?;
            let repository = self.validated_repository(&channel).await?;
            let manifest = self.latest_manifest(&repository, &channel).await?;
            self.installer
                .verify_release_content(&repository, &manifest)
                .await?;
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
        store = self.subscriptions.load_mutable()?;
        let index = subscription_index(&store.subscriptions, repository_id)?;
        if let Err(error) = super::channel_update::validate_manifest_progress(
            &store.subscriptions[index],
            &manifest,
        ) {
            super::subscription_remote::persist_remote_failure(
                &self.subscriptions,
                &mut store,
                index,
                &error,
            )?;
            return Err(error);
        }
        if active_skill(&manifest, &installed.id).is_some()
            && !super::channel_update::has_pending_removal(
                &store.subscriptions[index],
                &installed.id,
            )
        {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::ReleaseConflict,
                "The latest channel release contains this Skill again; refresh before choosing a removal action",
            ));
        }
        self.persist_refreshed_channel(&channel, &repository)?;
        store = self.subscriptions.load_mutable()?;
        let index = subscription_index(&store.subscriptions, repository_id)?;
        let installed = store.subscriptions[index]
            .skills
            .iter()
            .find(|skill| skill.id.eq_ignore_ascii_case(skill_id))
            .cloned()
            .ok_or_else(|| {
                SharedChannelError::new(
                    SharedChannelErrorCode::SubscriptionSelectionInvalid,
                    "This Skill is no longer tracked by the channel subscription",
                )
            })?;
        let mut next_subscription = store.subscriptions[index].clone();
        untrack_skill(&mut next_subscription, &installed.id);
        let snapshot = match self
            .refresh_after_selection_change(&mut next_subscription, &manifest)
            .await
        {
            Ok(snapshot) => snapshot,
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
        let mut next_store = store.clone();
        next_store.subscriptions[index] = next_subscription;
        let local_name = match action {
            RemovedSkillAction::Uninstall => {
                let subscriptions = self.subscriptions.clone();
                let committed_store = next_store.clone();
                self.installer
                    .uninstall_and_commit(
                        &installed,
                        Box::new(move || subscriptions.save(&committed_store)),
                    )
                    .await?;
                None
            }
            RemovedSkillAction::ConvertToLocal(local_name) => {
                let result_name = local_name.clone();
                let subscriptions = self.subscriptions.clone();
                let committed_store = next_store.clone();
                self.installer
                    .convert_to_local_and_commit(
                        &installed,
                        &local_name,
                        Box::new(move || subscriptions.save(&committed_store)),
                    )
                    .await?;
                Some(result_name)
            }
        };
        Ok(HandleRemovedChannelSkillResult {
            skill_id: installed.id,
            local_name,
            snapshot,
        })
    }

    async fn handle_revoked_skill(
        &self,
        repository_id: u64,
        skill_id: &str,
        action: RemovedSkillAction,
    ) -> Result<HandleRevokedChannelSkillResult, SharedChannelError> {
        validate_skill_id(skill_id)?;
        let _mutation_guard = SHARED_CHANNEL_MUTATION_GATE.lock().await;
        let _registry_lease = self.channels.acquire_mutation_lease().await?;
        let _subscription_lease = self.subscriptions.acquire_mutation_lease().await?;
        let mut store = self.subscriptions.load_mutable()?;
        let index = subscription_index(&store.subscriptions, repository_id)?;
        if store.subscriptions[index].remote_state.status
            != super::ChannelSubscriptionRemoteStatus::Revoked
        {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::SubscriptionSelectionInvalid,
                "This subscription is not frozen by revoked GitHub access",
            ));
        }
        let installed = store.subscriptions[index]
            .skills
            .iter()
            .find(|skill| skill.id.eq_ignore_ascii_case(skill_id))
            .cloned()
            .ok_or_else(|| {
                SharedChannelError::new(
                    SharedChannelErrorCode::SubscriptionSelectionInvalid,
                    "This Skill is no longer tracked by the channel subscription",
                )
            })?;
        untrack_skill(&mut store.subscriptions[index], &installed.id);
        if let Some(snapshot) = &mut store.subscriptions[index].last_update {
            snapshot
                .items
                .retain(|item| !item.id.eq_ignore_ascii_case(&installed.id));
        }
        if store.subscriptions[index]
            .last_update
            .as_ref()
            .is_some_and(|snapshot| snapshot.items.is_empty())
        {
            store.subscriptions[index].last_update = None;
        } else if let Some(snapshot) = &mut store.subscriptions[index].last_update {
            super::channel_update::refresh_snapshot_status(snapshot);
        }
        store.subscriptions[index].updated_at = Utc::now().to_rfc3339();
        let local_name = match action {
            RemovedSkillAction::Uninstall => {
                let subscriptions = self.subscriptions.clone();
                let committed_store = store.clone();
                self.installer
                    .uninstall_and_commit(
                        &installed,
                        Box::new(move || subscriptions.save(&committed_store)),
                    )
                    .await?;
                None
            }
            RemovedSkillAction::ConvertToLocal(local_name) => {
                let result_name = local_name.clone();
                let subscriptions = self.subscriptions.clone();
                let committed_store = store.clone();
                self.installer
                    .convert_to_local_and_commit(
                        &installed,
                        &local_name,
                        Box::new(move || subscriptions.save(&committed_store)),
                    )
                    .await?;
                Some(result_name)
            }
        };
        Ok(HandleRevokedChannelSkillResult {
            skill_id: installed.id,
            local_name,
            subscription: store.subscriptions[index].clone(),
        })
    }

    async fn refresh_after_selection_change(
        &self,
        subscription: &mut ChannelSubscription,
        manifest: &ChannelReleaseManifest,
    ) -> Result<ChannelUpdateSnapshot, SharedChannelError> {
        let mut snapshot = self.build_update_snapshot(subscription, manifest).await?;
        let selected_at_target = snapshot
            .items
            .iter()
            .filter(|item| item.selected)
            .all(|item| {
                matches!(
                    item.state,
                    ChannelUpdateItemState::Current | ChannelUpdateItemState::Applied
                )
            });
        let has_notification = snapshot
            .items
            .iter()
            .any(|item| item.state == ChannelUpdateItemState::Notification);
        if selected_at_target && !has_notification {
            subscription.target = release_target(manifest);
            subscription.known_skill_ids = active_skill_ids(manifest);
            snapshot = self.build_update_snapshot(subscription, manifest).await?;
        }
        subscription.last_update = Some(snapshot.clone());
        subscription.updated_at = Utc::now().to_rfc3339();
        Ok(snapshot)
    }
}

fn untrack_skill(subscription: &mut ChannelSubscription, skill_id: &str) {
    subscription
        .skills
        .retain(|skill| !skill.id.eq_ignore_ascii_case(skill_id));
    subscription
        .known_skill_ids
        .retain(|id| !id.eq_ignore_ascii_case(skill_id));
    subscription
        .pins
        .retain(|pin| !pin.skill_id.eq_ignore_ascii_case(skill_id));
}

fn active_skill<'a>(
    manifest: &'a ChannelReleaseManifest,
    skill_id: &str,
) -> Option<&'a super::ChannelReleaseSkill> {
    manifest.skills.iter().find(|skill| {
        skill.id.eq_ignore_ascii_case(skill_id)
            && skill.status != ChannelSkillReleaseStatus::Removed
    })
}

fn active_skill_ids(manifest: &ChannelReleaseManifest) -> Vec<String> {
    let mut ids = manifest
        .skills
        .iter()
        .filter(|skill| skill.status != ChannelSkillReleaseStatus::Removed)
        .map(|skill| skill.id.clone())
        .collect::<Vec<_>>();
    ids.sort();
    ids
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

fn validate_skill_id(skill_id: &str) -> Result<(), SharedChannelError> {
    skillstar_skills::content::validate_skill_name(skill_id).map_err(|_| {
        SharedChannelError::new(
            SharedChannelErrorCode::SubscriptionSelectionInvalid,
            "Choose a valid subscribed Skill",
        )
    })
}

fn validate_local_copy_name(skill_id: &str, local_name: &str) -> Result<(), SharedChannelError> {
    skillstar_skills::content::validate_skill_name(local_name).map_err(|_| {
        SharedChannelError::new(
            SharedChannelErrorCode::SubscriptionSelectionInvalid,
            "Choose a valid conflict-safe name for the local Skill copy",
        )
    })?;
    if skill_id.eq_ignore_ascii_case(local_name) {
        return Err(SharedChannelError::new(
            SharedChannelErrorCode::SubscriptionSelectionInvalid,
            "The local copy needs a distinct name so it cannot overwrite the channel Skill",
        ));
    }
    Ok(())
}

async fn rollback_install<I: ChannelSubscriptionInstaller>(
    installer: &I,
    receipt: &ChannelInstallReceipt,
    error: SharedChannelError,
) -> SharedChannelError {
    match ChannelSubscriptionInstaller::rollback(installer, receipt).await {
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
