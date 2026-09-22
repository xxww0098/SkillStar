//! Pasted userInfo JSON or token, and `state.vscdb` import.
//!
//! Plaintext keys (`aicoding.auth.*`) win over `secret://` rows. Ciphertext is
//! decrypted only with caller-supplied [`KeyMaterial`]; this path does not
//! touch the system keychain or the real home. `SKILLSTAR_TOOL_SYNC_HOME`
//! selects the database via [`crate::tool_paths::qoder_state_db_path`].

use std::path::Path;

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use serde_json::Value;

use super::http::DeviceGrant;
use super::login::{self, user_data_root};
use super::quota::Identity;
use super::{CATALOG_ID, QoderMachine, object_string, pick_string};
use crate::catalog::AuthMode;
use crate::subscription::{BillingCycle, Subscription};
use crate::token_import::ImportedToken;
use crate::tool_store::safe_storage::{self, KeyMaterial};
use crate::{UsageError, UsageResult};

const MIN_TOKEN_LEN: usize = 20;
const USER_INFO_KEYS: &[&str] = &["aicoding.auth.userInfo", "secret://aicoding.auth.userInfo"];
const USER_PLAN_KEYS: &[&str] = &["aicoding.auth.userPlan", "secret://aicoding.auth.userPlan"];
const CREDIT_KEYS: &[&str] = &[
    "aicoding.auth.creditUsage",
    "secret://aicoding.auth.creditUsage",
];
const TOKEN_KEYS: &[&str] = &["token", "securityOauthToken", "accessToken", "access_token"];

pub(crate) fn import_from_token(raw: &str) -> UsageResult<ImportedToken> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(UsageError::Other("Qoder 令牌为空".into()));
    }
    if trimmed.starts_with(['{', '[', '"']) {
        let value: Value = serde_json::from_str(trimmed)
            .map_err(|_| UsageError::Other("Qoder 凭据 JSON 无法解析".into()))?;
        return imported_from_value(&value);
    }
    bare_token(trimmed)
}

pub(crate) fn import_from_local() -> UsageResult<ImportedToken> {
    import_from_local_with_key(None)
}

pub(crate) fn import_from_local_with_key(key: Option<&KeyMaterial>) -> UsageResult<ImportedToken> {
    let path = crate::tool_paths::qoder_state_db_path()
        .ok_or_else(|| UsageError::Other("无法解析 Qoder 数据目录".into()))?;
    read_local_db(&path, key)
}

