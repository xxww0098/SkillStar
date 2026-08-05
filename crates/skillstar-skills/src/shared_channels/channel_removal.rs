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
        crate::content::validate_skill_name(&request.local_name).map_err(|_| {
            SharedChannelError::new(
                SharedChannelErrorCode::SubscriptionSelectionInvalid,
                "Choose a valid conflict-safe name for the local Skill copy",
            )
        })?;
        if request.skill_id.eq_ignore_ascii_case(&request.local_name) {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::SubscriptionSelectionInvalid,
                "The local copy needs a distinct name so it cannot overwrite the channel Skill",
            ));
        }
        self.handle_removed_skill(
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
        let channel = self.active_channel(repository_id)?;
        let repository = self.validated_repository(&channel).await?;
        let manifest = self.latest_manifest(&repository, &channel).await?;
        super::channel_update::validate_manifest_progress(&store.subscriptions[index], &manifest)?;
        let released = active_skill(&manifest, skill_id).ok_or_else(|| {
            SharedChannelError::new(
                SharedChannelErrorCode::ReleaseConflict,
                "The latest channel release no longer contains this Skill",
            )
        })?;
        if released.id != skill_id {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::Integrity,
                "The reintroduced channel Skill changed identity casing",
            ));
        }
        let receipt = self
            .installer
            .install(ChannelInstallRequest {
                repository: repository.clone(),
                manifest: manifest.clone(),
                selected_skill_ids: vec![released.id.clone()],
            })
            .await?;
        if let Err(error) = validate_receipt(&receipt, &repository, &manifest, &[released]) {
            return Err(rollback_install(&self.installer, &receipt, error).await);
        }
        store.subscriptions[index]
            .skills
            .extend(receipt.skills.clone());
        store.subscriptions[index]
            .skills
            .sort_by(|left, right| left.id.cmp(&right.id));
        store.subscriptions[index]
            .known_skill_ids
            .retain(|id| !id.eq_ignore_ascii_case(skill_id));
        store.subscriptions[index]
            .known_skill_ids
            .push(released.id.clone());
        store.subscriptions[index].known_skill_ids.sort();
        let snapshot = match self
            .refresh_after_selection_change(&mut store.subscriptions[index], &manifest)
            .await
        {
            Ok(snapshot) => snapshot,
            Err(error) => return Err(rollback_install(&self.installer, &receipt, error).await),
        };
        let subscriptions = self.subscriptions.clone();
        let committed_store = store.clone();
        self.installer
            .verify_and_commit_install(
                &receipt,
                Box::new(move || subscriptions.save(&committed_store)),
            )
            .await?;
        Ok(InstallChannelSkillResult {
            subscription: store.subscriptions[index].clone(),
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
        let channel = self.active_channel(repository_id)?;
        let repository = self.validated_repository(&channel).await?;
        let manifest = self.latest_manifest(&repository, &channel).await?;
        super::channel_update::validate_manifest_progress(&store.subscriptions[index], &manifest)?;
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
        untrack_skill(&mut store.subscriptions[index], &installed.id);
        let snapshot = self
            .refresh_after_selection_change(&mut store.subscriptions[index], &manifest)
            .await?;
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
        Ok(HandleRemovedChannelSkillResult {
            skill_id: installed.id,
            local_name,
            snapshot,
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
    crate::content::validate_skill_name(skill_id).map_err(|_| {
        SharedChannelError::new(
            SharedChannelErrorCode::SubscriptionSelectionInvalid,
            "Choose a valid subscribed Skill",
        )
    })
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
