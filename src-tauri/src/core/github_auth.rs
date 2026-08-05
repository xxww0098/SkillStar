//! Tauri-owned lifetime for the GitHub authentication facade.

use skillstar_skills::git::transport::{
    GitAuthMaterial, GitOperationProgress, GitOperationSession, GitProgressSink,
};
use skillstar_skills::git_skill::GitSkillFacade;
use skillstar_skills::github_auth::{
    GitHubAuthError, GitHubAuthFacade, KeyringCredentialStore, ProductionGitHubGateway, SystemClock,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};

pub type ProductionGitHubAuth =
    GitHubAuthFacade<ProductionGitHubGateway, KeyringCredentialStore, SystemClock>;

pub struct GitHubAuthState {
    facade: ProductionGitHubAuth,
    git_sessions: Mutex<HashMap<String, GitOperationSession>>,
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
        }
    }

    pub fn facade(&self) -> &ProductionGitHubAuth {
        &self.facade
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
        let sessions = self
            .git_sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(session) = sessions.get(session_id) else {
            return false;
        };
        session.cancel();
        true
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
        self.facade.logout()
    }
}

impl Default for GitHubAuthState {
    fn default() -> Self {
        Self::new()
    }
}