pub(crate) fn oauth_row_from_imported(imported: ImportedToken) -> UsageResult<Subscription> {
    if imported.access_token.trim().is_empty() {
        return Err(UsageError::Other("Qoder 导入没有可用令牌".into()));
    }
    let now = chrono::Utc::now().timestamp();
    let provider_state = imported
        .provider_state
        .filter(|value| !value.trim().is_empty());
    Ok(Subscription {
        id: uuid::Uuid::new_v4().to_string(),
        catalog_id: CATALOG_ID.to_string(),
        display_name: imported.display_name,
        auth_mode: AuthMode::OAuth,
        plan_tier: None,
        monthly_price: None,
        currency: imported.currency.unwrap_or_else(|| "USD".to_string()),
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

pub(super) fn from_grant(
    grant: DeviceGrant,
    machine: QoderMachine,
    identity: Identity,
) -> ImportedToken {
    token_row(
        grant.token,
        grant.refresh_token,
        grant.expires_at,
        identity.email,
        identity.user_id.or(grant.user_id),
        identity.name,
        machine,
    )
}

fn imported_from_value(value: &Value) -> UsageResult<ImportedToken> {
    match value {
        Value::String(text) => bare_token(text),
        Value::Array(items) => first_account(items),
        Value::Object(map) => {
            let nested = map.get("accounts").and_then(Value::as_array);
            let itself = access_token_from(value).is_some();
            if let Some(items) = nested.filter(|_| !itself) {
                return first_account(items);
            }
            imported_from_object(value)
        }
        _ => Err(UsageError::Other(
            "Qoder 凭据 JSON 须是对象、数组或令牌字符串".into(),
        )),
    }
}

fn first_account(items: &[Value]) -> UsageResult<ImportedToken> {
    let mut last = UsageError::Other("Qoder 凭据 JSON 缺少可用令牌".into());
    for item in items {
        match imported_from_object(item) {
            Ok(imported) => return Ok(imported),
            Err(err) => last = err,
        }
    }
    Err(last)
}

fn imported_from_object(value: &Value) -> UsageResult<ImportedToken> {
    let token = access_token_from(value)
        .ok_or_else(|| UsageError::Other("Qoder 凭据 JSON 缺少可用令牌".into()))?;
    let refresh = pick_string(value, &["refreshToken", "refresh_token"]);
    let email = pick_string(value, &["email", "mail"])
        .filter(|email| crate::fetchers::oauth::common::looks_like_email(email));
    let user_id = account_user_id(value, &token);
    let name = pick_string(value, &["name", "nickname", "displayName"]);
    let expires = pick_string(value, &["expireTime", "expiresAt", "expires_at"])
        .and_then(|text| super::http::parse_expiry(&text));
    let mut machine = QoderMachine::from_value(value);
    if let Some(raw) = value
        .get("auth_user_info_raw")
        .or_else(|| value.get("userInfo"))
    {
        machine.fill_missing(&QoderMachine::from_value(raw));
    }
    Ok(token_row(
        token, refresh, expires, email, user_id, name, machine,
    ))
}

fn bare_token(raw: &str) -> UsageResult<ImportedToken> {
    let token = raw.trim();
    if !looks_like_token(token) {
        return Err(UsageError::Other(
            "Qoder 令牌无法识别：请粘贴 userInfo JSON 或 token".into(),
        ));
    }
    Ok(token_row(
        token.to_string(),
        None,
        None,
        None,
        None,
        None,
        QoderMachine::default(),
    ))
}

fn token_row(
    access_token: String,
    refresh_token: Option<String>,
    expires_at: Option<i64>,
    email: Option<String>,
    user_id: Option<String>,
    name: Option<String>,
    machine: QoderMachine,
) -> ImportedToken {
    let display_name = email
        .clone()
        .filter(|value| crate::fetchers::oauth::common::looks_like_email(value))
        .or(name)
        .unwrap_or_else(|| "Qoder".to_string());
    ImportedToken {
        display_name,
        access_token,
        refresh_token,
        expires_at,
        oauth_account_id: user_id.filter(|id| !id.is_empty()),
        provider_state: machine.to_json(),
        currency: None,
        oauth_region: None,
        id_token: None,
        api_key: None,
    }
}

fn account_user_id(value: &Value, token: &str) -> Option<String> {
    if let Some(id) = pick_string(value, &["userId", "user_id", "uid"]) {
        return Some(id);
    }
    for key in [
        "userInfo",
        "auth_user_info_raw",
        "authUserInfo",
        "data",
        "result",
    ] {
        if let Some(child) = value.get(key)
            && let Some(id) = object_string(child, &["id"])
            && id != token
            && !id.contains('@')
        {
            return Some(id);
        }
    }
    object_string(value, &["id"]).filter(|id| id != token && !id.contains('@'))
}

fn access_token_from(value: &Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        return looks_like_token(text).then(|| text.trim().to_string());
    }
    let mut roots = vec![value];
    for key in [
        "userInfo",
        "auth_user_info_raw",
        "authUserInfo",
        "data",
        "result",
    ] {
        if let Some(child) = value.get(key) {
            roots.push(child);
        }
    }
    for root in roots {
        if let Some(token) = pick_string(root, TOKEN_KEYS).filter(|token| looks_like_token(token)) {
            return Some(token);
        }
    }
    None
}

fn looks_like_token(token: &str) -> bool {
    let token = token.trim();
    token.len() >= MIN_TOKEN_LEN
        && !token.contains(char::is_whitespace)
        && token.chars().any(|ch| ch.is_ascii_alphabetic())
}

fn read_local_db(path: &Path, key: Option<&KeyMaterial>) -> UsageResult<ImportedToken> {
    if !path.is_file() {
        return Err(UsageError::Other(format!(
            "未找到 Qoder state.vscdb：{}",
            path.display()
        )));
    }
    let user_info = read_field(path, USER_INFO_KEYS, key)?;
    let Some(user_info) = user_info else {
        return Err(UsageError::Other(
            "Qoder 未登录（state.vscdb 没有 aicoding.auth.userInfo）".into(),
        ));
    };
    let plan = read_field(path, USER_PLAN_KEYS, key)?;
    let usage = read_field(path, CREDIT_KEYS, key)?;
    let mut combined = serde_json::Map::new();
    combined.insert("userInfo".into(), user_info);
    if let Some(plan) = plan {
        combined.insert("userPlan".into(), plan);
    }
    if let Some(usage) = usage {
        combined.insert("creditUsage".into(), usage);
    }
    let mut imported = imported_from_object(&Value::Object(combined))?;
    let mut machine = imported
        .provider_state
        .as_deref()
        .map(QoderMachine::parse)
        .unwrap_or_default();
    machine.fill_missing(&login::load_machine_at(&user_data_root(path)).machine);
    imported.provider_state = machine.to_json();
    Ok(imported)
}

fn read_field(path: &Path, keys: &[&str], key: Option<&KeyMaterial>) -> UsageResult<Option<Value>> {
    let raws = crate::vscdb::read_item_strings(path, keys)?;
    for raw in raws.into_iter().flatten() {
        if raw.trim().is_empty() {
            continue;
        }
        let text = decode_stored(&raw, key)?;
        return Ok(Some(value_from_plain(&text)));
    }
    Ok(None)
}

fn decode_stored(raw: &str, key: Option<&KeyMaterial>) -> UsageResult<String> {
    let trimmed = raw.trim();
    if let Ok(parsed) = serde_json::from_str::<Value>(trimmed) {
        if is_buffer(&parsed) {
            return decrypt_encoded(&ciphertext_base64(trimmed), key);
        }
        if let Some(text) = parsed.as_str() {
            if looks_like_safe_storage(text) {
                return decrypt_encoded(text.trim(), key);
            }
            return Ok(text.to_string());
        }
        return Ok(trimmed.to_string());
    }
    if looks_like_safe_storage(trimmed) {
        return decrypt_encoded(trimmed, key);
    }
    Ok(trimmed.to_string())
}

fn decrypt_encoded(encoded: &str, key: Option<&KeyMaterial>) -> UsageResult<String> {
    let Some(key) = key else {
        return Err(missing_key_material());
    };
    let bytes = safe_storage::decrypt_secret(key, encoded)
        .map_err(|err| UsageError::Other(format!("Qoder secret:// 解密失败: {err}")))?;
    String::from_utf8(bytes).map_err(|_| UsageError::Other("Qoder secret:// 不是 UTF-8".into()))
}

fn missing_key_material() -> UsageError {
    UsageError::Other(
        "Qoder secret:// 已加密，但没有注入 Safe Storage 密钥（不会读取系统钥匙串）".into(),
    )
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

fn value_from_plain(text: &str) -> Value {
    let trimmed = text.trim();
    serde_json::from_str(trimmed).unwrap_or_else(|_| Value::String(trimmed.to_string()))
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
        "user-token-value-0123456789"
    }

    #[test]
    fn qoder_token_json_bare_token_and_garbage() {
        let user_info = import_from_token(&format!(
            r#"{{"userInfo":{{"token":"{}","id":"user-1","email":"ada@qoder.dev","name":"Ada","machineToken":"mt","machineId":"mid"}}}}"#,
            token()
        ))
        .unwrap();
        assert_eq!(user_info.access_token, token());
        assert_eq!(user_info.oauth_account_id.as_deref(), Some("user-1"));
        assert_eq!(user_info.display_name, "ada@qoder.dev");
        let state = QoderMachine::parse(user_info.provider_state.as_deref().unwrap());
        assert_eq!(state.machine_token.as_deref(), Some("mt"));
        assert_eq!(state.machine_id.as_deref(), Some("mid"));

        let account = import_from_token(&format!(
            r#"{{"id":"qoder_uid_user-1","email":"ada@qoder.dev","user_id":"user-1","auth_user_info_raw":{{"token":"{}","refreshToken":"refresh-token-value-0123456789"}},"created_at":1,"last_used":1}}"#,
            token()
        ))
        .unwrap();
        assert_eq!(account.access_token, token());
        assert_eq!(account.oauth_account_id.as_deref(), Some("user-1"));
        assert_eq!(
            account.refresh_token.as_deref(),
            Some("refresh-token-value-0123456789")
        );

        let wrapped = import_from_token(&format!(
            r#"{{"accounts":[{{"userInfo":{{"token":"{}"}}}}]}}"#,
            token()
        ))
        .unwrap();
        assert_eq!(wrapped.access_token, token());
        assert_eq!(wrapped.display_name, "Qoder");

        let bare = import_from_token(token()).unwrap();
        assert_eq!(bare.access_token, token());
        assert!(bare.provider_state.is_none());

        let quoted = import_from_token(&format!("\"{}\"", token())).unwrap();
        assert_eq!(quoted.access_token, token());

        for raw in [
            "",
            "   ",
            "hello",
            "not-a-token",
            "{",
            "[]",
            "{}",
            "null",
            "12345678901234567890",
            r#"{"note":"nope"}"#,
            r#"{"token":"short"}"#,
        ] {
            assert!(import_from_token(raw).is_err(), "{raw}");
        }
    }

    #[test]
    fn qoder_local_plaintext_beats_secret_and_decrypts_injected_password() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = EnvGuard::sandbox(dir.path());
        let db = crate::tool_paths::qoder_state_db_path().expect("db");
        let secret = {
            let key = KeyMaterial::macos_v10("injected-password");
            let body = format!(
                r#"{{"token":"{}","id":"secret-user","email":"secret@qoder.dev"}}"#,
                token()
            );
            encrypt_secret(&key, body.as_bytes()).unwrap()
        };
        write_items(
            &db,
            &[
                (
                    "aicoding.auth.userInfo",
                    &format!(
                        r#"{{"token":"{}","id":"plain-user","email":"plain@qoder.dev","machineToken":"from-user"}}"#,
                        token()
                    ),
                ),
                ("secret://aicoding.auth.userInfo", &secret),
                ("aicoding.auth.userPlan", r#"{"plan":"Pro Plus"}"#),
            ],
        );
        let root = user_data_root(&db);
        let cache = root.join("SharedClientCache").join("cache");
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(
            cache.join("machine_token.json"),
            r#"{"token":"from-cache","id":"cache-id","hostname":"box","os":"aarch64_darwin","version":"1.2.3"}"#,
        )
        .unwrap();

        let imported = import_from_local().unwrap();
        assert_eq!(imported.oauth_account_id.as_deref(), Some("plain-user"));
        assert_eq!(imported.display_name, "plain@qoder.dev");
        let state = QoderMachine::parse(imported.provider_state.as_deref().unwrap());
        assert_eq!(state.machine_token.as_deref(), Some("from-user"));
        assert_eq!(state.machine_id.as_deref(), Some("cache-id"));
        assert_eq!(state.hostname.as_deref(), Some("box"));
        assert_eq!(state.cosy_version.as_deref(), Some("1.2.3"));

        let row = oauth_row_from_imported(imported).unwrap();
        assert_eq!(row.catalog_id, "qoder");
        assert_eq!(row.auth_mode, AuthMode::OAuth);
        assert!(row.platform_token_encrypted.is_none());
        let plain = crate::crypto::decrypt(row.provider_state_encrypted.as_deref().unwrap());
        assert!(plain.contains("from-user"));
        assert_ne!(plain, row.provider_state_encrypted.unwrap());
        let access = crate::crypto::decrypt(row.access_token_encrypted.as_deref().unwrap());
        assert_eq!(access, token());
    }

    #[test]
    fn qoder_secret_round_trip_uses_the_injected_password_only() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = EnvGuard::sandbox(dir.path());
        let db = crate::tool_paths::qoder_state_db_path().expect("db");
        let key = KeyMaterial::macos_v10("injected-password");
        let body = format!(
            r#"{{"token":"{}","id":"secret-user","email":"secret@qoder.dev"}}"#,
            token()
        );
        let encoded = encrypt_secret(&key, body.as_bytes()).unwrap();
        write_items(&db, &[("secret://aicoding.auth.userInfo", &encoded)]);

        let missing = expect_err(import_from_local());
        assert!(
            missing.to_string().contains("不会读取系统钥匙串"),
            "{missing}"
        );

        let imported = import_from_local_with_key(Some(&key)).unwrap();
        assert_eq!(imported.oauth_account_id.as_deref(), Some("secret-user"));
        assert_eq!(imported.display_name, "secret@qoder.dev");

        let wrong = KeyMaterial::macos_v10("other-password");
        let err = expect_err(import_from_local_with_key(Some(&wrong)));
        assert!(err.to_string().contains("解密"), "{err}");
    }

    #[test]
    fn qoder_local_import_reads_the_alternate_state_db_candidate() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = EnvGuard::sandbox(dir.path());
        let preferred = crate::tool_paths::qoder_state_db_path().expect("preferred");
        assert!(!preferred.exists());
        let root = user_data_root(&preferred);
        let alternate = root.join("globalStorage").join("state.vscdb");
        write_items(
            &alternate,
            &[(
                "secret://aicoding.auth.userInfo",
                &format!(
                    r#"{{"securityOauthToken":"{}","userId":"alt-user","email":"alt@qoder.dev"}}"#,
                    token()
                ),
            )],
        );
        let resolved = crate::tool_paths::qoder_state_db_path().expect("resolved");
        assert_eq!(resolved, alternate);
        let imported = import_from_local().unwrap();
        assert_eq!(imported.access_token, token());
        assert_eq!(imported.oauth_account_id.as_deref(), Some("alt-user"));
        assert_eq!(imported.display_name, "alt@qoder.dev");
    }

    #[test]
    fn qoder_missing_database_names_state_vscdb() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = EnvGuard::sandbox(dir.path());
        let err = expect_err(import_from_local());
        assert!(err.to_string().contains("state.vscdb"), "{err}");
    }

    fn expect_err<T>(result: UsageResult<T>) -> UsageError {
        match result {
            Ok(_) => panic!("expected Qoder import to fail"),
            Err(error) => error,
        }
    }
}
