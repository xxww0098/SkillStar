//! Devin Desktop（原 Windsurf）：state.vscdb 里的三行登录状态。
//!
//! 客户端把凭据放在编辑器数据库的 ItemTable 里：
//!
//! * windsurfAuthStatus —— 权威登录（apiKey、apiServerUrl、email、name、auth1Token）
//! * codeium.windsurf-windsurf_auth —— "选中的那条登录"，内容与上面一致
//! * windsurf.apiServerUrl —— 明文 apiServer，客户端启动时先读它
//!
//! 写回三行并回读同一个 apiKey 才算成功；任何一步失败按 [ClientSnapshot::Items]
//! 把三行放回去。**不写系统钥匙串**：Devin Desktop 自己读这三行，safeStorage
//! 加密的 secret 行（windsurf_auth.sessions）本 crate 不碰——它是客户端的会话
//! 密钥，不是账号凭据；缺它时客户端会用自己的 apiKey 重新建会话。
//!
//! 数据库不存在时不新建：客户端没启动过就没有可切的现场（与 Cursor 同一策略）。

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde_json::{Map, Value, json};

use super::paths::{self, PathRoots};
use super::vscdb;
use super::{ClientSnapshot, LocalCredential, SwitchOutcome, SwitchRequest};
use crate::error::{AccountError, AccountResult};
use crate::oauth::{Identity, TokenSet};
use crate::usage::fetch::devin_desktop_parse;

/// 权威登录行。
pub(crate) const AUTH_STATUS_KEY: &str = "windsurfAuthStatus";
/// 选中的登录行：客户端从它恢复当前会话。
const SELECTED_AUTH_KEY: &str = "codeium.windsurf-windsurf_auth";
/// 明文 apiServer 行。
const API_SERVER_PLAIN_KEY: &str = "windsurf.apiServerUrl";
/// provider_state 里的键。
const API_SERVER_STATE_KEY: &str = "api_server_url";

/// The database Devin Desktop keeps its login in.
pub fn credential_path(roots: &PathRoots) -> PathBuf {
    paths::devin_desktop_state_db_path(roots)
}

/// Write the Devin Desktop login.
pub fn write(roots: &PathRoots, request: &SwitchRequest<'_>) -> AccountResult<SwitchOutcome> {
    let path = credential_path(roots);
    if !path.is_file() {
        return Err(missing_database(&path));
    }
    let api_key = request.tokens.access_token.trim();
    if api_key.is_empty() {
        return Err(AccountError::Client(
            "this account has no apiKey to write into Devin Desktop".to_string(),
        ));
    }
    let api_server_url = api_server_of(request.provider_state);
    let document = auth_status_json(request, api_key, api_server_url.as_deref());

    let snapshot = capture(&path)?;
    match write_rows(&path, &document, api_server_url.as_deref()) {
        Ok(()) => Ok(SwitchOutcome {
            agent: "devin-desktop".to_string(),
            live: live_identity(&identity_of(request)),
            target: path.display().to_string(),
        }),
        Err(error) => {
            if let Err(rollback) = super::restore(&snapshot) {
                tracing::error!(
                    target: "accounts",
                    cause = %error,
                    error = %rollback,
                    "putting Devin Desktop's previous login back failed"
                );
            }
            Err(error)
        }
    }
}

/// 三行的内容。auth1Token 只在记录里有时写（旧版登录用它）。
fn auth_status_json(
    request: &SwitchRequest<'_>,
    api_key: &str,
    api_server_url: Option<&str>,
) -> String {
    let mut map = Map::new();
    map.insert("apiKey".to_string(), json!(api_key));
    if let Some(api_server_url) = api_server_url {
        map.insert("apiServerUrl".to_string(), json!(api_server_url));
    }
    if let Some(email) = email_of(request) {
        map.insert("email".to_string(), json!(email));
    }
    if let Some(name) = display_name_of(request) {
        map.insert("name".to_string(), json!(name));
    }
    Value::Object(map).to_string()
}

fn write_rows(path: &Path, document: &str, api_server_url: Option<&str>) -> AccountResult<()> {
    let mut rows = vec![
        (AUTH_STATUS_KEY, Some(document)),
        (SELECTED_AUTH_KEY, Some(document)),
    ];
    if let Some(url) = api_server_url {
        rows.push((API_SERVER_PLAIN_KEY, Some(url)));
    }
    vscdb::write_rows(
        path,
        vscdb::DEFAULT_BUSY_TIMEOUT,
        true,
        "Devin Desktop",
        missing_database,
        &rows,
    )?;
    // 回读：写不进去（或写进去的是别的账号）必须让切换失败。
    let connection = open(path)?;
    let stored = vscdb::read_value(&connection, AUTH_STATUS_KEY, "Devin Desktop")?;
    let Some(stored) = stored else {
        return Err(AccountError::Client(format!(
            "{} did not keep the Devin Desktop login",
            path.display()
        )));
    };
    let stored_key = json_field(&stored, "apiKey");
    let wanted_key = json_field(document, "apiKey");
    if stored_key != wanted_key {
        return Err(AccountError::Client(format!(
            "{} did not keep the Devin Desktop login: it reads back a different apiKey",
            path.display()
        )));
    }
    Ok(())
}

