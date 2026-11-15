//! Kiro：AWS SSO 缓存里的那一份登录。
//!
//! 一次切号写三处（缺一处客户端就读到半个会话）：
//!
//! 1. ~/.aws/sso/cache/kiro-auth-token.json（唯一全局登录文件）
//! 2. ~/.aws/sso/cache/<sha1(startUrl)>.json（IdC 客户端注册，secret 只在这里）
//! 3. <Kiro app data>/profile.json（窗口显示的身份）
//!
//! 还有一处**缓存**：state.vscdb 里的 kiro.kiroAgent 行。它只是用量面板的初始
//! 快照，客户端随后自己刷新，所以写在提交之后、失败只记日志——它不是凭据，
//! 不该让一次成功的切换因为面板缓存而回滚。
//!
//! 不写系统钥匙串：Kiro 三个平台都只读上面这些文件。

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::kiro_store::{self as store, Material};
use super::paths::{self, PathRoots};
use super::vscdb;
use super::{
    ClientSnapshot, LocalCredential, SwitchOutcome, SwitchRequest, read_json_file, write_restricted,
};
use crate::error::{AccountError, AccountResult};
use crate::oauth::agents::kiro::KiroState;
use crate::oauth::{Identity, TokenSet};

/// state.vscdb 里那条用量缓存。
const USAGE_DB_KEY: &str = "kiro.kiroAgent";

/// The login file Kiro reads at startup.
pub fn credential_path(roots: &PathRoots) -> PathBuf {
    paths::kiro_auth_token_path(roots)
}

/// Write the Kiro login into every store the client reads.
pub fn write(roots: &PathRoots, request: &SwitchRequest<'_>) -> AccountResult<SwitchOutcome> {
    let token_path = credential_path(roots);
    let access_token = request.tokens.access_token.trim();
    let refresh_token = request
        .tokens
        .refresh_token
        .as_deref()
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .ok_or_else(|| {
            AccountError::Client(
                "this account has no Kiro refresh token; a switch needs both tokens".to_string(),
            )
        })?;
    if access_token.is_empty() {
        return Err(AccountError::Client(
            "this account has no access token to write into Kiro".to_string(),
        ));
    }
    let state = KiroState::parse(request.provider_state);
    let material = material(request, &state, access_token, refresh_token);

    let registration_path = match state
        .start_url
        .as_deref()
        .filter(|_| state.has_idc_client())
    {
        Some(start_url) => Some(registration_path(roots, start_url)?),
        None => None,
    };
    let profile_path = paths::kiro_profile_path(roots);

    let snapshot = snapshot_with(roots, registration_path.as_deref())?;
    match write_all(&token_path, registration_path.as_deref(), &profile_path, &material) {
        Ok(()) => {
            // 提交之后再补用量缓存：失败只记日志（它是缓存，不是凭据）。
            cache_usage(roots, &material);
            Ok(SwitchOutcome {
                agent: "kiro".to_string(),
                live: live_identity(&identity_of(&material)),
                target: token_path.display().to_string(),
            })
        }
        Err(error) => {
            if let Err(rollback) = super::restore(&snapshot) {
                tracing::error!(
                    target: "accounts",
                    cause = %error,
                    error = %rollback,
                    "putting Kiro's previous login back failed"
                );
            }
            Err(error)
        }
    }
}

/// 记录 + provider_state → 要写进客户端的一份会话。
fn material<'a>(
    request: &'a SwitchRequest<'_>,
    state: &'a KiroState,
    access_token: &'a str,
    refresh_token: &'a str,
) -> Material<'a> {
    let email = request
        .email
        .map(str::trim)
        .filter(|email| email.contains('@'))
        .or_else(|| {
            request
                .display_name
                .contains('@')
                .then(|| request.display_name.trim())
        });
    Material {
        access_token,
        refresh_token,
        expires_at: request
            .tokens
            .expires_at
            .or_else(|| crate::identity::jwt_exp(access_token)),
        email,
        user_id: request.account_id.map(str::trim),
        profile_arn: state.profile_arn.as_deref(),
        region: state.region.as_deref(),
        start_url: state.start_url.as_deref(),
        client_id: state.client_id.as_deref(),
        client_secret: state.client_secret.as_deref(),
        idc: state.has_idc_client(),
    }
}

