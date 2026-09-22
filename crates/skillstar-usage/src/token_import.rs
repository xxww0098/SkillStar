//! Paste-a-token import.
//!
//! [`TOKEN_IMPORTERS`] is the only production registry. The paste is an opaque
//! string: it is not logged, not emitted, and not copied onto a DTO.

use crate::catalog::AuthMode;
use crate::crypto;
use crate::fetchers::oauth::common::carry_over_user_metadata;
use crate::storage;
use crate::subscription::{BillingCycle, Subscription, SubscriptionUsage};
use crate::{UsageError, UsageResult};

/// Credential material a provider parsed out of a paste.
///
/// The paste itself does not belong in this struct. Providers copy only the
/// fields they understood.
pub(crate) struct ImportedToken {
    pub display_name: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<i64>,
    pub oauth_account_id: Option<String>,
    pub provider_state: Option<String>,
    pub currency: Option<String>,
    pub oauth_region: Option<String>,
    /// Plaintext OIDC / provider JWT. ZCode stores the zcode JWT here.
    pub id_token: Option<String>,
    /// Plaintext API key. Distinct from `access_token`.
    pub api_key: Option<String>,
}

type ImportFromToken = fn(&str) -> UsageResult<ImportedToken>;

struct TokenImporter {
    catalog_id: &'static str,
    import_from_token: ImportFromToken,
}

/// Production importers. One row per catalog that accepts a pasted credential.
const TOKEN_IMPORTERS: &[TokenImporter] = &[
    TokenImporter {
        catalog_id: "github-copilot",
        import_from_token: crate::fetchers::oauth::github_copilot::import_from_token,
    },
    TokenImporter {
        catalog_id: "windsurf",
        import_from_token: crate::fetchers::oauth::windsurf::import_from_token,
    },
    TokenImporter {
        catalog_id: "kiro",
        import_from_token: crate::fetchers::oauth::kiro::import_from_token,
    },
    TokenImporter {
        catalog_id: "qoder",
        import_from_token: crate::fetchers::oauth::qoder::import_from_token,
    },
    TokenImporter {
        catalog_id: "codebuddy",
        import_from_token: crate::fetchers::oauth::codebuddy::import_from_token,
    },
    TokenImporter {
        catalog_id: "codebuddy-cn",
        import_from_token: crate::fetchers::oauth::codebuddy::import_from_token_cn,
    },
    TokenImporter {
        catalog_id: "trae",
        import_from_token: crate::fetchers::oauth::trae::import_from_token,
    },
    TokenImporter {
        catalog_id: "trae-solo",
        import_from_token: crate::fetchers::oauth::trae::import_from_token_solo,
    },
    TokenImporter {
        catalog_id: "trae-cn",
        import_from_token: crate::fetchers::oauth::trae::import_from_token_cn,
    },
    TokenImporter {
        catalog_id: "trae-solo-cn",
        import_from_token: crate::fetchers::oauth::trae::import_from_token_solo_cn,
    },
    TokenImporter {
        catalog_id: "zed",
        import_from_token: crate::fetchers::oauth::zed::import_from_token,
    },
    TokenImporter {
        catalog_id: "zcode",
        import_from_token: crate::fetchers::oauth::zcode::import_from_token,
    },
];

pub fn token_import_supported(catalog_id: &str) -> bool {
    importer_for(catalog_id).is_some()
}

fn importer_for(catalog_id: &str) -> Option<ImportFromToken> {
    TOKEN_IMPORTERS
        .iter()
        .find(|importer| importer.catalog_id == catalog_id)
        .map(|importer| importer.import_from_token)
        .or_else(|| test_importer(catalog_id))
}

#[cfg(not(test))]
fn test_importer(_catalog_id: &str) -> Option<ImportFromToken> {
    None
}

#[cfg(test)]
fn test_importer(catalog_id: &str) -> Option<ImportFromToken> {
    test_hooks::importer(catalog_id)
}

