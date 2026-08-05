use super::subscription::release_target;
use super::{
    ChannelReleaseManifest, ChannelReleaseTarget, ChannelSkillPin, ChannelSkillReleaseStatus,
    ChannelSkillUpdateRequest, ChannelSubscription, ChannelSubscriptionFacade,
    ChannelSubscriptionGateway, ChannelSubscriptionInstaller, ChannelSubscriptionRegistry,
    ChannelSubscriptionUpdater, ChannelUpdateSnapshot, RemoteRepository,
    SHARED_CHANNEL_MUTATION_GATE, SharedChannelError, SharedChannelErrorCode,
    SharedChannelRegistry, validate_manifest,
};
use crate::skill_update::LocalDivergenceResolution;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChannelSkillRollbackTarget {
    pub target: ChannelReleaseTarget,
    pub title: String,
    pub published_at: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RollbackChannelSkillRequest {
    pub repository_id: u64,
    pub skill_id: String,
    pub target: ChannelReleaseTarget,
    #[serde(default)]
    pub resolution: Option<LocalDivergenceResolution>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChannelSkillRollbackResult {
    pub snapshot: ChannelUpdateSnapshot,
    pub pin: ChannelSkillPin,
}

impl<G, C, S, I> ChannelSubscriptionFacade<G, C, S, I>
where
    G: ChannelSubscriptionGateway,
    C: SharedChannelRegistry,
    S: ChannelSubscriptionRegistry + Clone + 'static,
    I: ChannelSubscriptionInstaller + ChannelSubscriptionUpdater,
{
    pub async fn list_skill_rollback_targets(
        &self,
        repository_id: u64,
        skill_id: &str,
    ) -> Result<Vec<ChannelSkillRollbackTarget>, SharedChannelError> {
        validate_skill_id(skill_id)?;
        let _mutation_guard = SHARED_CHANNEL_MUTATION_GATE.lock().await;
        let _registry_lease = self.channels.acquire_mutation_lease().await?;
        let _subscription_lease = self.subscriptions.acquire_mutation_lease().await?;
        let mut store = self.subscriptions.load_mutable()?;
        let index = subscription_index(&store.subscriptions, repository_id)?;
        super::subscription_remote::ensure_remote_access(&store.subscriptions[index])?;
        let installed = installed_skill(&store.subscriptions[index], skill_id)?.clone();
        let channel = self.active_channel(repository_id)?;
        let remote = async {
            let repository = self.validated_repository(&channel).await?;
            let manifests = self
                .verified_manifests(&repository, channel.repository_id, channel.organization_id)
                .await?;
            Ok::<_, SharedChannelError>(manifests)
        }
        .await;
        let manifests = match remote {
            Ok(manifests) => manifests,
            Err(error) => {
                super::subscription_remote::persist_definitive_remote_failure(
                    &self.subscriptions,
                    &mut store,
                    index,
                    &error,
                )?;
                return Err(error);
            }
        };
        let current =
            installed_release_target(&store.subscriptions[index], &installed, &manifests)?;

        let mut targets = manifests
            .into_values()
            .filter(|manifest| manifest.revision < current.revision)
            .filter_map(|manifest| {
                active_skill(&manifest, &installed.id).map(|skill| ChannelSkillRollbackTarget {
                    target: release_target(&manifest),
                    title: manifest.title.clone(),
                    published_at: manifest.published_at.clone(),
                    content_hash: skill.content_hash.clone(),
                })
            })
            .collect::<Vec<_>>();
        targets.sort_by_key(|target| std::cmp::Reverse(target.target.revision));
        Ok(targets)
    }

    pub async fn rollback_skill(
        &self,
        request: RollbackChannelSkillRequest,
    ) -> Result<ChannelSkillRollbackResult, SharedChannelError> {
        validate_skill_id(&request.skill_id)?;
        let _mutation_guard = SHARED_CHANNEL_MUTATION_GATE.lock().await;
        let _registry_lease = self.channels.acquire_mutation_lease().await?;
        let _subscription_lease = self.subscriptions.acquire_mutation_lease().await?;
        let mut store = self.subscriptions.load_mutable()?;
        let index = subscription_index(&store.subscriptions, request.repository_id)?;
        super::subscription_remote::ensure_remote_access(&store.subscriptions[index])?;
        let channel = self.active_channel(request.repository_id)?;
        let remote = async {
            let repository = self.validated_repository(&channel).await?;
            let manifests = self
                .verified_manifests(&repository, channel.repository_id, channel.organization_id)
                .await?;
            Ok::<_, SharedChannelError>((repository, manifests))
        }
        .await;
        let (repository, manifests) = match remote {
            Ok(value) => value,
            Err(error) => {
                super::subscription_remote::persist_definitive_remote_failure(
                    &self.subscriptions,
                    &mut store,
                    index,
                    &error,
                )?;
                return Err(error);
            }
        };
        let latest = manifests
            .last_key_value()
            .map(|(_, manifest)| manifest.clone())
            .ok_or_else(no_releases)?;
        super::channel_update::validate_manifest_progress(&store.subscriptions[index], &latest)?;
        let manifest = manifests
            .get(&request.target.revision)
            .filter(|manifest| release_target(manifest) == request.target)
            .cloned()
            .ok_or_else(release_conflict)?;
        let installed = installed_skill(&store.subscriptions[index], &request.skill_id)?.clone();
        let current =
            installed_release_target(&store.subscriptions[index], &installed, &manifests)?;
        if request.target.revision >= current.revision {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::ReleaseConflict,
                "Choose a published release older than the Skill's installed release",
            ));
        }
        let released = active_skill(&manifest, &installed.id)
            .cloned()
            .ok_or_else(|| {
                SharedChannelError::new(
                    SharedChannelErrorCode::ReleaseConflict,
                    "The selected historical release does not contain this Skill",
                )
            })?;
        if released.id != installed.id {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::Integrity,
                "The historical release changed the Skill identity casing",
            ));
        }

        let receipt = self
            .installer
            .apply(ChannelSkillUpdateRequest {
                repository,
                manifest: manifest.clone(),
                released,
                installed,
                resolution: request.resolution,
            })
            .await;
        let receipt = match receipt {
            Ok(receipt) => receipt,
            Err(error) => {
                super::subscription_remote::persist_definitive_remote_failure(
                    &self.subscriptions,
                    &mut store,
                    index,
                    &error,
                )?;
                return Err(error);
            }
        };
        if let Err(error) = self.installer.verify(&receipt).await {
            return Err(rollback_after_error(&self.installer, &receipt, error).await);
        }

        if let Some(skill) = store.subscriptions[index]
            .skills
            .iter_mut()
            .find(|skill| skill.id.eq_ignore_ascii_case(&request.skill_id))
        {
            *skill = receipt.installed.clone();
        }
        let pin = ChannelSkillPin {
            skill_id: receipt.installed.id.clone(),
            target: release_target(&manifest),
        };
        store.subscriptions[index]
            .pins
            .retain(|existing| !existing.skill_id.eq_ignore_ascii_case(&pin.skill_id));
        store.subscriptions[index].pins.push(pin.clone());
        store.subscriptions[index]
            .pins
            .sort_by(|left, right| left.skill_id.cmp(&right.skill_id));
        let snapshot = match self
            .build_update_snapshot(&store.subscriptions[index], &latest)
            .await
        {
            Ok(snapshot) => snapshot,
            Err(error) => return Err(rollback_after_error(&self.installer, &receipt, error).await),
        };
        store.subscriptions[index].last_update = Some(snapshot.clone());
        store.subscriptions[index].updated_at = Utc::now().to_rfc3339();
        let subscriptions = self.subscriptions.clone();
        if let Err(error) = self
            .installer
            .verify_and_commit(&receipt, Box::new(move || subscriptions.save(&store)))
            .await
        {
            return Err(rollback_after_error(&self.installer, &receipt, error).await);
        }
        Ok(ChannelSkillRollbackResult { snapshot, pin })
    }

    pub async fn resume_following_skill(
        &self,
        repository_id: u64,
        skill_id: &str,
    ) -> Result<ChannelUpdateSnapshot, SharedChannelError> {
        validate_skill_id(skill_id)?;
        let _mutation_guard = SHARED_CHANNEL_MUTATION_GATE.lock().await;
        let _registry_lease = self.channels.acquire_mutation_lease().await?;
        let _subscription_lease = self.subscriptions.acquire_mutation_lease().await?;
        let mut store = self.subscriptions.load_mutable()?;
        let index = subscription_index(&store.subscriptions, repository_id)?;
        super::subscription_remote::ensure_remote_access(&store.subscriptions[index])?;
        installed_skill(&store.subscriptions[index], skill_id)?;
        let channel = self.active_channel(repository_id)?;
        let remote = async {
            let repository = self.validated_repository(&channel).await?;
            let manifests = self
                .verified_manifests(&repository, channel.repository_id, channel.organization_id)
                .await?;
            Ok::<_, SharedChannelError>(manifests)
        }
        .await;
        let manifests = match remote {
            Ok(manifests) => manifests,
            Err(error) => {
                super::subscription_remote::persist_definitive_remote_failure(
                    &self.subscriptions,
                    &mut store,
                    index,
                    &error,
                )?;
                return Err(error);
            }
        };
        let latest = manifests
            .last_key_value()
            .map(|(_, manifest)| manifest)
            .ok_or_else(no_releases)?;
        super::channel_update::validate_manifest_progress(&store.subscriptions[index], latest)?;
        store.subscriptions[index]
            .pins
            .retain(|pin| !pin.skill_id.eq_ignore_ascii_case(skill_id));
        let snapshot = self
            .build_update_snapshot(&store.subscriptions[index], latest)
            .await?;
        store.subscriptions[index].last_update = Some(snapshot.clone());
        store.subscriptions[index].updated_at = Utc::now().to_rfc3339();
        self.subscriptions.save(&store)?;
        Ok(snapshot)
    }

    async fn verified_manifests(
        &self,
        repository: &RemoteRepository,
        repository_id: u64,
        organization_id: u64,
    ) -> Result<BTreeMap<u64, ChannelReleaseManifest>, SharedChannelError> {
        let mut verified = BTreeMap::new();
        for manifest in self.gateway.published_manifests(repository).await? {
            validate_manifest(&manifest, repository_id, organization_id)?;
            if verified.insert(manifest.revision, manifest).is_some() {
                return Err(SharedChannelError::new(
                    SharedChannelErrorCode::Integrity,
                    "The channel contains duplicate published release revisions",
                ));
            }
        }
        Ok(verified)
    }
}

