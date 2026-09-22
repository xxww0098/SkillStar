//! Token paste and `state.vscdb` import.
//!
//! `windsurf_auth-*` is a local usage cache and is never read. `secret://`
//! rows are decrypted only with caller-supplied [`KeyMaterial`]; this path
//! does not touch the system keychain.

use std::path::Path;

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use serde_json::Value;

use super::{
    API_SERVER_SECRET_KEY, AUTH_STATUS_KEY, AUTH1_API_SERVER, SESSIONS_SECRET_KEY, WindsurfState,
};
use crate::catalog::AuthMode;
use crate::subscription::{BillingCycle, Subscription};
use crate::token_import::ImportedToken;
use crate::tool_store::safe_storage::{self, KeyMaterial};
use crate::{UsageError, UsageResult};

const MIN_JSON_SECRET: usize = 8;

pub(crate) fn import_from_token(raw: &str) -> UsageResult<ImportedToken> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(UsageError::Other("Windsurf 令牌为空".into()));
    }
    if trimmed.starts_with(['{', '[', '"']) {
        let value: Value = serde_json::from_str(trimmed)
            .map_err(|_| UsageError::Other("Windsurf JSON 无法解析".into()))?;
        return match value {
            Value::Object(_) => imported_from_object(&value),
            Value::String(text) => bare_token(&text),
            _ => Err(UsageError::Other(
                "Windsurf JSON 须是 apiKey 字符串或包含 apiKey 的对象".into(),
            )),
        };
    }
    bare_token(trimmed)
}

pub(crate) fn import_from_local() -> UsageResult<ImportedToken> {
    import_from_local_with_key(None)
}

pub(crate) fn import_from_local_with_key(key: Option<&KeyMaterial>) -> UsageResult<ImportedToken> {
    let path = crate::tool_paths::windsurf_state_db_path()
        .ok_or_else(|| UsageError::Other("无法解析 Windsurf 数据目录".into()))?;
    read_local_db(&path, key)
}

pub(crate) fn oauth_row_from_imported(imported: ImportedToken) -> UsageResult<Subscription> {
    let provider_state = imported
        .provider_state
        .filter(|value| !value.trim().is_empty());
    if imported.access_token.trim().is_empty() && provider_state.is_none() {
        return Err(UsageError::Other("Windsurf 导入没有可用凭据".into()));
    }
    let now = chrono::Utc::now().timestamp();
    let currency = imported.currency.unwrap_or_else(|| "USD".to_string());
    Ok(Subscription {
        id: uuid::Uuid::new_v4().to_string(),
        catalog_id: super::CATALOG_ID.to_string(),
        display_name: imported.display_name,
        auth_mode: AuthMode::OAuth,
        plan_tier: None,
        monthly_price: None,
        currency,
        billing_cycle: BillingCycle::Monthly,
        start_date: 0,
        renew_date: 0,
        auto_renew: false,
        api_key_encrypted: None,
        platform_token_encrypted: None,
        access_token_encrypted: (!imported.access_token.is_empty())
            .then(|| crate::crypto::encrypt(&imported.access_token)),
        refresh_token_encrypted: imported
            .refresh_token
            .filter(|value| !value.is_empty())
            .map(|value| crate::crypto::encrypt(&value)),
        access_token_expires_at: imported.expires_at,
        id_token_encrypted: None,
        oauth_account_id: imported.oauth_account_id,
        oauth_region: None,
        requires_reauth: false,
        provider_state_encrypted: provider_state.as_deref().map(crate::crypto::encrypt),
        cookie_jar_encrypted: None,
        cookie_session_expires_at: None,
        manual_quota: None,
        note: None,
        sort_index: 0,
        created_at: now,
        updated_at: now,
    })
}

fn bare_token(raw: &str) -> UsageResult<ImportedToken> {
    let token = raw.trim();
    if is_prefixed(token, "sk-ws-", 12) {
        return Ok(token_row(
            String::new(),
            None,
            None,
            WindsurfState {
                api_key: Some(token.to_string()),
                ..WindsurfState::default()
            },
        ));
    }
    if is_prefixed(token, "auth1_", 14) {
        return Ok(token_row(
            String::new(),
            None,
            None,
            WindsurfState {
                auth1_token: Some(token.to_string()),
                ..WindsurfState::default()
            },
        ));
    }
    if is_prefixed(token, "devin-session-token$", 28) {
        return Ok(token_row(
            token.to_string(),
            None,
            None,
            WindsurfState {
                api_key: Some(token.to_string()),
                api_server_url: Some(AUTH1_API_SERVER.to_string()),
                auth1_token: None,
            },
        ));
    }
    Err(UsageError::Other(
        "Windsurf 令牌无法识别：请粘贴 apiKey（sk-ws-）或包含 apiKey 的 JSON".into(),
    ))
}

