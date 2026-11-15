//! Antigravity：`state.vscdb` 里的 unified OAuth 投影，加上新版客户端的系统凭据。
//!
//! <userData>/User/globalStorage/state.vscdb，表 ItemTable，键
//! `antigravityUnifiedStateSync.oauthToken`，值是 `base64(protobuf)`（结构见
//! [super::antigravity_protobuf]）。
//!
//! 一次切号要落四处，缺一处客户端就读到半个会话：
//!
//! 1. macOS Keychain 的 `gemini/antigravity`（新版权威来源）
//! 2. `~/.gemini/jetski-standalone-oauth-token`（Keyring 不可用时的回退）
//! 3. `~/.gemini/antigravity-cli/antigravity-oauth-token`（装了 agy CLI 时）
//! 4. `state.vscdb` 里的那条投影
//!
//! 1-3 只在 macOS 且非沙箱时写：Windows/Linux 上 Cockpit Tools 用的
//! WinCred/secret-tool 通路没有搬过来，写了也没人读。任何一步失败都按
//! [AntigravitySnapshot] 把整组现场放回去，绝不留一个半切换的会话。

use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use rusqlite::Connection;

use super::antigravity_credential::{self, CommandRunner, GeminiPaths, SystemCredential, SystemRunner};
use super::antigravity_protobuf::{self as protobuf, Projection, StoredSession};
use super::paths::{self, DesktopOs, PathRoots};
use super::vscdb;
use super::{ClientSnapshot, LocalCredential, SwitchOutcome, SwitchRequest};
use crate::error::{AccountError, AccountResult};
use crate::oauth::{Identity, TokenSet};

/// ItemTable 里那条 unified state 的键。
pub(crate) const OAUTH_TOKEN_KEY: &str = "antigravityUnifiedStateSync.oauthToken";

/// 一次切号要写的现场快照：vscdb 那一行、Keychain 载荷、两个回退文件。
#[derive(Debug, Clone)]
pub struct AntigravitySnapshot {
    vscdb: PathBuf,
    /// 旧 ItemTable 值（base64）；None 表示这条键原来不存在。
    oauth_row: Option<String>,
    /// 旧系统凭据载荷 JSON；None 表示原来没有这条（或这次不写系统凭据）。
    keychain: Option<String>,
    /// 每个回退文件的旧字节；None 表示文件原来不存在。
    gemini: Vec<(PathBuf, Option<Vec<u8>>)>,
    /// 这份现场是否包含系统凭据（macOS 非沙箱）。
    system: bool,
}

/// The database Antigravity keeps its session in.
pub fn credential_path(roots: &PathRoots) -> PathBuf {
    paths::antigravity_state_db_path(roots)
}

/// Write the Antigravity credential into every store the client reads.
pub fn write(roots: &PathRoots, request: &SwitchRequest<'_>) -> AccountResult<SwitchOutcome> {
    write_with_runner(roots, request, &SystemRunner)
}

/// Write with an injected command runner, so a test never touches the Keychain.
pub(crate) fn write_with_runner(
    roots: &PathRoots,
    request: &SwitchRequest<'_>,
    runner: &dyn CommandRunner,
) -> AccountResult<SwitchOutcome> {
    let path = credential_path(roots);
    let access_token = request.tokens.access_token.trim().to_string();
    let refresh_token = request
        .tokens
        .refresh_token
        .as_deref()
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::to_string);
    let Some(refresh_token) = refresh_token else {
        return Err(AccountError::Client(
            "this account has no Antigravity refresh token; a switch needs both tokens".to_string(),
        ));
    };
    if access_token.is_empty() {
        return Err(AccountError::Client(
            "this account has no access token to write into Antigravity".to_string(),
        ));
    }
    if !path.is_file() {
        return Err(missing_database(&path));
    }
    let email = request
        .email
        .map(str::trim)
        .filter(|email| email.contains('@'))
        .map(str::to_string)
        .or_else(|| {
            request
                .display_name
                .contains('@')
                .then(|| request.display_name.trim().to_string())
        });
    let expires_at = request
        .tokens
        .expires_at
        .or_else(|| crate::identity::jwt_exp(&access_token))
        .unwrap_or_default();

    let session = Session {
        access_token: &access_token,
        refresh_token: &refresh_token,
        expires_at,
        email: email.as_deref(),
        id_token: request.tokens.id_token.as_deref(),
    };
    let snapshot = capture(roots, runner)?;
    let outcome = write_all(roots, runner, &path, &session);
    match outcome {
        Ok(()) => Ok(SwitchOutcome {
            agent: "antigravity".to_string(),
            live: email,
            target: path.display().to_string(),
        }),
        Err(error) => {
            if let Err(rollback) = restore_with_runner(&snapshot, runner) {
                tracing::error!(
                    target: "accounts",
                    cause = %error,
                    error = %rollback,
                    "putting Antigravity's previous session back failed"
                );
            }
            Err(error)
        }
    }
}

