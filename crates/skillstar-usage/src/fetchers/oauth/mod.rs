//! OAuth fetchers (Cursor, Codex, Antigravity, Trae, Qoder).
//!
//! Each submodule is independent. They share helpers from
//! `crate::oauth::{pkce, local_server, poll_flow, token_refresh, ...}`.

pub mod antigravity;
pub mod codex;
pub mod cursor;
pub mod qoder;
pub mod trae;

use crate::subscription::{Subscription, SubscriptionUsage};
use crate::UsageResult;

/// Dispatch by `catalog_id`. Called from `fetchers::refresh` for OAuth subs.
pub async fn dispatch(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    match subscription.catalog_id.as_str() {
        "cursor" => cursor::fetch(subscription).await,
        "codex" => codex::fetch(subscription).await,
        "antigravity" => antigravity::fetch(subscription).await,
        "trae" => trae::fetch(subscription).await,
        "qoder" => qoder::fetch(subscription).await,
        other => Err(super::unsupported(other)),
    }
}

/// Kick off the browser OAuth login. Returns the URL to open + pending id.
pub async fn start_login(
    catalog_id: &str,
    region: Option<&str>,
) -> UsageResult<(String, String)> {
    match catalog_id {
        "cursor" => cursor::start_login(region).await,
        "codex" => codex::start_login(region).await,
        "antigravity" => antigravity::start_login(region).await,
        "trae" => trae::start_login(region).await,
        "qoder" => qoder::start_login(region).await,
        other => Err(super::unsupported(other)),
    }
}
