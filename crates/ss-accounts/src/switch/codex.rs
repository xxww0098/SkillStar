//! Codex: the official CLI's own auth file.
//!
//! <codex home>/auth.json carries:
//!
//!   { "OPENAI_API_KEY": null,
//!     "tokens": { "id_token": "...", "access_token": "...",
//!                 "refresh_token": "...", "account_id": "..." },
//!     "last_refresh": "2026-11-15T05:00:00.123456Z" }
//!
//! Codex home is SKILLSTAR_TOOL_SYNC_HOME/.codex, then $CODEX_HOME, then
//! ~/.codex (see [super::paths]).
//!
//! v1 writes this file and nothing else. On macOS the Codex CLI may prefer its
//! "Codex Auth" keychain entry when one exists, and this crate does not bridge
//! that keychain: a stale keychain entry can keep winning after a switch. That
//! is stated in the accounts page rather than papered over.

use std::path::PathBuf;

use serde_json::{Map, Value};

use super::paths::{self, PathRoots};
use super::{
    ClientSnapshot, LocalCredential, SwitchOutcome, SwitchRequest, read_json_file, snapshot_file,
    write_restricted,
};
use crate::error::{AccountError, AccountResult};

/// The id-token claim that names the ChatGPT account.
const ACCOUNT_ID_CLAIM_PARENT: &str = "https://api.openai.com/auth";
const ACCOUNT_ID_CLAIM_CHILD: &str = "chatgpt_account_id";

/// The file Codex reads at startup.
pub fn credential_path(roots: &PathRoots) -> PathBuf {
    paths::codex_auth_path(roots)
}

/// Write the Codex CLI credential.
pub fn write(roots: &PathRoots, request: &SwitchRequest<'_>) -> AccountResult<SwitchOutcome> {
    let path = credential_path(roots);
    let tokens = request.tokens;
    let access_token = tokens.access_token.trim();
    if access_token.is_empty() {
        return Err(AccountError::Client(
            "this account has no access token to write into Codex".to_string(),
        ));
    }

    // Read-modify-write: top-level keys the CLI itself added survive.
    let mut root = match read_json_file(&path)? {
        Some(Value::Object(existing)) => existing,
        _ => Map::new(),
    };
    let mut token_map = match root.get("tokens") {
        Some(Value::Object(existing)) => existing.clone(),
        _ => Map::new(),
    };
    token_map.insert(
        "access_token".to_string(),
        Value::String(access_token.to_string()),
    );
    // An empty string is what the CLI itself leaves behind for a credential it
    // never received; keeping the previous account's token here would let Codex
    // refresh into the wrong account.
    token_map.insert(
        "refresh_token".to_string(),
        Value::String(tokens.refresh_token.clone().unwrap_or_default().trim().to_string()),
    );
    token_map.insert(
        "id_token".to_string(),
        Value::String(tokens.id_token.clone().unwrap_or_default().trim().to_string()),
    );
    token_map.insert(
        "account_id".to_string(),
        match account_id(request) {
            Some(account_id) => Value::String(account_id),
            None => Value::Null,
        },
    );
    root.insert("OPENAI_API_KEY".to_string(), Value::Null);
    root.insert("tokens".to_string(), Value::Object(token_map));
    root.insert(
        "last_refresh".to_string(),
        Value::String(
            chrono::Utc::now()
                .format("%Y-%m-%dT%H:%M:%S%.6fZ")
                .to_string(),
        ),
    );

    let body = serde_json::to_vec_pretty(&Value::Object(root))?;
    write_restricted(&path, &body)?;

    let live = super::live_with(roots, "codex")?;
    Ok(SwitchOutcome {
        agent: "codex".to_string(),
        live: live.identity,
        target: path.display().to_string(),
    })
}

/// Read the Codex CLI credential in full.
pub fn read(roots: &PathRoots) -> AccountResult<Option<LocalCredential>> {
    let path = credential_path(roots);
    let Some(root) = read_json_file(&path)? else {
        return Ok(None);
    };
    let tokens = root.get("tokens");
    let field = |name: &str| {
        tokens
            .and_then(|tokens| tokens.get(name))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    // An API-key login has no OAuth session to import.
    let Some(access_token) = field("access_token") else {
        return Ok(None);
    };
    let id_token = field("id_token");
    let email = id_token
        .as_deref()
        .and_then(crate::identity::jwt_email)
        .or_else(|| crate::identity::jwt_email(&access_token));
    Ok(Some(LocalCredential {
        tokens: crate::oauth::TokenSet {
            expires_at: crate::identity::jwt_exp(&access_token),
            access_token,
            refresh_token: field("refresh_token"),
            id_token,
            token_type: "Bearer".to_string(),
        },
        identity: crate::oauth::Identity {
            display_name: email.clone(),
            email,
            plan: None,
            account_id: field("account_id"),
        },
        provider_state: None,
        source: path.display().to_string(),
    }))
}

/// The identity a live Codex client is serving: its account id, nothing else.
pub(crate) fn live_identity(identity: &crate::oauth::Identity) -> Option<String> {
    identity.account_id.clone()
}

/// Capture the auth file so a failed switch can put it back.
pub fn snapshot(roots: &PathRoots) -> AccountResult<ClientSnapshot> {
    snapshot_file(&credential_path(roots))
}

#[cfg(test)]
#[path = "codex_tests.rs"]
mod codex_tests;

/// The account id to write: the record's own, else what the id token attests.
fn account_id(request: &SwitchRequest<'_>) -> Option<String> {
    if let Some(account_id) = request.account_id.map(str::trim).filter(|id| !id.is_empty()) {
        return Some(account_id.to_string());
    }
    let id_token = request.tokens.id_token.as_deref()?;
    crate::identity::jwt_nested_claim(
        id_token,
        ACCOUNT_ID_CLAIM_PARENT,
        ACCOUNT_ID_CLAIM_CHILD,
    )
}
