//! Gemini CLI：`~/.gemini` 下的 OAuth 凭据文件。
//!
//! Gemini CLI 把会话放在两处，缺一处它就会认为"缓存凭据不属于当前账号"：
//!
//! 1. `~/.gemini/oauth_creds.json`：真正的令牌（`expiry_date` 是毫秒）
//! 2. `~/.gemini/google_accounts.json`：`{active, old[]}`，当前 Google 账号
//!
//! 所以一次切号要两处都写、都回读校验，任何一处对不上就把整组现场（[GeminiCliSnapshot]）
//! 放回去。**绝不写系统钥匙串**：Gemini CLI 在三个平台上都只读这两个文件。
//!
//! 注意 `~/.gemini` 是**共用前缀、不同产品**：Antigravity 的文件也在这里
//! （见 [super::antigravity]），两类文件互不覆盖。

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use super::paths::{self, PathRoots};
use super::{
    ClientSnapshot, LocalCredential, SwitchOutcome, SwitchRequest, read_json_file, write_restricted,
};
use crate::error::{AccountError, AccountResult};
use crate::oauth::{Identity, TokenSet};

/// `google_accounts.json` 里最多留几个历史账号（Gemini CLI 自己也只留一小组）。
const MAX_OLD_ACCOUNTS: usize = 10;

/// 主凭据文件：Gemini CLI 启动时读它。
pub fn credential_path(roots: &PathRoots) -> PathBuf {
    paths::gemini_cli_oauth_creds_path(roots)
}

/// 账号归属文件：Gemini CLI 用它判断缓存凭据属于哪个 Google 账号。
pub fn accounts_path(roots: &PathRoots) -> PathBuf {
    paths::gemini_cli_accounts_path(roots)
}

/// 写 Gemini CLI 的凭据（两个文件都写完才算提交）。
pub fn write(roots: &PathRoots, request: &SwitchRequest<'_>) -> AccountResult<SwitchOutcome> {
    let creds_path = credential_path(roots);
    let accounts_path = accounts_path(roots);
    let access_token = request.tokens.access_token.trim();
    let refresh_token = request
        .tokens
        .refresh_token
        .as_deref()
        .map(str::trim)
        .filter(|token| !token.is_empty());
    if access_token.is_empty() {
        return Err(AccountError::Client(
            "this account has no access token to write into the Gemini CLI".to_string(),
        ));
    }
    // Gemini CLI 没有 refresh token 就会在第一次刷新时把用户踢下线；写一个空串进
    // 去等于静默降级成一个小时的有效期。
    let Some(refresh_token) = refresh_token else {
        return Err(AccountError::Client(
            "this account has no Gemini CLI refresh token; a switch needs both tokens"
                .to_string(),
        ));
    };
    let email = account_email(request);
    let snapshot = capture(roots)?;
    match write_all(
        &creds_path,
        &accounts_path,
        request,
        access_token,
        refresh_token,
        email.as_deref(),
    ) {
        Ok(()) => Ok(SwitchOutcome {
            agent: "gemini-cli".to_string(),
            live: email,
            target: creds_path.display().to_string(),
        }),
        Err(error) => {
            if let Err(rollback) = super::restore(&snapshot) {
                tracing::error!(
                    target: "accounts",
                    cause = %error,
                    error = %rollback,
                    "putting Gemini CLI's previous credential back failed"
                );
            }
            Err(error)
        }
    }
}

fn write_all(
    creds_path: &Path,
    accounts_path: &Path,
    request: &SwitchRequest<'_>,
    access_token: &str,
    refresh_token: &str,
    email: Option<&str>,
) -> AccountResult<()> {
    write_creds(creds_path, request, access_token, refresh_token)?;
    if let Some(email) = email {
        write_accounts(accounts_path, email)?;
    }
    // 回读：写不进去（或写进去的是别的账号）必须让切换失败，而不是报告成功。
    verify(creds_path, accounts_path, access_token, refresh_token, email)
}

/// `oauth_creds.json`：读写都按 Gemini CLI 的字段拼，未知字段原样保留。
fn write_creds(
    path: &Path,
    request: &SwitchRequest<'_>,
    access_token: &str,
    refresh_token: &str,
) -> AccountResult<()> {
    let mut root = match read_json_file(path)? {
        Some(Value::Object(existing)) => existing,
        _ => Map::new(),
    };
    root.insert(
        "access_token".to_string(),
        Value::String(access_token.to_string()),
    );
    root.insert(
        "refresh_token".to_string(),
        Value::String(refresh_token.to_string()),
    );
    root.insert(
        "id_token".to_string(),
        Value::String(
            request
                .tokens
                .id_token
                .as_deref()
                .map(str::trim)
                .unwrap_or_default()
                .to_string(),
        ),
    );
    root.insert(
        "token_type".to_string(),
        Value::String(token_type(request.tokens)),
    );
    // Gemini CLI 的过期时间是**毫秒**；不知道就说不知道，别写一个假的时间点。
    match expires_at(request.tokens) {
        Some(expires_at) => {
            root.insert(
                "expiry_date".to_string(),
                Value::Number((expires_at * 1000).into()),
            );
        }
        None => {
            root.remove("expiry_date");
        }
    }
    let body = serde_json::to_vec_pretty(&Value::Object(root))?;
    write_restricted(path, &body)
}

