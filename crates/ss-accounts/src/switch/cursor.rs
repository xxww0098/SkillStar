//! Cursor: its global storage SQLite database, cursorAuth keys.
//!
//! <userData>/User/globalStorage/state.vscdb, table ItemTable(key, value).
//! Cursor keeps its OAuth session as plain key/value rows rather than one JSON
//! file, so a switch is one SQLite transaction that INSERT OR REPLACEs the rows
//! the editor reads at startup:
//!
//!   cursorAuth/accessToken, cursorAuth/refreshToken, cursorAuth/cachedEmail,
//!   cursorAuth/authId, plus the cursor.accessToken / cursor.email mirrors.
//!
//! The authId is the part of the access token's JWT subject after the bar
//! (Cursor's subject is "<user>|<auth id>"). Every row is read back after the
//! commit; a database that does not keep them fails the switch instead of
//! reporting success. Nothing else in the database is touched, and the file
//! itself must already exist: Cursor owns its schema, and creating a stand-in
//! database would only look like a switch.

use std::path::{Path, PathBuf};

use rusqlite::Connection;

use super::paths::{self, PathRoots};
use super::vscdb;
use super::{ClientSnapshot, LocalCredential, SwitchOutcome, SwitchRequest};
use crate::error::{AccountError, AccountResult};

const ACCESS_TOKEN_KEY: &str = "cursorAuth/accessToken";
const REFRESH_TOKEN_KEY: &str = "cursorAuth/refreshToken";
const EMAIL_KEY: &str = "cursorAuth/cachedEmail";
const AUTH_ID_KEY: &str = "cursorAuth/authId";
const MIRROR_ACCESS_TOKEN_KEY: &str = "cursor.accessToken";
const MIRROR_EMAIL_KEY: &str = "cursor.email";

/// Cursor 3.29+ 在 `authTokenEncryptionAtRest` 下写入的 safeStorage 密文前缀。
pub(crate) const SAFE_STORAGE_PREFIX: &str = "cursor-safe-storage:v1:";

pub(crate) fn is_safe_storage_ciphertext(value: &str) -> bool {
    value.trim().starts_with(SAFE_STORAGE_PREFIX)
}

/// Every row a switch writes, in the order it writes them.
const SWITCH_KEYS: [&str; 6] = [
    ACCESS_TOKEN_KEY,
    REFRESH_TOKEN_KEY,
    EMAIL_KEY,
    AUTH_ID_KEY,
    MIRROR_ACCESS_TOKEN_KEY,
    MIRROR_EMAIL_KEY,
];

/// How long a writer waits for Cursor to finish a write of its own.
const BUSY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

/// The database Cursor keeps its session in.
pub fn credential_path(roots: &PathRoots) -> PathBuf {
    paths::cursor_state_db_path(roots)
}

/// Write the Cursor credential.
pub fn write(roots: &PathRoots, request: &SwitchRequest<'_>) -> AccountResult<SwitchOutcome> {
    let path = credential_path(roots);
    let tokens = request.tokens;
    let access_token = tokens.access_token.trim().to_string();
    if access_token.is_empty() {
        return Err(AccountError::Client(
            "this account has no access token to write into Cursor".to_string(),
        ));
    }
    if is_safe_storage_ciphertext(&access_token) {
        return Err(AccountError::Client(
            "Cursor 凭据是本地加密密文（cursor-safe-storage:v1:），无法写回；请先刷新或重新授权"
                .to_string(),
        ));
    }
    let mut connection = open(&path)?;

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
        })
        .or(read_non_empty(&connection, EMAIL_KEY)?)
        .or(read_non_empty(&connection, MIRROR_EMAIL_KEY)?);
    let auth_id = request
        .account_id
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .or_else(|| crate::identity::jwt_subject_tail(&access_token));
    let refresh_token = tokens
        .refresh_token
        .as_deref()
        .map(str::trim)
        .filter(|token| !token.is_empty() && !is_safe_storage_ciphertext(token))
        .map(str::to_string);

    let transaction = connection
        .transaction()
        .map_err(|error| vscdb::sqlite_error("开始事务", &path, error))?;
    vscdb::apply_rows(
        &transaction,
        &[
            (ACCESS_TOKEN_KEY, Some(access_token.as_str())),
            (MIRROR_ACCESS_TOKEN_KEY, Some(access_token.as_str())),
            (REFRESH_TOKEN_KEY, refresh_token.as_deref()),
            (EMAIL_KEY, email.as_deref()),
            (MIRROR_EMAIL_KEY, email.as_deref()),
            (AUTH_ID_KEY, auth_id.as_deref()),
        ],
        "Cursor",
    )?;
    transaction
        .commit()
        .map_err(|error| vscdb::sqlite_error("提交事务", &path, error))?;

    // Read the session back: a database that rolled the rows away must fail the
    // switch rather than report a success the editor will not see.
    let stored_access = vscdb::read_value(&connection, ACCESS_TOKEN_KEY, "Cursor")?;
    let stored_refresh = vscdb::read_value(&connection, REFRESH_TOKEN_KEY, "Cursor")?;
    let stored_email = vscdb::read_value(&connection, EMAIL_KEY, "Cursor")?;
    if stored_access.as_deref() != Some(access_token.as_str())
        || stored_refresh != refresh_token
        || stored_email != email
    {
        return Err(AccountError::Client(format!(
            "{} did not keep the session rows",
            path.display()
        )));
    }
    Ok(SwitchOutcome {
        agent: "cursor".to_string(),
        live: email,
        target: path.display().to_string(),
    })
}

