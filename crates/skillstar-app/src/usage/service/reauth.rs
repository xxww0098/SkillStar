//! The gateway's 401 self-heal (specs/usage-models-evolution slice 11):
//! the reauthorize hook the gateway's turn state machine calls, its Usage
//! implementation, and the bridge that lets the sync hook run Usage's async
//! serialization domain no matter which runtime the listener rides.
//!
//! ## The chain
//!
//! `reauthorize_catalog` is the same lock-order template as a card refresh
//! (`refresh.rs`): the catalog's serialization domain, then the CLI refresh
//! lease, then adopt the CLI's newer token generation, refresh, persist,
//! and sync the refreshed credential back to the CLI. No new lock exists
//! here; the failure this order prevents (a SkillStar refresh spending the
//! refresh-token generation the Codex CLI was about to use) applies to a
//! heal exactly as it applies to a card refresh.
//!
//! ## The bridge
//!
//! The gateway listener builds its own tokio runtime, and the hook is a
//! sync trait method polled inside it. The probe (the gateway-side
//! `the_heal_hook_blocks_across_runtimes…` test plus the tests here) settled
//! the shape: one long-lived bridge thread with its own runtime, the turn
//! blocking on the answer through a bounded channel. Nesting runtimes is
//! avoided entirely, and the shared HTTP client's pooled connections stay
//! bound to a runtime that lives as long as the process. A heal the turn
//! cannot wait for degrades to passing the 401 through — the seat then
//! rests in `AUTH_REST` — while the heal itself finishes off the turn path.

use std::sync::{Arc, LazyLock};
use std::time::Duration;

use skillstar_gateway::{AccountBook, AccountSnapshot, AllowanceSnapshot};
use skillstar_usage::catalog::AuthMode;
use skillstar_usage::usage_switch::{self, signing_material};
use skillstar_usage::{UsageError, fetchers, storage};

use super::refresh::refresh_failure;

/// How long a turn waits for its heal before passing the 401 through. An
/// OAuth refresh roundtrip plus the per-catalog gap fits comfortably; a
/// stuck serialization domain must not hold the agent's request hostage.
const HEAL_WAIT: Duration = Duration::from_secs(5);
/// The heal's own hard stop, off the turn path: it frees the single bridge
/// thread for the next request even when a chain wedges.
const HEAL_HARD_STOP: Duration = Duration::from_secs(30);

/// Renew this catalog's credentials and write them back where signing
/// reads them, following the refresh lock-order template. `None` means the
/// turn passes the 401 through: no healable row, a dead grant, or storage
/// that could not be written. Called on the bridge's runtime, never on the
/// listener's.
pub(super) async fn reauthorize_catalog(catalog_id: &str) -> Option<AccountSnapshot> {
    // Cheap gate before the serialization domain: only an OAuth or
    // token-import row has a refresh leg. Manual / api-key / cookie rows
    // cannot renew themselves, so their 401 is a user action, not a heal.
    let subscription_id = healable_row(catalog_id)?;
    skillstar_usage::refresh_guard::with_catalog_refresh(catalog_id, || async {
        let mut sub = storage::get_subscription(&subscription_id).ok()?;
        let cli_lease = usage_switch::acquire_cli_refresh_lease(&sub.catalog_id)
            .await
            .ok()?;
        // Adopt first: the CLI may already have rotated a newer generation
        // in. Spending the stored refresh token without adopting is the
        // 401-looking failure this order exists to prevent.
        usage_switch::adopt_active_cli_session_before_refresh(&mut sub, &cli_lease).ok()?;
        let should_sync_cli;
        match fetchers::refresh(&mut sub).await {
            Ok(usage) => {
                should_sync_cli = true;
                sub.requires_reauth = false;
                sub = storage::patch_fetcher_state(&sub).ok()?;
                storage::save_usage_snapshot(usage).ok()?;
            }
            Err(error) => {
                // Only a real auth verdict gives up; a transient failure may
                // still have rotated credentials worth one resend.
                let dead = matches!(error, UsageError::AuthRequired);
                let previous = storage::get_usage_snapshot(&sub.id).ok()?;
                let (latch_reauth, snapshot) = refresh_failure(&sub.id, previous.as_ref(), &error);
                if latch_reauth {
                    sub.requires_reauth = true;
                }
                sub = storage::patch_fetcher_state(&sub).ok()?;
                storage::save_usage_snapshot(snapshot).ok()?;
                if dead {
                    return None;
                }
                should_sync_cli = true;
            }
        }
        if should_sync_cli {
            // Mirror the template: a refresh (or a transient failure that
            // still rotated tokens) writes the newer generation back to the
            // CLI when this row is the active one. Not syncing is what once
            // let a SkillStar refresh revoke the CLI's own generation.
            let _ = usage_switch::sync_refreshed_active_subscription(&mut sub, &cli_lease);
        }
        snapshot_of(catalog_id)
    })
    .await
    .ok()
    .flatten()
}