fn identity_of(material: &Material<'_>) -> Identity {
    Identity {
        display_name: material.email.map(str::to_string),
        email: material.email.map(str::to_string),
        plan: None,
        account_id: material.user_id.map(str::to_string),
    }
}

fn write_all(
    token_path: &Path,
    registration_path: Option<&Path>,
    profile_path: &Path,
    material: &Material<'_>,
) -> AccountResult<()> {
    // 注册文件与 token 文件同目录；先写注册，再写 token（token 里有 clientIdHash
    // 指向注册），最后写 profile。任何一步失败都由快照整组放回。
    if let (Some(path), Some(client_id), Some(client_secret)) = (
        registration_path,
        material.client_id,
        material.client_secret,
    ) {
        let existing = read_json_file(path)?;
        let document = store::registration_document(existing.as_ref(), client_id, client_secret);
        write_json(path, &document)?;
    }
    let existing_token = read_json_file(token_path)?;
    let document = merge_token(existing_token.as_ref(), material);
    write_json(token_path, &document)?;
    if let Some(profile) = store::profile_document(material) {
        write_json(profile_path, &profile)?;
    }
    verify(token_path, material)
}

/// token 文件是"我们的键覆盖 + 客户端的未知字段保留"：Kiro 自己会往里加字段，
/// 抹掉它们等于把客户端状态清空。
fn merge_token(existing: Option<&Value>, material: &Material<'_>) -> Value {
    let mut document = store::token_document(material);
    if let (Some(existing), Some(target)) = (
        existing.and_then(Value::as_object),
        document.as_object_mut(),
    ) {
        for (key, value) in existing {
            target.entry(key.clone()).or_insert_with(|| value.clone());
        }
    }
    document
}

fn write_json(path: &Path, value: &Value) -> AccountResult<()> {
    let body = serde_json::to_vec_pretty(value)?;
    write_restricted(path, &body)
}

/// 写后回读：同一个 access / refresh 才算切换成功。
fn verify(token_path: &Path, material: &Material<'_>) -> AccountResult<()> {
    let Some(stored) = read_stored(token_path)? else {
        return Err(AccountError::Client(format!(
            "{} did not keep the Kiro login: no readable token file",
            token_path.display()
        )));
    };
    if stored.tokens.access_token != material.access_token
        || stored.tokens.refresh_token.as_deref() != Some(material.refresh_token)
    {
        return Err(AccountError::Client(format!(
            "{} did not keep the Kiro login: it reads back a different token",
            token_path.display()
        )));
    }
    Ok(())
}

/// Read the Kiro login in full (this is how "add from this machine" works).
pub fn read(roots: &PathRoots) -> AccountResult<Option<LocalCredential>> {
    let path = credential_path(roots);
    let Some(stored) = read_stored(&path)? else {
        return Ok(None);
    };
    let provider_state = kiro_provider_state(&stored);
    Ok(Some(LocalCredential {
        tokens: stored.tokens,
        identity: stored.identity,
        provider_state,
        source: path.display().to_string(),
    }))
}

/// The identity a live Kiro is serving: the provider account id, then the email.
pub(crate) fn live_identity(identity: &Identity) -> Option<String> {
    identity.account_id.clone().or_else(|| identity.email.clone())
}

/// Capture the login file, the registration and the profile so any failure can
/// put the client's previous session back.
pub fn snapshot(roots: &PathRoots) -> AccountResult<ClientSnapshot> {
    // 快照不知道这次会写哪一份注册文件，所以按默认 BuilderId 的哈希取一份；
    // 真正的写入路径会在 write 里按 provider_state 重新算，并在此之前重拍一次。
    snapshot_with(roots, None)
}

fn snapshot_with(roots: &PathRoots, registration: Option<&Path>) -> AccountResult<ClientSnapshot> {
    let mut entries = vec![
        (credential_path(roots), read_bytes(&credential_path(roots))?),
        (
            paths::kiro_profile_path(roots),
            read_bytes(&paths::kiro_profile_path(roots))?,
        ),
    ];
    if let Some(registration) = registration {
        entries.push((registration.to_path_buf(), read_bytes(registration)?));
    }
    Ok(ClientSnapshot::Files { entries })
}

