//! Pasted `{user_id, access_token}` JSON, and the macOS Zed internet-password.
//!
//! A bare token is rejected: the quota call needs both halves. Local import
//! learns the account from keychain metadata, then calls
//! [`crate::tool_store::keychain_cli::find_internet_password`]. Sandbox
//! refusal happens inside that helper, before `security` is spawned. Tests
//! pass a fake pair into [`credentials_to_imported`] and do not touch the
//! login keychain.

use serde_json::Value;

use super::{CATALOG_ID, KEYCHAIN_SERVER, PLACEHOLDER_NAME};
use crate::fetchers::oauth::common::SubscriptionBuilder;
use crate::subscription::Subscription;
use crate::token_import::ImportedToken;
use crate::{UsageError, UsageResult};

const MACOS_ONLY: &str = "Zed 本机导入仅支持 macOS";

pub(crate) fn local_import_available() -> bool {
    cfg!(target_os = "macos")
}

pub(crate) fn import_from_token(payload: &str) -> UsageResult<ImportedToken> {
    let trimmed = payload.trim();
    if !trimmed.starts_with('{') {
        return Err(UsageError::Other(
            "Zed 令牌导入需要 JSON：{\"user_id\",\"access_token\"}".into(),
        ));
    }
    let value: Value = serde_json::from_str(trimmed)
        .map_err(|_| UsageError::Other("Zed 凭据 JSON 无法解析".into()))?;
    let user_id =
        json_user_id(&value).ok_or_else(|| UsageError::Other("Zed 凭据缺少 user_id".into()))?;
    let access_token = value
        .get("access_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .ok_or_else(|| UsageError::Other("Zed 凭据缺少 access_token".into()))?
        .to_string();
    Ok(credentials_to_imported(user_id, access_token))
}

pub(crate) fn import_from_local() -> UsageResult<ImportedToken> {
    let (user_id, access_token) = read_local_pair()?;
    Ok(credentials_to_imported(user_id, access_token))
}

pub(super) fn credentials_to_imported(user_id: String, access_token: String) -> ImportedToken {
    ImportedToken {
        display_name: if user_id.is_empty() {
            PLACEHOLDER_NAME.to_string()
        } else {
            user_id.clone()
        },
        access_token,
        refresh_token: None,
        expires_at: None,
        oauth_account_id: Some(user_id),
        provider_state: None,
        currency: None,
        oauth_region: None,
        id_token: None,
        api_key: None,
    }
}

pub(crate) fn oauth_row_from_imported(imported: ImportedToken) -> UsageResult<Subscription> {
    let user_id = imported
        .oauth_account_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| UsageError::Other("Zed 导入缺少 user_id".into()))?
        .to_string();
    if imported.access_token.trim().is_empty() {
        return Err(UsageError::Other("Zed 导入缺少 access_token".into()));
    }
    if imported.refresh_token.is_some() {
        return Err(UsageError::Other("Zed 没有 refresh token".into()));
    }
    Ok(SubscriptionBuilder::new(
        CATALOG_ID,
        imported.display_name,
        "USD",
        imported.access_token,
        None,
    )
    .oauth_account_id(Some(user_id))
    .build())
}

fn read_local_pair() -> UsageResult<(String, String)> {
    if !crate::tool_paths::is_tool_sync_sandboxed() && !local_import_available() {
        return Err(UsageError::Other(MACOS_ONLY.into()));
    }
    let account = crate::tool_store::keychain_cli::find_internet_password_account(KEYCHAIN_SERVER)?
        .ok_or_else(|| UsageError::Other("未在本机 Zed 登录态中找到可导入的账号".into()))?;
    let item = crate::tool_store::keychain_cli::find_internet_password(KEYCHAIN_SERVER, &account)?
        .ok_or_else(|| UsageError::Other("未在本机 Zed 登录态中找到可导入的账号".into()))?;
    if item.password.trim().is_empty() {
        return Err(UsageError::Other("Zed Keychain access_token 为空".into()));
    }
    Ok((item.account, item.password))
}

