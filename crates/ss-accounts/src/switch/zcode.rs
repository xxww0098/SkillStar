//! ZCode：~/.zcode/v2 下的两个文件 + enc:v1 凭据。
//!
//! 一次切号写两处，缺一处客户端就读到半个会话：
//!
//! 1. v2/credentials.json：oauth:active_provider、oauth:{provider}:access_token、
//!    oauth:{provider}:refresh_token、oauth:{provider}:user_info、zcodejwttoken，
//!    值全是 enc:v1 密文。
//! 2. v2/setting.json：modelProviderFamilyModes[provider] = "oauth"。
//!
//! 密钥派生在 [zcode_store]：与官方客户端一致（env secret 优先，否则平台回退
//! 材料）。**不写系统钥匙串**：ZCode 只读这两个文件。

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use super::paths::{self, PathRoots};
use super::zcode_store::{self as store, EncV1Error};
use super::{
    ClientSnapshot, LocalCredential, SwitchOutcome, SwitchRequest, read_json_file, write_restricted,
};
use crate::error::{AccountError, AccountResult};
use crate::oauth::agents::zcode;
use crate::oauth::{Identity, TokenSet};

/// credentials.json 里的键。
const ACTIVE: &str = "oauth:active_provider";
const JWT: &str = "zcodejwttoken";
/// setting.json 里的键。
const FAMILY_MODES: &str = "modelProviderFamilyModes";
const MODE_OAUTH: &str = "oauth";

/// The credentials file ZCode reads at startup.
pub fn credential_path(roots: &PathRoots) -> PathBuf {
    paths::zcode_credentials_path(roots)
}

/// The settings file that decides which provider family is in use.
pub fn settings_path(roots: &PathRoots) -> PathBuf {
    paths::zcode_settings_path(roots)
}

fn access_key(provider: &str) -> String {
    format!("oauth:{provider}:access_token")
}

fn refresh_key(provider: &str) -> String {
    format!("oauth:{provider}:refresh_token")
}

fn user_key(provider: &str) -> String {
    format!("oauth:{provider}:user_info")
}

/// Write the ZCode OAuth session into both files.
pub fn write(roots: &PathRoots, request: &SwitchRequest<'_>) -> AccountResult<SwitchOutcome> {
    let credentials_path = credential_path(roots);
    let settings_path = settings_path(roots);
    let provider = zcode::provider_of(request.provider_state)
        .map_err(|error| AccountError::Client(error.to_string()))?;
    let access_token = request.tokens.access_token.trim();
    if access_token.is_empty() {
        return Err(AccountError::Client(
            "this account has no access token to write into ZCode".to_string(),
        ));
    }
    // zcodejwttoken 是 ZCode 用来查计费的那条令牌：没有它，账号能登录但看不到额度。
    let jwt = request
        .tokens
        .id_token
        .as_deref()
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .ok_or_else(|| {
            AccountError::Client(
                "this ZCode account has no JWT; a switch needs the token exchange's jwt".to_string(),
            )
        })?;
    let key = store::credential_key(&roots.home, &store::current_username());
    let snapshot = capture(roots)?;
    match write_all(
        &credentials_path,
        &settings_path,
        request,
        provider,
        access_token,
        jwt,
        &key,
    ) {
        Ok(()) => Ok(SwitchOutcome {
            agent: "zcode".to_string(),
            live: live_identity(&identity_of(request)),
            target: credentials_path.display().to_string(),
        }),
        Err(error) => {
            if let Err(rollback) = super::restore(&snapshot) {
                tracing::error!(
                    target: "accounts",
                    cause = %error,
                    error = %rollback,
                    "putting ZCode's previous credential back failed"
                );
            }
            Err(error)
        }
    }
}

fn write_all(
    credentials_path: &Path,
    settings_path: &Path,
    request: &SwitchRequest<'_>,
    provider: &str,
    access_token: &str,
    jwt: &str,
    key: &[u8; 32],
) -> AccountResult<()> {
    let mut credentials = match read_json_file(credentials_path)? {
        Some(Value::Object(existing)) => existing,
        _ => Map::new(),
    };
    credentials.insert(ACTIVE.to_string(), sealed(key, provider)?);
    credentials.insert(access_key(provider), sealed(key, access_token)?);
    match request
        .tokens
        .refresh_token
        .as_deref()
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        Some(refresh_token) => {
            credentials.insert(refresh_key(provider), sealed(key, refresh_token)?);
        }
        None => {
            credentials.remove(&refresh_key(provider));
        }
    }
    if let Some(user_info) = user_info_json(request) {
        credentials.insert(user_key(provider), sealed(key, &user_info)?);
    }
    credentials.insert(JWT.to_string(), sealed(key, jwt)?);
    write_json(credentials_path, &Value::Object(credentials))?;

    // setting.json：家族模式改成 oauth，客户端才会用 credentials.json 里的行。
    let mut settings = match read_json_file(settings_path)? {
        Some(Value::Object(existing)) => existing,
        _ => Map::new(),
    };
    let mode = settings
        .entry(FAMILY_MODES.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(mode) = mode.as_object_mut() else {
        return Err(AccountError::Client(
            "ZCode 的 setting.json 里 modelProviderFamilyModes 不是对象，切换未生效"
                .to_string(),
        ));
    };
    mode.insert(provider.to_string(), Value::String(MODE_OAUTH.to_string()));
    write_json(settings_path, &Value::Object(settings))?;

    verify(credentials_path, provider, access_token, key)
}

fn sealed(key: &[u8; 32], plaintext: &str) -> AccountResult<Value> {
    store::seal(key, plaintext)
        .map(Value::String)
        .map_err(|error| AccountError::Client(error.to_string()))
}

