//! OAuth login kickoff metadata returned to the desktop shell.

#[derive(Debug, Clone)]
pub struct OAuthStartInfo {
    pub auth_url: String,
    pub pending_id: String,
}

impl OAuthStartInfo {
    pub fn browser(auth_url: String, pending_id: String) -> Self {
        Self {
            auth_url,
            pending_id,
        }
    }
}