/// 要写进客户端的一份会话。
struct Session<'a> {
    access_token: &'a str,
    refresh_token: &'a str,
    expires_at: i64,
    email: Option<&'a str>,
    id_token: Option<&'a str>,
}

fn write_all(
    roots: &PathRoots,
    runner: &dyn CommandRunner,
    path: &Path,
    session: &Session<'_>,
) -> AccountResult<()> {
    // 系统凭据先写，vscdb 最后写：vscdb 那一行是"切换已提交"的落点，它写完还
    // 要能解回同一个 refresh_token 才算成功。
    if uses_system_credential(roots) {
        let payload = antigravity_credential::payload_json(
            session.access_token,
            session.refresh_token,
            session.expires_at,
            session.id_token,
        );
        antigravity_credential::write_keychain(runner, &payload)?;
        antigravity_credential::write_gemini_files(&gemini_paths(roots), &payload)?;
    }
    write_oauth_row(path, session)
}

/// 把会话投影进 vscdb，并回读校验。
fn write_oauth_row(path: &Path, session: &Session<'_>) -> AccountResult<()> {
    let existing = read_row_value(path)?;
    let existing_blob = existing
        .as_deref()
        .map(|value| decode_row(path, value))
        .transpose()?;
    let blob = protobuf::project(
        existing_blob.as_deref(),
        &Projection {
            access_token: session.access_token,
            refresh_token: session.refresh_token,
            expires_at: session.expires_at,
            email: session.email,
        },
    );
    let encoded = STANDARD.encode(&blob);

    vscdb::write_rows(
        path,
        vscdb::DEFAULT_BUSY_TIMEOUT,
        true,
        "Antigravity",
        missing_database,
        &[(OAUTH_TOKEN_KEY, Some(encoded.as_str()))],
    )?;

    // 回读：写不进去（或写进去的是别的账号）必须让切换失败，而不是报告成功。
    match read_session(path)? {
        Some(stored) if stored.refresh_token == session.refresh_token => Ok(()),
        Some(_) => Err(AccountError::Client(format!(
            "{} did not keep the Antigravity session: it reads back a different refresh token",
            path.display()
        ))),
        None => Err(AccountError::Client(format!(
            "{} did not keep the Antigravity session: no readable oauthToken",
            path.display()
        ))),
    }
}

/// Read the Antigravity credential the client is serving.
pub fn read(roots: &PathRoots) -> AccountResult<Option<LocalCredential>> {
    read_with_runner(roots, &SystemRunner)
}

/// Read with an injected command runner.
pub(crate) fn read_with_runner(
    roots: &PathRoots,
    runner: &dyn CommandRunner,
) -> AccountResult<Option<LocalCredential>> {
    // 新版客户端把系统凭据当权威来源；沙箱里完全不碰钥匙串。
    if uses_system_credential(roots)
        && let Some(payload) = read_keychain_payload(runner)?
    {
        let credential = antigravity_credential::parse_payload(&payload)?;
        return Ok(Some(local_credential(
            credential,
            format!(
                "{}:{} (macOS Keychain)",
                antigravity_credential::KEYCHAIN_SERVICE,
                antigravity_credential::KEYCHAIN_ACCOUNT
            ),
            None,
        )));
    }
    let path = credential_path(roots);
    if !path.is_file() {
        return Ok(None);
    }
    let Some(session) = read_session(&path)? else {
        return Ok(None);
    };
    Ok(Some(local_credential(
        SystemCredential {
            access_token: Some(session.access_token),
            refresh_token: session.refresh_token,
            expires_at: session.expires_at,
        },
        path.display().to_string(),
        session.email,
    )))
}

/// The identity a live Antigravity is serving: the email its session carries.
pub(crate) fn live_identity(identity: &Identity) -> Option<String> {
    identity.email.clone()
}

/// Capture every store this switch touches, so any failure can put it back.
pub fn snapshot(roots: &PathRoots) -> AccountResult<ClientSnapshot> {
    snapshot_with_runner(roots, &SystemRunner)
}

/// Capture with an injected command runner.
pub(crate) fn snapshot_with_runner(
    roots: &PathRoots,
    runner: &dyn CommandRunner,
) -> AccountResult<ClientSnapshot> {
    Ok(ClientSnapshot::Antigravity(Box::new(capture(roots, runner)?)))
}

/// 把现场整组放回去（生产路径：Keychain 走真实命令）。
pub(crate) fn restore(snapshot: &AntigravitySnapshot) -> AccountResult<()> {
    restore_with_runner(snapshot, &SystemRunner)
}

/// 把现场整组放回去。回滚顺序与写入相反：先文件、再系统凭据、最后 vscdb 那行。
pub(crate) fn restore_with_runner(
    snapshot: &AntigravitySnapshot,
    runner: &dyn CommandRunner,
) -> AccountResult<()> {
    if snapshot.system {
        for (path, previous) in &snapshot.gemini {
            super::restore_file(path, previous)?;
        }
        match &snapshot.keychain {
            Some(payload) => antigravity_credential::write_keychain(runner, payload)?,
            None => antigravity_credential::delete_keychain(runner)?,
        }
    }
    match &snapshot.oauth_row {
        Some(value) => write_raw_row(&snapshot.vscdb, value),
        None => drop_row(&snapshot.vscdb),
    }
}