fn read_bytes(path: &Path) -> AccountResult<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(AccountError::Io(error)),
    }
}

/// 注册文件路径：AWS 用 startUrl 的 SHA-1 命名，且必须是 40 位小写十六进制
/// （客户端只按这个名字找文件，写错名字等于没写）。
fn registration_path(roots: &PathRoots, start_url: &str) -> AccountResult<PathBuf> {
    let hash = store::client_id_hash(start_url);
    if !store::is_client_hash(&hash) {
        return Err(AccountError::Client(format!(
            "Kiro: {start_url} does not hash to a clientIdHash ({hash})"
        )));
    }
    Ok(paths::aws_sso_cache_dir(roots).join(format!("{hash}.json")))
}

/// 一份读出来的登录。
struct StoredLogin {
    tokens: TokenSet,
    identity: Identity,
    /// 从盘上能认出来的 IdC 注册信息（区域 / issuer / clientId），导入时存进
    /// provider_state，账号才不用重新登录就能刷新。
    state: KiroState,
}

/// 导入时该存进 provider_state 的 IdC 注册信息。clientSecret 不在这里：它只在
/// 注册文件里，导入时读不回（读它需要 clientIdHash 与客户端自己的约定）。
fn kiro_provider_state(login: &StoredLogin) -> Option<Value> {
    let mut state = login.state.clone();
    state.client_secret = None;
    state.to_value()
}

fn read_stored(path: &Path) -> AccountResult<Option<StoredLogin>> {
    let Some(root) = read_json_file(path)? else {
        return Ok(None);
    };
    let field = |keys: &[&str]| {
        keys.iter().find_map(|key| {
            root.get(*key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
    };
    let Some(access_token) = field(&["accessToken", "access_token"]) else {
        return Ok(None);
    };
    let refresh_token = field(&["refreshToken", "refresh_token"]);
    let email = field(&["email", "loginHint"]);
    let user_id = field(&["userId", "user_id", "sub", "profileArn"]);
    let expires_at = root
        .get("expiresAt")
        .or_else(|| root.get("expires_at"))
        .and_then(store::parse_expires)
        .or_else(|| crate::identity::jwt_exp(&access_token));
    let state = KiroState {
        client_id: field(&["clientId", "client_id"]),
        client_secret: None,
        region: field(&["region", "idcRegion", "idc_region"]),
        start_url: field(&["issuerUrl", "issuer_url", "startUrl", "start_url"]),
        profile_arn: field(&["profileArn", "profile_arn"]),
    };
    Ok(Some(StoredLogin {
        state,
        tokens: TokenSet {
            access_token,
            refresh_token,
            id_token: field(&["idToken", "id_token"]),
            expires_at,
            token_type: field(&["tokenType", "token_type"]).unwrap_or_else(|| "Bearer".to_string()),
        },
        identity: Identity {
            display_name: email.clone(),
            email,
            plan: None,
            account_id: user_id,
        },
    }))
}

/// 用量缓存：state.vscdb 存在时写一行，失败只记日志。
fn cache_usage(roots: &PathRoots, material: &Material<'_>) {
    let Some(document) = store::usage_document(material) else {
        return;
    };
    let path = paths::kiro_state_db_path(roots);
    if !path.is_file() {
        return;
    }
    if let Err(error) = write_usage_row(&path, &document) {
        tracing::warn!(
            target: "accounts",
            path = %path.display(),
            error = %error,
            "Kiro's usage cache could not be refreshed; the IDE will re-read it"
        );
    }
}

fn write_usage_row(path: &Path, document: &str) -> AccountResult<()> {
    vscdb::write_rows(
        path,
        vscdb::DEFAULT_BUSY_TIMEOUT,
        true,
        "Kiro",
        |missing| AccountError::Client(format!("opening {}: missing", missing.display())),
        &[(USAGE_DB_KEY, Some(document))],
    )
}

#[cfg(test)]
#[path = "kiro_tests.rs"]
mod kiro_tests;
