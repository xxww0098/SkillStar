//! Tauri-owned lifetime for the GitHub authentication facade.

use skillstar_skills::git::transport::{
    GitAuthMaterial, GitOperationProgress, GitOperationSession, GitProgressSink,
};
use skillstar_skills::git_skill::GitSkillFacade;
use skillstar_skills::github_auth::{
    GitHubAuthError, GitHubAuthFacade, KeyringCredentialStore, ProductionGitHubGateway, SystemClock,
};
use skillstar_skills::shared_channels::{
    ChannelMembershipFacade, ChannelPublicationFacade, ChannelPublishSessions,
    ChannelSubscriptionFacade, DiskChannelSubscriptionRegistry, DiskSharedChannelRegistry,
    ExistingChannelRegistrationFacade, ExistingChannelRegistrationSessions, GitChannelDraftScanner,
    GitChannelSubscriptionInstaller, GitExistingRepositoryScanner, ProductionSharedChannelGateway,
    SharedChannelError, SharedChannelFacade,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};

pub type ProductionGitHubAuth =
    GitHubAuthFacade<ProductionGitHubGateway, KeyringCredentialStore, SystemClock>;

pub struct GitHubAuthState {
    facade: ProductionGitHubAuth,
    git_sessions: Mutex<HashMap<String, GitOperationSession>>,
    existing_channel_sessions: ExistingChannelRegistrationSessions,
    channel_publish_sessions: ChannelPublishSessions,
}

struct TauriGitProgressSink(AppHandle);

impl GitProgressSink for TauriGitProgressSink {
    fn emit(&self, progress: GitOperationProgress) {
        let _ = self.0.emit("skillstar://git-progress", progress);
    }
}

impl GitHubAuthState {
    pub fn new() -> Self {
        let facade = GitHubAuthFacade::new(
            ProductionGitHubGateway::from_environment(),
            KeyringCredentialStore,
            SystemClock,
        );
        Self {
            facade,
            git_sessions: Mutex::new(HashMap::new()),
            existing_channel_sessions: ExistingChannelRegistrationSessions::default(),
            channel_publish_sessions: ChannelPublishSessions::default(),
        }
    }

    pub fn facade(&self) -> &ProductionGitHubAuth {
        &self.facade
    }

    pub fn shared_channel_facade(
        &self,
    ) -> Result<
        SharedChannelFacade<ProductionSharedChannelGateway, DiskSharedChannelRegistry>,
        SharedChannelError,
    > {
        let credential = self.facade.api_credential()?;
        Ok(SharedChannelFacade::new(
            ProductionSharedChannelGateway::new(credential),
            DiskSharedChannelRegistry,
        ))
    }

    pub fn existing_channel_registration_facade(
        &self,
    ) -> Result<
        ExistingChannelRegistrationFacade<
            ProductionSharedChannelGateway,
            DiskSharedChannelRegistry,
        >,
        SharedChannelError,
    > {
        let credential = self.facade.api_credential()?;
        Ok(ExistingChannelRegistrationFacade::without_scanner(
            ProductionSharedChannelGateway::new(credential),
            DiskSharedChannelRegistry,
            self.existing_channel_sessions.clone(),
        ))
    }

    pub fn existing_channel_scan_facade(
        &self,
        git_facade: GitSkillFacade,
    ) -> Result<
        ExistingChannelRegistrationFacade<
            ProductionSharedChannelGateway,
            DiskSharedChannelRegistry,
        >,
        SharedChannelError,
    > {
        let credential = self.facade.api_credential()?;
        Ok(ExistingChannelRegistrationFacade::new(
            ProductionSharedChannelGateway::new(credential),
            DiskSharedChannelRegistry,
            GitExistingRepositoryScanner::new(git_facade),
            self.existing_channel_sessions.clone(),
        ))
    }

    pub fn channel_publication_facade(
        &self,
    ) -> Result<
        ChannelPublicationFacade<ProductionSharedChannelGateway, DiskSharedChannelRegistry>,
        SharedChannelError,
    > {
        let credential = self.facade.api_credential()?;
        Ok(ChannelPublicationFacade::without_scanner(
            ProductionSharedChannelGateway::new(credential),
            DiskSharedChannelRegistry,
            self.channel_publish_sessions.clone(),
        ))
    }

