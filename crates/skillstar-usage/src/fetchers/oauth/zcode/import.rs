//! Pasted ZCode account JSON, and `credentials.json` decrypted with `enc:v1`.
//!
//! Cockpit accepts one account object, `{accounts:[...]}`, or an array. This
//! importer keeps a single account and rejects the rest. API keys go to
//! `api_key_encrypted` with `provider_state.kind = api_key`. OAuth tokens keep
//! the provider access token, refresh token, and zcode JWT (`id_token`).
//!
//! The credential key is the OS home (or `SKILLSTAR_TOOL_SYNC_HOME` in tests),
//! not `{dataBaseDir}/.zcode`. The file path still follows `zcode_home()`.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use super::http::pick_string;
use super::{CATALOG_ID, PLACEHOLDER_NAME, nonempty, normalize_provider, provider_state_json};
use crate::fetchers::oauth::common::SubscriptionBuilder;
use crate::subscription::Subscription;
use crate::token_import::ImportedToken;
use crate::tool_store::enc_v1::{decrypt_enc_v1, zcode_credential_key, zcode_credentials_path};
use crate::{UsageError, UsageResult};

const ACTIVE_PROVIDER: &str = "oauth:active_provider";
const JWT_KEY: &str = "zcodejwttoken";

pub(crate) fn import_from_token(payload: &str) -> UsageResult<ImportedToken> {
    let trimmed = payload.trim();
    if !trimmed.starts_with('{') && !trimmed.starts_with('[') {
        return Err(UsageError::Other("ZCode 令牌导入需要 JSON".into()));
    }
    let root: Value = serde_json::from_str(trimmed)
        .map_err(|_| UsageError::Other("ZCode 凭据 JSON 无法解析".into()))?;
    let account = single_account(&root)?;
    account_from_json(account)
}

pub(crate) fn import_from_local() -> UsageResult<ImportedToken> {
    let path = zcode_credentials_path();
    if !path.is_file() {
        return Err(UsageError::Other(format!(
            "未找到 ZCode 凭据：{}",
            path.display()
        )));
    }
    let text = std::fs::read_to_string(&path)
        .map_err(|error| UsageError::Other(format!("读取 ZCode 凭据失败: {error}")))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|_| UsageError::Other("ZCode credentials.json 无法解析".into()))?;
    let object = value
        .as_object()
        .ok_or_else(|| UsageError::Other("ZCode credentials.json 必须是对象".into()))?;
    account_from_credentials(object, &local_credential_key())
}

pub(crate) fn oauth_row_from_imported(imported: ImportedToken) -> UsageResult<Subscription> {
    if imported.access_token.trim().is_empty() {
        return Err(UsageError::Other("ZCode 导入缺少 access_token".into()));
    }
    let jwt = imported
        .id_token
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| UsageError::Other("ZCode 导入缺少 zcode JWT".into()))?;
    let region = imported
        .oauth_region
        .clone()
        .ok_or_else(|| UsageError::Other("ZCode 导入缺少上游".into()))?;
    normalize_provider(Some(&region))?;
    Ok(SubscriptionBuilder::new(
        CATALOG_ID,
        imported.display_name,
        "USD",
        imported.access_token,
        imported.expires_at,
    )
    .refresh_token(imported.refresh_token)
    .id_token(Some(jwt))
    .oauth_account_id(imported.oauth_account_id)
    .oauth_region(Some(region))
    .provider_state(
        imported
            .provider_state
            .unwrap_or_else(|| provider_state_json("oauth")),
    )
    .build())
}

