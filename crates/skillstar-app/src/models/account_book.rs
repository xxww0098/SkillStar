//! Copy Usage's stored rows into the gateway's account book.
//!
//! This reads the subscription list and the usage snapshots already on disk.
//! It does not start a login, refresh a token, or request a quota URL.

use skillstar_gateway::{AccountBook, AccountSnapshot, AllowanceSnapshot};
use skillstar_usage::crypto;
use skillstar_usage::storage;
use skillstar_usage::subscription::{Subscription, SubscriptionUsage};

/// One process-wide view of the subscriptions Usage has already saved.
#[derive(Debug, Default, Clone, Copy)]
pub struct UsageAccountBook;

impl AccountBook for UsageAccountBook {
    fn account(&self, catalog_id: &str) -> Option<AccountSnapshot> {
        let row = stored_row(catalog_id)?;
        Some(AccountSnapshot {
            access_token: decrypt(&row.access_token_encrypted),
            account_id: nonempty(row.oauth_account_id),
            api_key: decrypt(&row.api_key_encrypted),
        })
    }

    fn allowance(&self, catalog_id: &str) -> Option<AllowanceSnapshot> {
        let row = stored_row(catalog_id)?;
        let snapshots = storage::list_usage_snapshots().ok()?;
        let usage = snapshots.get(&row.id)?;
        written_used(usage).map(|used| AllowanceSnapshot { used })
    }
}

fn stored_row(catalog_id: &str) -> Option<Subscription> {
    let rows = storage::list_subscriptions().ok()?;
    let active = storage::get_active_subscription(catalog_id)
        .ok()
        .flatten();
    pick(&rows, catalog_id, active.as_deref()).cloned()
}

fn pick<'a>(
    rows: &'a [Subscription],
    catalog_id: &str,
    active: Option<&str>,
) -> Option<&'a Subscription> {
    if let Some(active) = active {
        if let Some(row) = rows
            .iter()
            .find(|row| row.id == active && row.catalog_id == catalog_id)
        {
            return Some(row);
        }
    }
    rows.iter().find(|row| row.catalog_id == catalog_id)
}

fn decrypt(encoded: &Option<String>) -> Option<String> {
    let encoded = encoded.as_deref()?;
    nonempty(Some(crypto::decrypt(encoded)))
}

fn nonempty(value: Option<String>) -> Option<String> {
    value.filter(|text| !text.is_empty())
}

/// Largest percent Usage already wrote. Missing percents stay unknown.
fn written_used(usage: &SubscriptionUsage) -> Option<f64> {
    [&usage.hourly, &usage.weekly, &usage.monthly]
        .into_iter()
        .filter_map(|window| window.as_ref().and_then(|window| window.percent))
        .max()
        .map(|percent| percent as f64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{ENV_LOCK, EnvGuard};
    use skillstar_gateway::{ProviderSnapshot, SignInput, sign_upstream};
    use skillstar_usage::subscription::{BillingCycle, UsageWindow};
    use skillstar_usage::{AuthMode, Subscription};

    fn row(id: &str, token: &str, account_id: &str, refresh: &str) -> Subscription {
        Subscription {
            id: id.to_string(),
            catalog_id: "codex".to_string(),
            display_name: id.to_string(),
            auth_mode: AuthMode::OAuth,
            plan_tier: None,
            monthly_price: None,
            currency: "USD".to_string(),
            billing_cycle: BillingCycle::Monthly,
            start_date: 0,
            renew_date: 0,
            auto_renew: false,
            api_key_encrypted: None,
            platform_token_encrypted: None,
            access_token_encrypted: Some(crypto::encrypt(token)),
            refresh_token_encrypted: Some(crypto::encrypt(refresh)),
            access_token_expires_at: None,
            id_token_encrypted: None,
            oauth_account_id: Some(account_id.to_string()),
            oauth_region: None,
            requires_reauth: false,
            provider_state_encrypted: None,
            cookie_jar_encrypted: None,
            cookie_session_expires_at: None,
            manual_quota: None,
            note: None,
            sort_index: 0,
            created_at: 0,
            updated_at: 0,
        }
    }

    fn window(percent: i32) -> UsageWindow {
        UsageWindow {
            label: "7d".to_string(),
            used: i64::from(percent),
            total: Some(100),
            percent: Some(percent),
            reset_at: None,
            breakdown: Vec::new(),
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn usage_account_book_reads_the_pinned_row() {
        let _lock = ENV_LOCK.lock().await;
        let root = tempfile::tempdir().unwrap();
        let _env = EnvGuard::set(&[
            ("SKILLSTAR_DATA_DIR", root.path()),
            ("SKILLSTAR_TOOL_SYNC_HOME", root.path()),
            ("HOME", root.path()),
        ]);

        storage::upsert_subscription(row("codex-1", "access-1", "acct-1", "refresh-1")).unwrap();
        storage::upsert_subscription(row("codex-2", "access-2", "acct-2", "refresh-2")).unwrap();
        storage::set_active_subscription("codex", "codex-2").unwrap();
        let mut first = SubscriptionUsage::default();
        first.subscription_id = "codex-1".to_string();
        first.weekly = Some(window(90));
        let mut pinned = SubscriptionUsage::default();
        pinned.subscription_id = "codex-2".to_string();
        pinned.weekly = Some(window(40));
        storage::save_usage_snapshot(first).unwrap();
        storage::save_usage_snapshot(pinned).unwrap();

        let book = UsageAccountBook;
        let account = book.account("codex").expect("pinned codex row");
        assert_eq!(account.access_token.as_deref(), Some("access-2"));
        assert_eq!(account.account_id.as_deref(), Some("acct-2"));
        assert!(account.api_key.is_none());
        let rendered = format!("{account:?}");
        assert!(!rendered.contains("refresh-1"));
        assert!(!rendered.contains("refresh-2"));
        assert_eq!(
            book.allowance("codex"),
            Some(AllowanceSnapshot { used: 40.0 })
        );
        assert!(book.account("gemini-cli").is_none());
        assert!(book.allowance("gemini-cli").is_none());

        let mut asked = 0u32;
        let signed = sign_upstream(
            &book,
            &SignInput {
                catalog_id: "codex",
                provider: None,
                body: b"{}",
            },
            &mut |_url| asked += 1,
        );
        assert_eq!(asked, 0);
        assert_eq!(
            signed.headers,
            vec![
                ("Authorization".to_string(), "Bearer access-2".to_string()),
                ("chatgpt-account-id".to_string(), "acct-2".to_string()),
            ]
        );

        let provider = ProviderSnapshot {
            api_key: Some("sk-provider".to_string()),
        };
        let api_only = sign_upstream(
            &book,
            &SignInput {
                catalog_id: "gemini-cli",
                provider: Some(&provider),
                body: b"{}",
            },
            &mut |_url| asked += 1,
        );
        assert_eq!(asked, 0);
        assert_eq!(
            api_only.headers,
            vec![(
                "Authorization".to_string(),
                "Bearer sk-provider".to_string()
            )]
        );
    }
}
