//! Production wiring and credential-free local readers for `ss-channels`.
//!
//! Exposes every shared-channel capability to `ss-app` CLI commands and
//! `ss-gui-bridge` without adding a `ss-gui-bridge -> ss-channels` dependency edge.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, OnceLock};

use serde::Serialize;
pub use ss_channels::*;
use ss_git::transport::{
    GitAuthMaterial, GitOperationSession, GitProgressSink, NoopGitProgressSink,
};
use ss_skills::git_skill::GitSkillFacade;
pub use ss_skills::skill_update::{
    LocalDivergenceReason, LocalDivergenceResolution, divergence::suggested_local_name,
};
use ss_skills::github_auth::{
    FileCredentialStore, GitHubApiCredential, GitHubAuthError, GitHubAuthFacade,
    ProductionGitHubGateway, SystemClock,
};

pub type ProdSharedChannelFacade =
    SharedChannelFacade<ProductionSharedChannelGateway, DiskSharedChannelRegistry>;

pub type ProdRegistrationFacade =
    ExistingChannelRegistrationFacade<ProductionSharedChannelGateway, DiskSharedChannelRegistry>;

pub type ProdPublicationFacade =
    ChannelPublicationFacade<ProductionSharedChannelGateway, DiskSharedChannelRegistry>;

pub type ProdMembershipFacade =
    ChannelMembershipFacade<ProductionSharedChannelGateway, DiskSharedChannelRegistry>;

pub type ProdSubscriptionFacade = ChannelSubscriptionFacade<
    ProductionSharedChannelGateway,
    DiskSharedChannelRegistry,
    DiskChannelSubscriptionRegistry,
    GitChannelSubscriptionInstaller,
>;

/// Local read-only subscription record combining `ChannelSubscriptionView`
/// with any persisted update snapshot and skill pins.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LocalSubscriptionRecord {
    pub view: ChannelSubscriptionView,
    pub update_snapshot: Option<ChannelUpdateSnapshot>,
    pub pins: Vec<ChannelSkillPin>,
}

/// Local read-only channel workspace snapshot (works offline and signed out).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LocalChannelsSnapshot {
    pub channels: Vec<SharedChannelDescriptor>,
    pub subscriptions: Vec<LocalSubscriptionRecord>,
}

fn registration_sessions() -> &'static ExistingChannelRegistrationSessions {
    static SESSIONS: OnceLock<ExistingChannelRegistrationSessions> = OnceLock::new();
    SESSIONS.get_or_init(ExistingChannelRegistrationSessions::default)
}

fn publish_sessions() -> &'static ChannelPublishSessions {
    static SESSIONS: OnceLock<ChannelPublishSessions> = OnceLock::new();
    SESSIONS.get_or_init(ChannelPublishSessions::default)
}

fn auth_facade() -> GitHubAuthFacade<ProductionGitHubGateway, FileCredentialStore, SystemClock> {
    GitHubAuthFacade::new(
        ProductionGitHubGateway::from_environment(),
        FileCredentialStore::default(),
        SystemClock,
    )
}

pub(crate) fn api_credential() -> Result<GitHubApiCredential, GitHubAuthError> {
    auth_facade().api_credential()
}

pub(crate) fn git_skill_facade(progress_sink: Arc<dyn GitProgressSink>) -> GitSkillFacade {
    let auth = auth_facade();
    let session = GitOperationSession::new(
        uuid::Uuid::new_v4().to_string(),
        auth.git_auth_material()
            .unwrap_or_else(|error| GitAuthMaterial::unavailable(error.to_string())),
        progress_sink,
    );
    GitSkillFacade::new(session)
}

fn gateway() -> Result<ProductionSharedChannelGateway, SharedChannelError> {
    let credential = api_credential().map_err(SharedChannelError::from)?;
    Ok(ProductionSharedChannelGateway::new(credential))
}

/// Build the production channel creation/listing facade.
pub fn shared_channel_facade() -> Result<ProdSharedChannelFacade, SharedChannelError> {
    Ok(SharedChannelFacade::new(gateway()?, DiskSharedChannelRegistry))
}

/// Build the production existing-repository registration facade.
pub fn registration_facade() -> Result<ProdRegistrationFacade, SharedChannelError> {
    registration_facade_with_progress(Arc::new(NoopGitProgressSink))
}

/// Build the production existing-repository registration facade with a custom Git progress sink.
pub fn registration_facade_with_progress(
    progress_sink: Arc<dyn GitProgressSink>,
) -> Result<ProdRegistrationFacade, SharedChannelError> {
    Ok(ExistingChannelRegistrationFacade::new(
        gateway()?,
        DiskSharedChannelRegistry,
        GitExistingRepositoryScanner::new(git_skill_facade(progress_sink)),
        registration_sessions().clone(),
    ))
}

/// Build the production channel publication facade.
pub fn publication_facade() -> Result<ProdPublicationFacade, SharedChannelError> {
    publication_facade_with_progress(Arc::new(NoopGitProgressSink))
}

/// Build the production channel publication facade with a custom Git progress sink.
pub fn publication_facade_with_progress(
    progress_sink: Arc<dyn GitProgressSink>,
) -> Result<ProdPublicationFacade, SharedChannelError> {
    Ok(ChannelPublicationFacade::new(
        gateway()?,
        DiskSharedChannelRegistry,
        GitChannelDraftScanner::new(git_skill_facade(progress_sink)),
        publish_sessions().clone(),
    ))
}

