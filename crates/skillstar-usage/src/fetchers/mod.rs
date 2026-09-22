//! Per-provider quota fetchers.
//!
//! Two flavors:
//! - `api_key` — pure HTTP with `Authorization: Bearer <key>` (or variant)
//! - `oauth`   — uses access/refresh tokens, with browser-driven login flow
//!
//! Top-level [`refresh`] is the single entry point used by Tauri commands;
//! it dispatches by `catalog_id` to the right implementation.

pub mod api_key;
pub mod cookie;
pub mod oauth;
/// Device-proof signer. Quota, login, and import live in [`oauth::trae`].
pub(crate) mod trae;

use chrono::Utc;

use crate::subscription::{Subscription, SubscriptionUsage};
use crate::{UsageError, UsageResult};

/// Dispatch a refresh based on the subscription's catalog id + auth mode.
pub async fn refresh(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    use crate::catalog::AuthMode;
    match subscription.auth_mode {
        AuthMode::ApiKey => api_key::dispatch(subscription).await,
        AuthMode::OAuth | AuthMode::TokenImport => oauth::dispatch(subscription).await,
        AuthMode::Cookie => cookie::dispatch(subscription).await,
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

/// Shared proxy-aware HTTP client used by the OAuth fetchers.
///
/// Equivalent to the per-file `fn http_client()` shims that used to live in
/// each fetcher: they were all one-line wrappers around
/// [`crate::http_client::usage_http_client`]. Fetchers should call this
/// directly instead of re-declaring a local alias.
pub(crate) fn http_client() -> UsageResult<reqwest::Client> {
    crate::http_client::usage_http_client()
}

/// Decrypt a required credential field.
///
/// `field_label` is used only in the "missing" error message (e.g.
/// `"缺少 access_token"`), so each fetcher keeps its original wording while
/// sharing the decrypt + empty-check logic that used to be copy-pasted
/// across the OAuth fetchers.
pub(crate) fn decrypt_required(cipher: &Option<String>, field_label: &str) -> UsageResult<String> {
    let cipher = cipher
        .as_deref()
        .ok_or_else(|| UsageError::Other(format!("缺少 {field_label}")))?;
    let pt = crate::crypto::decrypt(cipher);
    if pt.is_empty() {
        return Err(UsageError::AuthRequired);
    }
    Ok(pt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::AuthMode;

    #[tokio::test]
    async fn token_import_refresh_uses_the_oauth_dispatcher() {
        let mut sub =
            oauth::common::SubscriptionBuilder::new("cursor", "imported", "USD", "at", None)
                .build();
        // OpenCode's OAuth arm returns a fixed error and never touches the network.
        // API-key dispatch would refuse a missing key first; cookie dispatch would
        // refuse a missing jar; manual dispatch would succeed.
        sub.catalog_id = "opencode".into();
        sub.auth_mode = AuthMode::TokenImport;

        let error = refresh(&mut sub)
            .await
            .expect_err("opencode oauth is unavailable");
        assert!(
            error.to_string().contains("OpenCode 官方 OAuth token"),
            "{error}"
        );
    }
}