/// Import `payload` as a token-import row.
///
/// `target_subscription_id` replaces that row and keeps user metadata. The
/// new row is written only after a follow-up refresh succeeds.
pub async fn import_subscription_from_token(
    catalog_id: &str,
    payload: String,
    target_subscription_id: Option<&str>,
) -> UsageResult<Subscription> {
    let catalog_id = catalog_id.to_string();
    let target = target_subscription_id
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string);
    crate::refresh_guard::with_catalog_lock(&catalog_id.clone(), || async move {
        let Some(import_from_token) = importer_for(&catalog_id) else {
            return Err(UsageError::Other(format!("不支持从令牌导入：{catalog_id}")));
        };
        let imported = import_from_token(&payload)?;
        drop(payload);
        let mut sub = subscription_from_import(&catalog_id, imported)?;
        if let Some(target_id) = target.as_deref() {
            let existing = storage::get_subscription(target_id)?;
            if existing.catalog_id != catalog_id {
                return Err(UsageError::Other(format!(
                    "不支持从令牌导入：目标账号不属于 {catalog_id}"
                )));
            }
            carry_over_user_metadata(&mut sub, &existing, &[]);
        }
        let usage = verify_imported(&mut sub).await?;
        let saved = storage::upsert_subscription(sub)?;
        storage::save_usage_snapshot(usage)?;
        Ok(saved)
    })
    .await?
}

fn subscription_from_import(
    catalog_id: &str,
    imported: ImportedToken,
) -> UsageResult<Subscription> {
    let provider_state = imported
        .provider_state
        .filter(|value| !value.trim().is_empty());
    let has_refresh = imported
        .refresh_token
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty());
    let id_token = imported.id_token.filter(|value| !value.trim().is_empty());
    let api_key = imported.api_key.filter(|value| !value.trim().is_empty());
    if imported.access_token.trim().is_empty()
        && provider_state.is_none()
        && !has_refresh
        && id_token.is_none()
        && api_key.is_none()
    {
        return Err(UsageError::Other("令牌导入没有可用凭据".into()));
    }
    let now = chrono::Utc::now().timestamp();
    let currency = imported.currency.unwrap_or_else(|| {
        crate::catalog::find(catalog_id)
            .map(|entry| entry.default_currency.to_string())
            .unwrap_or_else(|| "USD".to_string())
    });
    Ok(Subscription {
        id: uuid::Uuid::new_v4().to_string(),
        catalog_id: catalog_id.to_string(),
        display_name: imported.display_name,
        auth_mode: AuthMode::TokenImport,
        plan_tier: None,
        monthly_price: None,
        currency,
        billing_cycle: BillingCycle::Monthly,
        start_date: 0,
        renew_date: 0,
        auto_renew: false,
        api_key_encrypted: api_key.as_deref().map(crypto::encrypt),
        platform_token_encrypted: None,
        access_token_encrypted: (!imported.access_token.is_empty())
            .then(|| crypto::encrypt(&imported.access_token)),
        refresh_token_encrypted: imported
            .refresh_token
            .filter(|value| !value.is_empty())
            .map(|value| crypto::encrypt(&value)),
        access_token_expires_at: imported.expires_at,
        id_token_encrypted: id_token.as_deref().map(crypto::encrypt),
        oauth_account_id: imported.oauth_account_id,
        oauth_region: imported.oauth_region,
        requires_reauth: false,
        provider_state_encrypted: provider_state.as_deref().map(crypto::encrypt),
        cookie_jar_encrypted: None,
        cookie_session_expires_at: None,
        manual_quota: None,
        note: None,
        sort_index: 0,
        created_at: now,
        updated_at: now,
    })
}

async fn verify_imported(sub: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    #[cfg(test)]
    if test_hooks::is_registered(&sub.catalog_id) {
        return test_hooks::refresh(sub).await;
    }
    crate::fetchers::refresh(sub).await
}

