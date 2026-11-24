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
    let text = if agent == "zcode" && text.starts_with("enc:v1:") {
        open_zcode_value(&zcode_key(), &text)?
    } else {
        text
    };
    if text.trim_start().starts_with('{') {
        let root: Value = serde_json::from_str(&text)?;
        if agent == "zcode" {
            parse_zcode_json(&root)
        } else {
            parse_json(agent, &root)
        }
    } else {
        let provider_state =
            (agent == "zcode").then(|| crate::oauth::agents::zcode::provider_state("zai"));
        build(
            agent,
            Fields {
                access_token: text,
                provider_state,
                ..Fields::default()
            },
        )
    }
}

/// 把 `go-keyring-base64:` 包装剥掉（Antigravity 的 macOS Keychain / Windows WinCred 值）。
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

fn zcode_key() -> [u8; 32] {
    let roots = crate::switch::paths::PathRoots::from_env();
    crate::switch::zcode_store::credential_key(
        &roots.home,
        &crate::switch::zcode_store::current_username(),
    )
}

fn open_zcode_value(key: &[u8; 32], raw: &str) -> AccountResult<String> {
    if !raw.starts_with("enc:v1:") {
        return Ok(raw.to_string());
    }
    crate::switch::zcode_store::open(key, raw).map_err(|error| AccountError::Client(error.to_string()))
}

/// The expiry spellings the clients on disk actually use: `expires_at`/`expiry`
/// from the OAuth clients, `expiry_date` (in milliseconds) from Google's own
/// credential files.
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
    plan: Option<String>,
    provider_state: Option<Value>,
}

fn parse_zcode_json(root: &Value) -> AccountResult<ImportedToken> {
    let session = nested_session("zcode", root);
    let key = std::cell::OnceCell::new();
    let get_key = || key.get_or_init(zcode_key);
    let decode = |raw: &str| -> AccountResult<Option<String>> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        let plain = if trimmed.starts_with("enc:v1:") {
            open_zcode_value(get_key(), trimmed)?
        } else {
            trimmed.to_string()
        };
        Ok(Some(plain).filter(|value| !value.trim().is_empty()))
    };
    let zcode_field = |keys: &[&str]| -> AccountResult<Option<String>> {
        for scope in [session, root] {
            for k in keys {
                if let Some(raw) = scope.get(*k).and_then(Value::as_str)
                    && let Some(val) = decode(raw)?
                {
                    return Ok(Some(val));
                }
            }
        }
        Ok(None)
    };
    let inferred_provider = zcode_field(&["oauth:active_provider", "provider", "active_provider", "oauthProvider"])?
        .or_else(|| {
            if session.get("oauth:bigmodel:access_token").is_some()
                || root.get("oauth:bigmodel:access_token").is_some()
            {
                Some("bigmodel".to_string())
            } else if session.get("oauth:zai:access_token").is_some()
                || root.get("oauth:zai:access_token").is_some()
            {
                Some("zai".to_string())
            } else {
                None
            }
        });
    let provider = crate::oauth::agents::zcode::normalize_provider(inferred_provider.as_deref())
        .map_err(|error| AccountError::Client(error.to_string()))?;

    let access_key = format!("oauth:{provider}:access_token");
    let refresh_key = format!("oauth:{provider}:refresh_token");
    let user_key = format!("oauth:{provider}:user_info");

    let id_token = zcode_field(&["zcodejwttoken", "zcodeJwtToken", "id_token", "idToken", "jwt"])?;
    let access_token = zcode_field(&[&access_key, "access_token", "accessToken", "token"])?
        .or_else(|| id_token.clone())
        .ok_or_else(|| {
            AccountError::Client("the pasted JSON carries no access token".to_string())
        })?;
    let refresh_token = zcode_field(&[&refresh_key, "refresh_token", "refreshToken"])?;

    let user_info = [session, root].into_iter().find_map(|scope| {
        for k in [user_key.as_str(), "user_info", "userInfo"] {
            match scope.get(k) {
                Some(Value::Object(_)) => return scope.get(k).cloned(),
                Some(Value::String(raw)) => {
                    if let Ok(Some(decoded)) = decode(raw)
                        && let Ok(parsed) = serde_json::from_str::<Value>(&decoded)
                    {
                        return Some(parsed);
                    }
                }
                _ => {}
            }
        }
        None
    });

    let token_preview = TokenSet {
        access_token: access_token.clone(),
        refresh_token: refresh_token.clone(),
        id_token: id_token.clone(),
        expires_at: None,
        token_type: "Bearer".to_string(),
    };
    let zcode_identity = crate::oauth::agents::zcode::identity_of(&token_preview, user_info.as_ref());
    let email = zcode_field(&["email", "cachedEmail"])?.or(zcode_identity.email);
    let display_name = zcode_field(&["display_name", "displayName", "name"])?
        .or(zcode_identity.display_name);
    let account_id = zcode_field(&["account_id", "accountId", "authId", "userId", "user_id"])?
        .or(zcode_identity.account_id);

    let expires_at = timestamp_field(session, &EXPIRY_KEYS)
        .or_else(|| timestamp_field(root, &EXPIRY_KEYS))
        .or_else(|| id_token.as_deref().and_then(identity::jwt_exp));

    build(
        "zcode",
        Fields {
            access_token,
            refresh_token,
            id_token,
            expires_at,
            account_id,
            email,
            display_name,
            plan: None,
            provider_state: Some(crate::oauth::agents::zcode::provider_state(provider)),
        },
    )
}

