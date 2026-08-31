//! Grok CLI: `~/.grok/auth.json` (honouring `GROK_HOME`), a JSON object keyed
//! by OIDC scope URL, plus the official `auth.json.lock` the CLI itself takes
//! while rotating tokens.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde_json::{Map, Value};
use skillstar_usage::crypto;
use skillstar_usage::oauth::token_refresh;
use skillstar_usage::subscription::Subscription;

use super::super::error::{CustodyError, CustodyResult, MaterializeError};
use super::{
    AccountIdentity, CliCredentialTarget, secret, subscription_expiry, subscription_identity,
};

/// What the user calls this CLI, for messages the user reads. `tool_id` stays
/// lowercase because the UI keys off it.
const TOOL: &str = "Grok";

/// Scopes the Grok CLI itself requires. A token holding only `api:access`
/// authenticates for billing but makes every CLI call fail, so writing one
/// would look like a successful switch and behave like a broken login.
const REQUIRED_CLI_SCOPES: [&str; 3] = [
    "grok-cli:access",
    "conversations:read",
    "conversations:write",
];

/// Claims the CLI expects to find mirrored onto the entry itself.
const MIRRORED_CLAIMS: [&str; 11] = [
    "email",
    "user_id",
    "principal_id",
    "principal_type",
    "team_id",
    "team_name",
    "team_role",
    "subscription_tier",
    "first_name",
    "last_name",
    "profile_image_asset_id",
];

pub(in crate::usage_switch) struct GrokTarget;

fn scope_key() -> String {
    format!(
        "https://auth.x.ai::{}",
        skillstar_usage::fetchers::oauth::xai::client_id()
    )
}

impl GrokTarget {
    fn entry<'a>(&self, root: &'a Value) -> Option<&'a Value> {
        root.get(scope_key())
    }
}

impl CliCredentialTarget for GrokTarget {
    fn catalog_id(&self) -> &'static str {
        "xai"
    }

    fn tool_id(&self) -> &'static str {
        "grok"
    }

    fn live_path(&self) -> CustodyResult<PathBuf> {
        skillstar_models::tool_sync::resolve_grok_auth_path().map_err(|error| {
            CustodyError::ResolveLivePath {
                tool: TOOL,
                detail: error.to_string(),
            }
        })
    }

    /// Grok's own lock, not a private one: the CLI takes it before rotating,
    /// so sharing it is what keeps a single-use refresh token from being spent
    /// twice.
    fn lock_path(&self, live: &Path) -> PathBuf {
        live.with_extension("json.lock")
    }

    fn lock_holder_line(&self) -> bool {
        true
    }

    fn access_token(&self, root: &Value) -> Option<String> {
        self.entry(root)?
            .get("key")
            .and_then(Value::as_str)
            .filter(|token| !token.is_empty())
            .map(str::to_string)
    }

    fn expires_at(&self, root: &Value) -> Option<i64> {
        let stored = self
            .entry(root)
            .and_then(|entry| entry.get("expires_at"))
            .and_then(Value::as_str)
            .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
            .map(|parsed| parsed.timestamp());
        let jwt = self
            .access_token(root)
            .and_then(|token| token_refresh::jwt_exp(&token));
        match (stored, jwt) {
            (Some(stored), Some(jwt)) => Some(stored.min(jwt)),
            (stored, jwt) => stored.or(jwt),
        }
    }

    fn identity(&self, root: &Value) -> AccountIdentity {
        let mut identity = AccountIdentity::default();
        let Some(entry) = self.entry(root) else {
            return identity;
        };
        for field in ["sub", "user_id", "principal_id"] {
            identity.push_subject(entry.get(field).and_then(Value::as_str));
        }
        identity.push_email(entry.get("email").and_then(Value::as_str));
        identity
    }

    fn materialize(
        &self,
        sub: &Subscription,
        base: Option<&Value>,
    ) -> Result<Value, MaterializeError> {
        let access_token = secret(sub.access_token_encrypted.as_deref()).ok_or(
            MaterializeError::MissingSecret {
                tool: TOOL,
                field: "access_token",
                remedy: "请重新授权该账号",
            },
        )?;
        check_cli_scopes(&access_token)?;

        let mut root = base
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_else(Map::new);
        let scope = scope_key();
        let mut entry = root
            .get(&scope)
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_else(Map::new);

        entry.insert("key".into(), Value::String(access_token.clone()));
        if let Some(refresh) = secret(sub.refresh_token_encrypted.as_deref()) {
            entry.insert("refresh_token".into(), Value::String(refresh));
        }
        if let Some(expires_at) = subscription_expiry(sub).and_then(format_rfc3339) {
            entry.insert("expires_at".into(), Value::String(expires_at));
        }
        // Identity fields carried over from *another* account's entry would
        // make the new login answer to the old name. The same account's own
        // fields are kept: they are CLI-private metadata (team, profile) that
        // a rotated opaque token cannot reproduce.
        if self
            .identity(base.unwrap_or(&Value::Null))
            .conflicts(&subscription_identity(sub))
        {
            for field in MIRRORED_CLAIMS {
                entry.remove(field);
            }
        }
        fill_schema(&mut entry, &access_token, sub);
        require_restorable(&entry)?;

        root.insert(scope, Value::Object(entry));
        Ok(Value::Object(root))
    }

    fn absorb(&self, root: &Value, sub: &mut Subscription) {
        let Some(entry) = self.entry(root) else {
            return;
        };
        if let Some(token) = entry.get("key").and_then(Value::as_str) {
            sub.access_token_encrypted = Some(crypto::encrypt(token));
        }
        sub.refresh_token_encrypted = entry
            .get("refresh_token")
            .and_then(Value::as_str)
            .filter(|token| !token.is_empty())
            .map(crypto::encrypt);
        sub.access_token_expires_at = self.expires_at(root);
        if let Some(user_id) = ["user_id", "principal_id", "sub"]
            .into_iter()
            .find_map(|field| entry.get(field).and_then(Value::as_str))
            .filter(|value| !value.is_empty())
        {
            sub.oauth_account_id = Some(user_id.to_string());
        }
        sub.requires_reauth = false;
    }
}

