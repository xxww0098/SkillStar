//! Antigravity：`state.vscdb` 里的 unified OAuth 投影，加上新版客户端的系统凭据。
//!
//! <userData>/User/globalStorage/state.vscdb，表 ItemTable，键
//! `antigravityUnifiedStateSync.oauthToken`，值是 `base64(protobuf)`（结构见
//! [super::antigravity_protobuf]）。
//!
//! 一次切号要落四处，缺一处客户端就读到半个会话：
//!
//! 1. 系统凭据存储：macOS Keychain 的 `gemini/antigravity`（`go-keyring-base64:`），
//!    或 Windows Credential Manager 的 `gemini:antigravity`（UTF-8 JSON）
//! 2. `~/.gemini/jetski-standalone-oauth-token`（Keyring 不可用时的回退）
//! 3. `~/.gemini/antigravity-cli/antigravity-oauth-token`（装了 agy CLI 时）
//! 4. `state.vscdb` 里的那条投影（**旧版 IDE 才有**）
//!
//! 2.x 起客户端不再创建 `state.vscdb`，所以投影按"文件在不在"决定写不写：
//! 不在场既不是错误，也不由我们建库。1-3 在非沙箱的 macOS 与 Windows 上写：
//! Linux 仍要求和旧版一样有 `state.vscdb`。任何一步失败都按
//! [AntigravitySnapshot] 把整组现场放回去，绝不留一个半切换的会话。

use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use rusqlite::Connection;

use super::antigravity_credential::{self, CommandRunner, GeminiPaths, SystemCredential, SystemRunner};
use super::antigravity_protobuf::{self as protobuf, Projection, StoredSession};
use super::antigravity_wincred::{
    self, SystemWinCred, WINCRED_TARGET, WINCRED_USERNAME, WinCredStore,
};
use super::paths::{self, DesktopOs, PathRoots};
use super::vscdb;
use super::{ClientSnapshot, LocalCredential, SwitchOutcome, SwitchRequest};
use crate::error::{AccountError, AccountResult};
use crate::oauth::{Identity, TokenSet};

/// ItemTable 里那条 unified state 的键。
pub(crate) const OAUTH_TOKEN_KEY: &str = "antigravityUnifiedStateSync.oauthToken";

/// 注入的系统凭据读写端：macOS Keychain 命令执行器与 Windows WinCred。
#[derive(Clone, Copy)]
pub(crate) struct SystemStores<'a> {
    pub(crate) runner: &'a dyn CommandRunner,
    pub(crate) wincred: &'a dyn WinCredStore,
}

impl<'a> SystemStores<'a> {
    fn with_runner(runner: &'a dyn CommandRunner) -> Self {
        Self {
            runner,
            wincred: &SystemWinCred,
        }
    }
}

/// 一次切号要写的现场快照：vscdb 那一行、系统凭据载荷、两个回退文件。
#[derive(Debug, Clone)]
pub struct AntigravitySnapshot {
    vscdb: PathBuf,
    /// 旧 ItemTable 值（base64）；None 表示这条键原来不存在。
    oauth_row: Option<String>,
    /// 旧系统凭据载荷 JSON；None 表示原来没有这条（或这次不写系统凭据）。
    keychain: Option<String>,
    /// 每个回退文件的旧字节；None 表示文件原来不存在。
    gemini: Vec<(PathBuf, Option<Vec<u8>>)>,
    /// 这份现场是否包含系统凭据（macOS / Windows 非沙箱）。
    system: bool,
    /// 快照所属桌面平台，回滚时据此选择 Keychain 或 WinCred。
    os: DesktopOs,
}

/// The database the legacy IDE keeps its session in (2.x keeps none).
pub fn credential_path(roots: &PathRoots) -> PathBuf {
    paths::antigravity_state_db_path(roots)
}

/// 系统凭据的落点，用来在消息里说清这次切号写到了哪。
fn keychain_source(os: DesktopOs) -> String {
    match os {
        DesktopOs::Windows => format!("{WINCRED_TARGET} (Windows WinCred)"),
        _ => format!(
            "{}:{} (macOS Keychain)",
            antigravity_credential::KEYCHAIN_SERVICE,
            antigravity_credential::KEYCHAIN_ACCOUNT
        ),
    }
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
    write_with_stores(roots, request, SystemStores::with_runner(runner))
}

