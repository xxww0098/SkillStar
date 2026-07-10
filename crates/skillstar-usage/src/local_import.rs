//! Import OAuth subscriptions from well-known on-disk tool credentials.

use base64::{Engine as _, engine::general_purpose};
use chrono::Utc;
use serde::Deserialize;

use crate::catalog::AuthMode;
use crate::cloud_code;
use crate::crypto;
use crate::oauth::token_refresh;
use crate::protobuf_oauth;
use crate::storage;
use crate::subscription::{BillingCycle, Subscription};
use crate::tool_paths::{antigravity_state_db_path, codex_auth_path};
use crate::vscdb;
use crate::{UsageError, UsageResult};

const ANTIGRAVITY_OAUTH_KEY: &str = "antigravityUnifiedStateSync.oauthToken";

/// Catalog ids that support `import_subscription_from_local`.
pub fn local_import_supported(catalog_id: &str) -> bool {
    matches!(catalog_id, "codex" | "antigravity")
}

pub async fn import_subscription_from_local(catalog_id: &str) -> UsageResult<Subscription> {
    match catalog_id {
        "codex" => import_codex_from_auth_json().await,
        "antigravity" => import_antigravity_from_state_db().await,
        other => Err(UsageError::Other(format!(
            "不支持从本地导入：{other}（支持 codex、antigravity）"
        ))),
    }
}

#[derive(Debug, Deserialize, Default)]
struct CodexAuthFile {
    #[serde(default)]
    tokens: Option<CodexAuthTokens>,
}

#[derive(Debug, Deserialize, Default)]
struct CodexAuthTokens {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    id_token: Option<String>,
}

async fn import_codex_from_auth_json() -> UsageResult<Subscription> {
    let path = codex_auth_path();
    if !path.exists() {
        return Err(UsageError::Other("未找到 ~/.codex/auth.json".into()));
    }
    let content = std::fs::read_to_string(&path).map_err(UsageError::Io)?;
    let auth: CodexAuthFile = serde_json::from_str(&content).map_err(UsageError::Serde)?;

    let tokens = auth.tokens.ok_or_else(|| {
        UsageError::Other("auth.json 缺少 tokens，请先在 Codex CLI 完成 OAuth".into())
    })?;
    let access_token = tokens
        .access_token
        .filter(|s| !s.is_empty())
        .ok_or_else(|| UsageError::Other("auth.json tokens 缺少 access_token".into()))?;

    let display_name = tokens
        .id_token
        .as_deref()
        .and_then(|jwt| token_refresh::jwt_string(jwt, &["email", "preferred_username"]))
        .unwrap_or_else(|| "Codex".to_string());

    let expires_at = tokens
        .id_token
        .as_deref()
        .and_then(token_refresh::jwt_exp)
        .or_else(|| token_refresh::jwt_exp(&access_token));

    upsert_oauth_subscription(
        "codex",
        display_name,
        access_token,
        tokens.refresh_token,
        expires_at,
        "USD",
    )
    .await
}

async fn import_antigravity_from_state_db() -> UsageResult<Subscription> {
    let db_path = antigravity_state_db_path()
        .ok_or_else(|| UsageError::Other("无法解析 Antigravity IDE 数据目录".into()))?;
    if !db_path.exists() {
        return Err(UsageError::Other(format!(
            "未找到 Antigravity state.vscdb：{}",
            db_path.display()
        )));
    }

    let state_data = vscdb::read_item_string(&db_path, ANTIGRAVITY_OAUTH_KEY)?
        .ok_or_else(|| UsageError::Other("Antigravity IDE 未登录（缺少 oauthToken）".into()))?;

    let blob = general_purpose::STANDARD
        .decode(state_data.trim())
        .map_err(|e| UsageError::Other(format!("Antigravity OAuth Base64 解码失败：{e}")))?;

    let refresh_token = protobuf_oauth::extract_refresh_token_from_unified_oauth_token(&blob)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| UsageError::Other("无法从 Antigravity 本地数据解析 refresh_token".into()))?;

    let tokens = cloud_code::refresh_antigravity_access_token(&refresh_token).await?;
    let access_token = tokens
        .access_token
        .ok_or_else(|| UsageError::Other("Google refresh 缺少 access_token".into()))?;
    let expires_at = tokens
        .expires_in
        .map(|s| Utc::now().timestamp() + s)
        .or_else(|| token_refresh::jwt_exp(&access_token));

    let display_name = token_refresh::jwt_string(&access_token, &["email"])
        .unwrap_or_else(|| "Antigravity".to_string());

    upsert_oauth_subscription(
        "antigravity",
        display_name,
        access_token,
        tokens.refresh_token.or(Some(refresh_token)),
        expires_at,
        "USD",
    )
    .await
}


