use super::{
    ChannelReleaseManifest, ChannelSkillReleaseStatus, RemoteRepository,
    SHARED_CHANNEL_MUTATION_GATE, SharedChannelDescriptor, SharedChannelError,
    SharedChannelErrorCode, SharedChannelRegistry, SharedChannelRole, SharedChannelStatus,
    project_role, validate_manifest, validate_remote_repository,
};
use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use super::subscription_validation::{
    refreshed_channel_view, route_migration_error, selected_ids, selected_ids_from_manifest,
    selected_manifest_skills, validate_selection,
};
pub(super) use super::subscription_validation::{release_target, validate_receipt};

pub const CHANNEL_SUBSCRIPTION_STORE_VERSION: u32 = 1;
pub const CHANNEL_SUBSCRIPTION_DESCRIPTOR_VERSION: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChannelSubscriptionRemoteStatus {
    #[default]
    Active,
    Revoked,
    Offline,
    IntegrityError,
    RecoverableFailure,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelSubscriptionRemoteState {
    pub status: ChannelSubscriptionRemoteStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl Default for ChannelSubscriptionRemoteState {
    fn default() -> Self {
        Self {
            status: ChannelSubscriptionRemoteStatus::Active,
            checked_at: None,
            message: None,
        }
    }
}

impl ChannelSubscriptionRemoteState {
    pub fn active() -> Self {
        Self {
            status: ChannelSubscriptionRemoteStatus::Active,
            checked_at: Some(Utc::now().to_rfc3339()),
            message: None,
        }
    }

    pub fn revoked(message: impl Into<String>) -> Self {
        Self {
            status: ChannelSubscriptionRemoteStatus::Revoked,
            checked_at: Some(Utc::now().to_rfc3339()),
            message: Some(message.into()),
        }
    }
}

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
pub struct ChannelSkillPin {
    pub skill_id: String,
    pub target: ChannelReleaseTarget,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelSubscription {
    pub descriptor_version: u32,
    pub repository_id: u64,
    pub organization_id: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repository_url_aliases: Vec<String>,
    pub target: ChannelReleaseTarget,
    pub skills: Vec<ChannelSubscribedSkill>,
    #[serde(default)]
    pub known_skill_ids: Vec<String>,
    #[serde(default)]
    pub pins: Vec<ChannelSkillPin>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_update: Option<super::ChannelUpdateSnapshot>,
    #[serde(default)]
    pub auto_update: super::ChannelAutoUpdateState,
    #[serde(default)]
    pub remote_state: ChannelSubscriptionRemoteState,
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
    pub auto_update: super::ChannelAutoUpdateState,
    pub remote_state: ChannelSubscriptionRemoteState,
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
            auto_update: subscription.auto_update.clone(),
            remote_state: subscription.remote_state.clone(),
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
    fn auto_update_scope_key(&self) -> String {
        std::any::type_name::<Self>().to_string()
    }

    fn try_acquire_auto_update_run_lease(
        &self,
        _repository_id: u64,
    ) -> Result<Option<Box<dyn super::SharedChannelMutationLease>>, SharedChannelError> {
        Ok(Some(Box::new(())))
    }

    async fn acquire_mutation_lease(
        &self,
    ) -> Result<Box<dyn super::SharedChannelMutationLease>, SharedChannelError> {
        Ok(Box::new(()))
    }

    fn list_views(&self) -> Result<Vec<ChannelSubscriptionView>, SharedChannelError>;
    fn load_mutable(&self) -> Result<ChannelSubscriptionStore, SharedChannelError>;
    fn save(&self, store: &ChannelSubscriptionStore) -> Result<(), SharedChannelError>;

    /// Returns the installed-Skill lockfile owned by this registry, when any.
    ///
    /// In-memory registries used by callers and tests do not own the process-wide
    /// SkillStar lockfile, so their route migrations must not mutate it.
    fn repository_route_lockfile(&self) -> Option<std::path::PathBuf> {
        None
    }
}

#[async_trait]
pub trait ChannelSubscriptionInstaller: Send + Sync {
    async fn verify_release_content(
        &self,
        repository: &RemoteRepository,
        manifest: &ChannelReleaseManifest,
    ) -> Result<(), SharedChannelError>;

    async fn install(
        &self,
        request: ChannelInstallRequest,
    ) -> Result<ChannelInstallReceipt, SharedChannelError>;

    async fn rollback(&self, receipt: &ChannelInstallReceipt) -> Result<(), SharedChannelError>;

    async fn verify_and_commit_install(
        &self,
        receipt: &ChannelInstallReceipt,
        commit: Box<dyn FnOnce() -> Result<(), SharedChannelError> + Send>,
    ) -> Result<(), SharedChannelError> {
        if let Err(error) = commit() {
            return Err(match self.rollback(receipt).await {
                Ok(()) => error,
                Err(rollback) => SharedChannelError::new(
                    error.code,
                    format!(
                        "{}; the staged channel Skill could not be rolled back: {}",
                        error.message, rollback.message
                    ),
                ),
            });
        }
        Ok(())
    }
}

pub struct ChannelSubscriptionFacade<G, C, S, I> {
    pub(super) gateway: G,
    pub(super) channels: C,
    pub(super) subscriptions: S,
    pub(super) installer: I,
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
        let existing = self
            .subscriptions
            .list_views()?
            .into_iter()
            .find(|subscription| subscription.repository_id == repository_id);
        let _mutation_guard = SHARED_CHANNEL_MUTATION_GATE.lock().await;
        let _registry_lease = self.channels.acquire_mutation_lease().await?;
        let _subscription_lease = self.subscriptions.acquire_mutation_lease().await?;
        let channel = match self.active_channel(repository_id) {
            Ok(channel) => channel,
            Err(error) => {
                if existing
                    .as_ref()
                    .is_some_and(|subscription| !subscription.read_only)
                {
                    let mut store = self.subscriptions.load_mutable()?;
                    if let Some(index) = store
                        .subscriptions
                        .iter()
                        .position(|subscription| subscription.repository_id == repository_id)
                    {
                        super::subscription_remote::persist_remote_failure(
                            &self.subscriptions,
                            &mut store,
                            index,
                            &error,
                        )?;
                    }
                }
                return Err(error);
            }
        };
        let repository = match self.validated_repository(&channel).await {
            Ok(repository) => repository,
            Err(error) => {
                if existing
                    .as_ref()
                    .is_some_and(|subscription| !subscription.read_only)
                {
                    let mut store = self.subscriptions.load_mutable()?;
                    if let Some(index) = store
                        .subscriptions
                        .iter()
                        .position(|subscription| subscription.repository_id == repository_id)
                    {
                        super::subscription_remote::persist_remote_failure(
                            &self.subscriptions,
                            &mut store,
                            index,
                            &error,
                        )?;
                    }
                }
                return Err(error);
            }
        };
        let manifest = match self.latest_manifest(&repository, &channel).await {
            Ok(manifest) => manifest,
            Err(error) => {
                if existing
                    .as_ref()
                    .is_some_and(|subscription| !subscription.read_only)
                {
                    let mut store = self.subscriptions.load_mutable()?;
                    if let Some(index) = store
                        .subscriptions
                        .iter()
                        .position(|subscription| subscription.repository_id == repository_id)
                    {
                        super::subscription_remote::persist_remote_failure(
                            &self.subscriptions,
                            &mut store,
                            index,
                            &error,
                        )?;
                    }
                }
                return Err(error);
            }
        };
        if let Err(error) = self
            .installer
            .verify_release_content(&repository, &manifest)
            .await
        {
            if existing
                .as_ref()
                .is_some_and(|subscription| !subscription.read_only)
            {
                let mut store = self.subscriptions.load_mutable()?;
                if let Some(index) = store
                    .subscriptions
                    .iter()
                    .position(|subscription| subscription.repository_id == repository_id)
                {
                    super::subscription_remote::persist_remote_failure(
                        &self.subscriptions,
                        &mut store,
                        index,
                        &error,
                    )?;
                }
            }
            return Err(error);
        }
        if existing
            .as_ref()
            .is_some_and(|subscription| !subscription.read_only)
        {
            let mut store = self.subscriptions.load_mutable()?;
            if let Some(index) = store
                .subscriptions
                .iter()
                .position(|subscription| subscription.repository_id == repository_id)
                && let Err(error) = super::channel_update::validate_manifest_progress(
                    &store.subscriptions[index],
                    &manifest,
                )
            {
                super::subscription_remote::persist_remote_failure(
                    &self.subscriptions,
                    &mut store,
                    index,
                    &error,
                )?;
                return Err(error);
            }
        }
        let refreshed_channel = if existing.as_ref().is_some_and(|item| item.read_only) {
            refreshed_channel_view(&channel, &repository)
        } else {
            self.persist_refreshed_channel(&channel, &repository)?
        };
        if existing
            .as_ref()
            .is_some_and(|subscription| !subscription.read_only)
        {
            let mut store = self.subscriptions.load_mutable()?;
            if let Some(index) = store
                .subscriptions
                .iter()
                .position(|subscription| subscription.repository_id == repository_id)
            {
                super::subscription_remote::mark_remote_active(&mut store.subscriptions[index]);
                self.subscriptions.save(&store)?;
            }
        }
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
            channel: refreshed_channel,
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
    ) -> Result<ChannelSubscription, SharedChannelError>
    where
        S: Clone + 'static,
    {
        validate_selection(&request.selected_skill_ids)?;
        let _mutation_guard = SHARED_CHANNEL_MUTATION_GATE.lock().await;
        let _registry_lease = self.channels.acquire_mutation_lease().await?;
        let _subscription_lease = self.subscriptions.acquire_mutation_lease().await?;
        let mut store = self.subscriptions.load_mutable()?;
        let mut existing_index = store
            .subscriptions
            .iter()
            .position(|subscription| subscription.repository_id == request.repository_id);
        let remote = async {
            let channel = self.active_channel(request.repository_id)?;
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
                if let Some(index) = existing_index {
                    super::subscription_remote::persist_remote_failure(
                        &self.subscriptions,
                        &mut store,
                        index,
                        &error,
                    )?;
                }
                return Err(error);
            }
        };
        if release_target(&manifest) != request.target {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::ReleaseConflict,
                "The channel release changed after review; review it again before subscribing",
            ));
        }
        let selected = selected_manifest_skills(&manifest, &request.selected_skill_ids)?;
        if let Some(index) = existing_index {
            let existing = &store.subscriptions[index];
            if existing.target == release_target(&manifest)
                && selected_ids(&existing.skills) == selected_ids_from_manifest(&selected)
            {
                self.persist_refreshed_channel(&channel, &repository)?;
                store = self.subscriptions.load_mutable()?;
                existing_index = store
                    .subscriptions
                    .iter()
                    .position(|subscription| subscription.repository_id == request.repository_id);
                let index = existing_index.ok_or_else(|| {
                    SharedChannelError::new(
                        SharedChannelErrorCode::SubscriptionNotFound,
                        "The channel subscription disappeared while refreshing its route",
                    )
                })?;
                super::subscription_remote::mark_remote_active(&mut store.subscriptions[index]);
                self.subscriptions.save(&store)?;
                return Ok(store.subscriptions[index].clone());
            }
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::SubscriptionAlreadyExists,
                "This channel is already subscribed; use the channel update flow to change its tracked release",
            ));
        }

        self.persist_refreshed_channel(&channel, &repository)?;

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
            repository_url_aliases: Vec::new(),
            target: release_target(&manifest),
            skills: receipt.skills.clone(),
            known_skill_ids: manifest
                .skills
                .iter()
                .filter(|skill| skill.status != ChannelSkillReleaseStatus::Removed)
                .map(|skill| skill.id.clone())
                .collect(),
            pins: Vec::new(),
            last_update: None,
            auto_update: super::ChannelAutoUpdateState::default(),
            remote_state: ChannelSubscriptionRemoteState::default(),
            created_at: now.clone(),
            updated_at: now,
        };
        store.upsert(subscription.clone());
        let subscriptions = self.subscriptions.clone();
        self.installer
            .verify_and_commit_install(&receipt, Box::new(move || subscriptions.save(&store)))
            .await?;
        Ok(subscription)
    }

    pub(super) fn active_channel(
        &self,
        repository_id: u64,
    ) -> Result<SharedChannelDescriptor, SharedChannelError> {
        let channel = self
            .channels
            .load()
            .map_err(|error| {
                if error.code == SharedChannelErrorCode::Storage {
                    SharedChannelError::new(SharedChannelErrorCode::Protocol, error.message)
                } else {
                    error
                }
            })?
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

    pub(super) async fn validated_repository(
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
                SharedChannelErrorCode::AppRepositoryAccessRequired,
                "The current GitHub identity no longer has repository read access",
            ));
        }
        Ok(repository)
    }

    pub(super) async fn latest_manifest(
        &self,
        repository: &RemoteRepository,
        channel: &SharedChannelDescriptor,
    ) -> Result<ChannelReleaseManifest, SharedChannelError> {
        let mut revisions = BTreeSet::new();
        let mut latest = None;
        for manifest in self.gateway.published_manifests(repository).await? {
            validate_manifest(&manifest, channel.repository_id, channel.organization_id)?;
            if !revisions.insert(manifest.revision) {
                return Err(SharedChannelError::new(
                    SharedChannelErrorCode::Integrity,
                    "The channel contains duplicate published release revisions",
                ));
            }
            if latest
                .as_ref()
                .is_none_or(|current: &ChannelReleaseManifest| manifest.revision > current.revision)
            {
                latest = Some(manifest);
            }
        }
        latest.ok_or_else(|| {
            SharedChannelError::new(
                SharedChannelErrorCode::ReleaseNotFound,
                "This channel has no published release to subscribe to",
            )
        })
    }

    pub(super) fn persist_refreshed_channel(
        &self,
        channel: &SharedChannelDescriptor,
        repository: &RemoteRepository,
    ) -> Result<SharedChannelDescriptor, SharedChannelError> {
        let mut store = self.channels.load()?;
        let stored_index = store
            .channels
            .iter()
            .position(|candidate| candidate.repository_id == channel.repository_id)
            .ok_or_else(|| {
                SharedChannelError::new(
                    SharedChannelErrorCode::RepositoryNotFound,
                    "The shared channel disappeared while refreshing repository routing",
                )
            })?;
        let stored = &store.channels[stored_index];
        if stored.organization_id != channel.organization_id {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::Integrity,
                "The shared channel organization changed while refreshing repository routing",
            ));
        }
        let role = project_role(&repository.permissions).unwrap_or(SharedChannelRole::Subscriber);
        let changed = stored.owner != repository.owner_login
            || stored.name != repository.name
            || stored.html_url != repository.html_url
            || stored.clone_url != repository.clone_url
            || stored.role != role;
        let mut subscriptions = self.subscriptions.load_mutable()?;
        let subscription_route_changed = subscriptions
            .subscriptions
            .iter()
            .find(|candidate| candidate.repository_id == channel.repository_id)
            .is_some_and(|subscription| {
                subscription.skills.iter().any(|skill| {
                    !skillstar_skills::source_resolver::same_remote_url(
                        &skill.provenance.repository_url,
                        &repository.clone_url,
                    )
                })
            });
        if !changed && !subscription_route_changed {
            return Ok(stored.clone());
        }

        let previous_store = store.clone();
        let previous_clone_url = stored.clone_url.clone();
        let route_changed = subscription_route_changed
            || !skillstar_skills::source_resolver::same_remote_url(&previous_clone_url, &repository.clone_url);
        let stored = &mut store.channels[stored_index];
        stored.owner = repository.owner_login.clone();
        stored.name = repository.name.clone();
        stored.html_url = repository.html_url.clone();
        stored.clone_url = repository.clone_url.clone();
        stored.role = role;
        stored.updated_at = Utc::now().to_rfc3339();
        let refreshed = stored.clone();

        if !route_changed {
            self.channels.save(&store)?;
            return Ok(refreshed);
        }

        let previous_subscriptions = subscriptions.clone();
        let tracked = subscriptions
            .subscriptions
            .iter_mut()
            .find(|candidate| candidate.repository_id == channel.repository_id)
            .map(|subscription| {
                let mut routes = Vec::with_capacity(subscription.skills.len());
                for skill in &mut subscription.skills {
                    if !skillstar_skills::source_resolver::same_remote_url(
                        &skill.provenance.repository_url,
                        &repository.clone_url,
                    ) && !subscription.repository_url_aliases.iter().any(|alias| {
                        skillstar_skills::source_resolver::same_remote_url(
                            alias,
                            &skill.provenance.repository_url,
                        )
                    }) {
                        subscription
                            .repository_url_aliases
                            .push(skill.provenance.repository_url.clone());
                    }
                    routes.push((skill.id.clone(), skill.provenance.repository_url.clone()));
                    skill.provenance.repository_url = repository.clone_url.clone();
                }
                routes
            });

        if tracked.is_none() {
            self.channels.save(&store)?;
            return Ok(refreshed);
        }

        let _update_guard =
            skillstar_skills::skill_update::acquire_update_transaction_lock().map_err(|error| {
                SharedChannelError::new(
                    SharedChannelErrorCode::Storage,
                    format!("Unable to lock repository route migration: {error}"),
                )
            })?;

        let lock_path = self.subscriptions.repository_route_lockfile();
        let _lock_guard = skillstar_skills::lockfile::get_mutex().lock().map_err(|_| {
            SharedChannelError::new(
                SharedChannelErrorCode::Storage,
                "Lockfile mutex poisoned during repository route migration",
            )
        })?;
        let mut lockfile = match lock_path.as_deref() {
            Some(lock_path) => skillstar_skills::lockfile::Lockfile::load(lock_path).map_err(|error| {
                SharedChannelError::new(
                    SharedChannelErrorCode::Storage,
                    format!("Unable to read repository routes from the Skill lockfile: {error}"),
                )
            })?,
            None => skillstar_skills::lockfile::Lockfile::default(),
        };
        let previous_lockfile = lockfile.clone();
        let mut lockfile_changed = false;
        if lock_path.is_some()
            && let Some(tracked) = tracked.as_ref()
        {
            for (skill_id, previous_skill_url) in tracked {
                let entry = lockfile
                    .skills
                    .iter_mut()
                    .find(|entry| entry.name.eq_ignore_ascii_case(skill_id));
                let Some(entry) = entry else {
                    if skillstar_core::infra::paths::hub_skills_dir()
                        .join(skill_id)
                        .symlink_metadata()
                        .is_ok()
                    {
                        return Err(SharedChannelError::new(
                            SharedChannelErrorCode::Integrity,
                            format!("Tracked Skill '{skill_id}' is missing lockfile provenance"),
                        ));
                    }
                    continue;
                };
                if !skillstar_skills::source_resolver::same_remote_url(&entry.git_url, previous_skill_url)
                    && !skillstar_skills::source_resolver::same_remote_url(
                        &entry.git_url,
                        &repository.clone_url,
                    )
                {
                    return Err(SharedChannelError::new(
                        SharedChannelErrorCode::Integrity,
                        format!(
                            "Tracked Skill '{skill_id}' has a lockfile route that does not match its channel"
                        ),
                    ));
                }
                if !skillstar_skills::source_resolver::same_remote_url(&entry.git_url, &repository.clone_url) {
                    entry.git_url = repository.clone_url.clone();
                    lockfile_changed = true;
                }
            }
        }

        if lockfile_changed {
            lockfile
                .save(
                    lock_path
                        .as_deref()
                        .expect("changed lockfile has an owned path"),
                )
                .map_err(|error| {
                    SharedChannelError::new(
                        SharedChannelErrorCode::Storage,
                        format!("Unable to save migrated Skill repository routes: {error}"),
                    )
                })?;
        }
        if let Err(error) = self.subscriptions.save(&subscriptions) {
            let rollback = lockfile_changed
                .then(|| {
                    previous_lockfile
                        .save(
                            lock_path
                                .as_deref()
                                .expect("changed lockfile has an owned path"),
                        )
                        .err()
                })
                .flatten();
            return Err(route_migration_error(error, rollback));
        }
        if let Err(error) = self.channels.save(&store) {
            let subscription_rollback = self.subscriptions.save(&previous_subscriptions).err();
            let lock_rollback = lockfile_changed
                .then(|| {
                    previous_lockfile
                        .save(
                            lock_path
                                .as_deref()
                                .expect("changed lockfile has an owned path"),
                        )
                        .err()
                })
                .flatten();
            let channel_rollback = self.channels.save(&previous_store).err();
            let rollback = [
                subscription_rollback.map(|error| error.message),
                lock_rollback.map(|error| error.to_string()),
                channel_rollback.map(|error| error.message),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
            return Err(SharedChannelError::new(
                error.code,
                if rollback.is_empty() {
                    error.message
                } else {
                    format!(
                        "{}; repository route rollback is incomplete: {}",
                        error.message,
                        rollback.join(", ")
                    )
                },
            ));
        }
        skillstar_skills::installed_skill::invalidate_cache();
        Ok(refreshed)
    }
}
