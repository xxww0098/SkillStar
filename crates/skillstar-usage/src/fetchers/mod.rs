//! Per-provider quota fetchers.
//!
//! Two flavors:
//! - `api_key` — pure HTTP with `Authorization: Bearer <key>` (or variant)
//! - `oauth`   — uses access/refresh tokens, with browser-driven login flow
//!
//! Top-level [`refresh`] is the single entry point used by Tauri commands;
//! it dispatches by `catalog_id` to the right implementation.

pub mod api_key;
pub mod oauth;

use async_trait::async_trait;
use chrono::Utc;

use crate::subscription::{Subscription, SubscriptionUsage};
use crate::{UsageError, UsageResult};

/// Implementations are stateless; they read tokens out of the subscription
/// passed in and (for OAuth) return updated tokens via the returned struct
/// or by setting fields on the `subscription` in place (when refresh occurred).
#[async_trait]
pub trait UsageFetcher: Send + Sync {
    fn catalog_id(&self) -> &'static str;

    /// Fetch usage. On token-refresh, the implementation should mutate
    /// `subscription` in place (the caller will persist it).
    async fn fetch(&self, subscription: &mut Subscription) -> UsageResult<SubscriptionUsage>;
}

/// Dispatch a refresh based on the subscription's catalog id + auth mode.
pub async fn refresh(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    use crate::catalog::AuthMode;
    match subscription.auth_mode {
        AuthMode::ApiKey => api_key::dispatch(subscription).await,
        AuthMode::OAuth => oauth::dispatch(subscription).await,
        AuthMode::Manual => Ok(SubscriptionUsage {
            subscription_id: subscription.id.clone(),
            fetched_at: Utc::now().timestamp(),
            plan_name: subscription.plan_tier.clone(),
            ..Default::default()
        }),
    }
}

pub(crate) fn unsupported(id: &str) -> UsageError {
    UsageError::Other(format!("`{}` 暂未实现自动同步（请等待 v1.1）", id))
}
