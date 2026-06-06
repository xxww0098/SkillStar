//! OAuth login kickoff metadata returned to the desktop shell.

#[derive(Debug, Clone)]
pub struct OAuthStartInfo {
    pub auth_url: String,
    pub pending_id: String,
    /// GitHub Device Flow user code (e.g. GitHub Copilot).
    pub user_code: Option<String>,
    pub verification_uri: Option<String>,
}

impl OAuthStartInfo {
    pub fn browser(auth_url: String, pending_id: String) -> Self {
        Self {
            auth_url,
            pending_id,
            user_code: None,
            verification_uri: None,
        }
    }

    pub fn with_user_code(mut self, user_code: String) -> Self {
        self.user_code = Some(user_code);
        self
    }

    pub fn with_verification_uri(mut self, verification_uri: String) -> Self {
        self.verification_uri = Some(verification_uri);
        self
    }
}
