//! Quota-read failures, classified the same three ways the login layer does:
//! the credential is gone, the provider is busy, or the provider answered
//! something we cannot read.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum UsageError {
    /// The credential no longer works (401 / revoked). The account layer marks
    /// the account `requires_reauth` and keeps the record.
    #[error("credential rejected: {0}")]
    AuthRequired(String),

    /// 429 / 5xx / transport: keep the last good snapshot and retry later.
    #[error("provider temporarily unavailable: {0}")]
    Transient(String),

    /// The endpoint answered something this crate cannot read (4xx, missing
    /// fields, a JSON shape change).
    #[error("usage unavailable: {0}")]
    Unreadable(String),

    /// No quota fetcher exists for this agent id.
    #[error("unsupported agent: {0}")]
    UnsupportedAgent(String),
}

#[cfg(test)]
impl UsageError {
    /// True when the failure is expected to clear on its own.
    pub fn is_transient(&self) -> bool {
        matches!(self, Self::Transient(_))
    }
}

pub type UsageResult<T> = std::result::Result<T, UsageError>;
