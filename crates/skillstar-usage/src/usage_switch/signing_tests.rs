//! Behaviour tests for the signing material seam (live-first),
//! specs/usage-models-evolution slice 02.
//!
//! Carries its own minimal harness, independent from `custody_tests.rs`
//! (that file is being split by the tree spec). Everything runs sandboxed:
//! `SKILLSTAR_DATA_DIR` + `SKILLSTAR_TOOL_SYNC_HOME` + `HOME` all point at
//! temp dirs — no real `$HOME`, no real CLI home, no macOS keychain.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tempfile::TempDir;

use crate::subscription::Subscription;
use crate::test_support::EnvGuard;
use crate::{crypto, storage};

use super::super::activate_subscription;
use super::{Freshness, SigningMaterial, signing_material};
use crate::usage_switch::target_for;

// sub=uid-dana / email=dana@example.com, empty signature — a codex row's id_token.
const DANA_ID_TOKEN: &str = concat!(
    "e30.",
    "eyJlbWFpbCI6ImRhbmFAZXhhbXBsZS5jb20iLCJzdWIiOiJ1aWQtZGFuYSIsImV4cCI6MTk5OTk5OTk5OX0",
    "."
);

// ── harness ──────────────────────────────────────────────────────────────

struct Sandbox {
    _env: EnvGuard,
    data: TempDir,
    home: TempDir,
}

fn sandbox() -> Sandbox {
    let data = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let env = EnvGuard::set(&[
        ("SKILLSTAR_DATA_DIR", data.path()),
        ("SKILLSTAR_TOOL_SYNC_HOME", home.path()),
        ("HOME", home.path()),
    ]);
    Sandbox {
        _env: env,
        data,
        home,
    }
}

impl Sandbox {
    fn live(&self, catalog: &str) -> PathBuf {
        target_for(catalog).unwrap().live_path().unwrap()
    }
}

fn write_json(path: &Path, value: &Value) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

