//! Login and token failures, classified the way every caller branches on them
//! (the three-state table the old account code kept, unchanged).

use thiserror::Error;

#[derive(Debug, Error)]
pub enum OAuthError {
    /// The credential is gone: 401, `invalid_grant`, a revoked refresh token.
    /// Callers clear the session and mark the account `requires_reauth`.
    #[error("login required: {0}")]
    AuthRequired(String),

    /// The provider is momentarily unavailable (429 / 5xx / transport). The
    /// caller keeps the last good state and retries later.
    #[error("provider temporarily unavailable: {0}")]
    Transient(String),

    /// The provider rejected the request itself (other 4xx, malformed body, a
    /// login the user denied or let expire).
    #[error("login rejected: {0}")]
    Rejected(String),

    /// No login flow exists for this agent id.
    #[error("unsupported agent: {0}")]
    UnsupportedAgent(String),

    /// No pending login has this id (finished, cancelled, or never existed).
    #[error("unknown login session: {0}")]
    UnknownSession(String),

    #[error("{0}")]
    Other(String),
}

pub type OAuthResult<T> = std::result::Result<T, OAuthError>;