fn installed_release_target(
    subscription: &ChannelSubscription,
    installed: &super::ChannelSubscribedSkill,
    manifests: &BTreeMap<u64, ChannelReleaseManifest>,
) -> Result<ChannelReleaseTarget, SharedChannelError> {
    if let Some(pin) = subscription
        .pins
        .iter()
        .find(|pin| pin.skill_id.eq_ignore_ascii_case(&installed.id))
    {
        let manifest = manifests
            .get(&pin.target.revision)
            .filter(|manifest| release_target(manifest) == pin.target)
            .ok_or_else(release_conflict)?;
        verify_installed_release(installed, manifest)?;
        return Ok(pin.target.clone());
    }
    if let Some(manifest) = manifests
        .get(&subscription.target.revision)
        .filter(|manifest| release_target(manifest) == subscription.target)
        .filter(|manifest| verify_installed_release(installed, manifest).is_ok())
    {
        return Ok(release_target(manifest));
    }
    manifests
        .values()
        .rev()
        .find(|manifest| verify_installed_release(installed, manifest).is_ok())
        .map(release_target)
        .ok_or_else(|| {
            SharedChannelError::new(
                SharedChannelErrorCode::Integrity,
                "The installed Skill does not match any verified channel release",
            )
        })
}

fn verify_installed_release(
    installed: &super::ChannelSubscribedSkill,
    manifest: &ChannelReleaseManifest,
) -> Result<(), SharedChannelError> {
    let released = active_skill(manifest, &installed.id).ok_or_else(release_conflict)?;
    if released.id != installed.id
        || released.content_root != installed.content_root
        || released.content_hash != installed.release_content_hash
        || released.content_hash_version != installed.release_content_hash_version
        || installed.provenance.repository_id != manifest.repository_id
        || installed.provenance.git_ref != manifest.commit_sha
        || installed.provenance.source_folder != released.content_root
    {
        return Err(release_conflict());
    }
    Ok(())
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

fn subscription_index(
    subscriptions: &[ChannelSubscription],
    repository_id: u64,
) -> Result<usize, SharedChannelError> {
    subscriptions
        .iter()
        .position(|subscription| subscription.repository_id == repository_id)
        .ok_or_else(subscription_not_found)
}

fn installed_skill<'a>(
    subscription: &'a ChannelSubscription,
    skill_id: &str,
) -> Result<&'a super::ChannelSubscribedSkill, SharedChannelError> {
    subscription
        .skills
        .iter()
        .find(|skill| skill.id.eq_ignore_ascii_case(skill_id))
        .ok_or_else(|| {
            SharedChannelError::new(
                SharedChannelErrorCode::SubscriptionSelectionInvalid,
                "The requested Skill is not part of this subscription",
            )
        })
}

