//! Sign an upstream request from an account the caller already copied in.
//!
//! This module does not open Usage's store, refresh a token, read a vendor
//! auth file, or request a quota URL. Anthropic generation stays on the
//! process bridge and produces no HTTP headers.

use serde_json::Value;

use crate::claude::AccountSnapshot;
use crate::route::order::AllowanceSnapshot;

/// Grok CLI version magpie sends when it has not probed a newer binary.
const GROK_CLIENT_VERSION: &str = "1.0.41";

/// Prefix the Antigravity sign-in test expects. The updater is not called.
const ANTIGRAVITY_HUB: &str = "antigravity/hub/3.1.4";

/// Copilot session header from magpie's saved-account sign path.
const COPILOT_INTEGRATION_ID: &str = "vscode-chat";

/// API key for a provider row. Accounts are not read for this path.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProviderSnapshot {
    pub api_key: Option<String>,
}

/// Read an account and the allowance Usage already stored.
///
/// Method names can change. The gateway tests supply a fake. The app
/// implements this with Usage's stored rows and does not fetch quota.
pub trait AccountBook {
    fn account(&self, catalog_id: &str) -> Option<AccountSnapshot>;
    fn allowance(&self, catalog_id: &str) -> Option<AllowanceSnapshot>;
}

/// What to sign. `body` is only read for Grok's model and conversation id.
pub struct SignInput<'a> {
    pub catalog_id: &'a str,
    pub provider: Option<&'a ProviderSnapshot>,
    pub body: &'a [u8],
}

/// Headers to put on the upstream request, plus the already-written allowance.
#[derive(Debug, Clone, PartialEq)]
pub struct SignedUpstream {
    pub headers: Vec<(String, String)>,
    /// `None` means the candidate stays unknown. This is not a quota fetch.
    pub allowance: Option<AllowanceSnapshot>,
    /// Anthropic stays on the process bridge. `headers` is empty.
    pub bridge: bool,
}

/// Sign `input` from `book`. `quota` is never called.
///
/// A missing account leaves `allowance` empty. Gemini CLI, Devin, WorkBuddy,
/// and Command Code ignore the account book and use `provider` only.
/// `?Sized` lets the caller pass a `dyn AccountBook`, which is how the
/// injected turn state machine holds the app's book.
pub fn sign_upstream(
    book: &(impl AccountBook + ?Sized),
    input: &SignInput<'_>,
    _quota: &mut dyn FnMut(&str),
) -> SignedUpstream {
    match input.catalog_id {
        "anthropic" => SignedUpstream {
            headers: Vec::new(),
            allowance: None,
            bridge: true,
        },
        "gemini" | "gemini-cli" | "devin" | "workbuddy" | "commandcode" => SignedUpstream {
            headers: api_key_headers(input.provider),
            allowance: None,
            bridge: false,
        },
        "codex" | "github-copilot" | "cursor" | "xai" | "kiro" | "zcode" | "antigravity" => {
            sign_account(book, input)
        }
        _ => SignedUpstream {
            headers: Vec::new(),
            allowance: None,
            bridge: false,
        },
    }
}

fn sign_account(book: &(impl AccountBook + ?Sized), input: &SignInput<'_>) -> SignedUpstream {
    let Some(account) = book.account(input.catalog_id) else {
        return SignedUpstream {
            headers: Vec::new(),
            allowance: None,
            bridge: false,
        };
    };
    let headers = match input.catalog_id {
        "codex" => codex_headers(&account),
        "github-copilot" => copilot_headers(&account),
        "cursor" => cursor_headers(&account),
        "xai" => xai_headers(&account, input.body),
        "kiro" => kiro_headers(&account),
        "zcode" => zcode_headers(&account),
        "antigravity" => antigravity_headers(&account),
        _ => Vec::new(),
    };
    SignedUpstream {
        headers,
        allowance: book.allowance(input.catalog_id),
        bridge: false,
    }
}

/// Bearer plus `accept`. The public Responses route does not take
/// `chatgpt-account-id`, `originator`, or a Codex affinity header.
fn codex_headers(account: &AccountSnapshot) -> Vec<(String, String)> {
    let Some(token) = secret(&account.access_token) else {
        return Vec::new();
    };
    vec![
        bearer(token),
        ("Accept".to_string(), "application/json".to_string()),
    ]
}

fn copilot_headers(account: &AccountSnapshot) -> Vec<(String, String)> {
    let Some(token) = secret(&account.access_token) else {
        return Vec::new();
    };
    vec![
        bearer(token),
        (
            "Copilot-Integration-Id".to_string(),
            COPILOT_INTEGRATION_ID.to_string(),
        ),
    ]
}

fn cursor_headers(account: &AccountSnapshot) -> Vec<(String, String)> {
    let Some(token) = secret(&account.access_token) else {
        return Vec::new();
    };
    vec![
        bearer(token),
        ("x-cursor-client-type".to_string(), "cli".to_string()),
    ]
}

fn kiro_headers(account: &AccountSnapshot) -> Vec<(String, String)> {
    secret(&account.access_token)
        .map(|token| vec![bearer(token)])
        .unwrap_or_default()
}

fn zcode_headers(account: &AccountSnapshot) -> Vec<(String, String)> {
    let Some(key) = secret(&account.api_key).or_else(|| secret(&account.access_token)) else {
        return Vec::new();
    };
    vec![
        ("x-api-key".to_string(), key.to_string()),
        bearer(key),
    ]
}

fn xai_headers(account: &AccountSnapshot, body: &[u8]) -> Vec<(String, String)> {
    let Some(token) = secret(&account.access_token) else {
        return Vec::new();
    };
    let mut headers = vec![
        bearer(token),
        (
            "x-grok-client-version".to_string(),
            GROK_CLIENT_VERSION.to_string(),
        ),
    ];
    if let Some(model) = json_str(body, "model") {
        headers.push(("x-grok-model-override".to_string(), model));
    }
    if let Some(conversation) = json_str(body, "prompt_cache_key") {
        headers.push(("x-grok-conv-id".to_string(), conversation));
    }
    headers.push((
        "User-Agent".to_string(),
        format!("grok-shell/{GROK_CLIENT_VERSION}"),
    ));
    headers
}

fn antigravity_headers(account: &AccountSnapshot) -> Vec<(String, String)> {
    let Some(token) = secret(&account.access_token) else {
        return Vec::new();
    };
    vec![
        bearer(token),
        (
            "User-Agent".to_string(),
            format!(
                "{ANTIGRAVITY_HUB} {}/{}",
                std::env::consts::OS,
                std::env::consts::ARCH
            ),
        ),
    ]
}

fn api_key_headers(provider: Option<&ProviderSnapshot>) -> Vec<(String, String)> {
    let Some(key) = provider.and_then(|row| secret(&row.api_key)) else {
        return Vec::new();
    };
    vec![bearer(key)]
}

fn bearer(token: &str) -> (String, String) {
    ("Authorization".to_string(), format!("Bearer {token}"))
}

fn secret(value: &Option<String>) -> Option<&str> {
    value.as_deref().filter(|text| !text.is_empty())
}

fn json_str(body: &[u8], key: &str) -> Option<String> {
    let value: Value = serde_json::from_slice(body).ok()?;
    let text = value.get(key)?.as_str()?;
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}
