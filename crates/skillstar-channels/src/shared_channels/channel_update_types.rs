//! Data contract for channel update review and apply.
//!
//! Split out of `channel_update.rs` to keep that module under the file-size
//! limit: these are the serialized shapes the IPC layer and the installer
//! agree on, with no behaviour attached. `channel_update.rs` keeps the
//! `ChannelSubscriptionFacade` logic that produces and consumes them.

use super::{
    ChannelPublisherIdentity, ChannelReleaseManifest, ChannelReleaseSkill, ChannelReleaseTarget,
    ChannelSubscribedSkill, RemoteRepository, SharedChannelError, SharedChannelErrorCode,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use skillstar_skills::skill_update::{LocalDivergenceReason, LocalDivergenceResolution};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelUpdateStatus {
    UpToDate,
    UpdateAvailable,
    PartiallyUpgraded,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelUpdateChange {
    Added,
    Updated,
    Removed,
    Unchanged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelUpdateItemState {
    Current,
    Available,
    Applied,
    Blocked,
    Failed,
    Notification,
    RemovedFromChannel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelUpdateBlockReason {
    LocalContentChanged,
    BaselineMissing,
    SnapshotFailed,
    RemovedUpstream,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelUpdateItem {
    pub id: String,
    pub change: ChannelUpdateChange,
    pub state: ChannelUpdateItemState,
    pub selected: bool,
    pub from_content_hash: Option<String>,
    pub to_content_hash: Option<String>,
    pub block_reason: Option<ChannelUpdateBlockReason>,
    pub suggested_local_name: Option<String>,
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned_target: Option<ChannelReleaseTarget>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<SharedChannelErrorCode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelUpdateSnapshot {
    pub target: ChannelReleaseTarget,
    pub title: String,
    pub notes: String,
    pub publisher: ChannelPublisherIdentity,
    pub published_at: String,
    pub checked_at: String,
    pub status: ChannelUpdateStatus,
    pub acknowledgement_required: bool,
    pub items: Vec<ChannelUpdateItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check_error_code: Option<SharedChannelErrorCode>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplyChannelUpdateRequest {
    pub repository_id: u64,
    pub target: ChannelReleaseTarget,
    #[serde(default)]
    pub resolutions: Vec<ChannelSkillUpdateResolution>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelSkillUpdateResolution {
    pub skill_id: String,
    pub resolution: LocalDivergenceResolution,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ApplyChannelUpdateResult {
    pub snapshot: ChannelUpdateSnapshot,
    pub applied_skill_ids: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ChannelSkillUpdateRequest {
    pub repository: RemoteRepository,
    pub manifest: ChannelReleaseManifest,
    pub released: ChannelReleaseSkill,
    pub installed: ChannelSubscribedSkill,
    pub resolution: Option<LocalDivergenceResolution>,
}

#[derive(Debug, Clone)]
pub struct ChannelSkillUpdateReceipt {
    pub previous: ChannelSubscribedSkill,
    pub installed: ChannelSubscribedSkill,
    pub previous_checkout: String,
    pub previous_lock_entry: skillstar_skills::lockfile::LockEntry,
    pub previous_update_available: Option<bool>,
    pub update_state_revision_after_apply: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelUpdateInspection {
    Clean,
    Divergent {
        reason: LocalDivergenceReason,
        suggested_local_name: String,
        error: Option<String>,
    },
}

#[async_trait]
pub trait ChannelSubscriptionUpdater: Send + Sync {
    async fn inspect(
        &self,
        skill: &ChannelSubscribedSkill,
    ) -> Result<ChannelUpdateInspection, SharedChannelError>;

    async fn apply(
        &self,
        request: ChannelSkillUpdateRequest,
    ) -> Result<ChannelSkillUpdateReceipt, SharedChannelError>;

    async fn verify(&self, receipt: &ChannelSkillUpdateReceipt) -> Result<(), SharedChannelError>;

    async fn verify_and_commit(
        &self,
        receipt: &ChannelSkillUpdateReceipt,
        commit: Box<dyn FnOnce() -> Result<(), SharedChannelError> + Send>,
    ) -> Result<(), SharedChannelError> {
        self.verify(receipt).await?;
        commit()
    }

    async fn rollback(&self, receipt: &ChannelSkillUpdateReceipt)
    -> Result<(), SharedChannelError>;
}
