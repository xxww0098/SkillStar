use super::{
    CHANNEL_CONTENT_HASH_VERSION, ChannelReleaseManifest, ChannelSkillReleaseStatus,
    RemoteRepository, SHARED_CHANNEL_MUTATION_GATE, SharedChannelDescriptor, SharedChannelError,
    SharedChannelErrorCode, SharedChannelRegistry, SharedChannelRole, SharedChannelStatus,
    project_role, validate_manifest, validate_remote_repository,
};
use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const CHANNEL_SUBSCRIPTION_STORE_VERSION: u32 = 1;
pub const CHANNEL_SUBSCRIPTION_DESCRIPTOR_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelReleaseTarget {
    pub revision: u64,
    pub tag_name: String,
    pub commit_sha: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelSkillProvenance {
    pub repository_id: u64,
    pub repository_url: String,
    pub git_ref: String,
    pub source_folder: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelSubscribedSkill {
    pub id: String,
    pub content_root: String,
    pub release_content_hash: String,
    pub release_content_hash_version: u32,
    pub baseline_hash: String,
    pub baseline_hash_version: u32,
    pub provenance: ChannelSkillProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelSubscription {
    pub descriptor_version: u32,
    pub repository_id: u64,
    pub organization_id: u64,
    pub target: ChannelReleaseTarget,
    pub skills: Vec<ChannelSubscribedSkill>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelSubscriptionStore {
    pub schema_version: u32,
    pub subscriptions: Vec<ChannelSubscription>,
}

impl Default for ChannelSubscriptionStore {
    fn default() -> Self {
        Self {
            schema_version: CHANNEL_SUBSCRIPTION_STORE_VERSION,
            subscriptions: Vec::new(),
        }
    }
}

impl ChannelSubscriptionStore {
    pub fn upsert(&mut self, subscription: ChannelSubscription) {
        if let Some(existing) = self
            .subscriptions
            .iter_mut()
            .find(|item| item.repository_id == subscription.repository_id)
        {
            *existing = subscription;
        } else {
            self.subscriptions.push(subscription);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelSubscriptionView {
    pub schema_version: u32,
    pub descriptor_version: u32,
    pub repository_id: u64,
    pub organization_id: Option<u64>,
    pub target: Option<ChannelReleaseTarget>,
    pub selected_skill_ids: Vec<String>,
    pub read_only: bool,
}

impl ChannelSubscriptionView {
    pub fn from_subscription(subscription: &ChannelSubscription) -> Self {
        Self {
            schema_version: CHANNEL_SUBSCRIPTION_STORE_VERSION,
            descriptor_version: subscription.descriptor_version,
            repository_id: subscription.repository_id,
            organization_id: Some(subscription.organization_id),
            target: Some(subscription.target.clone()),
            selected_skill_ids: subscription
                .skills
                .iter()
                .map(|skill| skill.id.clone())
                .collect(),
            read_only: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelSubscriptionReviewSkill {
    pub id: String,
    pub content_root: String,
    pub content_hash: String,
    pub content_hash_version: u32,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelRepositoryExposure {
    pub private_repository: bool,
    pub full_repository_contents_readable: bool,
    pub full_history_readable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelSubscriptionReview {
    pub channel: SharedChannelDescriptor,
    pub target: ChannelReleaseTarget,
    pub title: String,
    pub notes: String,
    pub publisher: super::ChannelPublisherIdentity,
    pub published_at: String,
    pub exposure: ChannelRepositoryExposure,
    pub skills: Vec<ChannelSubscriptionReviewSkill>,
    pub read_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubscribeChannelRequest {
    pub repository_id: u64,
    pub target: ChannelReleaseTarget,
    pub selected_skill_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelInstallRequest {
    pub repository: RemoteRepository,
    pub manifest: ChannelReleaseManifest,
    pub selected_skill_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelInstallReceipt {
    pub skills: Vec<ChannelSubscribedSkill>,
    pub newly_installed_skill_ids: Vec<String>,
}

#[async_trait]
pub trait ChannelSubscriptionGateway: Send + Sync {
    async fn accessible_repository(
        &self,
        repository_id: u64,
    ) -> Result<RemoteRepository, SharedChannelError>;

    async fn published_manifests(
        &self,
        repository: &RemoteRepository,
    ) -> Result<Vec<ChannelReleaseManifest>, SharedChannelError>;
}

#[async_trait]
pub trait ChannelSubscriptionRegistry: Send + Sync {
    async fn acquire_mutation_lease(
        &self,
    ) -> Result<Box<dyn super::SharedChannelMutationLease>, SharedChannelError> {
        Ok(Box::new(()))
    }

    fn list_views(&self) -> Result<Vec<ChannelSubscriptionView>, SharedChannelError>;
    fn load_mutable(&self) -> Result<ChannelSubscriptionStore, SharedChannelError>;
    fn save(&self, store: &ChannelSubscriptionStore) -> Result<(), SharedChannelError>;
}

#[async_trait]
pub trait ChannelSubscriptionInstaller: Send + Sync {
    async fn install(
        &self,
        request: ChannelInstallRequest,
    ) -> Result<ChannelInstallReceipt, SharedChannelError>;

    async fn rollback(&self, receipt: &ChannelInstallReceipt) -> Result<(), SharedChannelError>;
}

pub struct ChannelSubscriptionFacade<G, C, S, I> {
    gateway: G,
    channels: C,
    subscriptions: S,
    installer: I,
}

impl<G, C, S, I> ChannelSubscriptionFacade<G, C, S, I>
where
    G: ChannelSubscriptionGateway,
    C: SharedChannelRegistry,
    S: ChannelSubscriptionRegistry,
    I: ChannelSubscriptionInstaller,
{
    pub fn new(gateway: G, channels: C, subscriptions: S, installer: I) -> Self {
        Self {
            gateway,
            channels,
            subscriptions,
            installer,
        }
    }

    pub fn list_subscriptions(&self) -> Result<Vec<ChannelSubscriptionView>, SharedChannelError> {
        self.subscriptions.list_views()
    }

    pub async fn review(
        &self,
        repository_id: u64,
    ) -> Result<ChannelSubscriptionReview, SharedChannelError> {
        let channel = self.active_channel(repository_id)?;
        let repository = self.validated_repository(&channel).await?;
        let manifest = self.latest_manifest(&repository, &channel).await?;
        let existing = self
            .subscriptions
            .list_views()?
            .into_iter()
            .find(|subscription| subscription.repository_id == repository_id);
        let selected = existing
            .as_ref()
            .map(|subscription| {
                subscription
                    .selected_skill_ids
                    .iter()
                    .map(|id| id.to_ascii_lowercase())
                    .collect::<BTreeSet<_>>()
            })
            .unwrap_or_else(|| {
                manifest
                    .skills
                    .iter()
                    .filter(|skill| skill.status != ChannelSkillReleaseStatus::Removed)
                    .map(|skill| skill.id.to_ascii_lowercase())
                    .collect()
            });
        let skills = manifest
            .skills
            .iter()
            .filter(|skill| skill.status != ChannelSkillReleaseStatus::Removed)
            .map(|skill| ChannelSubscriptionReviewSkill {
                id: skill.id.clone(),
                content_root: skill.content_root.clone(),
                content_hash: skill.content_hash.clone(),
                content_hash_version: skill.content_hash_version,
                selected: selected.contains(&skill.id.to_ascii_lowercase()),
            })
            .collect();
        Ok(ChannelSubscriptionReview {
            channel: refreshed_channel(&channel, &repository),
            target: release_target(&manifest),
            title: manifest.title,
            notes: manifest.notes,
            publisher: manifest.publisher,
            published_at: manifest.published_at,
            exposure: ChannelRepositoryExposure {
                private_repository: repository.private,
                full_repository_contents_readable: true,
                full_history_readable: true,
            },
            skills,
            read_only: existing.as_ref().is_some_and(|item| item.read_only),
        })
    }

    pub async fn subscribe(
        &self,
        request: SubscribeChannelRequest,
    ) -> Result<ChannelSubscription, SharedChannelError> {
        validate_selection(&request.selected_skill_ids)?;
        let _mutation_guard = SHARED_CHANNEL_MUTATION_GATE.lock().await;
        let _registry_lease = self.channels.acquire_mutation_lease().await?;
        let _subscription_lease = self.subscriptions.acquire_mutation_lease().await?;
        let mut store = self.subscriptions.load_mutable()?;
        let channel = self.active_channel(request.repository_id)?;
        let repository = self.validated_repository(&channel).await?;
        let manifest = self.latest_manifest(&repository, &channel).await?;
        if release_target(&manifest) != request.target {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::ReleaseConflict,
                "The channel release changed after review; review it again before subscribing",
            ));
        }
        let selected = selected_manifest_skills(&manifest, &request.selected_skill_ids)?;
        if let Some(existing) = store
            .subscriptions
            .iter()
            .find(|subscription| subscription.repository_id == request.repository_id)
        {
            if existing.target == release_target(&manifest)
                && selected_ids(&existing.skills) == selected_ids_from_manifest(&selected)
            {
                return Ok(existing.clone());
            }
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::SubscriptionAlreadyExists,
                "This channel is already subscribed; use the channel update flow to change its tracked release",
            ));
        }

        let receipt = self
            .installer
            .install(ChannelInstallRequest {
                repository: repository.clone(),
                manifest: manifest.clone(),
                selected_skill_ids: selected.iter().map(|skill| skill.id.clone()).collect(),
            })
            .await?;
        if let Err(error) = validate_receipt(&receipt, &repository, &manifest, &selected) {
            return Err(match self.installer.rollback(&receipt).await {
                Ok(()) => error,
                Err(rollback_error) => SharedChannelError::new(
                    SharedChannelErrorCode::SubscriptionInstallFailed,
                    format!(
                        "{}; rollback is incomplete and manual cleanup may be required: {}",
                        error.message, rollback_error.message
                    ),
                ),
            });
        }
        let now = Utc::now().to_rfc3339();
        let subscription = ChannelSubscription {
            descriptor_version: CHANNEL_SUBSCRIPTION_DESCRIPTOR_VERSION,
            repository_id: manifest.repository_id,
            organization_id: manifest.organization_id,
            target: release_target(&manifest),
            skills: receipt.skills.clone(),
            created_at: now.clone(),
            updated_at: now,
        };
        store.upsert(subscription.clone());
        if let Err(error) = self.subscriptions.save(&store) {
            let rollback = self.installer.rollback(&receipt).await;
            return Err(match rollback {
                Ok(()) => error,
                Err(rollback_error) => SharedChannelError::new(
                    SharedChannelErrorCode::Storage,
                    format!(
                        "{}; installed Skills also could not be rolled back: {}",
                        error.message, rollback_error.message
                    ),
                ),
            });
        }
        Ok(subscription)
    }

    fn active_channel(
        &self,
        repository_id: u64,
    ) -> Result<SharedChannelDescriptor, SharedChannelError> {
        let channel = self
            .channels
            .load()?
            .channels
            .into_iter()
            .find(|channel| channel.repository_id == repository_id)
            .ok_or_else(|| {
                SharedChannelError::new(
                    SharedChannelErrorCode::RepositoryNotFound,
                    "The shared channel is not registered on this device",
                )
            })?;
        if channel.status != SharedChannelStatus::Active {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::PermissionDenied,
                "Finish accepting and importing the GitHub invitation before reviewing channel releases",
            ));
        }
        Ok(channel)
    }

    async fn validated_repository(
        &self,
        channel: &SharedChannelDescriptor,
    ) -> Result<RemoteRepository, SharedChannelError> {
        let repository = self
            .gateway
            .accessible_repository(channel.repository_id)
            .await?;
        let organization = super::GitHubOrganization {
            id: channel.organization_id,
            login: repository.owner_login.clone(),
            avatar_url: None,
            viewer_is_admin: false,
        };
        validate_remote_repository(&repository, &organization, channel.repository_id)?;
        if project_role(&repository.permissions).is_none() {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::PermissionDenied,
                "The current GitHub identity no longer has repository read access",
            ));
        }
        Ok(repository)
    }

    async fn latest_manifest(
        &self,
        repository: &RemoteRepository,
        channel: &SharedChannelDescriptor,
    ) -> Result<ChannelReleaseManifest, SharedChannelError> {
        let manifests = self.gateway.published_manifests(repository).await?;
        let manifest = manifests
            .into_iter()
            .max_by_key(|manifest| manifest.revision)
            .ok_or_else(|| {
                SharedChannelError::new(
                    SharedChannelErrorCode::ReleaseNotFound,
                    "This channel has no published release to subscribe to",
                )
            })?;
        validate_manifest(&manifest, channel.repository_id, channel.organization_id)?;
        Ok(manifest)
    }
}

fn selected_manifest_skills<'a>(
    manifest: &'a ChannelReleaseManifest,
    requested: &[String],
) -> Result<Vec<&'a super::ChannelReleaseSkill>, SharedChannelError> {
    let active = manifest
        .skills
        .iter()
        .filter(|skill| skill.status != ChannelSkillReleaseStatus::Removed)
        .map(|skill| (skill.id.to_ascii_lowercase(), skill))
        .collect::<BTreeMap<_, _>>();
    let mut selected = Vec::with_capacity(requested.len());
    for id in requested {
        let skill = active.get(&id.to_ascii_lowercase()).ok_or_else(|| {
            SharedChannelError::new(
                SharedChannelErrorCode::SubscriptionSelectionInvalid,
                format!("Selected Skill '{id}' is not present in this channel release"),
            )
        })?;
        selected.push(*skill);
    }
    selected.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(selected)
}

fn validate_selection(selected: &[String]) -> Result<(), SharedChannelError> {
    let mut seen = BTreeSet::new();
    for id in selected {
        crate::content::validate_skill_name(id).map_err(|_| {
            SharedChannelError::new(
                SharedChannelErrorCode::SubscriptionSelectionInvalid,
                "The channel subscription contains an invalid Skill identity",
            )
        })?;
        if !seen.insert(id.to_ascii_lowercase()) {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::SubscriptionSelectionInvalid,
                "The channel subscription contains duplicate Skill identities",
            ));
        }
    }
    Ok(())
}

fn validate_receipt(
    receipt: &ChannelInstallReceipt,
    repository: &RemoteRepository,
    manifest: &ChannelReleaseManifest,
    selected: &[&super::ChannelReleaseSkill],
) -> Result<(), SharedChannelError> {
    let expected = selected
        .iter()
        .map(|skill| (skill.id.to_ascii_lowercase(), *skill))
        .collect::<BTreeMap<_, _>>();
    if receipt.skills.len() != expected.len() {
        return Err(install_integrity_error());
    }
    let mut seen = BTreeSet::new();
    for installed in &receipt.skills {
        let key = installed.id.to_ascii_lowercase();
        let Some(released) = expected.get(&key) else {
            return Err(install_integrity_error());
        };
        if !seen.insert(key)
            || installed.content_root != released.content_root
            || installed.release_content_hash != released.content_hash
            || installed.release_content_hash_version != released.content_hash_version
            || installed.baseline_hash != released.content_hash
            || installed.baseline_hash_version != CHANNEL_CONTENT_HASH_VERSION
            || installed.provenance.repository_id != manifest.repository_id
            || !crate::source_resolver::same_remote_url(
                &installed.provenance.repository_url,
                &repository.clone_url,
            )
            || installed.provenance.git_ref != manifest.commit_sha
            || installed.provenance.source_folder != released.content_root
        {
            return Err(install_integrity_error());
        }
    }
    Ok(())
}

fn install_integrity_error() -> SharedChannelError {
    SharedChannelError::new(
        SharedChannelErrorCode::Integrity,
        "The installed channel Skills do not match the selected release",
    )
}

fn release_target(manifest: &ChannelReleaseManifest) -> ChannelReleaseTarget {
    ChannelReleaseTarget {
        revision: manifest.revision,
        tag_name: manifest.tag_name.clone(),
        commit_sha: manifest.commit_sha.clone(),
    }
}

fn selected_ids(skills: &[ChannelSubscribedSkill]) -> BTreeSet<String> {
    skills
        .iter()
        .map(|skill| skill.id.to_ascii_lowercase())
        .collect()
}

fn selected_ids_from_manifest(skills: &[&super::ChannelReleaseSkill]) -> BTreeSet<String> {
    skills
        .iter()
        .map(|skill| skill.id.to_ascii_lowercase())
        .collect()
}

fn refreshed_channel(
    channel: &SharedChannelDescriptor,
    repository: &RemoteRepository,
) -> SharedChannelDescriptor {
    let mut refreshed = channel.clone();
    refreshed.owner = repository.owner_login.clone();
    refreshed.name = repository.name.clone();
    refreshed.html_url = repository.html_url.clone();
    refreshed.clone_url = repository.clone_url.clone();
    refreshed.role = project_role(&repository.permissions).unwrap_or(SharedChannelRole::Subscriber);
    refreshed
}