/// Write with injected system credential stores (Keychain + WinCred).
pub(crate) fn write_with_stores(
    roots: &PathRoots,
    request: &SwitchRequest<'_>,
    stores: SystemStores<'_>,
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
    // 至少要有一处能落地：macOS / Windows 非沙箱写系统凭据，其余只有旧版 IDE 的库。
    // 新版客户端根本没有 state.vscdb，不能因此判它"没登录过"。
    if !path.is_file() && !uses_system_credential(roots) {
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
    // 这次只会写已有的库：投影不是我们建的，写回前后它的存在性不变。
    let wrote_database = path.is_file();
    let snapshot = capture(roots, stores)?;
    let outcome = write_all(roots, stores, &path, &session, snapshot.keychain.as_deref());
    match outcome {
        Ok(()) => Ok(SwitchOutcome {
            agent: "antigravity".to_string(),
            live: email,
            target: if wrote_database {
                path.display().to_string()
            } else {
                keychain_source(roots.os)
            },
        }),
        Err(error) => {
            if let Err(rollback) = restore_with_stores(&snapshot, stores) {
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

/// 写遍客户端读的每一处。`existing` 是本次切号前钥匙串里那份载荷：客户端自己的
/// 字段（`writer_id` 之类）要留着，不能整条换成我们的最小载荷。
fn write_all(
    roots: &PathRoots,
    stores: SystemStores<'_>,
    path: &Path,
    session: &Session<'_>,
    existing: Option<&str>,
) -> AccountResult<()> {
    // 系统凭据先写，vscdb 投影最后写：客户端真正读的那一处写完还回读得到同一个
    // refresh_token，才算"切换已提交"。
    let system = uses_system_credential(roots);
    if system {
        let fresh = antigravity_credential::payload_json(
            session.access_token,
            session.refresh_token,
            session.expires_at,
            session.id_token,
        );
        let payload = antigravity_credential::merge_payload(existing, &fresh, session.email);
        write_system_credential(roots.os, stores, &payload)?;
        antigravity_credential::write_gemini_files(&gemini_paths(roots), &payload)?;
    }
    // 新版客户端没有 state.vscdb：它不读投影，少这一处不算半个会话。
    if path.is_file() {
        write_oauth_row(path, session)?;
    }
    // 系统凭据是权威来源（旧版 IDE 现在也先读它），投影在场时两处一起校验。
    if system {
        verify_system_credential(roots.os, stores, session.refresh_token)?;
    }
    Ok(())
}

/// 回读系统凭据：写成别的账号或写丢，都要让切换失败而不是报告成功。
fn verify_system_credential(
    os: DesktopOs,
    stores: SystemStores<'_>,
    refresh_token: &str,
) -> AccountResult<()> {
    let source = keychain_source(os);
    let Some(payload) = read_system_payload(os, stores)? else {
        return Err(AccountError::Client(format!(
            "the {source} entry is gone after the switch: system store did not keep the Antigravity session"
        )));
    };
    let credential = antigravity_credential::parse_payload(&payload)?;
    if credential.refresh_token == refresh_token {
        return Ok(());
    }
    Err(AccountError::Client(format!(
        "{} did not keep the Antigravity session: it reads back a different refresh token",
        keychain_source(os)
    )))
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
    read_with_stores(roots, SystemStores::with_runner(runner))
}

/// Read with injected system credential stores (Keychain + WinCred).
pub(crate) fn read_with_stores(
    roots: &PathRoots,
    stores: SystemStores<'_>,
) -> AccountResult<Option<LocalCredential>> {
    // 新版客户端把系统凭据当权威来源；不可用或未写入时先退到 ~/.gemini 回退文件，最后退到 state.vscdb。
    if uses_system_credential(roots) {
        if let Some(payload) = read_system_payload(roots.os, stores)? {
            let credential = antigravity_credential::parse_payload(&payload)?;
            let email = credential.email.clone();
            return Ok(Some(local_credential(
                credential,
                keychain_source(roots.os),
                email,
            )));
        }
        if let Some(fallback_local) = read_gemini_fallback(&gemini_paths(roots).fallback)? {
            return Ok(Some(fallback_local));
        }
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
            email: session.email.clone(),
        },
        path.display().to_string(),
        session.email,
    )))
}

fn read_gemini_fallback(path: &Path) -> AccountResult<Option<LocalCredential>> {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(AccountError::Io(error)),
    };
    let Some(payload) = antigravity_credential::decode_payload_text(&raw)? else {
        return Ok(None);
    };
    let credential = antigravity_credential::parse_payload(&payload)?;
    let email = credential.email.clone();
    Ok(Some(local_credential(
        credential,
        path.display().to_string(),
        email,
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
    snapshot_with_stores(roots, SystemStores::with_runner(runner))
}

/// Capture with injected system credential stores.
pub(crate) fn snapshot_with_stores(
    roots: &PathRoots,
    stores: SystemStores<'_>,
) -> AccountResult<ClientSnapshot> {
    Ok(ClientSnapshot::Antigravity(Box::new(capture(roots, stores)?)))
}

/// 把现场整组放回去（生产路径：Keychain / WinCred 走系统实现）。
pub(crate) fn restore(snapshot: &AntigravitySnapshot) -> AccountResult<()> {
    restore_with_runner(snapshot, &SystemRunner)
}

/// 把现场整组放回去（注入命令执行器）。
pub(crate) fn restore_with_runner(
    snapshot: &AntigravitySnapshot,
    runner: &dyn CommandRunner,
) -> AccountResult<()> {
    restore_with_stores(snapshot, SystemStores::with_runner(runner))
}

/// 把现场整组放回去。回滚顺序与写入相反：先文件、再系统凭据、最后 vscdb 那行。
pub(crate) fn restore_with_stores(
    snapshot: &AntigravitySnapshot,
    stores: SystemStores<'_>,
) -> AccountResult<()> {
    if snapshot.system {
        for (path, previous) in &snapshot.gemini {
            super::restore_file(path, previous)?;
        }
        match &snapshot.keychain {
            Some(payload) => write_system_credential(snapshot.os, stores, payload)?,
            None => delete_system_credential(snapshot.os, stores)?,
        }
    }
    match &snapshot.oauth_row {
        Some(value) => write_raw_row(&snapshot.vscdb, value),
        None => drop_row(&snapshot.vscdb),
    }
}

fn capture(roots: &PathRoots, stores: SystemStores<'_>) -> AccountResult<AntigravitySnapshot> {
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
        keychain = read_system_payload(roots.os, stores)?;
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
        os: roots.os,
    })
}

/// 系统凭据在非沙箱 macOS（Keychain）与 Windows（WinCred）上读写；沙箱测试绝不碰宿主机凭据库。
fn uses_system_credential(roots: &PathRoots) -> bool {
    matches!(roots.os, DesktopOs::Macos | DesktopOs::Windows) && roots.sandbox_home.is_none()
}

fn gemini_paths(roots: &PathRoots) -> GeminiPaths {
    antigravity_credential::gemini_paths(&paths::antigravity_gemini_dir(roots))
}

fn read_system_payload(os: DesktopOs, stores: SystemStores<'_>) -> AccountResult<Option<String>> {
    match os {
        DesktopOs::Windows => {
            let Some(raw) = stores.wincred.read(WINCRED_TARGET)? else {
                return Ok(None);
            };
            antigravity_wincred::decode_wincred_payload(&raw)
        }
        _ => {
            let Some(raw) = antigravity_credential::read_keychain(stores.runner)? else {
                return Ok(None);
            };
            antigravity_credential::parse_keychain_value(&raw).map(Some)
        }
    }
}

fn write_system_credential(
    os: DesktopOs,
    stores: SystemStores<'_>,
    payload: &str,
) -> AccountResult<()> {
    match os {
        DesktopOs::Windows => {
            stores
                .wincred
                .write(WINCRED_TARGET, WINCRED_USERNAME, payload.as_bytes())
        }
        _ => antigravity_credential::write_keychain(stores.runner, payload),
    }
}

fn delete_system_credential(os: DesktopOs, stores: SystemStores<'_>) -> AccountResult<()> {
    match os {
        DesktopOs::Windows => stores.wincred.delete(WINCRED_TARGET),
        _ => antigravity_credential::delete_keychain(stores.runner),
    }
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

/// Open the editor's database strictly read-only for live checks and reads.
fn open_read_only(path: &Path) -> AccountResult<Connection> {
    vscdb::open_read_only(path, vscdb::READ_BUSY_TIMEOUT, missing_database)
}

fn missing_database(path: &Path) -> AccountError {
    AccountError::Client(format!(
        "Antigravity's state.vscdb is not at {} and there is no system credential to write here: start Antigravity once, sign in, then switch",
        path.display()
    ))
}

fn read_row_value(path: &Path) -> AccountResult<Option<String>> {
    let connection = open_read_only(path)?;
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
