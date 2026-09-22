//! OAuth fetchers for IDE / CLI subscription providers.
//!
//! Each submodule is independent. They share helpers from
//! `crate::oauth::{pkce, local_server, poll_flow, token_endpoint,
//! token_refresh, ...}`, plus [`common::SubscriptionBuilder`],
//! [`common::reauth_target`] / [`common::carry_over_user_metadata`] and
//! [`common::impl_oauth_fetch`] for the `Subscription`-construction and
//! `fetch()`-wrapper boilerplate every provider repeated (see `common.rs` for
//! details). `cursor.rs` uses only the re-authorization helpers and keeps its
//! own hand-written literal otherwise.

mod start_info;

use crate::UsageResult;
use crate::subscription::{Subscription, SubscriptionUsage};

pub mod anthropic;
pub mod antigravity;
pub mod codex;
pub(crate) mod common;
pub mod cursor;
// Local import only. `cursor.rs` stays untouched.
pub mod codebuddy;
pub(crate) mod cursor_import;
pub mod github_copilot;
pub mod kiro;
pub mod qoder;
pub mod trae;
pub mod xai;
// registration lands with the catalog row
pub mod windsurf;

// Zed callback RSA only. Not registered in `dispatch`.
#[allow(dead_code)]
mod zed_token;

pub use start_info::{OAuthFlow, OAuthStartInfo};

/// Pasted-callback rewrite, keyed by catalog id. No provider registers one
/// in this slice; a missing entry is the identity function.
type NormalizeCallbackInput = fn(&str) -> UsageResult<String>;

const CALLBACK_NORMALIZERS: &[(&str, NormalizeCallbackInput)] =
    &[("github-copilot", github_copilot::normalize_callback_input)];

pub(crate) fn normalize_callback_input(catalog_id: &str, input: &str) -> UsageResult<String> {
    if let Some((_, normalize)) = CALLBACK_NORMALIZERS
        .iter()
        .find(|(id, _)| *id == catalog_id)
    {
        return normalize(input);
    }
    Ok(input.to_string())
}

/// App entry for a pasted OAuth value. Branches on the pending session's flow.
pub async fn submit_callback(pending_id: &str, input: &str) -> UsageResult<()> {
    crate::oauth::manual_callback::deliver_manual_input(pending_id, input).await
}

/// Dispatch by `catalog_id`. Called from `fetchers::refresh` for OAuth subs.
pub async fn dispatch(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    match subscription.catalog_id.as_str() {
        "cursor" => cursor::fetch(subscription).await,
        "codex" => codex::fetch(subscription).await,
        "antigravity" => antigravity::fetch(subscription).await,
        "xai" => xai::fetch(subscription).await,
        "anthropic" => anthropic::fetch(subscription).await,
        "github-copilot" => github_copilot::fetch(subscription).await,
        "windsurf" => windsurf::fetch(subscription).await,
        "kiro" => kiro::fetch(subscription).await,
        "qoder" => qoder::fetch(subscription).await,
        "codebuddy" | "codebuddy-cn" => codebuddy::fetch(subscription).await,
        "trae" => trae::fetch(subscription).await,
        "trae-solo" => trae::fetch(subscription).await,
        "trae-cn" => trae::fetch(subscription).await,
        "trae-solo-cn" => trae::fetch(subscription).await,
        // OpenCode is Cookie/Manual only (`catalog.rs`). Its OAuth fetcher was
        // 265 lines that never issued a request — it only ever returned this
        // sentence. Legacy rows saved before the catalog narrowed still land
        // here, so they get the instruction rather than a generic
        // "unsupported".
        "opencode" => Err(crate::UsageError::Fetcher(
            OPENCODE_OAUTH_UNAVAILABLE.into(),
        )),
        other => Err(super::unsupported(other)),
    }
}

/// Why an OpenCode OAuth row cannot report usage, and what to do instead.
/// Mirrors the catalog entry's `warning`.
const OPENCODE_OAUTH_UNAVAILABLE: &str = "OpenCode 官方 OAuth token 只适用于 CLI 授权，不能读取 opencode.ai 控制台用量；请在订阅设置中切换到 Cookie 模式，并从 opencode.ai 控制台请求复制 Cookie。";

/// Kick off the browser OAuth login. Returns the URL to open + pending id.
///
/// `target_subscription_id` carries the row a re-authorization came from;
/// every provider threads it to its finalize so the completed login replaces
/// that card in place rather than adding a duplicate
/// (`docs/features/usage/README.md`).
pub async fn start_login(
    catalog_id: &str,
    region: Option<&str>,
    target_subscription_id: Option<&str>,
) -> UsageResult<OAuthStartInfo> {
    match catalog_id {
        "cursor" => cursor::start_login(region, target_subscription_id).await,
        "codex" => codex::start_login(region, target_subscription_id).await,
        "antigravity" => antigravity::start_login(region, target_subscription_id).await,
        "xai" => xai::start_login(region, target_subscription_id).await,
        // Not a browser flow: Claude Code owns the credential, so this adopts
        // the local store and resolves the pending login immediately.
        "anthropic" => anthropic::start_login(region, target_subscription_id).await,
        "github-copilot" => github_copilot::start_login(region, target_subscription_id).await,
        "windsurf" => windsurf::start_login(region, target_subscription_id).await,
        "kiro" => kiro::start_login(region, target_subscription_id).await,
        "qoder" => qoder::start_login(region, target_subscription_id).await,
        "codebuddy" | "codebuddy-cn" => {
            codebuddy::start_login(catalog_id, region, target_subscription_id).await
        }
        "trae" => trae::start_login(catalog_id, region, target_subscription_id).await,
        "trae-solo" => trae::start_login(catalog_id, region, target_subscription_id).await,
        "trae-cn" => trae::start_login(catalog_id, region, target_subscription_id).await,
        "trae-solo-cn" => trae::start_login(catalog_id, region, target_subscription_id).await,
        other => Err(super::unsupported(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{AuthMode, catalog};

    /// `start_login` must not offer a flow the catalog does not list — the
    /// frontend already filters by `auth_modes`, but the backend is the one
    /// that has to fail closed.
    #[tokio::test]
    async fn start_login_is_refused_for_catalogs_without_an_oauth_mode() {
        for entry in catalog() {
            if entry.auth_modes.contains(&AuthMode::OAuth) {
                continue;
            }
            let error = start_login(entry.id, None, None)
                .await
                .expect_err("non-OAuth catalog must not start an OAuth login");
            assert!(
                error.to_string().contains(entry.id),
                "{}: {error}",
                entry.id
            );
        }
    }

    #[test]
    fn normalize_callback_input_is_identity_until_a_provider_registers() {
        let pasted = "http://127.0.0.1:9/callback?code=abc";
        assert_eq!(normalize_callback_input("copilot", pasted).unwrap(), pasted);
        assert_eq!(normalize_callback_input("codex", "abc").unwrap(), "abc");
        let vscode = "https://vscode.dev/redirect?code=abc&state=http%3A%2F%2F127.0.0.1%3A9%2Fcallback%3Fnonce%3Dn";
        let normalized = normalize_callback_input("github-copilot", vscode).unwrap();
        assert!(
            normalized.starts_with("http://127.0.0.1:9/callback?"),
            "{normalized}"
        );
        assert_ne!(normalized, vscode);
    }
}
