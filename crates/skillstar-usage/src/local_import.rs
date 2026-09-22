//! Import OAuth subscriptions from well-known on-disk tool credentials.
//!
//! This module is the dispatch table and the catalog lock. Each provider's
//! read lives next to its fetcher.

use std::future::Future;
use std::pin::Pin;

use chrono::Utc;

use crate::catalog::AuthMode;
use crate::crypto;
use crate::storage;
use crate::subscription::{BillingCycle, Subscription};
use crate::{UsageError, UsageResult};

type LocalImportFuture = Pin<Box<dyn Future<Output = UsageResult<Subscription>> + Send>>;

struct LocalImporter {
    catalog_id: &'static str,
    import_from_local: fn() -> LocalImportFuture,
}

fn import_codex() -> LocalImportFuture {
    Box::pin(crate::fetchers::oauth::codex::import_from_local())
}

fn import_antigravity() -> LocalImportFuture {
    Box::pin(crate::fetchers::oauth::antigravity::import_from_local())
}

fn import_cursor() -> LocalImportFuture {
    Box::pin(crate::fetchers::oauth::cursor_import::import_from_local())
}

fn import_windsurf() -> LocalImportFuture {
    Box::pin(async {
        let imported = crate::fetchers::oauth::windsurf::import_from_local()?;
        let sub = crate::fetchers::oauth::windsurf::oauth_row_from_imported(imported)?;
        crate::storage::upsert_subscription(sub)
            .map_err(|err| crate::UsageError::Other(format!("Windsurf 订阅保存失败：{err}")))
    })
}

const LOCAL_IMPORTERS: &[LocalImporter] = &[
    LocalImporter {
        catalog_id: "codex",
        import_from_local: import_codex,
    },
    LocalImporter {
        catalog_id: "antigravity",
        import_from_local: import_antigravity,
    },
    LocalImporter {
        catalog_id: "cursor",
        import_from_local: import_cursor,
    },
    LocalImporter {
        catalog_id: "windsurf",
        import_from_local: import_windsurf,
    },
];

fn importer_for(catalog_id: &str) -> Option<fn() -> LocalImportFuture> {
    LOCAL_IMPORTERS
        .iter()
        .find(|importer| importer.catalog_id == catalog_id)
        .map(|importer| importer.import_from_local)
}

fn supported_catalogs() -> String {
    LOCAL_IMPORTERS
        .iter()
        .map(|importer| importer.catalog_id)
        .collect::<Vec<_>>()
        .join("、")
}

/// Catalog ids that support `import_subscription_from_local`.
pub fn local_import_supported(catalog_id: &str) -> bool {
    importer_for(catalog_id).is_some()
}

/// Import the CLI/IDE's own credentials as a new subscription.
///
/// Runs inside the catalog's serialization domain: the import issues a live
/// refresh (network + token rotation + storage write), which is exactly what
/// [`crate::refresh_guard`] exists to keep from interleaving with a concurrent
/// refresh of the same vendor.
pub async fn import_subscription_from_local(catalog_id: &str) -> UsageResult<Subscription> {
    let Some(import_from_local) = importer_for(catalog_id) else {
        return Err(UsageError::Other(format!(
            "不支持从本地导入：{catalog_id}（支持 {}）",
            supported_catalogs()
        )));
    };
    crate::refresh_guard::with_catalog_lock(catalog_id, || async move { import_from_local().await })
        .await?
}

pub(crate) async fn upsert_oauth_subscription(
    catalog_id: &str,
    display_name: String,
    access_token: String,
    refresh_token: Option<String>,
    expires_at: Option<i64>,
    currency: &str,
    oauth_account_id: Option<String>,
) -> UsageResult<Subscription> {
    let now = Utc::now().timestamp();
    let mut sub = Subscription {
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
        oauth_account_id,
        oauth_region: None,
        requires_reauth: false,
        provider_state_encrypted: None,
        cookie_jar_encrypted: None,
        cookie_session_expires_at: None,
        manual_quota: None,
        note: None,
        sort_index: 0,
        created_at: now,
        updated_at: now,
    };

    // Refresh the row itself, not a clone: the Antigravity import path rotates
    // to a fresh Google access token during this call, and a clone would drop
    // it on the floor and persist the already-spent one.
    if let Ok(usage) = crate::fetchers::refresh(&mut sub).await {
        storage::save_usage_snapshot(usage).ok();
    }

    storage::upsert_subscription(sub).map_err(|e| UsageError::Other(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// `import_subscription_from_local` reads `~/.codex/auth.json` via
    /// `paths::home_dir()`, which honours the platform home var — `$HOME` on
    /// Unix, `%USERPROFILE%` on Windows. Sandbox BOTH so the fixture applies
    /// on every platform (a HOME-only guard let these tests read the runner's
    /// real profile on Windows). Serialize with a mutex so they don't fight
    /// over the process-wide env vars; tolerate poisoning so one failure
    /// cannot cascade into sibling PoisonError panics.
    static HOME_LOCK: Mutex<()> = Mutex::new(());

    /// The home env vars `paths::home_dir()` consults, in platform order.
    const HOME_VARS: &[&str] = &["HOME", "USERPROFILE"];

    struct HomeGuard {
        _guard: std::sync::MutexGuard<'static, ()>,
        prev: Vec<(&'static str, Option<std::ffi::OsString>)>,
    }

    impl HomeGuard {
        fn new(tmp: &std::path::Path) -> Self {
            let guard = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let prev = HOME_VARS
                .iter()
                .map(|var| (*var, std::env::var_os(var)))
                .collect();
            // SAFETY: tests are serialized by HOME_LOCK, so no other thread is
            // reading these vars while we mutate them.
            unsafe {
                for var in HOME_VARS {
                    std::env::set_var(var, tmp);
                }
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
                for (var, value) in &self.prev {
                    match value {
                        Some(v) => std::env::set_var(var, v),
                        None => std::env::remove_var(var),
                    }
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
