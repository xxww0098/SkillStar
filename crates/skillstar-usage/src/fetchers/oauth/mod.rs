//! OAuth fetchers for IDE / CLI subscription providers.
//!
//! Each submodule is independent. They share helpers from
//! `crate::oauth::{pkce, local_server, poll_flow, token_refresh, ...}`.

mod start_info;

pub mod antigravity;
pub mod codex;
pub mod cursor;
pub mod opencode;
pub mod qoder;
pub mod trae;
pub mod xai;

pub use start_info::OAuthStartInfo;

use crate::UsageResult;
use crate::subscription::{Subscription, SubscriptionUsage};

/// Dispatch by `catalog_id`. Called from `fetchers::refresh` for OAuth subs.
pub async fn dispatch(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    match subscription.catalog_id.as_str() {
        "cursor" => cursor::fetch(subscription).await,
        "codex" => codex::fetch(subscription).await,
        "antigravity" => antigravity::fetch(subscription).await,
        "trae" => trae::fetch(subscription).await,
        "qoder" => qoder::fetch(subscription).await,
        "xai" => xai::fetch(subscription).await,
        "opencode" => opencode::fetch(subscription).await,
        other => Err(super::unsupported(other)),
    }
}

/// Kick off the browser OAuth login. Returns the URL to open + pending id.
pub async fn start_login(
    catalog_id: &str,
    region: Option<&str>,
    target_subscription_id: Option<&str>,
) -> UsageResult<OAuthStartInfo> {
    match catalog_id {
        "cursor" => cursor::start_login(region).await,
        "codex" => codex::start_login(region).await,
        "antigravity" => antigravity::start_login(region).await,
        "trae" => trae::start_login(region).await,
        "qoder" => qoder::start_login(region).await,
        "xai" => xai::start_login(region).await,
        "opencode" => opencode::start_login(region, target_subscription_id).await,
        other => Err(super::unsupported(other)),
    }
}
