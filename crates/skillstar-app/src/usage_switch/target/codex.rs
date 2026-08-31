//! Codex CLI: `~/.codex/auth.json` (honouring `CODEX_HOME`) plus, on macOS,
//! the keychain entry the CLI actually prefers.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};
use skillstar_usage::crypto;
use skillstar_usage::oauth::token_refresh;
use skillstar_usage::subscription::Subscription;

use super::super::error::{CustodyError, CustodyResult, ExternalStoreError, MaterializeError};
use super::{AccountIdentity, CliCredentialTarget, identity_from_jwt, secret};

/// What the user calls this CLI, for messages the user reads.
const TOOL: &str = "Codex";

/// Every "you have to log in again" message for this CLI ends the same way.
const REMEDY: &str = "请重新登录该账号补充凭证";

pub(in crate::usage_switch) struct CodexTarget;

impl CliCredentialTarget for CodexTarget {
    fn catalog_id(&self) -> &'static str {
        "codex"
    }

    fn tool_id(&self) -> &'static str {
        "codex"
    }

    fn live_path(&self) -> CustodyResult<PathBuf> {
        skillstar_models::tool_sync::resolve_codex_auth_path().map_err(|error| {
            CustodyError::ResolveLivePath {
                tool: TOOL,
                detail: error.to_string(),
            }
        })
    }

    fn access_token(&self, root: &Value) -> Option<String> {
        root.get("tokens")
            .and_then(|tokens| tokens.get("access_token"))
            .and_then(Value::as_str)
            .or_else(|| root.get("OPENAI_API_KEY").and_then(Value::as_str))
            .filter(|token| !token.is_empty())
            .map(str::to_string)
    }

    /// `auth.json` carries no expiry field of its own; the access token's JWT
    /// `exp` is the only clock, and API-key mode has none at all.
    fn expires_at(&self, root: &Value) -> Option<i64> {
        token_refresh::jwt_exp(&self.access_token(root)?)
    }

    fn identity(&self, root: &Value) -> AccountIdentity {
        let tokens = root.get("tokens");
        let mut identity = tokens
            .and_then(|t| t.get("id_token"))
            .and_then(Value::as_str)
            .map(identity_from_jwt)
            .unwrap_or_default();
        if identity.is_empty() {
            identity = self
                .access_token(root)
                .map(|token| identity_from_jwt(&token))
                .unwrap_or_default();
        }
        if identity.is_empty() {
            identity.push_subject(
                tokens
                    .and_then(|t| t.get("account_id"))
                    .and_then(Value::as_str),
            );
        }
        identity
    }

    /// Mirrors the schema the real CLI writes:
    /// `{ OPENAI_API_KEY, tokens{ id_token, access_token, refresh_token,
    /// account_id }, last_refresh }`. Unknown keys in `base` survive.
    fn materialize(
        &self,
        sub: &Subscription,
        base: Option<&Value>,
    ) -> Result<Value, MaterializeError> {
        let mut root = base
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_else(Map::new);

        if let Some(api_key) = secret(sub.api_key_encrypted.as_deref()) {
            root.insert("OPENAI_API_KEY".into(), Value::String(api_key));
            // An API key and an OAuth session are different logins; leaving
            // the old token block would let the CLI keep using it.
            root.remove("tokens");
            return Ok(Value::Object(root));
        }

        let missing = |field| MaterializeError::MissingSecret {
            tool: TOOL,
            field,
            remedy: REMEDY,
        };
        let access_token =
            secret(sub.access_token_encrypted.as_deref()).ok_or_else(|| missing("access_token"))?;
        let id_token =
            secret(sub.id_token_encrypted.as_deref()).ok_or_else(|| missing("id_token"))?;

        let tokens = serde_json::json!({
            "id_token": id_token,
            "access_token": access_token,
            "refresh_token": secret(sub.refresh_token_encrypted.as_deref()).unwrap_or_default(),
            "account_id": sub.oauth_account_id,
        });
        root.insert("OPENAI_API_KEY".into(), Value::Null);
        root.insert("tokens".into(), tokens);
        root.insert(
            "last_refresh".into(),
            Value::String(
                chrono::Utc::now()
                    .format("%Y-%m-%dT%H:%M:%S%.6fZ")
                    .to_string(),
            ),
        );
        Ok(Value::Object(root))
    }

    fn absorb(&self, root: &Value, sub: &mut Subscription) {
        let Some(tokens) = root.get("tokens") else {
            if let Some(key) = root.get("OPENAI_API_KEY").and_then(Value::as_str) {
                sub.api_key_encrypted = Some(crypto::encrypt(key));
            }
            return;
        };
        let field = |name: &str| {
            tokens
                .get(name)
                .and_then(Value::as_str)
                .filter(|v| !v.is_empty())
        };
        if let Some(access) = field("access_token") {
            sub.access_token_encrypted = Some(crypto::encrypt(access));
            sub.access_token_expires_at = token_refresh::jwt_exp(access);
        }
        sub.refresh_token_encrypted = field("refresh_token").map(crypto::encrypt);
        if let Some(id_token) = field("id_token") {
            sub.id_token_encrypted = Some(crypto::encrypt(id_token));
        }
        if let Some(account_id) = field("account_id") {
            sub.oauth_account_id = Some(account_id.to_string());
        }
        sub.requires_reauth = false;
    }

    fn external_root(&self, live: &Path) -> Option<Value> {
        #[cfg(target_os = "macos")]
        {
            super::super::keychain::read(live.parent()?)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = live;
            None
        }
    }

    fn publish_external(&self, live: &Path, root: &Value) -> Result<bool, ExternalStoreError> {
        #[cfg(target_os = "macos")]
        {
            let home = live.parent().ok_or(ExternalStoreError::MissingCodexHome)?;
            super::super::keychain::write_merged(home, root)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (live, root);
            Ok(false)
        }
    }
}