fn is_prefixed(token: &str, prefix: &str, min_len: usize) -> bool {
    token.starts_with(prefix) && token.len() >= min_len && !token.contains(char::is_whitespace)
}

fn imported_from_object(value: &Value) -> UsageResult<ImportedToken> {
    let api_key = usable(super::pick_string(Some(value), &["apiKey", "api_key"]));
    let auth_token = usable(super::pick_string(
        Some(value),
        &["authToken", "accessToken", "sessionToken", "auth_token"],
    ));
    let auth1 = usable(super::pick_string(
        Some(value),
        &["auth1Token", "auth1_token"],
    ));
    let refresh = usable(super::pick_string(
        Some(value),
        &["refreshToken", "refresh_token", "firebaseRefreshToken"],
    ));
    let email =
        super::pick_string(Some(value), &["email", "userEmail"]).filter(|value| !value.is_empty());
    let api_server = super::pick_string(Some(value), &["apiServerUrl", "api_server_url"]);
    if api_key.is_none() && auth_token.is_none() && auth1.is_none() {
        return Err(UsageError::Other(
            "Windsurf JSON 缺少 apiKey、authToken 或 auth1Token".into(),
        ));
    }
    let mut access = auth_token.unwrap_or_default();
    if access.is_empty()
        && api_key
            .as_deref()
            .is_some_and(|key| key.starts_with("devin-session-token$"))
    {
        access = api_key.clone().unwrap_or_default();
    }
    Ok(token_row(
        access,
        refresh,
        email,
        WindsurfState {
            api_key,
            api_server_url: api_server,
            auth1_token: auth1,
        },
    ))
}

fn usable(value: Option<String>) -> Option<String> {
    value.filter(|token| token.len() >= MIN_JSON_SECRET && !token.contains(char::is_whitespace))
}

fn token_row(
    access_token: String,
    refresh_token: Option<String>,
    email: Option<String>,
    state: WindsurfState,
) -> ImportedToken {
    let account_id = email.filter(|value| !value.is_empty());
    let display_name = account_id
        .clone()
        .filter(|value| crate::fetchers::oauth::common::looks_like_email(value))
        .unwrap_or_else(|| "Windsurf".to_string());
    ImportedToken {
        display_name,
        access_token,
        refresh_token,
        expires_at: None,
        oauth_account_id: account_id,
        provider_state: state.to_json(),
        currency: None,
        oauth_region: None,
        id_token: None,
        api_key: None,
    }
}

#[derive(Debug, Default)]
struct LocalAuth {
    api_key: Option<String>,
    api_server_url: Option<String>,
    auth_token: Option<String>,
    refresh_token: Option<String>,
    auth1_token: Option<String>,
    email: Option<String>,
    name: Option<String>,
}

fn read_local_db(path: &Path, key: Option<&KeyMaterial>) -> UsageResult<ImportedToken> {
    if !path.is_file() {
        return Err(UsageError::Other(format!(
            "未找到 Windsurf state.vscdb：{}",
            path.display()
        )));
    }
    let status_raw = crate::vscdb::read_item_string(path, AUTH_STATUS_KEY)?;
    let sessions_raw = crate::vscdb::read_item_string(path, SESSIONS_SECRET_KEY)?;
    let server_raw = crate::vscdb::read_item_string(path, API_SERVER_SECRET_KEY)?;

    let mut auth = status_raw
        .as_deref()
        .map(|raw| parse_auth_status(raw, key))
        .transpose()?
        .unwrap_or_default();

    if auth.api_key.is_none() {
        match decrypt_sessions(sessions_raw.as_deref(), key)? {
            Some((token, label)) => {
                auth.api_key = Some(token);
                if auth.email.is_none() {
                    auth.email = label
                        .filter(|value| crate::fetchers::oauth::common::looks_like_email(value));
                }
            }
            None => {
                if sessions_raw
                    .as_ref()
                    .is_some_and(|raw| !raw.trim().is_empty())
                    && key.is_none()
                {
                    return Err(missing_key_material());
                }
            }
        }
    }
    if auth.api_server_url.is_none()
        && let Some(url) = decrypt_server(server_raw.as_deref(), key)?
    {
        auth.api_server_url = Some(url);
    }
    if auth.api_key.is_none() && auth.auth_token.is_none() && auth.auth1_token.is_none() {
        return Err(UsageError::Other(
            "Windsurf 未登录（windsurfAuthStatus 没有 apiKey 或 session）".into(),
        ));
    }
    Ok(local_to_imported(auth))
}