fn json_user_id(value: &Value) -> Option<String> {
    match value.get("user_id")? {
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::AuthMode;

    fn must_import(payload: &str) -> ImportedToken {
        match import_from_token(payload) {
            Ok(imported) => imported,
            Err(error) => panic!("import failed: {error}"),
        }
    }

    #[test]
    fn json_import_stores_user_id_and_no_refresh_token() {
        let imported = must_import(r#"{"user_id":" user-1 ","access_token":" tok ","extra":1}"#);
        assert_eq!(imported.oauth_account_id.as_deref(), Some("user-1"));
        assert_eq!(imported.access_token, "tok");
        assert!(imported.refresh_token.is_none());
        assert_eq!(imported.display_name, "user-1");

        let numeric = must_import(r#"{"user_id":42,"access_token":"tok"}"#);
        assert_eq!(numeric.oauth_account_id.as_deref(), Some("42"));

        let row = oauth_row_from_imported(imported).expect("row");
        assert_eq!(row.catalog_id, CATALOG_ID);
        assert_eq!(row.auth_mode, AuthMode::OAuth);
        assert!(row.refresh_token_encrypted.is_none());
        assert_eq!(row.oauth_account_id.as_deref(), Some("user-1"));
        assert_eq!(
            crate::crypto::decrypt(row.access_token_encrypted.as_deref().unwrap()),
            "tok"
        );
    }

    #[test]
    fn garbage_token_import_is_rejected() {
        for payload in [
            "",
            "not-json",
            "tok",
            "[]",
            "{",
            "{}",
            r#"{"user_id":"u"}"#,
            r#"{"access_token":"t"}"#,
            r#"{"user_id":" ","access_token":"t"}"#,
            r#"{"user_id":"u","access_token":""}"#,
            r#"{"user_id":"u","access_token":1}"#,
            "null",
        ] {
            let error = match import_from_token(payload) {
                Ok(_) => panic!("expected rejection for {payload}"),
                Err(error) => error,
            };
            let message = error.to_string();
            assert!(
                message.contains("user_id")
                    || message.contains("access_token")
                    || message.contains("JSON"),
                "{payload} -> {message}"
            );
        }
    }

    #[test]
    fn fake_keychain_pair_becomes_an_imported_row() {
        let imported = credentials_to_imported("user-9".into(), "secret".into());
        let row = oauth_row_from_imported(imported).expect("row");
        assert_eq!(row.oauth_account_id.as_deref(), Some("user-9"));
        assert!(row.refresh_token_encrypted.is_none());
        assert_eq!(
            crate::crypto::decrypt(row.access_token_encrypted.as_deref().unwrap()),
            "secret"
        );
    }

    #[test]
    fn local_import_available_is_macos_only() {
        assert_eq!(local_import_available(), cfg!(target_os = "macos"));
    }

    struct EnvGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
        prev: Option<std::ffi::OsString>,
    }

    impl EnvGuard {
        fn sandbox(path: &std::path::Path) -> Self {
            let lock = crate::test_env_lock()
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let prev = std::env::var_os("SKILLSTAR_TOOL_SYNC_HOME");
            // SAFETY: serialized by the crate-wide test_env_lock.
            unsafe {
                std::env::set_var("SKILLSTAR_TOOL_SYNC_HOME", path);
            }
            Self { _lock: lock, prev }
        }

        #[cfg(not(target_os = "macos"))]
        fn clear_sandbox() -> Self {
            let lock = crate::test_env_lock()
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let prev = std::env::var_os("SKILLSTAR_TOOL_SYNC_HOME");
            // SAFETY: serialized by the crate-wide test_env_lock.
            unsafe {
                std::env::remove_var("SKILLSTAR_TOOL_SYNC_HOME");
            }
            Self { _lock: lock, prev }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            // SAFETY: still serialized by the crate-wide test_env_lock.
            unsafe {
                match &self.prev {
                    Some(value) => std::env::set_var("SKILLSTAR_TOOL_SYNC_HOME", value),
                    None => std::env::remove_var("SKILLSTAR_TOOL_SYNC_HOME"),
                }
            }
        }
    }

    #[test]
    fn sandboxed_local_import_returns_the_keychain_sandbox_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = EnvGuard::sandbox(dir.path());
        let error = match import_from_local() {
            Ok(_) => panic!("sandbox must not read the keychain"),
            Err(error) => error,
        };
        assert_eq!(
            error.to_string(),
            "SKILLSTAR_TOOL_SYNC_HOME 已设置，拒绝访问 macOS internet-password keychain"
        );
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn local_import_off_macos_explains_that_it_is_macos_only() {
        let _guard = EnvGuard::clear_sandbox();
        let error = match import_from_local() {
            Ok(_) => panic!("non-macOS must not read a keychain"),
            Err(error) => error,
        };
        let message = error.to_string();
        assert!(message.contains("仅支持 macOS"), "{message}");
        assert!(!message.contains("SKILLSTAR_TOOL_SYNC_HOME"), "{message}");
    }
}