#[cfg(test)]
mod test_hooks {
    use std::collections::HashMap;
    use std::sync::{LazyLock, Mutex};
    use std::time::Duration;

    use super::ImportedToken;
    use crate::subscription::{Subscription, SubscriptionUsage};
    use crate::{UsageError, UsageResult};

    pub type ImportFromToken = fn(&str) -> UsageResult<ImportedToken>;

    #[derive(Clone, Copy)]
    struct RefreshPlan {
        delay: Duration,
        error: Option<&'static str>,
    }

    static IMPORTERS: Mutex<Vec<(&'static str, ImportFromToken)>> = Mutex::new(Vec::new());
    static REFRESH: LazyLock<Mutex<HashMap<&'static str, RefreshPlan>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));

    pub struct Guard;

    impl Drop for Guard {
        fn drop(&mut self) {
            IMPORTERS
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .clear();
            REFRESH
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .clear();
        }
    }

    pub fn register(catalog_id: &'static str, import: ImportFromToken) -> Guard {
        IMPORTERS
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push((catalog_id, import));
        Guard
    }

    pub fn set_refresh(catalog_id: &'static str, delay: Duration, error: Option<&'static str>) {
        REFRESH
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(catalog_id, RefreshPlan { delay, error });
    }

    pub fn importer(catalog_id: &str) -> Option<ImportFromToken> {
        IMPORTERS
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .find(|(id, _)| *id == catalog_id)
            .map(|(_, import)| *import)
    }

    pub fn is_registered(catalog_id: &str) -> bool {
        importer(catalog_id).is_some()
    }

    pub async fn refresh(sub: &Subscription) -> UsageResult<SubscriptionUsage> {
        let plan = REFRESH
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get(sub.catalog_id.as_str())
            .copied();
        let Some(plan) = plan else {
            return Err(UsageError::Other("测试导入缺少 refresh 计划".into()));
        };
        if !plan.delay.is_zero() {
            tokio::time::sleep(plan.delay).await;
        }
        match plan.error {
            Some(message) => Err(UsageError::Fetcher(message.into())),
            None => Ok(SubscriptionUsage {
                subscription_id: sub.id.clone(),
                fetched_at: chrono::Utc::now().timestamp(),
                ..Default::default()
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    use super::test_hooks;
    use super::*;
    use crate::UsageResult;
    use crate::catalog::{AuthMode, catalog};
    use crate::crypto;
    use crate::fetchers::oauth::common::SubscriptionBuilder;
    use crate::storage;
    use crate::subscription::Subscription;

    static IMPORT_ENTERED: AtomicBool = AtomicBool::new(false);

    const PASTE: &str = "raw-paste-secret";

    struct EnvGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
        prev_data: Option<std::ffi::OsString>,
    }

    impl EnvGuard {
        fn hold() -> Self {
            let lock = crate::test_env_lock()
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            Self {
                prev_data: std::env::var_os("SKILLSTAR_DATA_DIR"),
                _lock: lock,
            }
        }

        fn data_dir(path: &std::path::Path) -> Self {
            let guard = Self::hold();
            // SAFETY: this test holds `test_env_lock` until drop.
            unsafe {
                std::env::set_var("SKILLSTAR_DATA_DIR", path);
            }
            guard
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            // SAFETY: still covered by `test_env_lock`.
            unsafe {
                match self.prev_data.take() {
                    Some(value) => std::env::set_var("SKILLSTAR_DATA_DIR", value),
                    None => std::env::remove_var("SKILLSTAR_DATA_DIR"),
                }
            }
        }
    }

    fn normalized_token(_payload: &str) -> UsageResult<ImportedToken> {
        Ok(ImportedToken {
            display_name: "Imported".into(),
            access_token: "normalized-access".into(),
            refresh_token: Some("normalized-refresh".into()),
            expires_at: Some(1_900_000_000),
            oauth_account_id: None,
            provider_state: None,
            currency: None,
            oauth_region: None,
            id_token: None,
            api_key: None,
        })
    }

    fn probe_token(payload: &str) -> UsageResult<ImportedToken> {
        let _ = payload;
        IMPORT_ENTERED.store(true, Ordering::SeqCst);
        normalized_token(payload)
    }

    fn plain(value: &Option<String>) -> String {
        value.as_deref().map(crypto::decrypt).unwrap_or_default()
    }

    fn seed_cursor() -> Subscription {
        let mut sub =
            SubscriptionBuilder::new("cursor", "My Cursor", "USD", "old-token", None).build();
        sub.id = "keep-me".into();
        sub.monthly_price = Some(12.5);
        sub.note = Some("hello".into());
        sub.sort_index = 7;
        storage::upsert_subscription(sub).unwrap()
    }

    fn tree_contains(root: &std::path::Path, needle: &str) -> bool {
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if std::fs::read_to_string(&path).is_ok_and(|text| text.contains(needle)) {
                    return true;
                }
            }
        }
        false
    }

    #[test]
    fn token_import_support_matches_the_catalog_auth_mode() {
        let _env = EnvGuard::hold();
        for entry in catalog() {
            let listed = entry.auth_modes.contains(&AuthMode::TokenImport);
            assert_eq!(token_import_supported(entry.id), listed, "{}", entry.id);
        }
        assert!(token_import_supported("github-copilot"));
        assert!(!token_import_supported("cursor"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn unknown_catalog_fails_closed_and_writes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let _env = EnvGuard::data_dir(tmp.path());
        let seeded = seed_cursor();

        let err = import_subscription_from_token("cursor", PASTE.to_string(), Some("keep-me"))
            .await
            .expect_err("cursor has no token importer");
        assert!(err.to_string().contains("不支持"), "{err}");

        let err = import_subscription_from_token("not-a-provider", PASTE.to_string(), None)
            .await
            .expect_err("unregistered catalog");
        assert!(err.to_string().contains("不支持"), "{err}");

        let err = import_subscription_from_token("github-copilot", PASTE.to_string(), None)
            .await
            .expect_err("random paste is not a github token");
        assert!(err.to_string().contains("无法识别"), "{err}");
        assert!(!err.to_string().contains(PASTE), "{err}");

        let stored = storage::get_subscription("keep-me").unwrap();
        assert_eq!(stored.note, seeded.note);
        assert_eq!(plain(&stored.access_token_encrypted), "old-token");
        assert_eq!(storage::list_subscriptions().unwrap().len(), 1);
        assert!(!tree_contains(tmp.path(), PASTE));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn token_import_holds_the_catalog_lock() {
        let _env = EnvGuard::hold();
        IMPORT_ENTERED.store(false, Ordering::SeqCst);
        let _guard = test_hooks::register("token-import-lock", probe_token);
        test_hooks::set_refresh(
            "token-import-lock",
            Duration::from_millis(200),
            Some("inactive"),
        );

        let import = tokio::spawn(async {
            import_subscription_from_token("token-import-lock", PASTE.to_string(), None).await
        });
        let started = Instant::now();
        while !IMPORT_ENTERED.load(Ordering::SeqCst) {
            if started.elapsed() > Duration::from_secs(2) {
                panic!("importer did not run");
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }

        let wait_started = Instant::now();
        let entered = tokio::time::timeout(Duration::from_secs(3), async {
            crate::refresh_guard::with_catalog_lock("token-import-lock", || async {}).await
        })
        .await
        .expect("catalog lock was not released");
        entered.expect("lock");
        assert!(
            wait_started.elapsed() >= Duration::from_millis(120),
            "import must hold the catalog lock while it runs"
        );

        let err = import.await.unwrap().expect_err("refresh failed");
        assert!(err.to_string().contains("inactive"), "{err}");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn target_row_keeps_user_metadata_when_refresh_succeeds() {
        let tmp = tempfile::tempdir().unwrap();
        let _env = EnvGuard::data_dir(tmp.path());
        let seeded = seed_cursor();
        let _guard = test_hooks::register("cursor", normalized_token);
        test_hooks::set_refresh("cursor", Duration::ZERO, None);

        let saved = import_subscription_from_token("cursor", PASTE.to_string(), Some("keep-me"))
            .await
            .unwrap();

        assert_eq!(saved.id, "keep-me");
        assert_eq!(saved.auth_mode, AuthMode::TokenImport);
        assert_eq!(saved.monthly_price, Some(12.5));
        assert_eq!(saved.note.as_deref(), Some("hello"));
        assert_eq!(saved.sort_index, 7);
        assert_eq!(saved.display_name, "My Cursor");
        assert_eq!(saved.created_at, seeded.created_at);
        assert_eq!(plain(&saved.access_token_encrypted), "normalized-access");
        assert!(!tree_contains(tmp.path(), PASTE));
        assert_eq!(storage::list_subscriptions().unwrap().len(), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn refresh_failure_does_not_keep_the_new_row() {
        let tmp = tempfile::tempdir().unwrap();
        let _env = EnvGuard::data_dir(tmp.path());
        seed_cursor();
        let _guard = test_hooks::register("cursor", normalized_token);
        test_hooks::set_refresh("cursor", Duration::ZERO, Some("inactive"));

        let err = import_subscription_from_token("cursor", PASTE.to_string(), Some("keep-me"))
            .await
            .expect_err("refresh failure");
        assert!(err.to_string().contains("inactive"), "{err}");
        let stored = storage::get_subscription("keep-me").unwrap();
        assert_eq!(stored.auth_mode, AuthMode::OAuth);
        assert_eq!(stored.monthly_price, Some(12.5));
        assert_eq!(stored.note.as_deref(), Some("hello"));
        assert_eq!(stored.sort_index, 7);
        assert_eq!(plain(&stored.access_token_encrypted), "old-token");

        let err = import_subscription_from_token("cursor", PASTE.to_string(), None)
            .await
            .expect_err("create is also rejected");
        assert!(err.to_string().contains("inactive"), "{err}");
        assert_eq!(storage::list_subscriptions().unwrap().len(), 1);
        assert!(!tree_contains(tmp.path(), PASTE));
    }

    fn rich_token(_payload: &str) -> UsageResult<ImportedToken> {
        Ok(ImportedToken {
            display_name: "Z".into(),
            access_token: String::new(),
            refresh_token: None,
            expires_at: None,
            oauth_account_id: None,
            provider_state: Some(r#"{"kind":"api_key"}"#.into()),
            currency: None,
            oauth_region: Some("zai".into()),
            id_token: Some("jwt-1".into()),
            api_key: Some("sk-test".into()),
        })
    }

    #[tokio::test(flavor = "current_thread")]
    async fn token_import_stores_id_token_and_api_key() {
        let tmp = tempfile::tempdir().unwrap();
        let _env = EnvGuard::data_dir(tmp.path());
        let _guard = test_hooks::register("cursor", rich_token);
        test_hooks::set_refresh("cursor", Duration::ZERO, None);

        let saved = import_subscription_from_token("cursor", "paste-secret".into(), None)
            .await
            .unwrap();
        assert_eq!(plain(&saved.id_token_encrypted), "jwt-1");
        assert_eq!(plain(&saved.api_key_encrypted), "sk-test");
        assert_eq!(
            plain(&saved.provider_state_encrypted),
            r#"{"kind":"api_key"}"#
        );
        assert!(saved.access_token_encrypted.is_none());
        assert_eq!(saved.oauth_region.as_deref(), Some("zai"));
        assert!(!tree_contains(tmp.path(), "paste-secret"));
    }
}