/// The row a heal may touch: the pinned one first, else the catalog's first
/// row — the same pick signing uses — and only when that row has a refresh
/// leg to spend.
fn healable_row(catalog_id: &str) -> Option<String> {
    let rows = storage::list_subscriptions().ok()?;
    let active = storage::get_active_subscription(catalog_id).ok().flatten();
    let row = usage_switch::pinned_row(&rows, catalog_id, active.as_deref())?;
    matches!(row.auth_mode, AuthMode::OAuth | AuthMode::TokenImport)
        .then(|| row.id.clone())
}

/// Fresh signing material for the resend, read the same way signing reads
/// it — live-first through the custody seam, so the CLI's own rotation
/// still wins over the row.
fn snapshot_of(catalog_id: &str) -> Option<AccountSnapshot> {
    let material = signing_material(catalog_id)?;
    Some(AccountSnapshot {
        access_token: material.access_token,
        account_id: material.account_id,
        api_key: material.api_key,
    })
}

/// One queued heal.
struct HealRequest {
    catalog_id: String,
    reply: std::sync::mpsc::SyncSender<Option<AccountSnapshot>>,
}

/// The heal bridge: one long-lived thread with its own runtime, serving the
/// sync hook from any calling context. A single thread also single-flights
/// heals — a burst of 401s from several turns renews once; the losing turns
/// pass their 401 through and park in the auth rest, instead of queueing
/// and racing the vendor's token endpoint — and serializes heals with
/// nothing else, because the chain itself takes Usage's own locks.
struct HealBridge {
    requests: std::sync::mpsc::SyncSender<HealRequest>,
    /// The turn's wait budget.
    wait: Duration,
}

impl HealBridge {
    fn spawn(wait: Duration, hard_stop: Duration) -> Self {
        // Capacity one: an idle bridge takes the request at once, a busy
        // bridge keeps at most one waiting, and a burst beyond that is
        // refused on the spot — the queue can never grow with 401s.
        let (tx, rx) = std::sync::mpsc::sync_channel::<HealRequest>(1);
        std::thread::Builder::new()
            .name("skillstar-gateway-heal".to_string())
            .spawn(move || {
                let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                else {
                    return;
                };
                while let Ok(request) = rx.recv() {
                    let catalog_id = request.catalog_id.clone();
                    let outcome = runtime.block_on(async {
                        tokio::time::timeout(hard_stop, reauthorize_catalog(&catalog_id))
                            .await
                            .ok()
                            .flatten()
                    });
                    // The turn may have left already; a dropped reply is
                    // fine — the heal still completed, or hit its stop.
                    let _ = request.reply.send(outcome);
                }
            })
            .expect("spawn the gateway heal thread");
        Self { requests: tx, wait }
    }

    /// Ask for one heal and wait at most the turn's budget. `None` covers
    /// every give-up: no healable row, a dead grant, a busy serialization
    /// domain, a slow vendor, or a queue already holding one request —
    /// the losing turn passes the 401 through and parks in the auth rest
    /// like any give-up, instead of queueing behind the running heal.
    fn heal(&self, catalog_id: &str) -> Option<AccountSnapshot> {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        self.requests
            .try_send(HealRequest {
                catalog_id: catalog_id.to_string(),
                reply: tx,
            })
            .ok()?;
        rx.recv_timeout(self.wait).ok().flatten()
    }
}

/// The process-wide bridge the wrapped books share.
static HEALS: LazyLock<Arc<HealBridge>> =
    LazyLock::new(|| Arc::new(HealBridge::spawn(HEAL_WAIT, HEAL_HARD_STOP)));