/// Read the Devin Desktop login (this is how "add from this machine" works).
pub fn read(roots: &PathRoots) -> AccountResult<Option<LocalCredential>> {
    let path = credential_path(roots);
    if !path.is_file() {
        return Ok(None);
    }
    let rows = read_rows(&path)?;
    let raw = rows
        .iter()
        .find(|(key, _)| key == AUTH_STATUS_KEY)
        .or_else(|| rows.iter().find(|(key, _)| key == SELECTED_AUTH_KEY))
        .and_then(|(_, value)| value.clone());
    let Some(raw) = raw else {
        return Ok(None);
    };
    let document: Value = serde_json::from_str(&raw).map_err(|error| {
        AccountError::Client(format!(
            "{}: the Devin Desktop login is not valid JSON: {error}",
            path.display()
        ))
    })?;
    let field = |name: &str| json_field(&raw, name);
    let Some(api_key) = field("apiKey").or_else(|| field("auth1Token")) else {
        return Ok(None);
    };
    let api_server_url = field("apiServerUrl")
        .or_else(|| {
            rows.iter()
                .find(|(key, _)| key == API_SERVER_PLAIN_KEY)
                .and_then(|(_, value)| value.clone())
        });
    let email = devin_desktop_parse::email_of(&document).or_else(|| field("email"));
    let name = field("name");
    Ok(Some(LocalCredential {
        tokens: TokenSet {
            access_token: api_key,
            refresh_token: None,
            id_token: None,
            // apiKey 没有过期时间：不编一个。
            expires_at: None,
            token_type: "Bearer".to_string(),
        },
        identity: Identity {
            display_name: name.or_else(|| email.clone()),
            email,
            plan: None,
            account_id: None,
        },
        provider_state: api_server_state(api_server_url.as_deref()),
        source: path.display().to_string(),
    }))
}

/// provider_state：只记 apiServer，别的一概不存。
pub(crate) fn api_server_state(api_server_url: Option<&str>) -> Option<Value> {
    let url = api_server_url.map(str::trim).filter(|url| !url.is_empty())?;
    Some(json!({ (API_SERVER_STATE_KEY): url }))
}

fn api_server_of(provider_state: Option<&Value>) -> Option<String> {
    provider_state
        .and_then(|state| state.get(API_SERVER_STATE_KEY))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(str::to_string)
}

/// The identity a live Devin Desktop is serving: the email it shows.
pub(crate) fn live_identity(identity: &Identity) -> Option<String> {
    identity.email.clone().or_else(|| identity.account_id.clone())
}

/// Capture the three rows so a failed switch can put them back.
pub fn snapshot(roots: &PathRoots) -> AccountResult<ClientSnapshot> {
    capture(&credential_path(roots))
}

fn capture(path: &Path) -> AccountResult<ClientSnapshot> {
    if !path.is_file() {
        // 没有数据库就等于没有可写的现场；空快照回滚时什么也不做。
        return Ok(ClientSnapshot::Items {
            path: path.to_path_buf(),
            rows: Vec::new(),
        });
    }
    let rows = read_rows(path)?;
    let wanted: Vec<(String, Option<String>)> = keys()
        .into_iter()
        .map(|key| {
            let previous = rows
                .iter()
                .find(|(row_key, _)| row_key == key)
                .and_then(|(_, value)| value.clone());
            (key.to_string(), previous)
        })
        .collect();
    Ok(ClientSnapshot::Items {
        path: path.to_path_buf(),
        rows: wanted,
    })
}

/// 这次切号会碰的行。
fn keys() -> [&'static str; 3] {
    [AUTH_STATUS_KEY, SELECTED_AUTH_KEY, API_SERVER_PLAIN_KEY]
}

fn missing_database(path: &Path) -> AccountError {
    AccountError::Client(format!(
        "Devin Desktop 的 state.vscdb 不在 {}：请先启动客户端并登录一次，再切号",
        path.display()
    ))
}

/// 打开客户端的数据库；表不存在时补一张（库文件本身绝不新建）。
fn open(path: &Path) -> AccountResult<Connection> {
    vscdb::open_existing(path, vscdb::DEFAULT_BUSY_TIMEOUT, true, missing_database)
}

fn read_rows(path: &Path) -> AccountResult<Vec<(String, Option<String>)>> {
    let connection = open(path)?;
    vscdb::read_values(&connection, &keys(), "Devin Desktop")
}

fn json_field(document: &str, name: &str) -> Option<String> {
    let value: Value = serde_json::from_str(document).ok()?;
    value
        .get(name)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn email_of(request: &SwitchRequest<'_>) -> Option<String> {
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
}

fn display_name_of(request: &SwitchRequest<'_>) -> Option<String> {
    let name = request.display_name.trim();
    (!name.is_empty() && !name.eq_ignore_ascii_case("devin desktop"))
        .then(|| name.to_string())
}

fn identity_of(request: &SwitchRequest<'_>) -> Identity {
    Identity {
        display_name: Some(request.display_name.to_string()),
        email: email_of(request),
        plan: None,
        account_id: None,
    }
}

#[cfg(test)]
#[path = "devin_desktop_tests.rs"]
mod devin_desktop_tests;