async fn upsert_oauth_subscription(
    catalog_id: &str,
    display_name: String,
    access_token: String,
    refresh_token: Option<String>,
    expires_at: Option<i64>,
    currency: &str,
) -> UsageResult<Subscription> {
    let now = Utc::now().timestamp();
    let sub = Subscription {
        id: uuid::Uuid::new_v4().to_string(),
        catalog_id: catalog_id.to_string(),
        display_name,
        auth_mode: AuthMode::OAuth,
        plan_tier: None,
        monthly_price: None,
        currency: currency.to_string(),
        billing_cycle: BillingCycle::Monthly,
        start_date: 0,
        renew_date: 0,
        auto_renew: false,
        api_key_encrypted: None,
        platform_token_encrypted: None,
        access_token_encrypted: Some(crypto::encrypt(&access_token)),
        refresh_token_encrypted: refresh_token.as_deref().map(crypto::encrypt),
        access_token_expires_at: expires_at,
        id_token_encrypted: None,
        oauth_account_id: None,
        oauth_region: None,
        requires_reauth: false,
        fingerprint_id: None,
        cookie_jar_encrypted: None,
        cookie_session_expires_at: None,
        manual_quota: None,
        note: None,
        sort_index: 0,
        created_at: now,
        updated_at: now,
    };

    if let Ok(usage) = crate::fetchers::refresh(&mut sub.clone()).await {
        storage::save_usage_snapshot(usage).ok();
    }

    storage::upsert_subscription(sub).map_err(|e| UsageError::Other(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// `import_subscription_from_local` reads `~/.codex/auth.json` via
    /// `home_dir()`, which honours `$HOME` under `cfg(test)`. Serialize tests
    /// with a mutex so they don't fight over the process-wide env var.
    static HOME_LOCK: Mutex<()> = Mutex::new(());

    struct HomeGuard {
        _guard: std::sync::MutexGuard<'static, ()>,
        prev: Option<std::ffi::OsString>,
    }

    impl HomeGuard {
        fn new(tmp: &std::path::Path) -> Self {
            let guard = HOME_LOCK.lock().unwrap();
            let prev = std::env::var_os("HOME");
            // SAFETY: tests are serialized by HOME_LOCK, so no other thread is
            // reading HOME while we mutate it.
            unsafe {
                std::env::set_var("HOME", tmp);
            }
            Self {
                _guard: guard,
                prev,
            }
        }
    }

    impl Drop for HomeGuard {
        fn drop(&mut self) {
            // SAFETY: same single-thread serialization via HOME_LOCK.
            unsafe {
                match &self.prev {
                    Some(v) => std::env::set_var("HOME", v),
                    None => std::env::remove_var("HOME"),
                }
            }
        }
    }

    #[tokio::test]
    async fn codex_import_errors_when_auth_json_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let _home = HomeGuard::new(tmp.path());
        // No .codex/auth.json created → must report a clear "not found" error.
        let err = import_subscription_from_local("codex")
            .await
            .expect_err("missing auth.json should error");
        let msg = err.to_string();
        assert!(
            msg.contains("auth.json"),
            "error should mention auth.json, got: {msg}"
        );
    }

    #[tokio::test]
    async fn codex_import_errors_when_auth_json_empty_object() {
        let tmp = tempfile::tempdir().unwrap();
        let _home = HomeGuard::new(tmp.path());
        let codex_dir = tmp.path().join(".codex");
        std::fs::create_dir_all(&codex_dir).unwrap();
        std::fs::write(codex_dir.join("auth.json"), "{}").unwrap();

        let err = import_subscription_from_local("codex")
            .await
            .expect_err("empty {} auth.json should error");
        let msg = err.to_string();
        assert!(
            msg.contains("tokens"),
            "error should explain missing tokens, got: {msg}"
        );
    }

    #[tokio::test]
    async fn codex_import_errors_when_access_token_blank() {
        let tmp = tempfile::tempdir().unwrap();
        let _home = HomeGuard::new(tmp.path());
        let codex_dir = tmp.path().join(".codex");
        std::fs::create_dir_all(&codex_dir).unwrap();
        // tokens present but access_token empty → must reject, not silently
        // create a subscription with a blank credential.
        std::fs::write(
            codex_dir.join("auth.json"),
            r#"{"tokens":{"access_token":"","refresh_token":"rt"}}"#,
        )
        .unwrap();

        let err = import_subscription_from_local("codex")
            .await
            .expect_err("blank access_token should error");
        let msg = err.to_string();
        assert!(
            msg.contains("access_token"),
            "error should mention access_token, got: {msg}"
        );
    }

    #[tokio::test]
    async fn local_import_rejects_unsupported_catalog_id() {
        let err = import_subscription_from_local("some-other-tool")
            .await
            .expect_err("unsupported catalog id should error");
        assert!(err.to_string().contains("不支持"));
    }
}
