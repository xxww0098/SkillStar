use super::update_types::ChannelUpdateSnapshot;
use super::{SharedChannelError, SharedChannelMutationLease};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const CHANNEL_SUBSCRIPTION_STORE_VERSION: u32 = 1;
pub const CHANNEL_SUBSCRIPTION_DESCRIPTOR_VERSION: u32 = 4;
pub const CHANNEL_AUTO_UPDATE_INTERVAL_SECS: i64 = 60 * 60;

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
    pub last_update: Option<ChannelUpdateSnapshot>,
    #[serde(default)]
    pub auto_update: ChannelAutoUpdateState,
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
    pub auto_update: ChannelAutoUpdateState,
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

#[async_trait]
pub trait ChannelSubscriptionRegistry: Send + Sync {
    fn auto_update_scope_key(&self) -> String {
        std::any::type_name::<Self>().to_string()
    }

    fn try_acquire_auto_update_run_lease(
        &self,
        _repository_id: u64,
    ) -> Result<Option<Box<dyn SharedChannelMutationLease>>, SharedChannelError> {
        Ok(Some(Box::new(())))
    }

    async fn acquire_mutation_lease(
        &self,
    ) -> Result<Box<dyn SharedChannelMutationLease>, SharedChannelError> {
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ChannelAutoUpdateState {
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_check_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_run: Option<ChannelAutoUpdateRun>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelAutoUpdateRunStatus {
    Checking,
    Checked,
    UpToDate,
    Applied,
    PartiallyApplied,
    Paused,
    RetryableFailure,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelAutoUpdatePauseReason {
    Pinned,
    LocalContentChanged,
    BaselineMissing,
    SnapshotFailed,
    PermissionChanged,
    RemovedUpstream,
    IntegrityError,
    UnresolvedFailure,
    NewSkillRequiresReview,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelAutoUpdatePause {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_id: Option<String>,
    pub reason: ChannelAutoUpdatePauseReason,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelAutoUpdateRun {
    pub started_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    pub status: ChannelAutoUpdateRunStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<ChannelReleaseTarget>,
    #[serde(default)]
    pub applied_skill_ids: Vec<String>,
    #[serde(default)]
    pub pauses: Vec<ChannelAutoUpdatePause>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default)]
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChannelAutoUpdateExecution {
    pub repository_id: u64,
    pub run: ChannelAutoUpdateRun,
}

impl ChannelAutoUpdateRun {
    pub fn checking(now: DateTime<Utc>) -> Self {
        Self {
            started_at: now.to_rfc3339(),
            completed_at: None,
            status: ChannelAutoUpdateRunStatus::Checking,
            target: None,
            applied_skill_ids: Vec::new(),
            pauses: Vec::new(),
            error: None,
            retryable: false,
        }
    }

    pub fn completed(
        started_at: &str,
        now: DateTime<Utc>,
        status: ChannelAutoUpdateRunStatus,
        target: Option<ChannelReleaseTarget>,
        applied_skill_ids: Vec<String>,
        pauses: Vec<ChannelAutoUpdatePause>,
        error: Option<String>,
    ) -> Self {
        Self {
            started_at: started_at.to_string(),
            completed_at: Some(now.to_rfc3339()),
            status,
            target,
            applied_skill_ids,
            pauses,
            error,
            retryable: false,
        }
    }
}
