//! Channel-ownership gate, persistent JSON registries, and pure serialized
//! descriptor/subscription/update schemas shared by `ss-skills` and `ss-channels`.
//!
//! No git transport, GitHub API, network, or orchestration service code belongs
//! here; those live in the `ss-channels` crate.

pub(crate) mod policy;
mod install_authority;
#[cfg(all(test, not(windows)))]
mod install_authority_tests;
mod store;
mod subscription_pruning;
mod subscription_store;
mod subscription_types;
mod types;
pub mod update_types;

pub use install_authority::ChannelInstallAuthority;
pub use store::DiskSharedChannelRegistry;
pub(crate) use subscription_pruning::{
    provenance_paths as subscription_provenance_paths, prune_removed_skills,
};
pub use subscription_store::DiskChannelSubscriptionRegistry;
pub(crate) use subscription_store::{managed_repository_for_skill, managed_repository_for_url};
pub use subscription_types::{
    CHANNEL_AUTO_UPDATE_INTERVAL_SECS, CHANNEL_SUBSCRIPTION_DESCRIPTOR_VERSION,
    CHANNEL_SUBSCRIPTION_STORE_VERSION, ChannelAutoUpdateExecution, ChannelAutoUpdatePause,
    ChannelAutoUpdatePauseReason, ChannelAutoUpdateRun, ChannelAutoUpdateRunStatus,
    ChannelAutoUpdateState, ChannelReleaseTarget, ChannelSkillPin, ChannelSkillProvenance,
    ChannelSubscribedSkill, ChannelSubscription, ChannelSubscriptionRegistry,
    ChannelSubscriptionRemoteState, ChannelSubscriptionRemoteStatus, ChannelSubscriptionStore,
    ChannelSubscriptionView,
};
pub use types::{
    CHANNEL_DESCRIPTOR_VERSION, SHARED_CHANNEL_STORE_VERSION, SharedChannelAuthorization,
    SharedChannelDescriptor, SharedChannelError, SharedChannelErrorCode,
    SharedChannelMutationLease, SharedChannelRegistry, SharedChannelRole, SharedChannelStatus,
    SharedChannelStore,
};
pub use update_types::{
    CHANNEL_CONTENT_HASH_VERSION, ChannelPublisherIdentity, ChannelUpdateBlockReason,
    ChannelUpdateChange, ChannelUpdateItem, ChannelUpdateItemState, ChannelUpdateSnapshot,
    ChannelUpdateStatus, revision_tag,
};

pub(crate) fn ensure_generic_skill_mutation_allowed(skill_id: &str) -> anyhow::Result<()> {
    if let Some(repository_id) = managed_repository_for_skill(skill_id)? {
        anyhow::bail!(
            "Skill '{skill_id}' is managed by shared channel repository {repository_id}; use the shared channel controls to update, remove, or convert it"
        );
    }
    Ok(())
}

pub(crate) fn ensure_generic_repository_mutation_allowed(
    repository_url: &str,
) -> anyhow::Result<()> {
    if let Some(repository_id) = managed_repository_for_url(repository_url)? {
        anyhow::bail!(
            "Repository is owned by shared channel {repository_id}; use the shared-channel update flow"
        );
    }
    Ok(())
}
