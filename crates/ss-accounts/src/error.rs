//! Account-layer failures.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AccountError {
    #[error("account not found: {0}")]
    NotFound(String),

    #[error("unsupported agent: {0}")]
    UnsupportedAgent(String),

    /// The client's own credential store could not be written or read back.
    #[error("client credential unavailable: {0}")]
    Client(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    /// The stored credential is gone: the account needs a new login.
    #[error("login required: {0}")]
    AuthRequired(String),

    /// Login or quota layer failure, passed through with its own message.
    #[error("{0}")]
    Domain(String),

    /// Placeholder while a slice is being implemented.
    #[error("not implemented yet")]
    Unimplemented,
}

impl From<crate::oauth::OAuthError> for AccountError {
    fn from(error: crate::oauth::OAuthError) -> Self {
        match error {
            crate::oauth::OAuthError::AuthRequired(message) => Self::AuthRequired(message),
            other => Self::Domain(other.to_string()),
        }
    }
}

impl From<crate::usage::UsageError> for AccountError {
    fn from(error: crate::usage::UsageError) -> Self {
        match error {
            crate::usage::UsageError::AuthRequired(message) => Self::AuthRequired(message),
            other => Self::Domain(other.to_string()),
        }
    }
}

pub type AccountResult<T> = std::result::Result<T, AccountError>;