pub(super) fn account_from_credentials(
    values: &Map<String, Value>,
    key: &[u8; 32],
) -> UsageResult<ImportedToken> {
    let provider = decrypt_field(values, ACTIVE_PROVIDER, key)?
        .ok_or_else(|| UsageError::Other("ZCode 本地凭据缺少 active provider".into()))?;
    let provider = normalize_provider(Some(&provider))?.to_string();
    let access = decrypt_field(values, &format!("oauth:{provider}:access_token"), key)?
        .ok_or_else(|| UsageError::Other("ZCode 本地凭据缺少 access token".into()))?;
    let refresh = decrypt_field(values, &format!("oauth:{provider}:refresh_token"), key)?;
    let jwt = decrypt_field(values, JWT_KEY, key)?
        .ok_or_else(|| UsageError::Other("ZCode 本地凭据缺少 zcode JWT".into()))?;
    let user_info = match decrypt_field(values, &format!("oauth:{provider}:user_info"), key)? {
        Some(text) => serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json_empty()),
        None => json_empty(),
    };
    Ok(oauth_imported(
        &provider, access, refresh, jwt, None, &user_info,
    ))
}

fn account_from_json(value: &Value) -> UsageResult<ImportedToken> {
    let provider = string_field(value, &["provider"])
        .ok_or_else(|| UsageError::Other("ZCode 导入缺少 provider".into()))?;
    let provider = normalize_provider(Some(&provider))?;
    match auth_mode(value)? {
        AuthKind::ApiKey => api_key_imported(provider, value),
        AuthKind::Oauth => {
            let access =
                string_field(value, &["access_token", "accessToken"]).ok_or_else(|| {
                    UsageError::Other("ZCode 导入的 OAuth 账号缺少 access_token".into())
                })?;
            let jwt = string_field(
                value,
                &["zcode_jwt_token", "zcodeJwtToken", "jwt", "id_token"],
            )
            .ok_or_else(|| UsageError::Other("ZCode 导入的 OAuth 账号缺少 zcode JWT".into()))?;
            let refresh = string_field(value, &["refresh_token", "refreshToken"]);
            Ok(oauth_imported(
                provider,
                access,
                refresh,
                jwt,
                expires_at(value),
                value,
            ))
        }
    }
}

fn oauth_imported(
    provider: &str,
    access_token: String,
    refresh_token: Option<String>,
    jwt: String,
    expires_at: Option<i64>,
    user_info: &Value,
) -> ImportedToken {
    let user_id = pick_string(
        user_info,
        &[&["user_id"], &["id"], &["customerNumber"], &["sub"]],
    );
    let email = pick_string(user_info, &[&["email"]]);
    let name = string_field(user_info, &["display_name", "displayName"]).or_else(|| {
        pick_string(
            user_info,
            &[
                &["name"],
                &["displayName"],
                &["username"],
                &["nickName"],
                &["customerName"],
            ],
        )
    });
    ImportedToken {
        display_name: email
            .clone()
            .filter(|value| crate::fetchers::oauth::common::looks_like_email(value))
            .or(name)
            .or(user_id.clone())
            .unwrap_or_else(|| PLACEHOLDER_NAME.to_string()),
        access_token,
        refresh_token,
        expires_at,
        oauth_account_id: user_id,
        provider_state: Some(provider_state_json("oauth")),
        currency: None,
        oauth_region: Some(provider.to_string()),
        id_token: nonempty(Some(jwt.as_str())),
        api_key: None,
    }
}

fn api_key_imported(provider: &str, value: &Value) -> UsageResult<ImportedToken> {
    let api_key = string_field(value, &["api_key", "apiKey"])
        .ok_or_else(|| UsageError::Other("ZCode 导入的 API Key 账号缺少 API Key".into()))?;
    if api_key.chars().any(char::is_whitespace) {
        return Err(UsageError::Other("ZCode API Key 不能包含空白字符".into()));
    }
    let explicit = string_field(value, &["display_name", "displayName"]);
    Ok(ImportedToken {
        display_name: explicit.unwrap_or_else(|| api_key_title(provider, &api_key)),
        access_token: String::new(),
        refresh_token: None,
        expires_at: None,
        oauth_account_id: None,
        provider_state: Some(provider_state_json("api_key")),
        currency: None,
        oauth_region: Some(provider.to_string()),
        id_token: None,
        api_key: Some(api_key),
    })
}

