//! Production wiring of the shared-channel facade for ss-app wake tasks.

use std::sync::Arc;

use ss_channels::{
    ChannelSubscriptionFacade, DiskChannelSubscriptionRegistry, DiskSharedChannelRegistry,
    GitChannelSubscriptionInstaller, ProductionSharedChannelGateway,
};
use ss_git::transport::NoopGitProgressSink;
use ss_skills::github_auth::GitHubAuthError;

pub(crate) type ProductionChannelFacade = ChannelSubscriptionFacade<
    ProductionSharedChannelGateway,
    DiskSharedChannelRegistry,
    DiskChannelSubscriptionRegistry,
    GitChannelSubscriptionInstaller,
>;

/// The facade with the signed-in GitHub credential. Channel repositories are
/// private, so a missing sign-in is an error (`NotAuthenticated`).
pub(crate) fn production_facade() -> Result<ProductionChannelFacade, GitHubAuthError> {
    let credential = crate::channel_ops::api_credential()?;
    Ok(ChannelSubscriptionFacade::new(
        ProductionSharedChannelGateway::new(credential),
        DiskSharedChannelRegistry,
        DiskChannelSubscriptionRegistry,
        GitChannelSubscriptionInstaller::new(crate::channel_ops::git_skill_facade(Arc::new(
            NoopGitProgressSink,
        ))),
    ))
}
