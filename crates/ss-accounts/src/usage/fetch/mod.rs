//! Per-agent quota fetchers and their dispatch table.
//!
//! Each fetcher is a vertical slice: endpoint, request shape, response parse,
//! and the error classification. The dispatch table is the only place that
//! knows which agents exist. Network I/O stays in the thin fetch wrapper and
//! the parse functions are pure, so fixtures drive them without a socket.

use std::future::Future;
use std::time::Duration;

use crate::oauth::{OAuthError, TokenSet};

use crate::usage::credentials::{Credentials, UsageOutcome};
use crate::usage::error::{UsageError, UsageResult};

pub mod antigravity;
mod antigravity_parse;
pub mod claude_code;
pub mod codex;
pub mod cursor;
pub mod gemini_cli;
mod gemini_cli_parse;
pub mod grok;
mod grok_parse;
pub mod kiro;
mod kiro_parse;
pub mod zcode;
mod zcode_parse;
pub mod devin_desktop;
pub(crate) mod devin_desktop_parse;
mod parse;

/// Every quota read shares this deadline: a card refresh must not hang the UI.
pub(crate) const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

/// Refresh this far ahead of expiry, so a read never races the deadline.
const REFRESH_SKEW_SECONDS: i64 = 300;

/// Read one account's quota. Refreshes the tokens first when they are near
/// expiry, and returns whatever set should be persisted.
pub fn fetch(agent: &str, creds: &Credentials) -> UsageResult<UsageOutcome> {
    let parsed = crate::Agent::from_id(agent)
        .map_err(|_| UsageError::UnsupportedAgent(agent.to_string()))?;
    (crate::agents::spec(parsed).usage)(creds)
}

/// The tokens one read runs with: rotated through `crate::oauth` when they are
/// about to expire, else the ones the account layer handed in.
pub(crate) struct Reading {
    pub(crate) tokens: TokenSet,
    /// The rotated provider state, when the refresh changed one.
    pub(crate) provider_state: Option<serde_json::Value>,
}

/// Refresh the tokens when they are inside REFRESH_SKEW_SECONDS of expiring;
/// otherwise read with the ones we were given.
///
/// Antigravity's fetcher deliberately does not call this: trading a Google
/// refresh token invalidates the same grant in the IDE, so its quota read only
/// ever uses the access token the account already holds.
pub(crate) fn tokens_for_read(agent: &str, creds: &Credentials) -> UsageResult<Reading> {
    if !creds.tokens.needs_refresh(unix_now(), REFRESH_SKEW_SECONDS) {
        return Ok(Reading {
            tokens: creds.tokens.clone(),
            provider_state: None,
        });
    }
    let outcome = crate::oauth::refresh(agent, &creds.tokens, creds.provider_state.as_ref())
        .map_err(refresh_error)?;
    Ok(Reading {
        tokens: outcome.tokens,
        provider_state: outcome.provider_state,
    })
}

/// Unix seconds: the clock every snapshot timestamp uses.
pub(crate) fn unix_now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Classify a failed refresh into the same three states a quota read uses.
fn refresh_error(error: OAuthError) -> UsageError {
    match error {
        OAuthError::AuthRequired(message) => UsageError::AuthRequired(message),
        OAuthError::Transient(message) => UsageError::Transient(message),
        OAuthError::UnsupportedAgent(agent) => UsageError::UnsupportedAgent(agent),
        other => UsageError::Unreadable(format!("token refresh: {other}")),
    }
}

/// One raw answer: the status and the body exactly as they arrived.
pub(crate) struct Answer {
    pub(crate) status: u16,
    pub(crate) body: String,
}

/// GET a quota endpoint through the shared probe client, so the user's proxy
/// settings apply and no second HTTP stack is introduced.
pub(crate) fn http_get(label: &str, url: &str, headers: &[(&str, String)]) -> UsageResult<Answer> {
    let client =
        ss_core::infra::http_client::probe_http_client(REQUEST_TIMEOUT).map_err(|error| {
            UsageError::Transient(format!("{label}: cannot build the HTTP client: {error}"))
        })?;
    let request = client.get(url).headers(header_map(label, headers)?);
    block_on_request(async move { send(request, label).await })?
}

/// POST a JSON body to a quota endpoint, same client and same answer shape as
/// `http_get`. Antigravity's Cloud Code endpoints are JSON RPC in all but name.
pub(crate) fn http_post_json(
    label: &str,
    url: &str,
    headers: &[(&str, String)],
    body: &serde_json::Value,
) -> UsageResult<Answer> {
    let client =
        ss_core::infra::http_client::probe_http_client(REQUEST_TIMEOUT).map_err(|error| {
            UsageError::Transient(format!("{label}: cannot build the HTTP client: {error}"))
        })?;
    let request = client
        .post(url)
        .headers(header_map(label, headers)?)
        .json(body);
    block_on_request(async move { send(request, label).await })?
}

/// One request, whether it carried a body or not.
async fn send(request: reqwest::RequestBuilder, label: &str) -> UsageResult<Answer> {
    let response = request
        .send()
        .await
        .map_err(|error| UsageError::Transient(format!("{label}: request failed: {error}")))?;
    let status = response.status().as_u16();
    let body = response.text().await.map_err(|error| {
        UsageError::Transient(format!("{label}: cannot read the reply body: {error}"))
    })?;
    Ok(Answer { status, body })
}

fn header_map(
    label: &str,
    headers: &[(&str, String)],
) -> UsageResult<reqwest::header::HeaderMap> {
    let mut map = reqwest::header::HeaderMap::new();
    for (name, value) in headers {
        let header_name =
            reqwest::header::HeaderName::from_bytes(name.as_bytes()).map_err(|error| {
                UsageError::Unreadable(format!("{label}: bad request header {name}: {error}"))
            })?;
        let header_value = reqwest::header::HeaderValue::from_str(value).map_err(|_| {
            UsageError::Unreadable(format!(
                "{label}: the stored credential carries a character an HTTP header cannot hold"
            ))
        })?;
        map.insert(header_name, header_value);
    }
    Ok(map)
}

/// A non-2xx answer, classified the three ways the account layer branches on:
/// the credential is gone, the provider is busy, or the answer is unreadable.
pub(crate) fn classify_status(label: &str, status: u16, body: &str) -> UsageError {
    let detail = format!("{label}: HTTP {status} {}", parse::snippet(body))
        .trim_end()
        .to_string();
    if status == 401 {
        UsageError::AuthRequired(detail)
    } else if status == 429 || (500..600).contains(&status) {
        UsageError::Transient(detail)
    } else {
        UsageError::Unreadable(detail)
    }
}

/// Drive one async request from this crate's synchronous entry points.
///
/// probe_http_client (the only client allowed to leave the app) is async and
/// reqwest::blocking would bypass it along with the proxy settings, so a
/// short-lived runtime bridges the two. Only call this from a blocking
/// context: starting a runtime from inside an async task panics.
fn block_on_request<F: Future>(future: F) -> UsageResult<F::Output> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| {
            UsageError::Transient(format!("cannot start the quota request runtime: {error}"))
        })?;
    Ok(runtime.block_on(future))
}