/// The account book the gateway listener injects (slice 11): the usage book
/// for signing and allowances, plus the reauthorize hook behind the bridge.
/// Wrapping — instead of teaching the usage book to heal — keeps the
/// healing shape at the assembly seam, so the plain book and its module
/// know nothing about it.
pub struct HealingBook {
    inner: Box<dyn AccountBook + Send + Sync>,
    bridge: Arc<HealBridge>,
}

impl HealingBook {
    /// Wrap the production usage book with the production bridge.
    pub fn wrap(inner: Box<dyn AccountBook + Send + Sync>) -> Self {
        Self {
            inner,
            bridge: Arc::clone(&HEALS),
        }
    }

    /// Wrap with an explicit bridge (the tests' budgets).
    #[cfg(test)]
    fn on(inner: Box<dyn AccountBook + Send + Sync>, bridge: Arc<HealBridge>) -> Self {
        Self { inner, bridge }
    }
}

impl AccountBook for HealingBook {
    fn account(&self, catalog_id: &str) -> Option<AccountSnapshot> {
        self.inner.account(catalog_id)
    }

    fn allowance(&self, catalog_id: &str) -> Option<AllowanceSnapshot> {
        self.inner.allowance(catalog_id)
    }

    fn reauthorize(&self, catalog_id: &str) -> Option<AccountSnapshot> {
        self.bridge.heal(catalog_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{ENV_LOCK, EnvGuard};
    use skillstar_usage::crypto;
    use skillstar_usage::subscription::BillingCycle;
    use skillstar_usage::{AuthMode, Subscription};

    fn codex_row(id: &str, access: Option<&str>, refresh: Option<&str>) -> Subscription {
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
            access_token_encrypted: access.map(crypto::encrypt),
            refresh_token_encrypted: refresh.map(crypto::encrypt),
            // Already expired, so the fetcher refreshes first instead of
            // spending the stale access token on a real quota request —
            // that is the state a 401-triggered heal starts from.
            access_token_expires_at: Some(1),
            id_token_encrypted: None,
            oauth_account_id: Some("acct-1".to_string()),
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

    fn sandbox() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    /// The cross-runtime probe (slice 11), against the real chain: the sync
    /// hook must run Usage's serialization domain from inside an async
    /// execution context — the shape the gateway listener calls it in —
    /// without nesting runtimes, deadlocking, or panicking. The OAuth row
    /// has no refresh token, so the chain resolves offline to the
    /// dead-grant verdict: the latch is set and the card blanked, which is
    /// the proof the chain ran on the bridge's runtime.
    #[tokio::test(flavor = "current_thread")]
    async fn the_heal_runs_the_usage_chain_from_an_async_context() {
        let _lock = ENV_LOCK.lock().await;
        let root = sandbox();
        let _env = EnvGuard::set(&[
            ("SKILLSTAR_DATA_DIR", root.path()),
            ("SKILLSTAR_TOOL_SYNC_HOME", root.path()),
            ("HOME", root.path()),
        ]);
        storage::upsert_subscription(codex_row("codex-1", Some("stale-access"), None)).unwrap();

        let bridge = Arc::new(HealBridge::spawn(
            Duration::from_secs(4),
            Duration::from_secs(10),
        ));
        let started = std::time::Instant::now();
        assert!(
            bridge.heal("codex").is_none(),
            "no refresh leg: the hook must give up"
        );
        assert!(
            started.elapsed() < Duration::from_secs(4),
            "the offline give-up must not ride out the whole budget"
        );

        let row = storage::get_subscription("codex-1").unwrap();
        assert!(row.requires_reauth, "the dead-grant verdict latched");
        let snapshot = storage::get_usage_snapshot("codex-1")
            .unwrap()
            .expect("the blanked card");
        assert_eq!(snapshot.error.as_deref(), Some("登录已失效，请重新授权。"));
    }

    /// The degraded path (slice 11): a serialization domain held elsewhere
    /// — a desktop-side refresh, say, on another runtime — makes the turn
    /// give up within its budget and pass the 401 through, while the queued
    /// heal finishes off the turn path once the domain frees up.
    #[tokio::test(flavor = "current_thread")]
    async fn a_busy_serialization_domain_degrades_to_a_pass_through() {
        let _lock = ENV_LOCK.lock().await;
        let root = sandbox();
        let _env = EnvGuard::set(&[
            ("SKILLSTAR_DATA_DIR", root.path()),
            ("SKILLSTAR_TOOL_SYNC_HOME", root.path()),
            ("HOME", root.path()),
        ]);
        storage::upsert_subscription(codex_row("codex-1", Some("stale-access"), None)).unwrap();

        // Hold the codex domain on this (foreign) runtime, the way a
        // desktop-side refresh would while the gateway turn 401s.
        let holder = tokio::spawn(async {
            skillstar_usage::refresh_guard::with_catalog_lock("codex", || async {
                tokio::time::sleep(Duration::from_millis(600)).await;
            })
            .await
            .unwrap();
        });
        tokio::time::sleep(Duration::from_millis(100)).await;

        let bridge = Arc::new(HealBridge::spawn(
            Duration::from_millis(150),
            Duration::from_secs(5),
        ));
        let started = std::time::Instant::now();
        assert!(
            bridge.heal("codex").is_none(),
            "the turn does not wait out a busy domain"
        );
        assert!(
            started.elapsed() >= Duration::from_millis(140),
            "it waited its budget before giving up"
        );
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "no deadlock: the wait is bounded"
        );

        // The heal itself is not cancelled: once the domain frees up, the
        // bridge finishes the chain off the turn path and the dead-grant
        // latch lands for the next turn to see.
        let _ = holder.await;
        let deadline = std::time::Instant::now() + Duration::from_secs(4);
        while !storage::get_subscription("codex-1").unwrap().requires_reauth {
            assert!(
                std::time::Instant::now() < deadline,
                "the queued heal must finish off the turn path"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    /// The gate: rows without a refresh leg (manual, api-key, cookie) and
    /// unknown catalogs decline before entering the serialization domain —
    /// nothing latches, nothing waits.
    #[tokio::test(flavor = "current_thread")]
    async fn rows_without_a_refresh_leg_do_not_heal() {
        let _lock = ENV_LOCK.lock().await;
        let root = sandbox();
        let _env = EnvGuard::set(&[
            ("SKILLSTAR_DATA_DIR", root.path()),
            ("SKILLSTAR_TOOL_SYNC_HOME", root.path()),
            ("HOME", root.path()),
        ]);
        let mut manual = codex_row("codex-1", Some("pasted"), None);
        manual.auth_mode = AuthMode::Manual;
        storage::upsert_subscription(manual).unwrap();

        let bridge = Arc::new(HealBridge::spawn(
            Duration::from_secs(2),
            Duration::from_secs(5),
        ));
        let started = std::time::Instant::now();
        assert!(bridge.heal("codex").is_none(), "a manual row has no leg to spend");
        assert!(bridge.heal("no-such-catalog").is_none());
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "the gate declines before entering the domain"
        );
        assert!(!storage::get_subscription("codex-1").unwrap().requires_reauth);
    }

    /// The wrapper delegates signing and allowances to the wrapped book and
    /// sends the heal through its bridge — the shape the assembly seam
    /// injects.
    #[tokio::test(flavor = "current_thread")]
    async fn the_wrapped_book_delegates_and_heals_through_the_bridge() {
        let _lock = ENV_LOCK.lock().await;
        let root = sandbox();
        let _env = EnvGuard::set(&[
            ("SKILLSTAR_DATA_DIR", root.path()),
            ("SKILLSTAR_TOOL_SYNC_HOME", root.path()),
            ("HOME", root.path()),
        ]);

        struct FixedBook;
        impl AccountBook for FixedBook {
            fn account(&self, _catalog_id: &str) -> Option<AccountSnapshot> {
                Some(AccountSnapshot {
                    access_token: Some("kept".to_string()),
                    ..AccountSnapshot::default()
                })
            }
            fn allowance(&self, _catalog_id: &str) -> Option<AllowanceSnapshot> {
                Some(AllowanceSnapshot {
                    percent: 7.5,
                    renews_at: None,
                })
            }
        }

        let book = HealingBook::on(
            Box::new(FixedBook),
            Arc::new(HealBridge::spawn(
                Duration::from_millis(500),
                Duration::from_secs(2),
            )),
        );
        assert_eq!(
            book.account("codex").and_then(|account| account.access_token),
            Some("kept".to_string())
        );
        assert_eq!(book.allowance("codex").map(|a| a.percent), Some(7.5));
        // The empty sandbox has no row to heal: the hook answers through
        // the bridge and gives up, which is what the turn passes through.
        assert_eq!(
            book.reauthorize("codex").and_then(|account| account.access_token),
            None
        );
    }
}