fn missing_key_material() -> UsageError {
    UsageError::Other(
        "Windsurf secret:// 已加密，但没有注入 Safe Storage 密钥（不会读取系统钥匙串）".into(),
    )
}

fn parse_auth_status(raw: &str, key: Option<&KeyMaterial>) -> UsageResult<LocalAuth> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(LocalAuth::default());
    }
    let json_text = if trimmed.starts_with('{') {
        trimmed.to_string()
    } else if let Some(key) = key {
        decrypt_secret_value(trimmed, key)?
    } else {
        return Err(missing_key_material());
    };
    let value: Value = serde_json::from_str(&json_text)
        .map_err(|err| UsageError::Other(format!("解析 windsurfAuthStatus 失败: {err}")))?;
    if !value.is_object() {
        return Err(UsageError::Other(
            "windsurfAuthStatus 不是 JSON 对象".into(),
        ));
    }
    Ok(LocalAuth {
        api_key: super::pick_string(Some(&value), &["apiKey", "api_key"]),
        api_server_url: super::pick_string(Some(&value), &["apiServerUrl", "api_server_url"]),
        auth_token: super::pick_string(
            Some(&value),
            &["authToken", "accessToken", "sessionToken", "auth_token"],
        ),
        refresh_token: super::pick_string(
            Some(&value),
            &["refreshToken", "refresh_token", "firebaseRefreshToken"],
        ),
        auth1_token: super::pick_string(Some(&value), &["auth1Token", "auth1_token"]),
        email: super::pick_string(Some(&value), &["email"]),
        name: super::pick_string(Some(&value), &["name"]),
    })
}

fn decrypt_sessions(
    raw: Option<&str>,
    key: Option<&KeyMaterial>,
) -> UsageResult<Option<(String, Option<String>)>> {
    let Some(raw) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let Some(key) = key else {
        return Ok(None);
    };
    let plain = decrypt_secret_value(raw, key)?;
    let value: Value = serde_json::from_str(&plain)
        .map_err(|err| UsageError::Other(format!("解析 windsurf_auth.sessions 失败: {err}")))?;
    let entry = value
        .as_array()
        .and_then(|items| items.first())
        .unwrap_or(&value);
    let token = super::pick_string(
        Some(entry),
        &["accessToken", "access_token", "apiKey", "api_key"],
    )
    .ok_or_else(|| UsageError::Other("Windsurf sessions 密文里没有 accessToken".into()))?;
    let label = entry
        .get("account")
        .and_then(|account| super::pick_string(Some(account), &["label", "id"]));
    Ok(Some((token, label)))
}

fn decrypt_server(raw: Option<&str>, key: Option<&KeyMaterial>) -> UsageResult<Option<String>> {
    let Some(raw) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let Some(key) = key else {
        return Ok(None);
    };
    let plain = decrypt_secret_value(raw, key)?;
    let url = plain.trim();
    if url.is_empty() {
        Ok(None)
    } else {
        Ok(Some(url.to_string()))
    }
}

fn decrypt_secret_value(stored: &str, key: &KeyMaterial) -> UsageResult<String> {
    let encoded = ciphertext_base64(stored);
    let bytes = safe_storage::decrypt_secret(key, &encoded)
        .map_err(|err| UsageError::Other(format!("Windsurf secret:// 解密失败: {err}")))?;
    String::from_utf8(bytes).map_err(|_| UsageError::Other("Windsurf secret:// 不是 UTF-8".into()))
}

/// Official rows are standard base64. Cockpit also writes
/// `{"type":"Buffer","data":[...]}`.
fn ciphertext_base64(stored: &str) -> String {
    let Ok(value) = serde_json::from_str::<Value>(stored.trim()) else {
        return stored.trim().to_string();
    };
    if value.get("type").and_then(Value::as_str) != Some("Buffer") {
        return stored.trim().to_string();
    }
    match value.get("data") {
        Some(Value::String(text)) => text.trim().to_string(),
        Some(Value::Array(bytes)) => {
            let raw: Vec<u8> = bytes
                .iter()
                .filter_map(|item| item.as_u64().map(|n| n as u8))
                .collect();
            BASE64.encode(raw)
        }
        _ => stored.trim().to_string(),
    }
}

