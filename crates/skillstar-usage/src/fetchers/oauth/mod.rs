//! OAuth fetchers for IDE / CLI subscription providers.
//!
//! Each submodule is independent. They share helpers from
//! `crate::oauth::{pkce, local_server, poll_flow, token_refresh, ...}`, plus
//! [`common::SubscriptionBuilder`] and [`common::impl_oauth_fetch`] for the
//! `Subscription`-construction and `fetch()`-wrapper boilerplate every
//! provider repeated (see `common.rs` for details). `cursor.rs` intentionally
//! does not use `common` — it is excluded from this refactor per project
//! rule.

mod start_info;

pub mod antigravity;
pub mod codex;
pub(crate) mod common;
pub mod cursor;
pub mod opencode;
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
        "xai" => xai::start_login(region, target_subscription_id).await,
        "opencode" => opencode::start_login(region, target_subscription_id).await,
        other => Err(super::unsupported(other)),
    }
}