fn capture(roots: &PathRoots, runner: &dyn CommandRunner) -> AccountResult<AntigravitySnapshot> {
    let vscdb = credential_path(roots);
    let oauth_row = if vscdb.is_file() {
        read_row_value(&vscdb)?
    } else {
        None
    };
    let system = uses_system_credential(roots);
    let mut gemini = Vec::new();
    let mut keychain = None;
    if system {
        keychain = read_keychain_payload(runner)?;
        let paths = gemini_paths(roots);
        for path in std::iter::once(paths.fallback).chain(paths.cli_session) {
            gemini.push((path.clone(), read_bytes(&path)?));
        }
    }
    Ok(AntigravitySnapshot {
        vscdb,
        oauth_row,
        keychain,
        gemini,
        system,
    })
}

/// 系统凭据只在 macOS 且非沙箱时读写：那里才有 Cockpit Tools 对应的 Keychain
/// 通路，也只有那里不会把测试的写入落到开发者真实的钥匙串上。
fn uses_system_credential(roots: &PathRoots) -> bool {
    roots.os == DesktopOs::Macos && roots.sandbox_home.is_none()
}

fn gemini_paths(roots: &PathRoots) -> GeminiPaths {
    antigravity_credential::gemini_paths(&paths::antigravity_gemini_dir(roots))
}

/// Keychain 里的载荷 JSON（已经剥掉 go-keyring 包装并解码）。
fn read_keychain_payload(runner: &dyn CommandRunner) -> AccountResult<Option<String>> {
    let Some(raw) = antigravity_credential::read_keychain(runner)? else {
        return Ok(None);
    };
    antigravity_credential::parse_keychain_value(&raw).map(Some)
}

fn read_bytes(path: &Path) -> AccountResult<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(AccountError::Io(error)),
    }
}

fn local_credential(
    credential: SystemCredential,
    source: String,
    email: Option<String>,
) -> LocalCredential {
    LocalCredential {
        tokens: TokenSet {
            access_token: credential.access_token.unwrap_or_default(),
            refresh_token: Some(credential.refresh_token),
            id_token: None,
            expires_at: credential.expires_at,
            token_type: "Bearer".to_string(),
        },
        identity: Identity {
            display_name: email.clone(),
            email,
            plan: None,
            account_id: None,
        },
        provider_state: None,
        source,
    }
}

// ---------------------------------------------------------------------------
// state.vscdb
// ---------------------------------------------------------------------------

/// Open the editor's database; the client owns the file, we only borrow it.
fn open(path: &Path) -> AccountResult<Connection> {
    vscdb::open_existing(path, vscdb::DEFAULT_BUSY_TIMEOUT, true, missing_database)
}

fn missing_database(path: &Path) -> AccountError {
    AccountError::Client(format!(
        "Antigravity's state.vscdb is not at {}: start Antigravity once, sign in, then switch",
        path.display()
    ))
}

fn read_row_value(path: &Path) -> AccountResult<Option<String>> {
    let connection = open(path)?;
    vscdb::read_value(&connection, OAUTH_TOKEN_KEY, "Antigravity")
}

fn read_session(path: &Path) -> AccountResult<Option<StoredSession>> {
    let Some(value) = read_row_value(path)? else {
        return Ok(None);
    };
    let blob = decode_row(path, &value)?;
    Ok(protobuf::read(&blob))
}

fn decode_row(path: &Path, value: &str) -> AccountResult<Vec<u8>> {
    STANDARD.decode(value.trim()).map_err(|error| {
        AccountError::Client(format!(
            "{}: the Antigravity oauthToken is not base64: {error}",
            path.display()
        ))
    })
}

fn write_raw_row(path: &Path, value: &str) -> AccountResult<()> {
    if !path.is_file() {
        tracing::warn!(
            target: "accounts",
            path = %path.display(),
            "Antigravity's state.vscdb is gone; nothing to roll the session back into"
        );
        return Ok(());
    }
    vscdb::write_rows(
        path,
        vscdb::DEFAULT_BUSY_TIMEOUT,
        true,
        "Antigravity",
        missing_database,
        &[(OAUTH_TOKEN_KEY, Some(value))],
    )
}

fn drop_row(path: &Path) -> AccountResult<()> {
    if !path.is_file() {
        return Ok(());
    }
    vscdb::write_rows(
        path,
        vscdb::DEFAULT_BUSY_TIMEOUT,
        true,
        "Antigravity",
        missing_database,
        &[(OAUTH_TOKEN_KEY, None)],
    )
}

#[cfg(test)]
#[path = "antigravity_tests.rs"]
mod antigravity_tests;