fn subscription(id: &str, catalog_id: &str) -> Subscription {
    Subscription {
        id: id.into(),
        catalog_id: catalog_id.into(),
        display_name: id.into(),
        auth_mode: crate::AuthMode::OAuth,
        plan_tier: None,
        monthly_price: None,
        currency: "USD".into(),
        billing_cycle: crate::BillingCycle::Monthly,
        start_date: 0,
        renew_date: 0,
        auto_renew: false,
        api_key_encrypted: None,
        platform_token_encrypted: None,
        access_token_encrypted: None,
        refresh_token_encrypted: None,
        access_token_expires_at: None,
        id_token_encrypted: None,
        oauth_account_id: None,
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

fn codex_row(id: &str, token: &str, account_id: &str) -> Subscription {
    let mut sub = subscription(id, "codex");
    sub.access_token_encrypted = Some(crypto::encrypt(token));
    sub.id_token_encrypted = Some(crypto::encrypt(DANA_ID_TOKEN));
    sub.refresh_token_encrypted = Some(crypto::encrypt(&format!("{id}-refresh")));
    sub.oauth_account_id = Some(account_id.into());
    storage::upsert_subscription(sub).unwrap()
}

// ── three probe states → three Freshness values ──────────────────────────

#[tokio::test(flavor = "current_thread")]
async fn missing_falls_back_to_the_pinned_row() {
    let sb = sandbox();
    codex_row("codex-1", "row-token-1", "acct-1");
    codex_row("codex-2", "row-token-2", "acct-2");
    storage::set_active_subscription("codex", "codex-2").unwrap();
    // No live file: the CLI is not installed or logged out, the stored row is
    // the whole truth.
    assert!(!sb.live("codex").exists());

    let material = signing_material("codex").expect("pinned row material");
    assert_eq!(material.freshness, Freshness::Row);
    assert_eq!(material.access_token.as_deref(), Some("row-token-2"));
    assert_eq!(material.account_id.as_deref(), Some("acct-2"));
    assert_eq!(material.subscription_id.as_deref(), Some("codex-2"));
    assert!(material.api_key.is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn linked_reads_the_live_credential_not_the_stored_row() {
    let sb = sandbox();
    codex_row("codex-2", "row-token-2", "acct-2");
    storage::set_active_subscription("codex", "codex-2").unwrap();
    activate_subscription("codex-2").await.unwrap();

    // The CLI rotated its own token (rename() clobbered the link with a real
    // file; id_token and account_id survive, so the identity still
    // attributes): live is the token the CLI is actually sending — the row
    // fallback would sign an already-replaced token.
    let live = sb.live("codex");
    let mut root = serde_json::from_slice::<Value>(&fs::read(&live).unwrap()).unwrap();
    root["tokens"]["access_token"] = json!("rotated-by-cli");
    fs::remove_file(&live).unwrap();
    write_json(&live, &root);

    let material = signing_material("codex").expect("live material");
    assert_eq!(material.freshness, Freshness::Live);
    assert_eq!(material.access_token.as_deref(), Some("rotated-by-cli"));
    assert_eq!(material.subscription_id.as_deref(), Some("codex-2"));
}

#[tokio::test(flavor = "current_thread")]
async fn diverged_signs_but_cannot_be_attributed() {
    let sb = sandbox();
    codex_row("codex-1", "row-token-1", "acct-1");
    // A `codex login` done in a terminal: attributable to nobody, no identity
    // conflict.
    write_json(
        &sb.live("codex"),
        &json!({ "tokens": { "access_token": "unknown-terminal-login" } }),
    );

    let material = signing_material("codex").expect("orphan material");
    assert_eq!(material.freshness, Freshness::Diverged);
    assert_eq!(material.access_token.as_deref(), Some("unknown-terminal-login"));
    assert_eq!(
        material.subscription_id, None,
        "an orphan has no ledger attribution, but the token still signs"
    );
    assert!(material.account_id.is_none());
    assert!(material.api_key.is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn linked_material_survives_its_row_being_deleted() {
    let _sb = sandbox();
    codex_row("codex-dana", "live-token", "acct-dana");
    activate_subscription("codex-dana").await.unwrap();
    // Row deleted, snapshot kept (storage's delete path never touches
    // custody).
    storage::delete_subscription("codex-dana").unwrap();

    let material = signing_material("codex").expect("live material without a row");
    assert_eq!(material.freshness, Freshness::Live);
    assert_eq!(material.access_token.as_deref(), Some("live-token"));
    assert_eq!(material.subscription_id.as_deref(), Some("codex-dana"));
}

#[tokio::test(flavor = "current_thread")]
async fn catalogs_without_a_cli_target_read_the_row_directly() {
    let _sb = sandbox();
    let mut row = subscription("zed-1", "zed");
    row.api_key_encrypted = Some(crypto::encrypt("sk-zed-row"));
    storage::upsert_subscription(row).unwrap();

    let material = signing_material("zed").expect("row material");
    assert_eq!(material.freshness, Freshness::Row);
    assert_eq!(material.api_key.as_deref(), Some("sk-zed-row"));
    assert_eq!(material.subscription_id.as_deref(), Some("zed-1"));
}

// ── credential red line: Debug never leaks plaintext ─────────────────────

#[test]
fn debug_output_carries_no_plaintext_secrets() {
    // Constructed directly (precedent: gateway trace.rs's Debug assertion).
    let material = SigningMaterial {
        access_token: Some("sk-access-secret".into()),
        account_id: Some("acct-1".into()),
        api_key: Some("sk-api-secret".into()),
        subscription_id: Some("codex-1".into()),
        freshness: Freshness::Live,
    };
    let text = format!("{material:?}");
    assert!(!text.contains("sk-access-secret"), "{text}");
    assert!(!text.contains("sk-api-secret"), "{text}");
    assert!(text.contains("<redacted>"), "{text}");
    assert!(text.contains("acct-1"), "non-secret fields stay readable: {text}");
}

#[tokio::test(flavor = "current_thread")]
async fn real_material_debug_stays_redacted() {
    let sb = sandbox();
    codex_row("codex-1", "secret-live-token", "acct-1");
    activate_subscription("codex-1").await.unwrap();
    let live = signing_material("codex").expect("live material");
    assert_eq!(live.freshness, Freshness::Live);

    write_json(
        &sb.live("codex"),
        &json!({ "tokens": { "access_token": "secret-orphan-token" } }),
    );
    let diverged = signing_material("codex").expect("orphan material");
    let row = {
        fs::remove_file(sb.live("codex")).unwrap();
        signing_material("codex").expect("row material")
    };

    for material in [&live, &diverged, &row] {
        let text = format!("{material:?}");
        assert!(!text.contains("secret-live-token"), "{text}");
        assert!(!text.contains("secret-orphan-token"), "{text}");
        assert!(
            !text.contains("acct-1-refresh"),
            "refresh tokens stay out of Debug too: {text}"
        );
    }
}

// ── probe read-only discipline: no CLI home, no lock file, no snapshot dir ─

#[tokio::test(flavor = "current_thread")]
async fn reading_material_creates_no_cli_home_lock_or_snapshot_dir() {
    let sb = sandbox();
    // Empty home, no rows: run all three CLI targets through probe → Missing
    // → row fallback.
    for catalog in ["codex", "xai", "opencode"] {
        assert!(signing_material(catalog).is_none(), "{catalog}");
    }
    assert!(
        fs::read_dir(sb.home.path())
            .unwrap()
            .filter_map(Result::ok)
            .next()
            .is_none(),
        "probe must not create CLI home directories or lock files (the 'reconcile takes no lock, builds no home' discipline binds this seam too)"
    );
    assert!(
        !sb.data.path().join("accounts").exists(),
        "the read-only path must not create the snapshot directory"
    );
}
