//! OpenCode: `$XDG_DATA_HOME/opencode/auth.json`, a flat
//! `providerID → credential` map at 0600 — the file `opencode auth login`
//! writes and the only one OpenCode's paid-model gate reads.
//!
//! Under custody this catalog stops needing an `api_key_encrypted` at all.
//! OpenCode's catalog auth modes are Cookie|Manual, so the old "project the
//! subscription's API key into the CLI" path could never fire — the switch
//! entry was permanently dead. Capturing the credential the CLI already holds
//! decouples "can this account be switched to" from "how did SkillStar
//! authenticate to read its usage".

use std::path::PathBuf;

use serde_json::{Map, Value};
use skillstar_usage::crypto;
use skillstar_usage::subscription::Subscription;

use super::super::error::{CustodyError, CustodyResult, MaterializeError};
use super::{AccountIdentity, CliCredentialTarget, secret};

/// What the user calls this CLI, for messages the user reads. `tool_id` stays
/// lowercase because the UI keys off it.
const TOOL: &str = "OpenCode";

/// OpenCode's own provider id for its first-party (Zen) service. A literal,
/// not a SkillStar-chosen name: `provider.ts` gates paid models on
/// `auth.json["opencode"]` and nothing else.
const PROVIDER_KEY: &str = "opencode";

pub(in crate::usage_switch) struct OpenCodeTarget;

impl OpenCodeTarget {
    fn credential<'a>(&self, root: &'a Value) -> Option<&'a Value> {
        root.get(PROVIDER_KEY)
    }
}

impl CliCredentialTarget for OpenCodeTarget {
    fn catalog_id(&self) -> &'static str {
        "opencode"
    }

    fn tool_id(&self) -> &'static str {
        "opencode"
    }

    fn live_path(&self) -> CustodyResult<PathBuf> {
        skillstar_models::tool_sync::resolve_opencode_auth_path().map_err(|error| {
            CustodyError::ResolveLivePath {
                tool: TOOL,
                detail: error.to_string(),
            }
        })
    }

    fn access_token(&self, root: &Value) -> Option<String> {
        let credential = self.credential(root)?;
        ["key", "access", "token"]
            .into_iter()
            .find_map(|field| credential.get(field).and_then(Value::as_str))
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    }

    /// Upstream stores OAuth expiry as epoch **milliseconds**; API keys have none.
    fn expires_at(&self, root: &Value) -> Option<i64> {
        let millis = self.credential(root)?.get("expires")?.as_i64()?;
        Some(millis / 1000)
    }

    /// OpenCode credentials carry no claims at all — no subject, no email.
    /// An empty identity is "unknown", which never conflicts, so capture can
    /// still attribute a session to the account the user is activating.
    fn identity(&self, _root: &Value) -> AccountIdentity {
        AccountIdentity::default()
    }

    fn materialize(
        &self,
        sub: &Subscription,
        base: Option<&Value>,
    ) -> Result<Value, MaterializeError> {
        let api_key = secret(sub.api_key_encrypted.as_deref())
            .ok_or(MaterializeError::NoCapturedSession { tool: TOOL })?;
        let mut root = base
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_else(Map::new);
        root.insert(
            PROVIDER_KEY.into(),
            serde_json::json!({ "type": "api", "key": api_key }),
        );
        Ok(Value::Object(root))
    }

    fn absorb(&self, root: &Value, sub: &mut Subscription) {
        let Some(credential) = self.credential(root) else {
            return;
        };
        match credential.get("type").and_then(Value::as_str) {
            Some("oauth") => {
                if let Some(access) = credential.get("access").and_then(Value::as_str) {
                    sub.access_token_encrypted = Some(crypto::encrypt(access));
                }
                sub.refresh_token_encrypted = credential
                    .get("refresh")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .map(crypto::encrypt);
                sub.access_token_expires_at = self.expires_at(root);
            }
            _ => {
                if let Some(key) = self.access_token(root) {
                    sub.api_key_encrypted = Some(crypto::encrypt(&key));
                }
            }
        }
    }
}