    pub fn channel_membership_facade(
        &self,
    ) -> Result<
        ChannelMembershipFacade<ProductionSharedChannelGateway, DiskSharedChannelRegistry>,
        SharedChannelError,
    > {
        let credential = self.facade.api_credential()?;
        Ok(ChannelMembershipFacade::new(
            ProductionSharedChannelGateway::new(credential),
            DiskSharedChannelRegistry,
        ))
    }

    pub fn channel_subscription_facade(
        &self,
        git_facade: GitSkillFacade,
    ) -> Result<
        ChannelSubscriptionFacade<
            ProductionSharedChannelGateway,
            DiskSharedChannelRegistry,
            DiskChannelSubscriptionRegistry,
            GitChannelSubscriptionInstaller,
        >,
        SharedChannelError,
    > {
        let credential = self.facade.api_credential()?;
        Ok(ChannelSubscriptionFacade::new(
            ProductionSharedChannelGateway::new(credential),
            DiskSharedChannelRegistry,
            DiskChannelSubscriptionRegistry,
            GitChannelSubscriptionInstaller::new(git_facade),
        ))
    }

    pub fn channel_publication_scan_facade(
        &self,
        git_facade: GitSkillFacade,
    ) -> Result<
        ChannelPublicationFacade<ProductionSharedChannelGateway, DiskSharedChannelRegistry>,
        SharedChannelError,
    > {
        let credential = self.facade.api_credential()?;
        Ok(ChannelPublicationFacade::new(
            ProductionSharedChannelGateway::new(credential),
            DiskSharedChannelRegistry,
            GitChannelDraftScanner::new(git_facade),
            self.channel_publish_sessions.clone(),
        ))
    }

    pub fn begin_git_operation(
        &self,
        app: AppHandle,
        requested_session_id: Option<String>,
    ) -> Result<GitSkillFacade, GitHubAuthError> {
        let session_id = match requested_session_id {
            Some(value) => uuid::Uuid::parse_str(&value)
                .map(|id| id.to_string())
                .map_err(|_| {
                    GitHubAuthError::new(
                        skillstar_skills::github_auth::GitHubAuthErrorCode::Protocol,
                        "Git operation session id must be a UUID",
                    )
                })?,
            None => uuid::Uuid::new_v4().to_string(),
        };
        let session = GitOperationSession::new(
            session_id,
            self.facade
                .git_auth_material()
                .unwrap_or_else(|error| GitAuthMaterial::unavailable(error.to_string())),
            Arc::new(TauriGitProgressSink(app)),
        );
        let mut sessions = self
            .git_sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if sessions.contains_key(session.id()) {
            return Err(GitHubAuthError::new(
                skillstar_skills::github_auth::GitHubAuthErrorCode::Protocol,
                "Git operation session id is already active",
            ));
        }
        sessions.insert(session.id().to_string(), session.clone());
        Ok(GitSkillFacade::new(session))
    }

    pub fn finish_git_operation(&self, session_id: &str) {
        self.git_sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(session_id);
    }

    pub fn cancel_git_operation(&self, session_id: &str) -> bool {
        let Ok(session_id) = uuid::Uuid::parse_str(session_id).map(|id| id.to_string()) else {
            return false;
        };
        let sessions = self
            .git_sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(session) = sessions.get(&session_id) else {
            return false;
        };
        session.cancel();
        true
    }

    pub fn cancel_existing_channel_registration(&self, session_id: &str) -> bool {
        let Ok(canonical_id) = uuid::Uuid::parse_str(session_id).map(|id| id.to_string()) else {
            return false;
        };
        self.cancel_git_operation(&canonical_id)
            | self.existing_channel_sessions.cancel(&canonical_id)
    }

    pub fn cancel_channel_publication(&self, session_id: &str) -> bool {
        let Ok(canonical_id) = uuid::Uuid::parse_str(session_id).map(|id| id.to_string()) else {
            return false;
        };
        self.cancel_git_operation(&canonical_id)
            | self.channel_publish_sessions.cancel(&canonical_id)
    }

    pub fn logout(&self) -> Result<(), GitHubAuthError> {
        let mut sessions = self
            .git_sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for session in sessions.values() {
            session.cancel();
        }
        sessions.clear();
        self.existing_channel_sessions.clear();
        self.channel_publish_sessions.clear();
        self.facade.logout()
    }
}

impl Default for GitHubAuthState {
    fn default() -> Self {
        Self::new()
    }
}
