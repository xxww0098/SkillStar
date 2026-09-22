//! Pasted token JSON and `state.vscdb` import.
//!
//! `secret://` values are decrypted only with caller-supplied [`KeyMaterial`].
//! This path does not read the system keychain or the real home.
//! `SKILLSTAR_TOOL_SYNC_HOME` selects the database.

use std::path::Path;

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use serde_json::Value;

use super::http::split_uid_token;
use super::{CN, Enterprise, GLOBAL, Host, LoginIdentity, json_i64, nonempty, pick_string};
use crate::catalog::AuthMode;
use crate::subscription::{BillingCycle, Subscription};
use crate::token_import::ImportedToken;
use crate::tool_store::safe_storage::{self, KeyMaterial};
use crate::{UsageError, UsageResult};

const MIN_BARE_LEN: usize = 20;

pub(crate) fn import_from_token(raw: &str) -> UsageResult<ImportedToken> {
    parse_token(&GLOBAL, raw)
}

pub(crate) fn import_from_token_cn(raw: &str) -> UsageResult<ImportedToken> {
    parse_token(&CN, raw)
}

pub(crate) fn import_from_local() -> UsageResult<ImportedToken> {
    read_local(&GLOBAL, None)
}

pub(crate) fn import_from_local_cn() -> UsageResult<ImportedToken> {
    read_local(&CN, None)
}

pub(crate) fn oauth_row_from_imported(imported: ImportedToken) -> UsageResult<Subscription> {
    oauth_row(&GLOBAL, imported)
}

pub(crate) fn oauth_row_from_imported_cn(imported: ImportedToken) -> UsageResult<Subscription> {
    oauth_row(&CN, imported)
}

pub(super) fn imported_from_login(host: &Host, identity: &LoginIdentity) -> ImportedToken {
    token_row(
        host,
        identity.access_token.clone(),
        identity.refresh_token.clone(),
        identity.expires_at,
        identity.uid.clone(),
        identity.email.clone(),
        identity.nickname.clone(),
        identity.enterprise.clone(),
    )
}

