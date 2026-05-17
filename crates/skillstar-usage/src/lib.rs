//! `skillstar-usage` — Subscription / usage / renewal tracking for AI coding plans.
//!
//! Provides a unified data model and storage for tracking subscriptions across
//! AI coding plan providers (DeepSeek, GLM, Kimi, MiniMax, Cursor, Codex,
//! Antigravity, Trae, Qoder, plus a curated set of manual-entry providers).
//!
//! The crate is organized into:
//! - [`subscription`] — Domain types (Subscription, SubscriptionUsage, UsageWindow, ...)
//! - [`storage`]      — JSON persistence at `~/.skillstar/config/usage/`
//! - [`catalog`]      — Fixed catalog of 18 supported providers
//! - [`crypto`]       — AES-256-GCM helpers for API keys / OAuth tokens
//! - [`alerts`]       — Threshold-based alert computation
//! - [`oauth`]        — PKCE / local-server / poll-flow / device-flow primitives
//! - [`fetchers`]     — Per-provider quota fetchers (API key + OAuth)

pub mod alerts;
pub mod catalog;
pub mod crypto;
pub mod fetchers;
pub mod oauth;
pub mod storage;
pub mod subscription;

pub use catalog::{AuthMode, CatalogEntry, catalog};
pub use subscription::{
    BillingCycle, ManualQuota, MonetaryBalance, Subscription, SubscriptionAlert,
    SubscriptionUsage, UsageWindow,
};

/// Crate-level error type (wraps anyhow under the hood for IO/serialization).
#[derive(Debug, thiserror::Error)]
pub enum UsageError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("subscription not found: {0}")]
    NotFound(String),

    #[error("unknown catalog id: {0}")]
    UnknownCatalogId(String),

    #[error("fetcher error: {0}")]
    Fetcher(String),

    #[error("auth required (token expired or revoked)")]
    AuthRequired,

    #[error("{0}")]
    Other(String),
}

pub type UsageResult<T> = std::result::Result<T, UsageError>;
