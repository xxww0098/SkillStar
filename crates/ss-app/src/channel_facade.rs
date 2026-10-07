//! Production wiring of the shared-channel facade for ss-app entry points:
//! the GUI wake task and the `skillstar channel` CLI.

use std::sync::Arc;

use ss_git::transport::{GitAuthMaterial, GitOperationSession, NoopGitProgressSink};
use ss_skills::channels::shared_channels::{
    ChannelSubscriptionFacade, DiskChannelSubscriptionRegistry, DiskSharedChannelRegistry,
    GitChannelSubscriptionInstaller, ProductionSharedChannelGateway,
};
use ss_skills::git_skill::GitSkillFacade;
use ss_skills::github_auth::{
    FileCredentialStore, GitHubAuthError, GitHubAuthFacade, ProductionGitHubGateway, SystemClock,
};

pub(crate) type ProductionChannelFacade = ChannelSubscriptionFacade<
    ProductionSharedChannelGateway,
    DiskSharedChannelRegistry,
    DiskChannelSubscriptionRegistry,
    GitChannelSubscriptionInstaller,
>;

/// The facade with the signed-in GitHub credential. Channel repositories are
/// private, so a missing sign-in is an error (`NotAuthenticated`).
pub(crate) fn production_facade() -> Result<ProductionChannelFacade, GitHubAuthError> {
    let auth = GitHubAuthFacade::new(
        ProductionGitHubGateway::from_environment(),
        FileCredentialStore::default(),
        SystemClock,
    );
    let session = GitOperationSession::new(
        uuid::Uuid::new_v4().to_string(),
        auth.git_auth_material()
            .unwrap_or_else(|error| GitAuthMaterial::unavailable(error.to_string())),
        Arc::new(NoopGitProgressSink),
    );
    let credential = auth.api_credential()?;
    Ok(ChannelSubscriptionFacade::new(
        ProductionSharedChannelGateway::new(credential),
        DiskSharedChannelRegistry,
        DiskChannelSubscriptionRegistry,
        GitChannelSubscriptionInstaller::new(GitSkillFacade::new(session)),
    ))
}