fn local_to_imported(auth: LocalAuth) -> ImportedToken {
    let mut access = auth.auth_token.unwrap_or_default();
    if access.is_empty()
        && auth
            .api_key
            .as_deref()
            .is_some_and(|key| key.starts_with("devin-session-token$"))
    {
        access = auth.api_key.clone().unwrap_or_default();
    }
    let email = auth.email.filter(|value| !value.is_empty());
    let display_name = email
        .clone()
        .filter(|value| crate::fetchers::oauth::common::looks_like_email(value))
        .or(auth.name)
        .unwrap_or_else(|| "Windsurf".to_string());
    let account_id = email.filter(|value| !value.is_empty());
    ImportedToken {
        display_name,
        access_token: access,
        refresh_token: auth.refresh_token,
        expires_at: None,
        oauth_account_id: account_id,
        provider_state: WindsurfState {
            api_key: auth.api_key,
            api_server_url: auth.api_server_url,
            auth1_token: auth.auth1_token,
        }
        .to_json(),
        currency: None,
        oauth_region: None,
        id_token: None,
        api_key: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool_store::safe_storage::encrypt_secret;
    use std::path::Path;

    struct EnvGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
        tool_sync: Option<std::ffi::OsString>,
    }

    impl EnvGuard {
        fn sandbox(path: &Path) -> Self {
            let lock = crate::test_env_lock()
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let tool_sync = std::env::var_os("SKILLSTAR_TOOL_SYNC_HOME");
            // SAFETY: serialized by the crate-wide test_env_lock.
            unsafe {
                std::env::set_var("SKILLSTAR_TOOL_SYNC_HOME", path);
            }
            Self {
                _lock: lock,
                tool_sync,
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            // SAFETY: still covered by test_env_lock.
            unsafe {
                match self.tool_sync.take() {
                    Some(value) => std::env::set_var("SKILLSTAR_TOOL_SYNC_HOME", value),
                    None => std::env::remove_var("SKILLSTAR_TOOL_SYNC_HOME"),
                }
            }
        }
    }

    fn write_items(path: &Path, items: &[(&str, &str)]) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let conn = rusqlite::Connection::open(path).unwrap();
        conn.execute(
            "CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT)",
            [],
        )
        .unwrap();
        for (key, value) in items {
            conn.execute(
                "INSERT INTO ItemTable (key, value) VALUES (?1, ?2)",
                [key, value],
            )
            .unwrap();
        }
    }

    fn sandboxed_db() -> (tempfile::TempDir, EnvGuard, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let guard = EnvGuard::sandbox(dir.path());
        let db = crate::tool_paths::windsurf_state_db_path().expect("windsurf db path");
        (dir, guard, db)
    }

    #[test]
    fn json_paste_keeps_session_email_and_api_key() {
        let imported = import_from_token(
            r#"{"apiKey":"sk-ws-from-json","authToken":"session-token","refreshToken":"firebase-refresh","email":"ada@wind.dev","apiServerUrl":"https://server.example.test","auth1_token":"auth1_long-enough"}"#,
        )
        .unwrap();
        assert_eq!(imported.access_token, "session-token");
        assert_eq!(imported.refresh_token.as_deref(), Some("firebase-refresh"));
        assert_eq!(imported.oauth_account_id.as_deref(), Some("ada@wind.dev"));
        assert_eq!(imported.display_name, "ada@wind.dev");
        let state = WindsurfState::parse(imported.provider_state.as_deref().unwrap());
        assert_eq!(state.api_key.as_deref(), Some("sk-ws-from-json"));
        assert_eq!(
            state.api_server_url.as_deref(),
            Some("https://server.example.test")
        );
        assert_eq!(state.auth1_token.as_deref(), Some("auth1_long-enough"));
    }

    #[test]
    fn garbage_and_short_secrets_are_rejected() {
        for raw in [
            "",
            "   ",
            "hello",
            "sk-ws-short",
            "{",
            "[]",
            "{}",
            r#"{"apiKey":"short"}"#,
            r#"{"note":"nope"}"#,
        ] {
            assert!(import_from_token(raw).is_err(), "{raw}");
        }
    }

    #[test]
    fn auth1_and_devin_session_pastes_are_recognized() {
        let auth1 = import_from_token("auth1_long-enough-token").unwrap();
        let state = WindsurfState::parse(auth1.provider_state.as_deref().unwrap());
        assert_eq!(
            state.auth1_token.as_deref(),
            Some("auth1_long-enough-token")
        );
        assert!(auth1.access_token.is_empty());

        let session = import_from_token("devin-session-token$abc123xyz").unwrap();
        assert_eq!(session.access_token, "devin-session-token$abc123xyz");
        let state = WindsurfState::parse(session.provider_state.as_deref().unwrap());
        assert_eq!(state.api_server_url.as_deref(), Some(AUTH1_API_SERVER));
    }

    #[test]
    fn local_auth_status_beats_the_usage_cache() {
        let (_dir, _guard, db) = sandboxed_db();
        write_items(
            &db,
            &[
                (
                    AUTH_STATUS_KEY,
                    r#"{"apiKey":"sk-ws-from-status","email":"status@wind.dev","apiServerUrl":"https://server.example.test","authToken":"status-session"}"#,
                ),
                (
                    "windsurf_auth-decoy-usages",
                    r#"{"apiKey":"sk-ws-do-not-use"}"#,
                ),
            ],
        );
        let imported = import_from_local().unwrap();
        assert_eq!(imported.access_token, "status-session");
        assert_eq!(
            imported.oauth_account_id.as_deref(),
            Some("status@wind.dev")
        );
        let state = WindsurfState::parse(imported.provider_state.as_deref().unwrap());
        assert_eq!(state.api_key.as_deref(), Some("sk-ws-from-status"));
        assert!(!state.api_key.unwrap().contains("do-not-use"));
    }

    #[test]
    fn secret_sessions_round_trip_with_an_injected_password() {
        let (_dir, _guard, db) = sandboxed_db();
        let key = KeyMaterial::macos_v10("injected-password");
        let sessions = r#"[{"accessToken":"sk-ws-from-secret-key","account":{"label":"secret@wind.dev","id":"secret@wind.dev"}}]"#;
        let encoded = encrypt_secret(&key, sessions.as_bytes()).unwrap();
        let server = encrypt_secret(&key, b"https://api.example.test").unwrap();
        write_items(
            &db,
            &[
                (AUTH_STATUS_KEY, r#"{"email":"status@wind.dev"}"#),
                (SESSIONS_SECRET_KEY, &encoded),
                (API_SERVER_SECRET_KEY, &server),
            ],
        );

        let imported = import_from_local_with_key(Some(&key)).unwrap();
        assert_eq!(
            imported.oauth_account_id.as_deref(),
            Some("status@wind.dev")
        );
        let state = WindsurfState::parse(imported.provider_state.as_deref().unwrap());
        assert_eq!(state.api_key.as_deref(), Some("sk-ws-from-secret-key"));
        assert_eq!(
            state.api_server_url.as_deref(),
            Some("https://api.example.test")
        );

        let wrong = KeyMaterial::macos_v10("other-password");
        let error = expect_err(import_from_local_with_key(Some(&wrong)));
        assert!(error.to_string().contains("解密"), "{error}");
    }

    #[test]
    fn encrypted_secret_without_a_key_does_not_touch_the_keychain() {
        let (_dir, _guard, db) = sandboxed_db();
        let key = KeyMaterial::macos_v10("injected-password");
        let encoded =
            encrypt_secret(&key, br#"[{"accessToken":"sk-ws-from-secret-key"}]"#).unwrap();
        write_items(&db, &[(SESSIONS_SECRET_KEY, &encoded)]);
        let error = expect_err(import_from_local());
        assert!(error.to_string().contains("不会读取系统钥匙串"), "{error}");
    }

    #[test]
    fn missing_database_is_an_error_inside_the_sandbox() {
        let (_dir, _guard, db) = sandboxed_db();
        assert!(!db.exists());
        let error = expect_err(import_from_local());
        assert!(error.to_string().contains("state.vscdb"), "{error}");
    }

    fn expect_err<T>(result: UsageResult<T>) -> UsageError {
        match result {
            Ok(_) => panic!("expected Windsurf import to fail"),
            Err(error) => error,
        }
    }

    #[test]
    fn oauth_row_encrypts_provider_state() {
        let imported = import_from_token("sk-ws-testkey12").unwrap();
        let row = oauth_row_from_imported(imported).unwrap();
        assert_eq!(row.catalog_id, "windsurf");
        assert_eq!(row.auth_mode, AuthMode::OAuth);
        assert!(row.access_token_encrypted.is_none());
        let cipher = row.provider_state_encrypted.expect("blob");
        let plain = crate::crypto::decrypt(&cipher);
        assert!(plain.contains("sk-ws-testkey12"));
        assert_ne!(cipher, plain);
    }
}