fn parse_json(agent: &str, root: &Value) -> AccountResult<ImportedToken> {
    let session = nested_session(agent, root);
    let access_keys: &[&str] = match agent {
        "devin-desktop" => &[
            "apiKey",
            "api_key",
            "auth1Token",
            "access_token",
            "accessToken",
            "token",
        ],
        "grok" => &["access_token", "accessToken", "token", "key"],
        _ => &["access_token", "accessToken", "token"],
    };
    let access_token = text_from(session, root, access_keys).ok_or_else(|| {
        AccountError::Client("the pasted JSON carries no access token".to_string())
    })?;
    let account_id = text_from(
        session,
        root,
        &[
            "account_id",
            "accountId",
            "authId",
            "user_id",
            "userId",
            "principal_id",
            "principalId",
            "sub",
        ],
    )
    .or_else(|| {
        (agent == "kiro")
            .then(|| text_from(session, root, &["profileArn", "profile_arn"]))
            .flatten()
    });
    let email = text_from(session, root, &["email", "cachedEmail", "loginHint"]);
    let display_name = text_from(session, root, &["display_name", "displayName", "name"])
        .or_else(|| {
            (agent == "grok")
                .then(|| grok_full_name(session).or_else(|| grok_full_name(root)))
                .flatten()
        });
    let plan = (agent == "claude-code")
        .then(|| text_from(session, root, &["subscriptionType", "rateLimitTier"]))
        .flatten();
    let provider_state = match agent {
        "kiro" => kiro_state_from(session, root),
        "devin-desktop" => crate::switch::devin_desktop::api_server_state(
            text_from(session, root, &["apiServerUrl", "api_server_url"]).as_deref(),
        ),
        "claude-code" => plan
            .as_ref()
            .map(|tier| serde_json::json!({ "plan": tier })),
        _ => None,
    };

    build(
        agent,
        Fields {
            access_token,
            refresh_token: text_from(session, root, &["refresh_token", "refreshToken"]),
            id_token: text_from(session, root, &["id_token", "idToken"]),
            expires_at: timestamp_field(session, &EXPIRY_KEYS)
                .or_else(|| timestamp_field(root, &EXPIRY_KEYS)),
            account_id,
            email,
            display_name,
            plan,
            provider_state,
        },
    )
}

