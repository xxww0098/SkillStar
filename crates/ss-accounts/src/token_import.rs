//! Reading a pasted token or credential JSON into a token set.
//!
//! Two shapes are accepted, because two are what people actually paste: a bare
//! access token, and the JSON a provider's own client keeps on disk. snake_case
//! and camelCase both count - Codex writes access_token, Cursor's storage is
//! accessToken - and a nested "tokens" / "token" object (Codex's auth.json,
//! Antigravity's go-keyring payload) is descended into.
//!
//! Antigravity also pastes the macOS Keychain value verbatim, which is
//! `go-keyring-base64:` + base64(that JSON); the wrapper is unwrapped first.
//!
//! Nothing here trades a refresh token for a new one: a Google refresh would
//! invalidate the same grant in the client, so an import copies what is on disk.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::Value;
use crate::oauth::{Identity, TokenSet};

use crate::error::{AccountError, AccountResult};
use crate::identity;

/// One pasted credential, ready to become an account row.
pub(crate) struct ImportedToken {
    pub tokens: TokenSet,
    pub identity: Identity,
    /// Provider-private context the credential came with, when it did.
    pub provider_state: Option<serde_json::Value>,
}

impl std::fmt::Debug for ImportedToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ImportedToken")
            .field("identity", &self.identity)
            .field("tokens", &"<redacted>")
            .finish()
    }
}

/// Parse a pasted token or credential JSON for the agent.
pub(crate) fn parse(agent: &str, payload: &str) -> AccountResult<ImportedToken> {
    let text = payload.trim();
    if text.is_empty() {
        return Err(AccountError::Client(
            "paste an access token or a credential JSON".to_string(),
        ));
    }
    let text = unwrap_keychain(text)?;
    if text.starts_with('{') {
        parse_json(agent, &serde_json::from_str(&text)?)
    } else {
        Ok(build(agent, Fields {
            access_token: text,
            ..Fields::default()
        }))
    }
}

/// 把 `go-keyring-base64:` 包装剥掉（Antigravity 的 macOS Keychain 值）。
fn unwrap_keychain(text: &str) -> AccountResult<String> {
    let prefix = crate::switch::antigravity_credential::KEYCHAIN_PREFIX;
    let Some(encoded) = text.strip_prefix(prefix) else {
        return Ok(text.to_string());
    };
    let decoded = STANDARD.decode(encoded.trim()).map_err(|error| {
        AccountError::Client(format!("粘贴的 Keychain 凭据解码失败: {error}"))
    })?;
    String::from_utf8(decoded).map_err(|_| {
        AccountError::Client("粘贴的 Keychain 凭据不是有效 UTF-8".to_string())
    })
}

/// The expiry spellings the clients on disk actually use: `expires_at`/`expiry`
/// from the OAuth clients, `expiry_date` (in milliseconds) from Gemini CLI.
const EXPIRY_KEYS: [&str; 4] = ["expires_at", "expiresAt", "expiry", "expiry_date"];

/// Every field a paste may carry, before the token itself fills in the rest.
#[derive(Default)]
struct Fields {
    access_token: String,
    refresh_token: Option<String>,
    id_token: Option<String>,
    expires_at: Option<i64>,
    account_id: Option<String>,
    email: Option<String>,
    display_name: Option<String>,
}

fn parse_json(agent: &str, root: &Value) -> AccountResult<ImportedToken> {
    // A pasted client file may nest the session (Codex's auth.json nests
    // "tokens", Antigravity's go-keyring payload nests "token").
    let session = nested_session(root);
    let access_token = text_field(session, &["access_token", "accessToken", "token"])
        .or_else(|| text_field(root, &["access_token", "accessToken", "token"]))
        .ok_or_else(|| {
            AccountError::Client("the pasted JSON carries no access token".to_string())
        })?;
    Ok(build(agent, Fields {
        access_token,
        refresh_token: text_field(session, &["refresh_token", "refreshToken"]),
        id_token: text_field(session, &["id_token", "idToken"]),
        expires_at: timestamp_field(session, &EXPIRY_KEYS)
            .or_else(|| timestamp_field(root, &EXPIRY_KEYS)),
        account_id: text_field(session, &["account_id", "accountId", "authId"])
            .or_else(|| text_field(root, &["account_id", "accountId", "authId"])),
        email: text_field(session, &["email", "cachedEmail"])
            .or_else(|| text_field(root, &["email", "cachedEmail"])),
        display_name: text_field(root, &["display_name", "displayName", "name"]),
    }))
}

/// The nested session object, when the payload carries one.
fn nested_session(root: &Value) -> &Value {
    ["tokens", "token"]
        .into_iter()
        .find_map(|key| root.get(key).filter(|value| value.is_object()))
        .unwrap_or(root)
}

fn build(agent: &str, fields: Fields) -> ImportedToken {
    let access_token = fields.access_token.trim().to_string();
    let id_token = clean(fields.id_token);
    let account_id = clean(fields.account_id)
        .or_else(|| provider_account_id(agent, &access_token, id_token.as_deref()));
    let email = clean(fields.email)
        .or_else(|| identity::jwt_email(&access_token))
        .or_else(|| id_token.as_deref().and_then(identity::jwt_email));
    let display_name = clean(fields.display_name).or_else(|| email.clone());
    ImportedToken {
        tokens: TokenSet {
            expires_at: fields.expires_at.or_else(|| identity::jwt_exp(&access_token)),
            access_token,
            refresh_token: clean(fields.refresh_token),
            id_token,
            token_type: "Bearer".to_string(),
        },
        identity: Identity {
            email,
            display_name,
            plan: None,
            account_id,
        },
        provider_state: None,
    }
}

/// The provider's own account id, when the tokens themselves attest one.
fn provider_account_id(agent: &str, access_token: &str, id_token: Option<&str>) -> Option<String> {
    crate::Agent::from_id(agent)
        .ok()
        .and_then(|parsed| (crate::agents::spec(parsed).provider_account_id)(access_token, id_token))
}

fn text_field(root: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        root.get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

/// An expiry time: epoch seconds / milliseconds as a number or numeric string,
/// or RFC 3339 (Antigravity writes the latter).
fn timestamp_field(root: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter().find_map(|key| {
        let value = root.get(*key)?;
        if let Some(number) = value.as_i64() {
            return Some(epoch_seconds(number));
        }
        let text = value.as_str()?.trim();
        if let Ok(number) = text.parse::<i64>() {
            return Some(epoch_seconds(number));
        }
        chrono::DateTime::parse_from_rfc3339(text)
            .ok()
            .map(|at| at.timestamp())
    })
}

/// Providers disagree: the OAuth module counts unix seconds, some clients store
/// milliseconds. Anything past 1973-in-milliseconds is treated as milliseconds.
fn epoch_seconds(raw: i64) -> i64 {
    if raw > 100_000_000_000 {
        raw / 1000
    } else {
        raw
    }
}

fn clean(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
#[path = "token_import_tests.rs"]
mod token_import_tests;