fn validate_skill_id(skill_id: &str) -> Result<(), SharedChannelError> {
    if skill_id.trim().is_empty() || skill_id != skill_id.trim() {
        return Err(SharedChannelError::new(
            SharedChannelErrorCode::SubscriptionSelectionInvalid,
            "Choose a subscribed Skill before selecting its history",
        ));
    }
    Ok(())
}

async fn rollback_after_error<I: ChannelSubscriptionUpdater>(
    installer: &I,
    receipt: &super::ChannelSkillUpdateReceipt,
    error: SharedChannelError,
) -> SharedChannelError {
    match installer.rollback(receipt).await {
        Ok(()) => error,
        Err(rollback) => SharedChannelError::new(
            error.code,
            format!(
                "{}; the staged historical version could not be rolled back: {}",
                error.message, rollback.message
            ),
        ),
    }
}

fn release_conflict() -> SharedChannelError {
    SharedChannelError::new(
        SharedChannelErrorCode::ReleaseConflict,
        "The selected historical channel release changed; refresh its history before retrying",
    )
}

fn subscription_not_found() -> SharedChannelError {
    SharedChannelError::new(
        SharedChannelErrorCode::SubscriptionNotFound,
        "This channel has not been subscribed on this device",
    )
}

fn no_releases() -> SharedChannelError {
    SharedChannelError::new(
        SharedChannelErrorCode::ReleaseNotFound,
        "This channel has no verified published release",
    )
}