/// Bring an entry up to the shape the Grok CLI reads, without overwriting
/// anything the CLI already put there.
fn fill_schema(entry: &mut Map<String, Value>, access_token: &str, sub: &Subscription) {
    entry
        .entry("auth_mode")
        .or_insert_with(|| Value::String("oidc".into()));
    entry
        .entry("oidc_issuer")
        .or_insert_with(|| Value::String("https://auth.x.ai".into()));
    entry.entry("oidc_client_id").or_insert_with(|| {
        Value::String(skillstar_usage::fetchers::oauth::xai::client_id().into())
    });

    if let Some(claims) = token_refresh::decode_jwt_payload(access_token) {
        for field in MIRRORED_CLAIMS {
            if let Some(value) = claims.get(field).and_then(Value::as_str) {
                entry
                    .entry(field)
                    .or_insert_with(|| Value::String(value.to_string()));
            }
        }
        if let Some(subject) = claims.get("sub").and_then(Value::as_str) {
            entry
                .entry("user_id")
                .or_insert_with(|| Value::String(subject.to_string()));
        }
        if let Some(opt_out) = claims
            .get("coding_data_retention_opt_out")
            .and_then(Value::as_bool)
        {
            entry
                .entry("coding_data_retention_opt_out")
                .or_insert(Value::Bool(opt_out));
        }
    }

    let identity = subscription_identity(sub);
    if let Some(email) = identity.email().map(str::to_string) {
        entry.entry("email").or_insert(Value::String(email));
    }
    if let Some(subject) = identity.subject().map(str::to_string) {
        entry
            .entry("user_id")
            .or_insert_with(|| Value::String(subject.clone()));
        entry
            .entry("principal_id")
            .or_insert(Value::String(subject));
    }
    if entry.get("expires_at").is_none()
        && let Some(expires_at) = token_refresh::jwt_exp(access_token).and_then(format_rfc3339)
    {
        entry.insert("expires_at".into(), Value::String(expires_at));
    }

    let email_prefix = entry
        .get("email")
        .and_then(Value::as_str)
        .and_then(|email| email.split('@').next())
        .unwrap_or_default()
        .to_string();
    entry
        .entry("create_time")
        .or_insert_with(|| Value::String(now_rfc3339()));
    entry
        .entry("first_name")
        .or_insert_with(|| Value::String(email_prefix));
    for field in ["last_name", "profile_image_asset_id", "team_id"] {
        entry
            .entry(field)
            .or_insert_with(|| Value::String(String::new()));
    }
    entry
        .entry("principal_type")
        .or_insert_with(|| Value::String("user".into()));
    entry
        .entry("coding_data_retention_opt_out")
        .or_insert(Value::Bool(false));
}

/// A synthesised entry must be complete enough that the CLI can restore a
/// session from it — a half-formed one looks logged in and behaves logged out.
fn require_restorable(entry: &Map<String, Value>) -> Result<(), MaterializeError> {
    for field in [
        "create_time",
        "user_id",
        "email",
        "first_name",
        "last_name",
        "profile_image_asset_id",
        "principal_type",
        "principal_id",
        "team_id",
        "expires_at",
    ] {
        if entry.get(field).and_then(Value::as_str).is_none() {
            return Err(MaterializeError::IncompleteEntry { tool: TOOL, field });
        }
    }
    if !entry
        .get("refresh_token")
        .and_then(Value::as_str)
        .is_some_and(|token| !token.trim().is_empty())
    {
        return Err(MaterializeError::MissingRefreshToken { tool: TOOL });
    }
    Ok(())
}

/// Opaque (non-JWT) tokens cannot be inspected; that is not evidence of a
/// missing scope, so they pass. A readable token missing a CLI scope does not.
fn check_cli_scopes(token: &str) -> Result<(), MaterializeError> {
    let Some(claims) = token_refresh::decode_jwt_payload(token) else {
        return Ok(());
    };
    let mut scopes = HashSet::new();
    for key in ["scope", "scp"] {
        match claims.get(key) {
            Some(Value::String(raw)) => scopes.extend(raw.split_whitespace().map(str::to_string)),
            Some(Value::Array(values)) => {
                scopes.extend(values.iter().filter_map(Value::as_str).map(str::to_string));
            }
            _ => {}
        }
    }
    let missing: Vec<String> = REQUIRED_CLI_SCOPES
        .iter()
        .filter(|required| !scopes.contains(**required))
        .map(|required| (*required).to_string())
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(MaterializeError::MissingCliScopes {
            tool: TOOL,
            missing,
        })
    }
}

fn format_rfc3339(seconds: i64) -> Option<String> {
    DateTime::<Utc>::from_timestamp(seconds, 0)
        .map(|date| date.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string())
}

fn now_rfc3339() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()
}
