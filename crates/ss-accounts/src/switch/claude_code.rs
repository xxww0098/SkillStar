//! Claude Code: its own credential file.
//!
//! <config dir>/.credentials.json holds a "claudeAiOauth" segment:
//!
//!   { "claudeAiOauth": { "accessToken": "...", "refreshToken": "...",
//!                        "expiresAt": 1763000000000, "scopes": [...] },
//!     "mcpOAuth": { ... } }
//!
//! Config dir is SKILLSTAR_TOOL_SYNC_HOME/.claude, then $CLAUDE_CONFIG_DIR, then
//! ~/.claude (see [super::paths]).
//!
//! The segment carries no email, so a live Claude Code is matched against a
//! stored account by the access-token fingerprint rather than by identity.
//!
//! The file is read-modify-written, never replaced wholesale: every other key in
//! the file (mcpOAuth and whatever else Claude Code stores) and every other key
//! inside claudeAiOauth (scopes, subscriptionType, ...) is kept exactly as it
//! was. expiresAt is in MILLISECONDS; the OAuth module carries unix seconds.
//!
//! v1 writes the file only. On macOS Claude Code may read the
//! "Claude Code-credentials" keychain item instead, and this crate does not
//! bridge that keychain: a stale keychain entry can keep winning after a switch.

use std::path::PathBuf;

use serde_json::{Map, Value};

use super::paths::{self, PathRoots};
use super::{
    ClientSnapshot, LocalCredential, SwitchOutcome, SwitchRequest, read_json_file, snapshot_file,
    write_restricted,
};
use crate::error::{AccountError, AccountResult};

/// The file segment Claude Code reads its OAuth session from.
const OAUTH_SEGMENT: &str = "claudeAiOauth";

/// ACCESS_TOKEN, refreshToken, expiresAt live inside the segment.
const ACCESS_TOKEN_FIELD: &str = "accessToken";
const REFRESH_TOKEN_FIELD: &str = "refreshToken";
const EXPIRES_AT_FIELD: &str = "expiresAt";
const EMAIL_FIELD: &str = "email";

/// The file Claude Code reads at startup.
pub fn credential_path(roots: &PathRoots) -> PathBuf {
    paths::claude_credentials_path(roots)
}

/// Write the Claude Code credential.
pub fn write(roots: &PathRoots, request: &SwitchRequest<'_>) -> AccountResult<SwitchOutcome> {
    let path = credential_path(roots);
    let tokens = request.tokens;
    let access_token = tokens.access_token.trim();
    if access_token.is_empty() {
        return Err(AccountError::Client(
            "this account has no access token to write into Claude Code".to_string(),
        ));
    }

    let mut root = match read_json_file(&path)? {
        Some(Value::Object(existing)) => existing,
        _ => Map::new(),
    };
    let mut segment = match root.get(OAUTH_SEGMENT) {
        Some(Value::Object(existing)) => existing.clone(),
        _ => Map::new(),
    };
    segment.insert(
        ACCESS_TOKEN_FIELD.to_string(),
        Value::String(access_token.to_string()),
    );
    match tokens.refresh_token.as_deref().map(str::trim) {
        Some(refresh_token) if !refresh_token.is_empty() => {
            segment.insert(
                REFRESH_TOKEN_FIELD.to_string(),
                Value::String(refresh_token.to_string()),
            );
        }
        // The previous account's refresh token must not stay behind: Claude Code
        // would spend it to resurrect that account.
        _ => {
            segment.remove(REFRESH_TOKEN_FIELD);
        }
    }
    match tokens.expires_at {
        Some(expires_at) => {
            segment.insert(
                EXPIRES_AT_FIELD.to_string(),
                // Milliseconds in this file; the OAuth module counts seconds.
                Value::from(expires_at.saturating_mul(1000)),
            );
        }
        None => {
            segment.remove(EXPIRES_AT_FIELD);
        }
    }
    root.insert(OAUTH_SEGMENT.to_string(), Value::Object(segment));

    let body = serde_json::to_vec_pretty(&Value::Object(root))?;
    write_restricted(&path, &body)?;

    let live = super::live_with(roots, "claude-code")?;
    if live.access_fingerprint.is_none() {
        return Err(AccountError::Client(format!(
            "{} did not keep the credential",
            path.display()
        )));
    }
    Ok(SwitchOutcome {
        agent: "claude-code".to_string(),
        live: live.identity,
        target: path.display().to_string(),
    })
}

/// Read the Claude Code credential in full.
pub fn read(roots: &PathRoots) -> AccountResult<Option<LocalCredential>> {
    let path = credential_path(roots);
    let Some(root) = read_json_file(&path)? else {
        return Ok(None);
    };
    let Some(segment) = root.get(OAUTH_SEGMENT) else {
        return Ok(None);
    };
    let text = |key: &str| {
        segment
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let Some(access_token) = text(ACCESS_TOKEN_FIELD) else {
        return Ok(None);
    };
    let email = text(EMAIL_FIELD).or_else(|| crate::identity::jwt_email(&access_token));
    let plan = text("subscriptionType").or_else(|| text("rateLimitTier"));
    let provider_state = plan
        .as_ref()
        .map(|tier| serde_json::json!({ "plan": tier }));
    Ok(Some(LocalCredential {
        tokens: crate::oauth::TokenSet {
            expires_at: milliseconds_to_seconds(segment.get(EXPIRES_AT_FIELD))
                .or_else(|| crate::identity::jwt_exp(&access_token)),
            access_token,
            refresh_token: text(REFRESH_TOKEN_FIELD),
            id_token: None,
            token_type: "Bearer".to_string(),
        },
        identity: crate::oauth::Identity {
            display_name: email.clone(),
            email,
            plan,
            account_id: None,
        },
        provider_state,
        source: path.display().to_string(),
    }))
}

/// The identity a live Claude Code client is serving: the email it shows.
pub(crate) fn live_identity(identity: &crate::oauth::Identity) -> Option<String> {
    identity.email.clone()
}

/// Capture the credential file so a failed switch can put it back.
pub fn snapshot(roots: &PathRoots) -> AccountResult<ClientSnapshot> {
    snapshot_file(&credential_path(roots))
}

#[cfg(test)]
#[path = "claude_code_tests.rs"]
mod claude_code_tests;

/// The segment's expiresAt is milliseconds; the OAuth module carries seconds. Anything
/// small enough to be seconds already is taken at face value.
fn milliseconds_to_seconds(value: Option<&Value>) -> Option<i64> {
    let raw = value?.as_i64()?;
    if raw <= 0 {
        return None;
    }
    Some(if raw > 100_000_000_000 {
        raw / 1000
    } else {
        raw
    })
}