/// Read the Cursor credential in full.
pub fn read(roots: &PathRoots) -> AccountResult<Option<LocalCredential>> {
    let path = credential_path(roots);
    if !path.is_file() {
        return Ok(None);
    }
    let connection = open_read_only(&path)?;
    let primary_raw = read_non_empty(&connection, ACCESS_TOKEN_KEY)?;
    let mirror_raw = read_non_empty(&connection, MIRROR_ACCESS_TOKEN_KEY)?;
    let access_token = match (
        primary_raw
            .as_deref()
            .filter(|value| !is_safe_storage_ciphertext(value)),
        mirror_raw
            .as_deref()
            .filter(|value| !is_safe_storage_ciphertext(value)),
    ) {
        (Some(token), _) => token.to_string(),
        (None, Some(token)) => token.to_string(),
        (None, None) => {
            if primary_raw
                .as_deref()
                .is_some_and(is_safe_storage_ciphertext)
                || mirror_raw
                    .as_deref()
                    .is_some_and(is_safe_storage_ciphertext)
            {
                return Err(AccountError::Client(
                    "Cursor 已启用本地凭据加密且缺少明文镜像令牌，请使用 OAuth 授权登录"
                        .to_string(),
                ));
            }
            return Ok(None);
        }
    };
    let email = read_non_empty(&connection, EMAIL_KEY)?
        .or(read_non_empty(&connection, MIRROR_EMAIL_KEY)?);
    let auth_id = read_non_empty(&connection, AUTH_ID_KEY)?;
    let refresh_token = read_non_empty(&connection, REFRESH_TOKEN_KEY)?
        .filter(|value| !is_safe_storage_ciphertext(value));
    Ok(Some(LocalCredential {
        tokens: crate::oauth::TokenSet {
            expires_at: crate::identity::jwt_exp(&access_token),
            access_token: access_token.clone(),
            refresh_token,
            id_token: None,
            token_type: "Bearer".to_string(),
        },
        identity: crate::oauth::Identity {
            display_name: email.clone(),
            email,
            plan: None,
            account_id: auth_id.or_else(|| crate::identity::jwt_subject_tail(&access_token)),
        },
        provider_state: None,
        source: path.display().to_string(),
    }))
}

fn read_non_empty(connection: &Connection, key: &str) -> AccountResult<Option<String>> {
    Ok(vscdb::read_value(connection, key, "Cursor")?
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty()))
}

/// The identity a live Cursor is serving: the cached email it shows.
pub(crate) fn live_identity(identity: &crate::oauth::Identity) -> Option<String> {
    identity.email.clone()
}

/// Capture the session rows so a failed switch can put them back.
pub fn snapshot(roots: &PathRoots) -> AccountResult<ClientSnapshot> {
    let path = credential_path(roots);
    if !path.is_file() {
        return Ok(ClientSnapshot::Items {
            path,
            rows: SWITCH_KEYS
                .iter()
                .map(|key| ((*key).to_string(), None))
                .collect(),
        });
    }
    let connection = open_read_only(&path)?;
    let rows = vscdb::read_values(&connection, &SWITCH_KEYS, "Cursor")?;
    Ok(ClientSnapshot::Items { path, rows })
}

#[cfg(test)]
#[path = "cursor_tests.rs"]
mod cursor_tests;

/// Open the editor's database for writing; the editor owns the file, we only borrow it.
fn open(path: &Path) -> AccountResult<Connection> {
    vscdb::open_existing(path, BUSY_TIMEOUT, false, |missing| {
        AccountError::Client(format!(
            "Cursor's state.vscdb is not at {}: start Cursor once, sign in, then switch",
            missing.display()
        ))
    })
}

/// Open the editor's database strictly read-only for live checks and snapshots.
fn open_read_only(path: &Path) -> AccountResult<Connection> {
    vscdb::open_read_only(path, vscdb::READ_BUSY_TIMEOUT, |missing| {
        AccountError::Client(format!(
            "Cursor's state.vscdb is not at {}: start Cursor once, sign in, then switch",
            missing.display()
        ))
    })
}
