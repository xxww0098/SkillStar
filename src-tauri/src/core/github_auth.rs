//! Tauri-owned lifetime for the GitHub authentication facade.

use skillstar_skills::github_auth::{
    GitHubAuthFacade, KeyringCredentialStore, ProductionGitHubGateway, SystemClock,
};

pub type ProductionGitHubAuth =
    GitHubAuthFacade<ProductionGitHubGateway, KeyringCredentialStore, SystemClock>;

pub struct GitHubAuthState {
    facade: ProductionGitHubAuth,
}

impl GitHubAuthState {
    pub fn new() -> Self {
        let facade = GitHubAuthFacade::new(
            ProductionGitHubGateway::from_environment(),
            KeyringCredentialStore,
            SystemClock,
        );
        Self { facade }
    }

    pub fn facade(&self) -> &ProductionGitHubAuth {
        &self.facade
    }
}

impl Default for GitHubAuthState {
    fn default() -> Self {
        Self::new()
    }
}