fn kiro_state_from(session: &Value, root: &Value) -> Option<Value> {
    crate::oauth::agents::kiro::KiroState {
        client_id: text_from(session, root, &["clientId", "client_id"]),
        client_secret: text_from(session, root, &["clientSecret", "client_secret"]),
        region: text_from(session, root, &["region", "idcRegion", "idc_region"]),
        start_url: text_from(
            session,
            root,
            &["issuerUrl", "issuer_url", "startUrl", "start_url"],
        ),
        profile_arn: text_from(session, root, &["profileArn", "profile_arn"]),
    }
    .to_value()
}

fn grok_full_name(value: &Value) -> Option<String> {
    let first = text_field(value, &["first_name", "firstName"]);
    let last = text_field(value, &["last_name", "lastName"]);
    match (first, last) {
        (Some(f), Some(l)) => Some(format!("{f} {l}")),
        (Some(f), None) => Some(f),
        (None, Some(l)) => Some(l),
        (None, None) => None,
    }
}

fn text_from(session: &Value, root: &Value, keys: &[&str]) -> Option<String> {
    text_field(session, keys).or_else(|| text_field(root, keys))
}

/// The nested session object, when the payload carries one.
fn nested_session<'a>(agent: &str, root: &'a Value) -> &'a Value {
    if agent == "grok" {
        let scope = crate::oauth::agents::grok::scope_key();
        if let Some(entry) = root.get(&scope).filter(|value| value.is_object()) {
            return entry;
        }
        if let Some(map) = root.as_object()
            && let Some(entry) = map.iter().find_map(|(k, v)| {
                (k.starts_with("https://auth.x.ai::") && v.is_object()).then_some(v)
            })
        {
            return entry;
        }
    }
    if agent == "claude-code"
        && let Some(entry) = root.get("claudeAiOauth").filter(|value| value.is_object())
    {
        return entry;
    }
    ["tokens", "token"]
        .into_iter()
        .find_map(|key| root.get(key).filter(|value| value.is_object()))
        .unwrap_or(root)
}

fn build(agent: &str, fields: Fields) -> AccountResult<ImportedToken> {
    let access_token = fields.access_token.trim().to_string();
    if crate::switch::cursor::is_safe_storage_ciphertext(&access_token) {
        return Err(AccountError::Client(
            "粘贴的 Cursor 令牌是本地加密密文（cursor-safe-storage:v1:），请改用「本地导入」或粘贴 cursor.accessToken 明文令牌"
                .to_string(),
        ));
    }
    let refresh_token = clean(fields.refresh_token)
        .filter(|token| !crate::switch::cursor::is_safe_storage_ciphertext(token));
    let id_token = clean(fields.id_token);
    let account_id = clean(fields.account_id)
        .or_else(|| provider_account_id(agent, &access_token, id_token.as_deref()));
    let email = clean(fields.email)
        .or_else(|| identity::jwt_email(&access_token))
        .or_else(|| id_token.as_deref().and_then(identity::jwt_email));
    let expires_at = fields.expires_at.or_else(|| identity::jwt_exp(&access_token));
    if agent == "grok" && email.is_none() && account_id.is_none() && expires_at.is_none() {
        return Err(AccountError::Client(
            "Grok 裸不透明令牌缺少可还原的元数据（email / user_id / expires_at），请改用 OAuth 登录、本地导入，或粘贴包含上述字段的 ~/.grok/auth.json"
                .to_string(),
        ));
    }
    let display_name = clean(fields.display_name).or_else(|| email.clone());
    Ok(ImportedToken {
        tokens: TokenSet {
            expires_at,
            access_token,
            refresh_token,
            id_token,
            token_type: "Bearer".to_string(),
        },
        identity: Identity {
            email,
            display_name,
            plan: clean(fields.plan),
            account_id,
        },
        provider_state: fields.provider_state,
    })
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
