//! Data contract for channel update review and apply.
//!
//! Split out of `channel_update.rs` to keep that module under the file-size
//! limit: these are the serialized shapes the IPC layer and the installer
//! agree on, with no behaviour attached. `channel_update.rs` keeps the
//! `ChannelSubscriptionFacade` logic that produces and consumes them.

use super::{
    ChannelReleaseManifest, ChannelReleaseSkill, ChannelReleaseTarget, ChannelSubscribedSkill,
    ChannelUpdateSnapshot, RemoteRepository, SharedChannelError,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use ss_skills::skill_update::{LocalDivergenceReason, LocalDivergenceResolution};

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
    pub previous_lock_entry: ss_skills::skill_lock::SkillLockEntry,
    pub previous_update_available: Option<bool>,
    pub update_state_revision_after_apply: Option<u64>,
    /// Local copy of the canonical content taken just before replacement, so
    /// a rollback restores exactly what was there (including edits the user
    /// chose to discard) without refetching. Released by
    /// [`ChannelSubscriptionUpdater::finalize`] or consumed by a rollback.
    pub retained: Option<RetainedChannelContent>,
}

#[derive(Debug, Clone)]
pub struct RetainedChannelContent {
    pub path: std::path::PathBuf,
    pub content_hash: String,
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

    /// The update is committed: release what [`rollback`] would have needed.
    ///
    /// [`rollback`]: ChannelSubscriptionUpdater::rollback
    async fn finalize(&self, _receipt: &ChannelSkillUpdateReceipt) {}
}