/// Build the production channel membership & invitation inbox facade.
pub fn membership_facade() -> Result<ProdMembershipFacade, SharedChannelError> {
    Ok(ChannelMembershipFacade::new(
        gateway()?,
        DiskSharedChannelRegistry,
    ))
}

/// Build the production channel subscription / update / rollback / removal facade.
pub fn subscription_facade() -> Result<ProdSubscriptionFacade, SharedChannelError> {
    subscription_facade_with_progress(Arc::new(NoopGitProgressSink))
}

/// Build the production channel subscription facade with a custom Git progress sink.
pub fn subscription_facade_with_progress(
    progress_sink: Arc<dyn GitProgressSink>,
) -> Result<ProdSubscriptionFacade, SharedChannelError> {
    Ok(ChannelSubscriptionFacade::new(
        gateway()?,
        DiskSharedChannelRegistry,
        DiskChannelSubscriptionRegistry,
        GitChannelSubscriptionInstaller::new(git_skill_facade(progress_sink)),
    ))
}

/// Cancel an active existing-repository registration scan or preview session.
pub fn cancel_registration_scan(session_id: &str) -> bool {
    registration_sessions().cancel(session_id)
}

/// Cancel an active channel publish preview session.
pub fn cancel_publish_preview(session_id: &str) -> bool {
    publish_sessions().cancel(session_id)
}

/// Clear all in-memory registration and publication preview sessions (e.g. on logout).
pub fn clear_sessions() {
    registration_sessions().clear();
    publish_sessions().clear();
}

// ---- Credential-free local read-only and local preference operations --------

/// List registered shared channels from local disk without requiring GitHub login.
pub fn list_local_channels() -> Result<Vec<SharedChannelDescriptor>, SharedChannelError> {
    list_shared_channels(&DiskSharedChannelRegistry)
}

/// List local channel subscriptions (read-only tolerant for future schemas) without GitHub login.
pub fn list_local_subscriptions() -> Result<Vec<ChannelSubscriptionView>, SharedChannelError> {
    list_channel_subscriptions(&DiskChannelSubscriptionRegistry)
}

/// Read the persisted update snapshot for `repository_id` without requiring GitHub login.
pub fn channel_update_state(
    repository_id: u64,
) -> Result<Option<ChannelUpdateSnapshot>, SharedChannelError> {
    ss_channels::channel_update_state(&DiskChannelSubscriptionRegistry, repository_id)
}

/// Read the persisted auto-update state for `repository_id` without requiring GitHub login.
pub fn channel_auto_update_state(
    repository_id: u64,
) -> Result<ChannelAutoUpdateState, SharedChannelError> {
    ss_channels::channel_auto_update_state(&DiskChannelSubscriptionRegistry, repository_id)
}

/// Toggle per-channel protected auto-update preference on local disk without requiring GitHub login.
pub async fn set_channel_auto_update_enabled(
    repository_id: u64,
    enabled: bool,
) -> Result<ChannelAutoUpdateState, SharedChannelError> {
    ss_channels::set_channel_auto_update_enabled(
        &DiskChannelSubscriptionRegistry,
        repository_id,
        enabled,
    )
    .await
}

/// Export an active subscribed channel as a Claude Code plugin marketplace directory (local-only).
pub fn export_channel_marketplace(
    repository_id: u64,
    out_dir: &Path,
) -> Result<ChannelMarketplaceExport, SharedChannelError> {
    ss_channels::export_channel_marketplace(repository_id, out_dir)
}

/// Read local channel registry and subscription projections together without GitHub login.
pub fn local_channels_snapshot() -> Result<LocalChannelsSnapshot, SharedChannelError> {
    let channels = list_local_channels()?;
    let views = list_local_subscriptions()?;
    let mutable_by_id: HashMap<u64, ChannelSubscription> =
        match DiskChannelSubscriptionRegistry.load_mutable() {
            Ok(store) => store
                .subscriptions
                .into_iter()
                .map(|sub| (sub.repository_id, sub))
                .collect(),
            Err(err) if err.code == SharedChannelErrorCode::SubscriptionSchemaUnsupported => {
                HashMap::new()
            }
            Err(err) => return Err(err),
        };
    let subscriptions = views
        .into_iter()
        .map(|view| {
            let stored = if view.read_only {
                None
            } else {
                mutable_by_id.get(&view.repository_id)
            };
            LocalSubscriptionRecord {
                update_snapshot: stored.and_then(|s| s.last_update.clone()),
                pins: stored.map(|s| s.pins.clone()).unwrap_or_default(),
                view,
            }
        })
        .collect();
    Ok(LocalChannelsSnapshot {
        channels,
        subscriptions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn local_readers_work_without_github_sign_in() {
        let _guard = crate::test_support::ENV_LOCK.lock().await;
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().join("data");
        std::fs::create_dir_all(&data_dir).unwrap();
        let prev = std::env::var_os("SKILLSTAR_DATA_DIR");
        unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", &data_dir) };

        assert!(list_local_channels().unwrap().is_empty());
        assert!(list_local_subscriptions().unwrap().is_empty());
        assert!(channel_update_state(42).unwrap().is_none());
        let snap = local_channels_snapshot().unwrap();
        assert!(snap.channels.is_empty());
        assert!(snap.subscriptions.is_empty());

        clear_sessions();

        unsafe {
            match prev {
                Some(v) => std::env::set_var("SKILLSTAR_DATA_DIR", v),
                None => std::env::remove_var("SKILLSTAR_DATA_DIR"),
            }
        }
    }
}