fn write_json(path: &Path, value: &Value) -> AccountResult<()> {
    let body = serde_json::to_vec_pretty(value)?;
    write_restricted(path, &body)
}

/// 写后回读：解出来的 access 和我们写的是同一条才算成功。
fn verify(
    credentials_path: &Path,
    provider: &str,
    access_token: &str,
    key: &[u8; 32],
) -> AccountResult<()> {
    let Some(stored) = read_stored(credentials_path, key)? else {
        return Err(AccountError::Client(format!(
            "{} did not keep the ZCode session: no readable credential",
            credentials_path.display()
        )));
    };
    if stored.provider != provider || stored.tokens.access_token != access_token {
        return Err(AccountError::Client(format!(
            "{} did not keep the ZCode session: it reads back a different account",
            credentials_path.display()
        )));
    }
    Ok(())
}

/// Read the ZCode OAuth session in full.
pub fn read(roots: &PathRoots) -> AccountResult<Option<LocalCredential>> {
    let path = credential_path(roots);
    let key = store::credential_key(&roots.home, &store::current_username());
    let Some(stored) = read_stored(&path, &key)? else {
        return Ok(None);
    };
    // 上游（zai / bigmodel）是账号的一部分：导入时记下来，额度与切号都读它。
    let provider_state = Some(zcode::provider_state(&stored.provider));
    Ok(Some(LocalCredential {
        tokens: stored.tokens,
        identity: stored.identity,
        provider_state,
        source: path.display().to_string(),
    }))
}

/// The identity a live ZCode is serving: the provider account id, then the email.
pub(crate) fn live_identity(identity: &Identity) -> Option<String> {
    identity.account_id.clone().or_else(|| identity.email.clone())
}

/// Capture both files so a failed switch can put them back.
pub fn snapshot(roots: &PathRoots) -> AccountResult<ClientSnapshot> {
    capture(roots)
}

fn capture(roots: &PathRoots) -> AccountResult<ClientSnapshot> {
    let credentials = credential_path(roots);
    let settings = settings_path(roots);
    Ok(ClientSnapshot::Files {
        entries: vec![
            (credentials.clone(), read_bytes(&credentials)?),
            (settings.clone(), read_bytes(&settings)?),
        ],
    })
}

fn read_bytes(path: &Path) -> AccountResult<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(AccountError::Io(error)),
    }
}

/// 一份解出来的会话。
struct StoredSession {
    provider: String,
    tokens: TokenSet,
    identity: Identity,
}

fn read_stored(path: &Path, key: &[u8; 32]) -> AccountResult<Option<StoredSession>> {
    let Some(root) = read_json_file(path)? else {
        return Ok(None);
    };
    let field = |name: &str| open(&root, name, key);
    // active_provider 决定读哪一组键；读不到说明这份文件不是 OAuth 会话。
    let Some(provider) = field(ACTIVE)? else {
        return Ok(None);
    };
    let Some(access_token) = field(&access_key(&provider))? else {
        return Ok(None);
    };
    let refresh_token = field(&refresh_key(&provider))?;
    let user_info = field(&user_key(&provider))?
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok());
    let jwt = field(JWT)?;
    let user_text = |key: &str| {
        user_info
            .as_ref()
            .and_then(|info| info.get(key))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let email = user_text("email").or_else(|| jwt.as_deref().and_then(crate::identity::jwt_email));
    let account_id = user_text("userId")
        .or_else(|| user_text("user_id"))
        .or_else(|| user_text("id"))
        .or_else(|| jwt.as_deref().and_then(|token| crate::identity::jwt_claim(token, "sub")));
    let expires_at = jwt
        .as_deref()
        .and_then(crate::identity::jwt_exp);
    Ok(Some(StoredSession {
        provider,
        tokens: TokenSet {
            access_token,
            refresh_token,
            id_token: jwt,
            expires_at,
            token_type: "Bearer".to_string(),
        },
        identity: Identity {
            display_name: email.clone(),
            email,
            plan: None,
            account_id,
        },
    }))
}

/// 读一个 enc:v1 字段：缺失给 None，解不开就是错误（不能当成空）。
fn open(root: &Value, name: &str, key: &[u8; 32]) -> AccountResult<Option<String>> {
    let Some(raw) = root.get(name).and_then(Value::as_str) else {
        return Ok(None);
    };
    store::open(key, raw).map(Some).map_err(|error| match error {
        EncV1Error::Format => AccountError::Client(format!(
            "ZCode 的 {name} 不是 enc:v1 密文，跳过这条凭据"
        )),
        other => AccountError::Client(other.to_string()),
    })
}

/// 记录侧的身份。
fn identity_of(request: &SwitchRequest<'_>) -> Identity {
    Identity {
        email: request
            .email
            .map(str::trim)
            .filter(|email| email.contains('@'))
            .map(str::to_string)
            .or_else(|| {
                request
                    .tokens
                    .id_token
                    .as_deref()
                    .and_then(crate::identity::jwt_email)
            }),
        display_name: None,
        plan: None,
        account_id: request.account_id.map(str::trim).map(str::to_string),
    }
}

/// oauth:{provider}:user_info 的内容：官方客户端读 email / userId。
fn user_info_json(request: &SwitchRequest<'_>) -> Option<String> {
    let identity = identity_of(request);
    let mut map = Map::new();
    if let Some(email) = identity.email {
        map.insert("email".to_string(), json!(email));
    }
    if let Some(account_id) = identity.account_id {
        map.insert("userId".to_string(), json!(account_id));
    }
    (!map.is_empty()).then(|| Value::Object(map).to_string())
}

#[cfg(test)]
#[path = "zcode_tests.rs"]
mod zcode_tests;
