//! OAuth login kickoff metadata returned to the desktop shell.

use crate::oauth::local_server;

#[derive(Debug, Clone)]
pub struct OAuthStartInfo {
    pub auth_url: String,
    pub pending_id: String,
    /// Seconds the login session stays alive waiting for the browser side,
    /// counted from `start_login`. Drives the dialog's countdown.
    pub expires_in_secs: Option<u32>,
}

impl OAuthStartInfo {
    pub fn browser(auth_url: String, pending_id: String) -> Self {
        Self {
            auth_url,
            pending_id,
            // Every browser flow shares this budget today: `local_server::wait`
            // defaults to it and Cursor's 2s×150 poll gives up at ~300s too.
            // A provider with a different budget overrides the field directly.
            expires_in_secs: Some(local_server::DEFAULT_CALLBACK_TIMEOUT.as_secs() as u32),
        }
    }
}
