//! Wire the gateway's account book into Usage's account system.
//!
//! `account()` is live-first (specs/usage-models-evolution slice 02):
//! custody decides who the CLI is serving right now, and that live/snapshot
//! credential is the signing source of truth; the stored row is only a
//! fallback for when the CLI has no usable credential. `allowance()` still
//! reads the usage snapshots — not a secret, not part of upstream signing,
//! so its stored-read behaviour is unchanged.

use skillstar_gateway::{AccountBook, AccountSnapshot, AllowanceSnapshot};
use skillstar_usage::storage;
use skillstar_usage::subscription::{Subscription, SubscriptionUsage};
use skillstar_usage::usage_switch::signing_material;

/// One process-wide view of the subscriptions Usage has already saved.
#[derive(Debug, Default, Clone, Copy)]
pub struct UsageAccountBook;

impl AccountBook for UsageAccountBook {
    fn account(&self, catalog_id: &str) -> Option<AccountSnapshot> {
        // Live-first: material and attribution (Freshness / subscription_id)
        // are decided by the seam. AccountSnapshot's fields and the sign.rs
        // trait contract are untouched; ledger-side attribution signals (the
        // key fingerprint, say) will be derived from subscription_id by the
        // later ledger slices.
        let material = signing_material(catalog_id)?;
        Some(AccountSnapshot {
            access_token: material.access_token,
            account_id: material.account_id,
            api_key: material.api_key,
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
    if let Some(active) = active
        && let Some(row) = rows
            .iter()
            .find(|row| row.id == active && row.catalog_id == catalog_id)
    {
        return Some(row);
    }
    rows.iter().find(|row| row.catalog_id == catalog_id)
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
    use skillstar_usage::crypto;
    use skillstar_usage::subscription::{BillingCycle, UsageWindow};
    use skillstar_usage::usage_switch::activate_subscription;
    use skillstar_usage::{AuthMode, Subscription};

    // Minimal JWT (sub=acct-1 / email=dana@example.com) used as the row's
    // id_token so a codex row can be activated (materialize requires one).
    // sub equals row 1's oauth_account_id — in a real login the id_token's
    // sub and tokens.account_id are the same account id, and a fixture that
    // is not self-consistent sends identity attribution down the wrong
    // channel.
    const ACCT_ID_TOKEN: &str = concat!(
        "e30.",
        "eyJlbWFpbCI6ImRhbmFAZXhhbXBsZS5jb20iLCJzdWIiOiJhY2N0LTEiLCJleHAiOjE5OTk5OTk5OTl9",
        "."
    );

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
            id_token_encrypted: Some(crypto::encrypt(ACCT_ID_TOKEN)),
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

    /// With no live credential (CLI not installed / logged out) custody
    /// reports Missing and account() falls back to the stored row — the
    /// pre-re-routing pinned semantics carry over unchanged.
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
        let first = SubscriptionUsage {
            subscription_id: "codex-1".to_string(),
            weekly: Some(window(90)),
            ..Default::default()
        };
        let pinned = SubscriptionUsage {
            subscription_id: "codex-2".to_string(),
            weekly: Some(window(40)),
            ..Default::default()
        };
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
                ("Accept".to_string(), "application/json".to_string()),
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

    /// When custody reports LinkedTo, signing material is read straight from
    /// the live credential: the pin and the row are only caches — the token
    /// the CLI is actually sending is the source of truth.
    #[tokio::test(flavor = "current_thread")]
    async fn usage_account_book_prefers_the_live_credential() {
        let _lock = ENV_LOCK.lock().await;
        let root = tempfile::tempdir().unwrap();
        let _env = EnvGuard::set(&[
            ("SKILLSTAR_DATA_DIR", root.path()),
            ("SKILLSTAR_TOOL_SYNC_HOME", root.path()),
            ("HOME", root.path()),
        ]);

        storage::upsert_subscription(row("codex-1", "access-1", "acct-1", "refresh-1")).unwrap();
        storage::set_active_subscription("codex", "codex-1").unwrap();
        activate_subscription("codex-1").await.unwrap();

        let book = UsageAccountBook;
        let account = book.account("codex").expect("live codex credential");
        // activate absorbed the same token into the row; this matches the
        // pre-re-routing (row-read) output, but from here on any CLI-side
        // rotation is followed live instead of being dragged back to the
        // row's older generation.
        assert_eq!(account.access_token.as_deref(), Some("access-1"));
        assert_eq!(account.account_id.as_deref(), Some("acct-1"));

        // The CLI clobbered the link via rename() and rotated the token:
        // signing must follow live.
        let live = root.path().join(".codex").join("auth.json");
        let mut json: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&live).unwrap()).unwrap();
        json["tokens"]["access_token"] = serde_json::json!("rotated-in-the-cli");
        std::fs::remove_file(&live).unwrap();
        std::fs::write(&live, serde_json::to_vec_pretty(&json).unwrap()).unwrap();

        let rotated = book.account("codex").expect("rotated live credential");
        assert_eq!(
            rotated.access_token.as_deref(),
            Some("rotated-in-the-cli"),
            "live-first: the token the CLI is sending wins over the stored row"
        );

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
                (
                    "Authorization".to_string(),
                    "Bearer rotated-in-the-cli".to_string()
                ),
                ("Accept".to_string(), "application/json".to_string()),
            ]
        );
    }

    /// A terminal CLI login (attributable to nobody) must still sign:
    /// Diverged serves the orphan material, just without a subscription
    /// attribution.
    #[tokio::test(flavor = "current_thread")]
    async fn usage_account_book_serves_an_orphan_login() {
        let _lock = ENV_LOCK.lock().await;
        let root = tempfile::tempdir().unwrap();
        let _env = EnvGuard::set(&[
            ("SKILLSTAR_DATA_DIR", root.path()),
            ("SKILLSTAR_TOOL_SYNC_HOME", root.path()),
            ("HOME", root.path()),
        ]);

        storage::upsert_subscription(row("codex-1", "access-1", "acct-1", "refresh-1")).unwrap();
        let live = root.path().join(".codex").join("auth.json");
        std::fs::create_dir_all(live.parent().unwrap()).unwrap();
        std::fs::write(
            &live,
            br#"{"tokens":{"access_token":"terminal-login-token"}}"#,
        )
        .unwrap();

        let account = UsageAccountBook
            .account("codex")
            .expect("orphan material still signs");
        assert_eq!(account.access_token.as_deref(), Some("terminal-login-token"));
    }
}