fn api_key_title(provider: &str, api_key: &str) -> String {
    let suffix: String = api_key
        .chars()
        .rev()
        .take(4)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    let label = if provider == "bigmodel" {
        "BigModel"
    } else {
        "Z.ai"
    };
    format!("{label} API Key ...{suffix}")
}

fn single_account(root: &Value) -> UsageResult<&Value> {
    let items: Vec<&Value> = match root {
        Value::Array(items) => items.iter().collect(),
        Value::Object(object) => object
            .get("accounts")
            .and_then(Value::as_array)
            .map(|items| items.iter().collect())
            .unwrap_or_else(|| vec![root]),
        _ => return Err(UsageError::Other("ZCode 导入数据必须是对象或数组".into())),
    };
    match items.as_slice() {
        [one] => Ok(*one),
        [] => Err(UsageError::Other("ZCode 导入需要一个账号".into())),
        _ => Err(UsageError::Other("一次只能导入一个 ZCode 账号".into())),
    }
}

enum AuthKind {
    Oauth,
    ApiKey,
}

fn auth_mode(value: &Value) -> UsageResult<AuthKind> {
    let Some(raw) = string_field(value, &["auth_mode", "authMode"]) else {
        return Ok(AuthKind::Oauth);
    };
    match raw.to_ascii_lowercase().as_str() {
        "oauth" | "oauth2" => Ok(AuthKind::Oauth),
        "api_key" | "apikey" | "api-key" => Ok(AuthKind::ApiKey),
        _ => Err(UsageError::Other("ZCode 导入的 auth_mode 无法识别".into())),
    }
}

fn string_field(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value
            .get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    })
}

fn expires_at(value: &Value) -> Option<i64> {
    let raw = value.get("expires_at").or_else(|| value.get("expiresAt"))?;
    match raw {
        Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_f64().map(|value| value as i64)),
        Value::String(text) => text.trim().parse::<i64>().ok(),
        _ => None,
    }
    .filter(|value| *value > 0)
}

fn decrypt_field(
    values: &Map<String, Value>,
    name: &str,
    key: &[u8; 32],
) -> UsageResult<Option<String>> {
    let Some(raw) = values.get(name).and_then(Value::as_str) else {
        return Ok(None);
    };
    decrypt_enc_v1(key, raw)
        .map(Some)
        .map_err(|error| UsageError::Other(error.to_string()))
}

fn json_empty() -> Value {
    Value::Object(Map::new())
}

/// Cockpit hashes the OS user home. Under `SKILLSTAR_TOOL_SYNC_HOME` that
/// variable is the home, so tests never bake the developer profile into the
/// key and never open the real `~/.zcode`. `dataBaseDir` is not part of the key.
pub(super) fn local_credential_key() -> [u8; 32] {
    zcode_credential_key(&credential_key_home(), &os_username())
}