/// `google_accounts.json`：`{active, old[]}`，写进去的账号成为 active，原来的
/// active 进历史。历史去重、限长，和 Gemini CLI 自己的维护方式一致。
fn write_accounts(path: &Path, email: &str) -> AccountResult<()> {
    let mut root = match read_json_file(path)? {
        Some(Value::Object(existing)) => existing,
        _ => Map::new(),
    };
    let previous_active = root
        .get("active")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let mut old = previous_active.into_iter().collect::<Vec<_>>();
    if let Some(existing) = root.get("old").and_then(Value::as_array) {
        for entry in existing {
            if let Some(text) = entry.as_str().map(str::trim).filter(|t| !t.is_empty()) {
                old.push(text.to_string());
            }
        }
    }
    let mut kept: Vec<String> = Vec::new();
    for entry in old {
        if entry.eq_ignore_ascii_case(email) || kept.iter().any(|seen| seen.eq_ignore_ascii_case(&entry)) {
            continue;
        }
        kept.push(entry);
        if kept.len() == MAX_OLD_ACCOUNTS {
            break;
        }
    }
    root.insert("active".to_string(), Value::String(email.to_string()));
    root.insert(
        "old".to_string(),
        Value::Array(kept.into_iter().map(Value::String).collect()),
    );
    let body = serde_json::to_vec_pretty(&Value::Object(root))?;
    write_restricted(path, &body)
}

/// 写后回读校验：同一个 access / refresh 才算切换成功；账号归属也要对得上。
fn verify(
    creds_path: &Path,
    accounts_path: &Path,
    access_token: &str,
    refresh_token: &str,
    email: Option<&str>,
) -> AccountResult<()> {
    let Some(stored) = read_creds(creds_path)? else {
        return Err(AccountError::Client(format!(
            "{} did not keep the Gemini CLI credential: no readable token file",
            creds_path.display()
        )));
    };
    if stored.access_token != access_token || stored.refresh_token.as_deref() != Some(refresh_token) {
        return Err(AccountError::Client(format!(
            "{} did not keep the Gemini CLI credential: it reads back a different token",
            creds_path.display()
        )));
    }
    if let Some(email) = email {
        let active = active_account(accounts_path)?;
        if !crate::identity::same_identity(active.as_deref(), Some(email)) {
            return Err(AccountError::Client(format!(
                "{} did not keep the Gemini CLI account: it reads back {:?}",
                accounts_path.display(),
                active
            )));
        }
    }
    Ok(())
}

/// 读 Gemini CLI 凭据（"从本机导入"和 reconcile 都走这里）。
pub fn read(roots: &PathRoots) -> AccountResult<Option<LocalCredential>> {
    let path = credential_path(roots);
    let Some(tokens) = read_creds(&path)? else {
        return Ok(None);
    };
    let accounts = active_account(&accounts_path(roots))?;
    let email = tokens
        .id_token
        .as_deref()
        .and_then(crate::identity::jwt_email)
        .or_else(|| crate::identity::jwt_email(&tokens.access_token))
        .or(accounts);
    Ok(Some(LocalCredential {
        tokens,
        identity: Identity {
            display_name: email.clone(),
            email,
            plan: None,
            account_id: None,
        },
        provider_state: None,
        source: path.display().to_string(),
    }))
}

/// reconcile 的判据：Gemini CLI 只暴露 email。
pub(crate) fn live_identity(identity: &Identity) -> Option<String> {
    identity.email.clone()
}

/// 捕获两个文件的现场，任何一步失败都按这份快照放回去。
pub fn snapshot(roots: &PathRoots) -> AccountResult<ClientSnapshot> {
    capture(roots)
}

fn capture(roots: &PathRoots) -> AccountResult<ClientSnapshot> {
    Ok(ClientSnapshot::Files {
        entries: vec![
            (credential_path(roots), read_bytes(&credential_path(roots))?),
            (accounts_path(roots), read_bytes(&accounts_path(roots))?),
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

/// `oauth_creds.json` 到 TokenSet；没有 access token 的文件不是可用凭据。
fn read_creds(path: &Path) -> AccountResult<Option<TokenSet>> {
    let Some(root) = read_json_file(path)? else {
        return Ok(None);
    };
    let field = |name: &str| {
        root.get(name)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let Some(access_token) = field("access_token") else {
        return Ok(None);
    };
    let refresh_token = field("refresh_token");
    let id_token = field("id_token");
    // 过期时间优先用文件里的毫秒值（Gemini CLI 写的），其次 access token 的 exp。
    let expires_at = root
        .get("expiry_date")
        .and_then(Value::as_i64)
        .map(|millis| millis / 1000)
        .or_else(|| crate::identity::jwt_exp(&access_token));
    Ok(Some(TokenSet {
        access_token,
        refresh_token,
        id_token,
        expires_at,
        token_type: field("token_type").unwrap_or_else(|| "Bearer".to_string()),
    }))
}

/// `google_accounts.json` 的 active 账号。
fn active_account(path: &Path) -> AccountResult<Option<String>> {
    let Some(root) = read_json_file(path)? else {
        return Ok(None);
    };
    Ok(root
        .get("active")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string))
}

/// 这个账号的 email：记录里的 email，其次显示名里那个像 email 的值。
fn account_email(request: &SwitchRequest<'_>) -> Option<String> {
    request
        .email
        .map(str::trim)
        .filter(|email| email.contains('@'))
        .map(str::to_string)
        .or_else(|| {
            request
                .display_name
                .contains('@')
                .then(|| request.display_name.trim().to_string())
        })
        .or_else(|| {
            request
                .tokens
                .id_token
                .as_deref()
                .and_then(crate::identity::jwt_email)
        })
}

fn expires_at(tokens: &TokenSet) -> Option<i64> {
    tokens
        .expires_at
        .or_else(|| crate::identity::jwt_exp(&tokens.access_token))
}

fn token_type(tokens: &TokenSet) -> String {
    let trimmed = tokens.token_type.trim();
    if trimmed.is_empty() {
        "Bearer".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
#[path = "gemini_cli_tests.rs"]
mod gemini_cli_tests;