pub(super) fn oauth_row(host: &Host, imported: ImportedToken) -> UsageResult<Subscription> {
    if imported.access_token.trim().is_empty() {
        return Err(UsageError::Other(format!(
            "{} 导入没有可用令牌",
            host.display_name
        )));
    }
    let now = chrono::Utc::now().timestamp();
    let currency = imported.currency.unwrap_or_else(|| {
        crate::catalog::find(host.catalog_id)
            .map(|entry| entry.default_currency.to_string())
            .unwrap_or_else(|| "USD".to_string())
    });
    let provider_state = imported
        .provider_state
        .filter(|value| !value.trim().is_empty());
    Ok(Subscription {
        id: uuid::Uuid::new_v4().to_string(),
        catalog_id: host.catalog_id.to_string(),
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
        access_token_encrypted: Some(crate::crypto::encrypt(&imported.access_token)),
        refresh_token_encrypted: imported
            .refresh_token
            .filter(|value| !value.trim().is_empty())
            .map(|value| crate::crypto::encrypt(&value)),
        access_token_expires_at: imported.expires_at,
        id_token_encrypted: None,
        oauth_account_id: imported.oauth_account_id,
        oauth_region: Some(host.oauth_region.to_string()),
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

pub(super) fn parse_token(host: &Host, raw: &str) -> UsageResult<ImportedToken> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(UsageError::Other(format!("{} 令牌为空", host.display_name)));
    }
    if trimmed.starts_with(['{', '[', '"']) {
        let value: Value = serde_json::from_str(trimmed)
            .map_err(|_| UsageError::Other(format!("{} 凭据 JSON 无法解析", host.display_name)))?;
        return imported_from_value(host, &value);
    }
    imported_from_raw_token(
        host,
        trimmed,
        Some(MIN_BARE_LEN),
        Enterprise::default(),
        None,
        None,
    )
}

fn read_local(host: &Host, key: Option<&KeyMaterial>) -> UsageResult<ImportedToken> {
    let path = (host.state_db)()
        .ok_or_else(|| UsageError::Other(format!("无法解析 {} 数据目录", host.display_name)))?;
    if !path.is_file() {
        return Err(UsageError::Other(format!(
            "未找到 {} state.vscdb：{}",
            host.display_name,
            path.display()
        )));
    }
    let raw = crate::vscdb::read_item_string(&path, &host.secret_item_key())?;
    let Some(raw) = raw.filter(|value| !value.trim().is_empty()) else {
        return Err(UsageError::Other(format!(
            "{} 未登录（state.vscdb 没有 access token）",
            host.display_name
        )));
    };
    let plain = decode_stored(host, &raw, key)?;
    parse_stored_secret(host, &plain)
}

fn parse_stored_secret(host: &Host, plain: &str) -> UsageResult<ImportedToken> {
    if let Ok(value) = serde_json::from_str::<Value>(plain.trim()) {
        return match value {
            Value::String(text) => {
                imported_from_raw_token(host, &text, None, Enterprise::default(), None, None)
            }
            other => imported_from_value(host, &other),
        };
    }
    imported_from_raw_token(host, plain, None, Enterprise::default(), None, None)
}

fn imported_from_value(host: &Host, value: &Value) -> UsageResult<ImportedToken> {
    match value {
        Value::Array(items) => first_account(host, items),
        Value::String(text) => imported_from_raw_token(
            host,
            text,
            Some(MIN_BARE_LEN),
            Enterprise::default(),
            None,
            None,
        ),
        Value::Object(_) if access_token_from(value).is_some() => imported_from_object(host, value),
        Value::Object(map) => {
            if let Some(list) = map
                .get("accounts")
                .or_else(|| map.get("items"))
                .and_then(Value::as_array)
            {
                return first_account(host, list);
            }
            Err(UsageError::Other(format!(
                "{} 凭据 JSON 没有 access token",
                host.display_name
            )))
        }
        _ => Err(UsageError::Other(format!(
            "{} 凭据 JSON 没有 access token",
            host.display_name
        ))),
    }
}

fn first_account(host: &Host, items: &[Value]) -> UsageResult<ImportedToken> {
    if items.is_empty() {
        return Err(UsageError::Other(format!(
            "{} 导入数组为空",
            host.display_name
        )));
    }
    let mut last = None;
    for item in items {
        match imported_from_value(host, item) {
            Ok(token) => return Ok(token),
            Err(err) => last = Some(err),
        }
    }
    Err(last.unwrap_or_else(|| {
        UsageError::Other(format!("{} 凭据 JSON 没有 access token", host.display_name))
    }))
}

fn imported_from_object(host: &Host, value: &Value) -> UsageResult<ImportedToken> {
    let raw = access_token_from(value).ok_or_else(|| {
        UsageError::Other(format!("{} 凭据 JSON 没有 access token", host.display_name))
    })?;
    let enterprise = Enterprise::from_value(value);
    let email = pick_string(value, &["email"]);
    let nickname = pick_string(value, &["nickname", "name"]);
    let refresh = pick_string(value, &["refreshToken", "refresh_token"]);
    let expires = pick_string(value, &["expiresAt", "expires_at"])
        .and_then(|text| text.parse::<i64>().ok())
        .or_else(|| json_i64(value.get("expiresAt").or_else(|| value.get("expires_at"))))
        .and_then(super::http::normalize_epoch_seconds);
    let uid = pick_string(value, &["uid", "userId", "user_id"]);
    imported_from_raw_token(host, &raw, None, enterprise, email, nickname).map(|mut token| {
        if token.oauth_account_id.is_none() {
            token.oauth_account_id = uid;
        }
        if token.refresh_token.is_none() {
            token.refresh_token = refresh;
        }
        if token.expires_at.is_none() {
            token.expires_at = expires;
        }
        if token.provider_state.is_none() {
            token.provider_state = enterprise_state(&Enterprise::from_value(value));
        }
        token
    })
}

fn imported_from_raw_token(
    host: &Host,
    raw: &str,
    min_len: Option<usize>,
    enterprise: Enterprise,
    email: Option<String>,
    nickname: Option<String>,
) -> UsageResult<ImportedToken> {
    let (split_uid, token) = split_uid_token(raw);
    if token.is_empty() || token.chars().any(char::is_whitespace) {
        return Err(UsageError::Other(format!(
            "{} 令牌无法识别",
            host.display_name
        )));
    }
    if let Some(min_len) = min_len
        && (token.chars().count() < min_len || !token.chars().any(|ch| ch.is_ascii_alphabetic()))
    {
        return Err(UsageError::Other(format!(
            "{} 令牌无法识别",
            host.display_name
        )));
    }
    Ok(token_row(
        host, token, None, None, split_uid, email, nickname, enterprise,
    ))
}

fn token_row(
    host: &Host,
    access_token: String,
    refresh_token: Option<String>,
    expires_at: Option<i64>,
    uid: Option<String>,
    email: Option<String>,
    nickname: Option<String>,
    enterprise: Enterprise,
) -> ImportedToken {
    ImportedToken {
        display_name: display_name(host, email.as_deref(), nickname.as_deref(), uid.as_deref()),
        access_token,
        refresh_token,
        expires_at,
        oauth_account_id: uid,
        provider_state: enterprise_state(&enterprise),
        currency: None,
        oauth_region: Some(host.oauth_region.to_string()),
    }
}

fn enterprise_state(enterprise: &Enterprise) -> Option<String> {
    enterprise.to_json()
}

fn display_name(
    host: &Host,
    email: Option<&str>,
    nickname: Option<&str>,
    uid: Option<&str>,
) -> String {
    nonempty(email)
        .or_else(|| nonempty(nickname))
        .or_else(|| nonempty(uid))
        .unwrap_or_else(|| host.display_name.to_string())
}

fn access_token_from(value: &Value) -> Option<String> {
    pick_string(value, &["accessToken", "access_token", "token"])
}

fn decode_stored(host: &Host, raw: &str, key: Option<&KeyMaterial>) -> UsageResult<String> {
    let trimmed = raw.trim();
    if let Ok(parsed) = serde_json::from_str::<Value>(trimmed) {
        if is_buffer(&parsed) {
            return decrypt_encoded(host, &ciphertext_base64(trimmed), key);
        }
        if let Some(text) = parsed.as_str() {
            if looks_like_safe_storage(text) {
                return decrypt_encoded(host, text.trim(), key);
            }
            return Ok(text.to_string());
        }
        return Ok(trimmed.to_string());
    }
    if looks_like_safe_storage(trimmed) {
        return decrypt_encoded(host, trimmed, key);
    }
    Ok(trimmed.to_string())
}

fn decrypt_encoded(host: &Host, encoded: &str, key: Option<&KeyMaterial>) -> UsageResult<String> {
    let Some(key) = key else {
        return Err(missing_key(host));
    };
    let bytes = safe_storage::decrypt_secret(key, encoded).map_err(|err| {
        UsageError::Other(format!("{} secret:// 解密失败: {err}", host.display_name))
    })?;
    String::from_utf8(bytes)
        .map_err(|_| UsageError::Other(format!("{} secret:// 不是 UTF-8", host.display_name)))
}

fn missing_key(host: &Host) -> UsageError {
    UsageError::Other(format!(
        "{} secret:// 已加密，但没有注入 Safe Storage 密钥（不会读取系统钥匙串）",
        host.display_name
    ))
}

fn is_buffer(value: &Value) -> bool {
    value.get("type").and_then(Value::as_str) == Some("Buffer") && value.get("data").is_some()
}

fn looks_like_safe_storage(encoded: &str) -> bool {
    let Ok(raw) = BASE64.decode(encoded.trim()) else {
        return false;
    };
    raw.starts_with(b"v10") || raw.starts_with(b"v11")
}

fn ciphertext_base64(stored: &str) -> String {
    let Ok(value) = serde_json::from_str::<Value>(stored.trim()) else {
        return stored.trim().to_string();
    };
    if !is_buffer(&value) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool_store::safe_storage::encrypt_secret;

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

    fn token() -> &'static str {
        "access-token-value-0123456789"
    }

    fn must_err<T>(result: UsageResult<T>) -> UsageError {
        match result {
            Ok(_) => panic!("expected CodeBuddy import to fail"),
            Err(error) => error,
        }
    }

    #[test]
    fn token_json_bare_token_uid_prefix_and_garbage() {
        let account = parse_token(
            &GLOBAL,
            &format!(
                r#"{{"access_token":"{}","refresh_token":"refresh-token-value-0123456789","uid":"user-1","email":"ada@example.com","enterprise_id":"ent","enterprise_name":"Acme","domain":"team.example","expires_at":1793368047}}"#,
                token()
            ),
        )
        .unwrap();
        assert_eq!(account.access_token, token());
        assert_eq!(account.oauth_account_id.as_deref(), Some("user-1"));
        assert_eq!(account.display_name, "ada@example.com");
        assert_eq!(
            account.refresh_token.as_deref(),
            Some("refresh-token-value-0123456789")
        );
        assert_eq!(account.expires_at, Some(1_793_368_047));
        assert_eq!(account.oauth_region.as_deref(), Some("global"));
        let state = Enterprise::parse(account.provider_state.as_deref().unwrap());
        assert_eq!(state.enterprise_id.as_deref(), Some("ent"));
        assert_eq!(state.domain.as_deref(), Some("team.example"));

        let wrapped = parse_token(
            &CN,
            &format!(
                r#"{{"accounts":[{{"accessToken":"{}","nickname":"Ada"}}]}}"#,
                token()
            ),
        )
        .unwrap();
        assert_eq!(wrapped.oauth_region.as_deref(), Some("cn"));
        assert_eq!(wrapped.display_name, "Ada");

        let prefixed = parse_token(&GLOBAL, &format!("uid-9+{}", token())).unwrap();
        assert_eq!(prefixed.access_token, token());
        assert_eq!(prefixed.oauth_account_id.as_deref(), Some("uid-9"));

        let bare = parse_token(&GLOBAL, token()).unwrap();
        assert_eq!(bare.access_token, token());
        assert!(bare.provider_state.is_none());

        for raw in [
            "",
            "   ",
            "hello",
            "{",
            "[]",
            "{}",
            "null",
            "12345678901234567890",
            r#"{"note":"nope"}"#,
        ] {
            assert!(parse_token(&GLOBAL, raw).is_err(), "{raw}");
        }
    }

    #[test]
    fn local_secret_uses_the_injected_key_and_the_host_path() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = EnvGuard::sandbox(dir.path());
        let global_db = crate::tool_paths::codebuddy_state_db_path().expect("global");
        let cn_db = crate::tool_paths::codebuddy_cn_state_db_path().expect("cn");
        assert_ne!(global_db, cn_db);
        assert!(global_db.starts_with(dir.path()), "{global_db:?}");
        assert!(cn_db.starts_with(dir.path()), "{cn_db:?}");
        assert!(
            global_db
                .components()
                .any(|part| part.as_os_str() == "CodeBuddy")
        );
        assert!(
            cn_db
                .components()
                .any(|part| part.as_os_str() == "CodeBuddy CN")
        );
        assert!(global_db.ends_with("state.vscdb"));

        let key = KeyMaterial::macos_v10("injected-password");
        let body = format!(
            r#"{{"accessToken":"uid-2+{}","email":"secret@example.com","enterpriseId":"ent-2","domain":"secret.example"}}"#,
            token()
        );
        let encoded = encrypt_secret(&key, body.as_bytes()).unwrap();
        write_items(&global_db, &[(&GLOBAL.secret_item_key(), encoded.as_str())]);

        let missing = must_err(read_local(&GLOBAL, None));
        assert!(
            missing.to_string().contains("不会读取系统钥匙串"),
            "{missing}"
        );

        let imported = read_local(&GLOBAL, Some(&key)).unwrap();
        assert_eq!(imported.access_token, token());
        assert_eq!(imported.oauth_account_id.as_deref(), Some("uid-2"));
        assert_eq!(imported.display_name, "secret@example.com");
        assert_eq!(imported.oauth_region.as_deref(), Some("global"));
        let state = Enterprise::parse(imported.provider_state.as_deref().unwrap());
        assert_eq!(state.enterprise_id.as_deref(), Some("ent-2"));
        assert_eq!(state.domain.as_deref(), Some("secret.example"));

        let row = oauth_row(&GLOBAL, imported).unwrap();
        assert_eq!(row.catalog_id, "codebuddy");
        assert_eq!(row.oauth_region.as_deref(), Some("global"));
        assert!(row.platform_token_encrypted.is_none());
        assert_eq!(
            super::super::decrypt_optional(&row.access_token_encrypted).as_deref(),
            Some(token())
        );
        let stored = super::super::decrypt_optional(&row.provider_state_encrypted).unwrap();
        assert!(stored.contains("ent-2"));
        assert_ne!(stored, row.provider_state_encrypted.clone().unwrap());

        let wrong = KeyMaterial::macos_v10("other-password");
        let err = must_err(read_local(&GLOBAL, Some(&wrong)));
        assert!(err.to_string().contains("解密"), "{err}");

        let cn_body = format!(
            r#"{{"token":"{}","uid":"cn-user","email":"cn@example.com"}}"#,
            token()
        );
        let cn_encoded = encrypt_secret(&key, cn_body.as_bytes()).unwrap();
        write_items(&cn_db, &[(&CN.secret_item_key(), cn_encoded.as_str())]);
        let cn = read_local(&CN, Some(&key)).unwrap();
        assert_eq!(cn.oauth_account_id.as_deref(), Some("cn-user"));
        let cn_row = oauth_row(&CN, cn).unwrap();
        assert_eq!(cn_row.catalog_id, "codebuddy-cn");
        assert_eq!(cn_row.oauth_region.as_deref(), Some("cn"));
        assert!(read_local(&CN, Some(&key)).is_ok());
    }

    #[test]
    fn missing_database_names_state_vscdb_and_does_not_touch_home() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = EnvGuard::sandbox(dir.path());
        let err = must_err(read_local(&GLOBAL, None));
        assert!(err.to_string().contains("state.vscdb"), "{err}");
        assert!(err.to_string().contains("CodeBuddy"), "{err}");
    }
}