fn credential_key_home() -> PathBuf {
    if crate::tool_paths::is_tool_sync_sandboxed() {
        return std::env::var_os("SKILLSTAR_TOOL_SYNC_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(skillstar_core::infra::paths::home_dir);
    }
    skillstar_core::infra::paths::home_dir()
}

fn os_username() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::AuthMode;
    use crate::tool_store::enc_v1::{decrypt_enc_v1, encrypt_enc_v1};
    use serde_json::json;

    const FIXTURE: &str =
        "enc:v1:AAECAwQFBgcICQoL.NTIF8rgqI66J7hvPIwTD8g.QTtgwDlfAEvz72ttQggYC2KZyVwLVA";
    const DARWIN_KEY: &str = "3ead18a3d8ab40c108ab7c53e698ef355b73fc2386174dff684c6e50f34cb80a";

    fn hex_key(hex: &str) -> [u8; 32] {
        let mut key = [0u8; 32];
        for (index, byte) in key.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).unwrap();
        }
        key
    }

    struct EnvGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
        tool_sync: Option<std::ffi::OsString>,
        secret: Option<std::ffi::OsString>,
    }

    impl EnvGuard {
        fn sandbox(path: &Path) -> Self {
            let lock = crate::test_env_lock()
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let tool_sync = std::env::var_os("SKILLSTAR_TOOL_SYNC_HOME");
            let secret = std::env::var_os("ZCODE_CREDENTIAL_SECRET");
            // SAFETY: this test holds test_env_lock until drop.
            unsafe {
                std::env::set_var("SKILLSTAR_TOOL_SYNC_HOME", path);
                std::env::remove_var("ZCODE_CREDENTIAL_SECRET");
            }
            Self {
                _lock: lock,
                tool_sync,
                secret,
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
                match self.secret.take() {
                    Some(value) => std::env::set_var("ZCODE_CREDENTIAL_SECRET", value),
                    None => std::env::remove_var("ZCODE_CREDENTIAL_SECRET"),
                }
            }
        }
    }

    #[test]
    fn token_json_accepts_oauth_and_api_key_and_rejects_garbage() {
        let oauth = import_from_token(
            r#"{"auth_mode":"oauth","provider":"zai","email":"a@b.c","user_id":"u1","access_token":"access","refresh_token":"refresh","zcode_jwt_token":"jwt","expires_at":1900000000}"#,
        )
        .unwrap();
        assert_eq!(oauth.access_token, "access");
        assert_eq!(oauth.refresh_token.as_deref(), Some("refresh"));
        assert_eq!(oauth.id_token.as_deref(), Some("jwt"));
        assert!(oauth.api_key.is_none());
        assert_eq!(oauth.oauth_region.as_deref(), Some("zai"));
        assert_eq!(oauth.oauth_account_id.as_deref(), Some("u1"));
        assert_eq!(oauth.display_name, "a@b.c");
        assert_eq!(oauth.provider_state.as_deref(), Some(r#"{"kind":"oauth"}"#));
        let row = oauth_row_from_imported(oauth).unwrap();
        assert_eq!(row.auth_mode, AuthMode::OAuth);
        assert_eq!(row.oauth_region.as_deref(), Some("zai"));
        assert_eq!(
            crate::crypto::decrypt(row.id_token_encrypted.as_deref().unwrap()),
            "jwt"
        );
        assert!(row.api_key_encrypted.is_none());

        let wrapped = import_from_token(
            r#"{"accounts":[{"auth_mode":"api_key","provider":"BigModel","api_key":"sk-secret-key"}]}"#,
        )
        .unwrap();
        assert!(wrapped.access_token.is_empty());
        assert_eq!(wrapped.api_key.as_deref(), Some("sk-secret-key"));
        assert_eq!(
            wrapped.provider_state.as_deref(),
            Some(r#"{"kind":"api_key"}"#)
        );
        assert_eq!(wrapped.oauth_region.as_deref(), Some("bigmodel"));
        assert!(wrapped.display_name.contains("BigModel"));
        assert!(oauth_row_from_imported(wrapped).is_err());

        for payload in [
            "",
            "not-json",
            "[]",
            "{}",
            "null",
            r#"{"provider":"zai","access_token":"a"}"#,
            r#"{"provider":"nope","access_token":"a","zcode_jwt_token":"j"}"#,
            r#"{"auth_mode":"api_key","provider":"zai"}"#,
            r#"{"auth_mode":"api_key","provider":"zai","api_key":"has space"}"#,
            r#"[{"provider":"zai"},{"provider":"bigmodel"}]"#,
            r#"{"accounts":[]}"#,
        ] {
            let error = match import_from_token(payload) {
                Ok(_) => panic!("expected rejection for {payload}"),
                Err(error) => error,
            };
            let message = error.to_string();
            assert!(
                message.contains("JSON")
                    || message.contains("provider")
                    || message.contains("access_token")
                    || message.contains("JWT")
                    || message.contains("API Key")
                    || message.contains("一个")
                    || message.contains("空白")
                    || message.contains("上游"),
                "{payload} -> {message}"
            );
        }
    }

    #[test]
    fn credentials_file_decrypts_the_official_enc_v1_vector() {
        let key = hex_key(DARWIN_KEY);
        assert_eq!(
            decrypt_enc_v1(&key, FIXTURE).unwrap(),
            "official-fixture-token"
        );
        let user = encrypt_enc_v1(&key, r#"{"user_id":"u1","email":"a@b.c"}"#).unwrap();
        let values = json!({
            "oauth:active_provider": encrypt_enc_v1(&key, "zai").unwrap(),
            "oauth:zai:access_token": FIXTURE,
            "oauth:zai:refresh_token": encrypt_enc_v1(&key, "refresh-1").unwrap(),
            "zcodejwttoken": encrypt_enc_v1(&key, "jwt-1").unwrap(),
            "oauth:zai:user_info": user,
        });
        let imported = account_from_credentials(values.as_object().unwrap(), &key).unwrap();
        assert_eq!(imported.access_token, "official-fixture-token");
        assert_eq!(imported.refresh_token.as_deref(), Some("refresh-1"));
        assert_eq!(imported.id_token.as_deref(), Some("jwt-1"));
        assert_eq!(imported.oauth_account_id.as_deref(), Some("u1"));
        assert_eq!(imported.display_name, "a@b.c");
        assert_eq!(imported.oauth_region.as_deref(), Some("zai"));
    }

    #[test]
    fn local_import_reads_setting_json_database_dir_inside_the_sandbox() {
        let tmp = tempfile::tempdir().unwrap();
        let _guard = EnvGuard::sandbox(tmp.path());
        let wrong = tmp.path().join(".zcode").join("v2");
        std::fs::create_dir_all(&wrong).unwrap();
        std::fs::write(
            wrong.join("settings.json"),
            r#"{"dataBaseDir":"/should/not/use"}"#,
        )
        .unwrap();
        let override_root = tmp.path().join("override");
        std::fs::write(
            wrong.join("setting.json"),
            serde_json::json!({ "dataBaseDir": override_root }).to_string(),
        )
        .unwrap();
        let path = zcode_credentials_path();
        assert!(path.starts_with(&override_root), "{}", path.display());
        assert!(path.starts_with(tmp.path()));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let key = local_credential_key();
        let home_key = zcode_credential_key(tmp.path(), &os_username());
        assert_eq!(key, home_key);
        let override_key = zcode_credential_key(&override_root.join(".zcode"), &os_username());
        assert_ne!(key, override_key);
        let values = json!({
            "oauth:active_provider": encrypt_enc_v1(&key, "bigmodel").unwrap(),
            "oauth:bigmodel:access_token": encrypt_enc_v1(&key, "access-b").unwrap(),
            "zcodejwttoken": encrypt_enc_v1(&key, "jwt-b").unwrap(),
            "oauth:bigmodel:user_info": encrypt_enc_v1(&key, r#"{"id":"bm-1","email":"b@c.d"}"#).unwrap(),
        });
        std::fs::write(&path, values.to_string()).unwrap();
        let imported = import_from_local().unwrap();
        assert_eq!(imported.access_token, "access-b");
        assert_eq!(imported.id_token.as_deref(), Some("jwt-b"));
        assert_eq!(imported.oauth_region.as_deref(), Some("bigmodel"));
        assert_eq!(imported.oauth_account_id.as_deref(), Some("bm-1"));
        assert!(
            !path.starts_with(skillstar_core::infra::paths::home_dir().join(".zcode"))
                || tmp
                    .path()
                    .starts_with(skillstar_core::infra::paths::home_dir())
        );
    }
}
