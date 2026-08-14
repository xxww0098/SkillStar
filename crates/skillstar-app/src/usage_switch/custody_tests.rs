//! Behaviour spec for symlink custody.
//!
//! Every test runs against a sandboxed `SKILLSTAR_DATA_DIR` +
//! `SKILLSTAR_TOOL_SYNC_HOME`; nothing here may touch a real `$HOME`, a real
//! `~/.grok` / `~/.codex`, or the macOS login keychain (which the sandbox
//! env var also switches off — see `keychain::enabled`).

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use skillstar_usage::subscription::Subscription;
use skillstar_usage::{crypto, storage};
use tempfile::TempDir;

use super::custody::{Custody, LinkMode, LinkState};
use super::error::{CustodyError, MaterializeError};
use super::target::{CliCredentialTarget, codex::CodexTarget, opencode::OpenCodeTarget};
use super::{
    CliAccountState, acquire_cli_refresh_lease, activate_subscription,
    adopt_active_cli_session_before_refresh, forget_subscription_session, reconcile_cli_account,
    reconcile_cli_accounts, resync_active_subscription, sync_refreshed_active_subscription,
    target_for,
};
use crate::test_support::{ENV_LOCK, EnvGuard};

// Full Grok CLI scopes; identity alice.
const ALICE_CLI_TOKEN: &str = concat!(
    "e30.",
    "eyJzY29wZSI6Im9wZW5pZCBwcm9maWxlIGVtYWlsIG9mZmxpbmVfYWNjZXNzIGdyb2stY2xpOmFjY2VzcyBjb252ZXJzYXRpb25zOnJlYWQgY29udmVyc2F0aW9uczp3cml0ZSBhcGk6YWNjZXNzIiwiZW1haWwiOiJhbGljZUBleGFtcGxlLmNvbSIsInN1YiI6InVpZC1hbGljZSIsInByaW5jaXBhbF90eXBlIjoidXNlciIsInByaW5jaXBhbF9pZCI6InVpZC1hbGljZSIsInRlYW1faWQiOiJ0ZWFtLWFsaWNlIiwiY29kaW5nX2RhdGFfcmV0ZW50aW9uX29wdF9vdXQiOnRydWUsImV4cCI6MTk5OTk5OTk5OX0",
    "."
);
// Same account, later generation.
const ALICE_CLI_TOKEN_V2: &str = concat!(
    "e30.",
    "eyJzY29wZSI6Im9wZW5pZCBwcm9maWxlIGVtYWlsIG9mZmxpbmVfYWNjZXNzIGdyb2stY2xpOmFjY2VzcyBjb252ZXJzYXRpb25zOnJlYWQgY29udmVyc2F0aW9uczp3cml0ZSBhcGk6YWNjZXNzIiwiZW1haWwiOiJhbGljZUBleGFtcGxlLmNvbSIsInN1YiI6InVpZC1hbGljZSIsInByaW5jaXBhbF90eXBlIjoidXNlciIsInByaW5jaXBhbF9pZCI6InVpZC1hbGljZSIsInRlYW1faWQiOiJ0ZWFtLWFsaWNlIiwiY29kaW5nX2RhdGFfcmV0ZW50aW9uX29wdF9vdXQiOnRydWUsImV4cCI6MjAwMDAwOTk5OX0",
    "."
);
const BOB_CLI_TOKEN: &str = concat!(
    "e30.",
    "eyJzY29wZSI6Im9wZW5pZCBwcm9maWxlIGVtYWlsIG9mZmxpbmVfYWNjZXNzIGdyb2stY2xpOmFjY2VzcyBjb252ZXJzYXRpb25zOnJlYWQgY29udmVyc2F0aW9uczp3cml0ZSBhcGk6YWNjZXNzIiwiZW1haWwiOiJib2JAZXhhbXBsZS5jb20iLCJzdWIiOiJ1aWQtYm9iIiwicHJpbmNpcGFsX3R5cGUiOiJ1c2VyIiwicHJpbmNpcGFsX2lkIjoidWlkLWJvYiIsInRlYW1faWQiOiJ0ZWFtLWJvYiIsImNvZGluZ19kYXRhX3JldGVudGlvbl9vcHRfb3V0IjpmYWxzZSwiZXhwIjoxOTk5OTk5OTk5fQ",
    "."
);
// Authenticates for billing but is rejected by every Grok CLI call.
const BILLING_ONLY_TOKEN: &str = concat!(
    "e30.",
    "eyJzY29wZSI6Im9wZW5pZCBwcm9maWxlIGVtYWlsIG9mZmxpbmVfYWNjZXNzIGFwaTphY2Nlc3MiLCJlbWFpbCI6ImNhcm9sQGV4YW1wbGUuY29tIiwic3ViIjoidWlkLWNhcm9sIiwiZXhwIjoxOTk5OTk5OTk5fQ",
    "."
);
const CODEX_ID_TOKEN: &str = concat!(
    "e30.",
    "eyJlbWFpbCI6ImRhbmFAZXhhbXBsZS5jb20iLCJzdWIiOiJ1aWQtZGFuYSIsImV4cCI6MTk5OTk5OTk5OX0",
    "."
);

// ── harness ──────────────────────────────────────────────────────────────

/// Field order is the drop order: env is restored while the lock is still
/// held, so a parallel test can never observe a half-restored root.
struct Sandbox {
    _env: EnvGuard,
    data: TempDir,
    home: TempDir,
    _lock: tokio::sync::MutexGuard<'static, ()>,
}

async fn sandbox() -> Sandbox {
    let lock = ENV_LOCK.lock().await;
    let data = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let env = EnvGuard::set(&[
        ("SKILLSTAR_DATA_DIR", data.path()),
        ("SKILLSTAR_TOOL_SYNC_HOME", home.path()),
    ]);
    Sandbox {
        _env: env,
        data,
        home,
        _lock: lock,
    }
}

impl Sandbox {
    fn live(&self, catalog: &str) -> PathBuf {
        target_for(catalog).unwrap().live_path().unwrap()
    }

    fn snapshot(&self, catalog: &str, id: &str) -> PathBuf {
        self.data
            .path()
            .join("accounts")
            .join(catalog)
            .join(format!("{id}.json"))
    }

    fn snapshot_names(&self, catalog: &str) -> Vec<String> {
        let dir = self.data.path().join("accounts").join(catalog);
        let mut names: Vec<_> = fs::read_dir(dir)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .map(|entry| entry.file_name().to_string_lossy().to_string())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    fn custody(&self, catalog: &str) -> Custody {
        Custody::open(target_for(catalog).unwrap()).unwrap()
    }
}

fn write_json(path: &Path, value: &Value) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn is_symlink(path: &Path) -> bool {
    path.symlink_metadata()
        .map(|meta| meta.file_type().is_symlink())
        .unwrap_or(false)
}

fn grok_scope() -> String {
    format!(
        "https://auth.x.ai::{}",
        skillstar_usage::fetchers::oauth::xai::client_id()
    )
}

fn subscription(id: &str, catalog_id: &str) -> Subscription {
    Subscription {
        id: id.into(),
        catalog_id: catalog_id.into(),
        display_name: id.into(),
        auth_mode: skillstar_usage::AuthMode::OAuth,
        plan_tier: None,
        monthly_price: None,
        currency: "USD".into(),
        billing_cycle: skillstar_usage::BillingCycle::Monthly,
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
        cookie_jar_encrypted: None,
        cookie_session_expires_at: None,
        manual_quota: None,
        note: None,
        sort_index: 0,
        created_at: 0,
        updated_at: 0,
    }
}

fn grok_account(id: &str, token: &str, expires_at: i64) -> Subscription {
    let mut sub = subscription(id, "xai");
    sub.access_token_encrypted = Some(crypto::encrypt(token));
    sub.refresh_token_encrypted = Some(crypto::encrypt(&format!("{id}-refresh")));
    sub.access_token_expires_at = Some(expires_at);
    storage::upsert_subscription(sub).unwrap()
}

fn grok_entry(token: &str, email: &str, user_id: &str) -> Value {
    json!({
        "key": token,
        "refresh_token": format!("{user_id}-cli-refresh"),
        "expires_at": "2033-05-18T03:33:19.000000Z",
        "email": email,
        "user_id": user_id,
        "principal_id": user_id,
        "principal_type": "user",
        "team_id": format!("team-{user_id}"),
        "first_name": email.split('@').next().unwrap(),
        "last_name": "",
        "profile_image_asset_id": "",
        "create_time": "2026-01-01T00:00:00.000000Z",
        "coding_data_retention_opt_out": false,
        "auth_mode": "oidc",
        "oidc_issuer": "https://auth.x.ai",
        "oidc_client_id": skillstar_usage::fetchers::oauth::xai::client_id(),
    })
}

// ── activate ─────────────────────────────────────────────────────────────

#[tokio::test(flavor = "current_thread")]
async fn activate_links_the_live_path_at_the_snapshot_and_keeps_sibling_blocks() {
    let sb = sandbox().await;
    grok_account("grok-alice", ALICE_CLI_TOKEN, 1_999_999_999);
    // The CLI is currently logged in as somebody SkillStar does not know,
    // and the file also carries Grok's legacy sign-in block.
    write_json(
        &sb.live("xai"),
        &json!({
            grok_scope(): grok_entry(BOB_CLI_TOKEN, "bob@example.com", "uid-bob"),
            "https://accounts.x.ai/sign-in": { "key": "legacy-session" },
        }),
    );

    let result = activate_subscription("grok-alice").await.unwrap();

    assert!(
        result.switch_result.success,
        "{:?}",
        result.switch_result.error
    );
    assert!(
        is_symlink(&sb.live("xai")),
        "the live path must become a link, not a second copy"
    );
    assert_eq!(
        fs::read_link(sb.live("xai")).unwrap(),
        sb.snapshot("xai", "grok-alice")
    );
    let live = read_json(&sb.live("xai"));
    assert_eq!(live[grok_scope()]["key"], ALICE_CLI_TOKEN);
    assert_eq!(live[grok_scope()]["email"], "alice@example.com");
    assert_eq!(
        live["https://accounts.x.ai/sign-in"]["key"], "legacy-session",
        "a sibling block in the same file must survive the swap"
    );
    assert_eq!(
        storage::get_active_subscription("xai").unwrap().as_deref(),
        Some("grok-alice")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn a_login_made_in_the_cli_is_captured_not_discarded() {
    let sb = sandbox().await;
    grok_account("grok-alice", ALICE_CLI_TOKEN, 1_999_999_999);
    write_json(
        &sb.live("xai"),
        &json!({ grok_scope(): grok_entry(BOB_CLI_TOKEN, "bob@example.com", "uid-bob") }),
    );

    activate_subscription("grok-alice").await.unwrap();

    let orphan = sb
        .snapshot_names("xai")
        .into_iter()
        .find(|name| name.starts_with("external-"))
        .expect("the unknown CLI session must be kept as a snapshot");
    let kept = read_json(&sb.data.path().join("accounts/xai").join(&orphan));
    assert_eq!(kept[grok_scope()]["key"], BOB_CLI_TOKEN);
    assert!(
        fs::read_dir(sb.home.path().join(".grok"))
            .unwrap()
            .filter_map(Result::ok)
            .any(|entry| entry.file_name().to_string_lossy().contains(".bak.")),
        "the replaced real file must also leave a rolling backup"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn a_failed_activation_leaves_the_old_pin_and_the_old_live_file_alone() {
    let sb = sandbox().await;
    grok_account("grok-alice", ALICE_CLI_TOKEN, 1_999_999_999);
    // Only `api:access`: authenticates for billing, rejected by the CLI.
    let mut carol = subscription("grok-carol", "xai");
    carol.access_token_encrypted = Some(crypto::encrypt(BILLING_ONLY_TOKEN));
    carol.refresh_token_encrypted = Some(crypto::encrypt("carol-refresh"));
    carol.access_token_expires_at = Some(1_999_999_999);
    storage::upsert_subscription(carol).unwrap();
    activate_subscription("grok-alice").await.unwrap();
    let before = read_json(&sb.live("xai"));

    let result = activate_subscription("grok-carol").await.unwrap();

    assert!(!result.switch_result.success);
    // Rejected before the live path was ever touched (`Stage::BeforeReplace`),
    // and the message the user reads is generated by the variant.
    assert_eq!(
        result.switch_result.error.as_deref(),
        Some(
            MaterializeError::MissingCliScopes {
                tool: "Grok",
                missing: vec![
                    "grok-cli:access".into(),
                    "conversations:read".into(),
                    "conversations:write".into(),
                ],
            }
            .to_string()
            .as_str()
        )
    );
    assert_eq!(
        storage::get_active_subscription("xai").unwrap().as_deref(),
        Some("grok-alice"),
        "a rejected switch must not move the pin"
    );
    assert_eq!(read_json(&sb.live("xai")), before);
    assert_eq!(
        fs::read_link(sb.live("xai")).unwrap(),
        sb.snapshot("xai", "grok-alice")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn a_snapshot_that_serves_nobody_is_rolled_back_to_the_previous_real_file() {
    let sb = sandbox().await;
    let mut sub = subscription("oc-go", "opencode");
    sub.auth_mode = skillstar_usage::AuthMode::Cookie;
    storage::upsert_subscription(sub).unwrap();
    // A previously captured file that happens to hold no OpenCode login at
    // all — activating it would leave the CLI logged out, so it must fail
    // *and* put the working file back.
    write_json(
        &sb.snapshot("opencode", "oc-go"),
        &json!({ "anthropic": { "type": "api", "key": "sk-anthropic" } }),
    );
    let working = json!({ "opencode": { "type": "api", "key": "sk-working" } });
    write_json(&sb.live("opencode"), &working);

    let result = activate_subscription("oc-go").await.unwrap();

    assert!(!result.switch_result.success);
    // The other half of the staging contract: this one failed *after* the
    // live path had been replaced (`Stage::AfterReplace`), which is why the
    // working file below had to be restored rather than merely left alone.
    assert_eq!(
        result.switch_result.error.as_deref(),
        Some(
            CustodyError::ReadBackMismatch {
                tool: "opencode",
                observed: LinkState::Missing,
            }
            .to_string()
            .as_str()
        )
    );
    assert!(
        storage::get_active_subscription("opencode")
            .unwrap()
            .is_none()
    );
    assert_eq!(read_json(&sb.live("opencode")), working);
    assert!(!is_symlink(&sb.live("opencode")));
}

// ── reconcile: compare content, never file type ──────────────────────────

#[tokio::test(flavor = "current_thread")]
async fn a_real_file_with_the_same_token_is_linked_not_diverged() {
    let sb = sandbox().await;
    grok_account("grok-alice", ALICE_CLI_TOKEN, 1_999_999_999);
    activate_subscription("grok-alice").await.unwrap();

    // What macOS Claude Code / Codex do after every run, and what any CLI
    // that writes with rename() does: the link becomes a real file again.
    let mirrored = fs::read(sb.live("xai")).unwrap();
    fs::remove_file(sb.live("xai")).unwrap();
    fs::write(sb.live("xai"), mirrored).unwrap();
    assert!(!is_symlink(&sb.live("xai")));

    let custody = sb.custody("xai");
    assert_eq!(
        custody.probe().unwrap(),
        LinkState::LinkedTo("grok-alice".into()),
        "identical content is not divergence"
    );
    assert_eq!(
        custody.reconcile().unwrap(),
        LinkState::LinkedTo("grok-alice".into())
    );
    assert!(
        is_symlink(&sb.live("xai")),
        "reconcile must silently rebuild the clobbered link"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn reconcile_reports_missing_and_diverged() {
    let sb = sandbox().await;
    grok_account("grok-alice", ALICE_CLI_TOKEN, 1_999_999_999);
    let custody = sb.custody("xai");
    assert_eq!(custody.probe().unwrap(), LinkState::Missing);

    // Present but holding no credential is a logged-out CLI, not divergence.
    write_json(&sb.live("xai"), &json!({}));
    assert_eq!(custody.probe().unwrap(), LinkState::Missing);

    write_json(
        &sb.live("xai"),
        &json!({ grok_scope(): grok_entry(BOB_CLI_TOKEN, "bob@example.com", "uid-bob") }),
    );
    assert_eq!(custody.probe().unwrap(), LinkState::Diverged);

    activate_subscription("grok-alice").await.unwrap();
    assert_eq!(
        sb.custody("xai").probe().unwrap(),
        LinkState::LinkedTo("grok-alice".into())
    );
}

#[tokio::test(flavor = "current_thread")]
async fn a_cli_side_rotation_reaches_the_snapshot_with_no_copy_back_logic() {
    let sb = sandbox().await;
    grok_account("grok-alice", ALICE_CLI_TOKEN, 1_999_999_999);
    activate_subscription("grok-alice").await.unwrap();

    // The CLI refreshes and writes its own file. It is the snapshot.
    let mut root = read_json(&sb.live("xai"));
    root[grok_scope()]["key"] = json!(ALICE_CLI_TOKEN_V2);
    root[grok_scope()]["refresh_token"] = json!("r2-rotated-by-grok");
    root[grok_scope()]["expires_at"] = json!("2033-05-18T03:33:19.000000Z");
    fs::write(sb.live("xai"), serde_json::to_vec_pretty(&root).unwrap()).unwrap();

    let snapshot = read_json(&sb.snapshot("xai", "grok-alice"));
    assert_eq!(
        snapshot[grok_scope()]["key"],
        ALICE_CLI_TOKEN_V2,
        "writing through the link updates the snapshot by construction"
    );

    // …and the row still has to learn about it, or SkillStar would spend a
    // refresh token the CLI already revoked.
    sb.custody("xai").reconcile().unwrap();
    let row = storage::get_subscription("grok-alice").unwrap();
    assert_eq!(
        crypto::decrypt(row.access_token_encrypted.as_deref().unwrap()),
        ALICE_CLI_TOKEN_V2
    );
    assert_eq!(
        crypto::decrypt(row.refresh_token_encrypted.as_deref().unwrap()),
        "r2-rotated-by-grok"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn a_rotation_written_into_a_clobbered_real_file_is_carried_into_the_snapshot() {
    // Both halves of the `rename()` hole at once, and the Windows copy
    // degradation's sync-back: the live path is a real file again *and* the
    // token in it moved on, so content equality alone cannot save us.
    let sb = sandbox().await;
    grok_account("grok-alice", ALICE_CLI_TOKEN, 1_999_999_999);
    activate_subscription("grok-alice").await.unwrap();

    let mut root = read_json(&sb.live("xai"));
    fs::remove_file(sb.live("xai")).unwrap();
    root[grok_scope()]["key"] = json!(ALICE_CLI_TOKEN_V2);
    root[grok_scope()]["expires_at"] = json!("2033-05-18T03:33:19.000000Z");
    write_json(&sb.live("xai"), &root);

    assert_eq!(
        sb.custody("xai").reconcile().unwrap(),
        LinkState::LinkedTo("grok-alice".into()),
        "identity keeps the account attributable across a rotation"
    );
    assert_eq!(
        read_json(&sb.snapshot("xai", "grok-alice"))[grok_scope()]["key"],
        ALICE_CLI_TOKEN_V2
    );
    assert!(is_symlink(&sb.live("xai")));
}

#[tokio::test(flavor = "current_thread")]
async fn the_pin_is_a_cache_and_the_file_wins() {
    let sb = sandbox().await;
    grok_account("grok-alice", ALICE_CLI_TOKEN, 1_999_999_999);
    grok_account("grok-bob", BOB_CLI_TOKEN, 1_999_999_999);
    activate_subscription("grok-bob").await.unwrap();
    activate_subscription("grok-alice").await.unwrap();

    // The user switched back inside the CLI, behind SkillStar's back.
    fs::remove_file(sb.live("xai")).unwrap();
    write_json(&sb.live("xai"), &read_json(&sb.snapshot("xai", "grok-bob")));

    assert_eq!(
        storage::get_active_subscription("xai").unwrap().as_deref(),
        Some("grok-alice"),
        "the pin still says alice…"
    );
    assert_eq!(
        sb.custody("xai").probe().unwrap(),
        LinkState::LinkedTo("grok-bob".into()),
        "…but the file is the truth"
    );
    // And the truth is what the badge reads: the pin never reaches the UI as
    // an answer to "which account is the CLI on".
    assert_eq!(
        reconcile_cli_accounts().await.unwrap().get("xai"),
        Some(&CliAccountState::LinkedTo {
            subscription_id: "grok-bob".into()
        }),
    );
}

// ── what the badge reads ─────────────────────────────────────────────────

#[tokio::test(flavor = "current_thread")]
async fn a_logged_out_cli_reads_as_missing_not_as_a_pinned_account() {
    let sb = sandbox().await;
    grok_account("grok-alice", ALICE_CLI_TOKEN, 1_999_999_999);
    activate_subscription("grok-alice").await.unwrap();

    // `grok logout`, or simply a machine where the credential was cleaned out.
    fs::remove_file(sb.live("xai")).unwrap();

    assert_eq!(
        storage::get_active_subscription("xai").unwrap().as_deref(),
        Some("grok-alice"),
        "the pin outlives the credential…"
    );
    assert_eq!(
        reconcile_cli_account("xai").await.unwrap(),
        Some(CliAccountState::Missing),
        "…so 'alice is current' would be a claim about a file that is gone"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn a_login_nobody_owns_reads_as_diverged_not_as_logged_out() {
    let sb = sandbox().await;
    grok_account("grok-alice", ALICE_CLI_TOKEN, 1_999_999_999);
    activate_subscription("grok-alice").await.unwrap();

    // The user ran `grok auth login` in a terminal as somebody else.
    fs::remove_file(sb.live("xai")).unwrap();
    write_json(
        &sb.live("xai"),
        &json!({ grok_scope(): grok_entry(BILLING_ONLY_TOKEN, "carol@example.com", "uid-carol") }),
    );

    assert_eq!(
        reconcile_cli_account("xai").await.unwrap(),
        Some(CliAccountState::Diverged),
        "somebody is logged in — just nobody SkillStar can name"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn a_catalog_without_a_cli_has_no_live_state_to_report() {
    let _sb = sandbox().await;
    assert_eq!(
        reconcile_cli_account("cursor").await.unwrap(),
        None,
        "an IDE keeps its credentials in state.vscdb; the pin is all there is"
    );
    assert!(!reconcile_cli_accounts().await.unwrap().contains_key("cursor"));
}

// ── Windows: a copy is not a symlink, and must not be described as one ────

/// Stand in for Windows refusing `symlink_file` without developer mode: the
/// staging name custody links through is occupied by a directory, so creating
/// the link fails exactly where it fails there.
fn block_symlink_creation(live: &Path) {
    let mut name = live.file_name().unwrap().to_os_string();
    name.push(format!(".skillstar-{}.link", std::process::id()));
    fs::create_dir_all(live.with_file_name(name)).unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn a_degraded_copy_binding_is_reported_and_not_called_a_symlink() {
    let sb = sandbox().await;
    grok_account("grok-alice", ALICE_CLI_TOKEN, 1_999_999_999);
    fs::create_dir_all(sb.live("xai").parent().unwrap()).unwrap();
    block_symlink_creation(&sb.live("xai"));

    let outcome = activate_subscription("grok-alice")
        .await
        .unwrap()
        .switch_result;

    assert!(outcome.success, "a copy still switches the account");
    assert_eq!(
        outcome.link_mode,
        Some(LinkMode::Copy),
        "the CLI's own rotations no longer write through — the user has to be told"
    );
    assert!(!is_symlink(&sb.live("xai")));
    assert_eq!(
        read_json(&sb.live("xai"))[grok_scope()]["key"],
        ALICE_CLI_TOKEN
    );
    assert_eq!(
        sb.custody("xai").probe().unwrap(),
        LinkState::LinkedTo("grok-alice".into()),
        "a copy is still the account the user asked for"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn a_symlink_binding_says_so() {
    let sb = sandbox().await;
    grok_account("grok-alice", ALICE_CLI_TOKEN, 1_999_999_999);

    let outcome = activate_subscription("grok-alice")
        .await
        .unwrap()
        .switch_result;

    assert_eq!(outcome.link_mode, Some(LinkMode::Symlink));
    assert!(is_symlink(&sb.live("xai")));
}

// ── forget ───────────────────────────────────────────────────────────────

#[tokio::test(flavor = "current_thread")]
async fn forgetting_the_active_account_leaves_a_usable_real_file() {
    let sb = sandbox().await;
    grok_account("grok-alice", ALICE_CLI_TOKEN, 1_999_999_999);
    activate_subscription("grok-alice").await.unwrap();

    forget_subscription_session("xai", "grok-alice").unwrap();

    assert!(
        !sb.snapshot("xai", "grok-alice").exists(),
        "the snapshot is the credential; deleting the account deletes it"
    );
    assert!(
        !is_symlink(&sb.live("xai")),
        "a dangling link is not 'logged out', it is 'cannot log in'"
    );
    assert_eq!(
        read_json(&sb.live("xai"))[grok_scope()]["key"],
        ALICE_CLI_TOKEN
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(sb.live("xai")).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn forgetting_an_inactive_account_does_not_touch_the_live_file() {
    let sb = sandbox().await;
    grok_account("grok-alice", ALICE_CLI_TOKEN, 1_999_999_999);
    grok_account("grok-bob", BOB_CLI_TOKEN, 1_999_999_999);
    activate_subscription("grok-bob").await.unwrap();
    activate_subscription("grok-alice").await.unwrap();

    forget_subscription_session("xai", "grok-bob").unwrap();

    assert!(!sb.snapshot("xai", "grok-bob").exists());
    assert!(is_symlink(&sb.live("xai")));
    assert_eq!(
        read_json(&sb.live("xai"))[grok_scope()]["key"],
        ALICE_CLI_TOKEN
    );
}

// ── the refresh window ───────────────────────────────────────────────────

#[tokio::test(flavor = "current_thread")]
async fn the_refresh_window_adopts_the_cli_generation_and_projects_ours() {
    let sb = sandbox().await;
    grok_account("grok-alice", ALICE_CLI_TOKEN, 1_999_999_999);
    activate_subscription("grok-alice").await.unwrap();

    // Grok rotated first.
    let mut root = read_json(&sb.live("xai"));
    root[grok_scope()]["key"] = json!(ALICE_CLI_TOKEN_V2);
    root[grok_scope()]["expires_at"] = json!("2033-05-18T03:33:19.000000Z");
    // A field only the CLI knows about — no JWT claim can reproduce it.
    root[grok_scope()]["profile_image_asset_id"] = json!("asset-123");
    fs::write(sb.live("xai"), serde_json::to_vec_pretty(&root).unwrap()).unwrap();

    let lease = acquire_cli_refresh_lease("xai").await.unwrap();
    let mut row = storage::get_subscription("grok-alice").unwrap();
    adopt_active_cli_session_before_refresh(&mut row, &lease).unwrap();
    assert_eq!(
        crypto::decrypt(row.access_token_encrypted.as_deref().unwrap()),
        ALICE_CLI_TOKEN_V2,
        "never spend a refresh token the CLI already replaced"
    );

    // Now SkillStar's own refresh produces the next generation.
    row.access_token_encrypted = Some(crypto::encrypt(ALICE_CLI_TOKEN));
    row.access_token_expires_at = Some(2_100_000_000);
    row.refresh_token_encrypted = Some(crypto::encrypt("r3-from-skillstar"));
    let row = storage::patch_oauth_credentials(&row).unwrap();
    let mut row = row;
    let outcome = sync_refreshed_active_subscription(&mut row, &lease)
        .unwrap()
        .expect("the active account must be projected");

    assert!(outcome.success, "{:?}", outcome.error);
    let live = read_json(&sb.live("xai"));
    assert_eq!(live[grok_scope()]["key"], ALICE_CLI_TOKEN);
    assert_eq!(live[grok_scope()]["refresh_token"], "r3-from-skillstar");
    assert_eq!(
        live[grok_scope()]["profile_image_asset_id"],
        "asset-123",
        "the same account's CLI-private metadata survives a projection"
    );
    assert_eq!(live[grok_scope()]["team_id"], "team-alice");
}

#[tokio::test(flavor = "current_thread")]
async fn the_refresh_window_is_a_no_op_for_an_account_that_is_not_current() {
    let sb = sandbox().await;
    grok_account("grok-alice", ALICE_CLI_TOKEN, 1_999_999_999);
    let mut bob = grok_account("grok-bob", BOB_CLI_TOKEN, 1_999_999_999);
    activate_subscription("grok-alice").await.unwrap();

    let lease = acquire_cli_refresh_lease("xai").await.unwrap();
    adopt_active_cli_session_before_refresh(&mut bob, &lease).unwrap();
    assert!(
        sync_refreshed_active_subscription(&mut bob, &lease)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        read_json(&sb.live("xai"))[grok_scope()]["key"],
        ALICE_CLI_TOKEN
    );
}

#[tokio::test(flavor = "current_thread")]
async fn catalogs_without_a_credential_file_take_no_lease_and_still_pin() {
    let _sb = sandbox().await;
    let mut sub = subscription("cursor-1", "cursor");
    sub.access_token_encrypted = Some(crypto::encrypt("cursor-token"));
    storage::upsert_subscription(sub).unwrap();

    let result = activate_subscription("cursor-1").await.unwrap();

    assert!(!result.switch_result.success);
    assert!(
        result.switch_result.error.is_none(),
        "an IDE is not a failed switch, it is not a switch"
    );
    assert_eq!(
        storage::get_active_subscription("cursor")
            .unwrap()
            .as_deref(),
        Some("cursor-1"),
        "the pin is still a UI preference for non-CLI catalogs"
    );
    let lease = acquire_cli_refresh_lease("cursor").await.unwrap();
    let mut row = storage::get_subscription("cursor-1").unwrap();
    assert!(
        sync_refreshed_active_subscription(&mut row, &lease)
            .unwrap()
            .is_none()
    );
}

// ── OpenCode: the auth_mode deadlock is gone ─────────────────────────────

#[tokio::test(flavor = "current_thread")]
async fn opencode_switches_by_capturing_the_cli_login_without_any_api_key() {
    let sb = sandbox().await;
    // Cookie auth mode, so `api_key_encrypted` can never be populated — this
    // is the account shape that made the old switch path unreachable.
    let mut sub = subscription("oc-go", "opencode");
    sub.auth_mode = skillstar_usage::AuthMode::Cookie;
    storage::upsert_subscription(sub).unwrap();
    write_json(
        &sb.live("opencode"),
        &json!({
            "opencode": { "type": "api", "key": "sk-from-opencode-auth-login" },
            "anthropic": { "type": "oauth", "refresh": "r", "access": "a", "expires": 1 },
        }),
    );

    let result = activate_subscription("oc-go").await.unwrap();

    assert!(
        result.switch_result.success,
        "{:?}",
        result.switch_result.error
    );
    assert!(is_symlink(&sb.live("opencode")));
    let snapshot = read_json(&sb.snapshot("opencode", "oc-go"));
    assert_eq!(snapshot["opencode"]["key"], "sk-from-opencode-auth-login");
    assert_eq!(
        snapshot["anthropic"]["refresh"], "r",
        "another provider's login travels with the snapshot"
    );
    assert_eq!(
        storage::get_active_subscription("opencode")
            .unwrap()
            .as_deref(),
        Some("oc-go")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn opencode_still_projects_an_api_key_when_the_row_has_one() {
    let sb = sandbox().await;
    let mut sub = subscription("oc-key", "opencode");
    sub.auth_mode = skillstar_usage::AuthMode::ApiKey;
    sub.api_key_encrypted = Some(crypto::encrypt("sk-from-skillstar"));
    storage::upsert_subscription(sub).unwrap();
    write_json(
        &sb.live("opencode"),
        &json!({ "anthropic": { "type": "api", "key": "sk-anthropic" } }),
    );

    let result = activate_subscription("oc-key").await.unwrap();

    assert!(
        result.switch_result.success,
        "{:?}",
        result.switch_result.error
    );
    let live = read_json(&sb.live("opencode"));
    assert_eq!(live["opencode"]["type"], "api");
    assert_eq!(live["opencode"]["key"], "sk-from-skillstar");
    assert_eq!(live["anthropic"]["key"], "sk-anthropic");
    assert!(
        live.get("skillstar").is_none(),
        "the invented provider key must stay gone"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn opencode_reports_the_missing_credential_instead_of_a_silent_dead_end() {
    let _sb = sandbox().await;
    let mut sub = subscription("oc-empty", "opencode");
    sub.auth_mode = skillstar_usage::AuthMode::Cookie;
    storage::upsert_subscription(sub).unwrap();

    let result = activate_subscription("oc-empty").await.unwrap();

    assert!(!result.switch_result.success);
    assert_eq!(
        result.switch_result.error.as_deref(),
        Some(
            MaterializeError::NoCapturedSession { tool: "OpenCode" }
                .to_string()
                .as_str()
        ),
        "the dead end has to name the CLI the user must log into"
    );
}

// ── Codex ────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "current_thread")]
async fn codex_activation_writes_the_cli_token_schema() {
    let sb = sandbox().await;
    let mut sub = subscription("codex-dana", "codex");
    sub.access_token_encrypted = Some(crypto::encrypt("codex-access"));
    sub.id_token_encrypted = Some(crypto::encrypt(CODEX_ID_TOKEN));
    sub.refresh_token_encrypted = Some(crypto::encrypt("codex-refresh"));
    sub.oauth_account_id = Some("uid-dana".into());
    storage::upsert_subscription(sub).unwrap();

    let result = activate_subscription("codex-dana").await.unwrap();

    assert!(
        result.switch_result.success,
        "{:?}",
        result.switch_result.error
    );
    let live = read_json(&sb.live("codex"));
    assert!(live["OPENAI_API_KEY"].is_null());
    assert_eq!(live["tokens"]["access_token"], "codex-access");
    assert_eq!(live["tokens"]["id_token"], CODEX_ID_TOKEN);
    assert_eq!(live["tokens"]["refresh_token"], "codex-refresh");
    assert_eq!(live["tokens"]["account_id"], "uid-dana");
    assert!(live["last_refresh"].is_string());
    assert!(is_symlink(&sb.live("codex")));
}

#[tokio::test(flavor = "current_thread")]
async fn codex_missing_id_token_fails_without_moving_the_pin() {
    let _sb = sandbox().await;
    let mut sub = subscription("codex-dana", "codex");
    sub.access_token_encrypted = Some(crypto::encrypt("codex-access"));
    storage::upsert_subscription(sub).unwrap();

    let result = activate_subscription("codex-dana").await.unwrap();

    assert!(!result.switch_result.success);
    assert_eq!(
        result.switch_result.error.as_deref(),
        Some(
            MaterializeError::MissingSecret {
                tool: "Codex",
                field: "id_token",
                remedy: "请重新登录该账号补充凭证",
            }
            .to_string()
            .as_str()
        )
    );
    assert!(storage::get_active_subscription("codex").unwrap().is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn codex_absorbs_a_rotation_the_cli_wrote_into_the_snapshot() {
    let sb = sandbox().await;
    let mut sub = subscription("codex-dana", "codex");
    sub.access_token_encrypted = Some(crypto::encrypt("codex-access"));
    sub.id_token_encrypted = Some(crypto::encrypt(CODEX_ID_TOKEN));
    sub.refresh_token_encrypted = Some(crypto::encrypt("codex-refresh"));
    storage::upsert_subscription(sub).unwrap();
    activate_subscription("codex-dana").await.unwrap();

    let mut root = read_json(&sb.live("codex"));
    root["tokens"]["access_token"] = json!("codex-access-v2");
    root["tokens"]["refresh_token"] = json!("codex-refresh-v2");
    fs::write(sb.live("codex"), serde_json::to_vec_pretty(&root).unwrap()).unwrap();

    assert_eq!(
        sb.custody("codex").reconcile().unwrap(),
        LinkState::LinkedTo("codex-dana".into())
    );
    let row = storage::get_subscription("codex-dana").unwrap();
    assert_eq!(
        crypto::decrypt(row.access_token_encrypted.as_deref().unwrap()),
        "codex-access-v2"
    );
    assert_eq!(
        crypto::decrypt(row.refresh_token_encrypted.as_deref().unwrap()),
        "codex-refresh-v2"
    );
}

// ── resync ───────────────────────────────────────────────────────────────

#[tokio::test(flavor = "current_thread")]
async fn resync_rebinds_the_current_account_without_changing_the_pin() {
    let sb = sandbox().await;
    grok_account("grok-alice", ALICE_CLI_TOKEN, 1_999_999_999);
    activate_subscription("grok-alice").await.unwrap();
    fs::remove_file(sb.live("xai")).unwrap();

    let outcome = resync_active_subscription("xai").await.unwrap();

    assert!(outcome.success, "{:?}", outcome.error);
    assert!(is_symlink(&sb.live("xai")));
    assert_eq!(
        storage::get_active_subscription("xai").unwrap().as_deref(),
        Some("grok-alice")
    );
}

// ── file-level invariants ────────────────────────────────────────────────

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn a_snapshot_is_never_world_readable() {
    use std::os::unix::fs::PermissionsExt;

    let sb = sandbox().await;
    grok_account("grok-alice", ALICE_CLI_TOKEN, 1_999_999_999);
    activate_subscription("grok-alice").await.unwrap();

    let mode = fs::metadata(sb.snapshot("xai", "grok-alice"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[tokio::test(flavor = "current_thread")]
async fn grok_shares_the_cli_lock_file_and_writes_its_holder_line() {
    let sb = sandbox().await;
    let custody = sb.custody("xai");
    let _lease = custody.lock().unwrap();

    let official = sb.home.path().join(".grok").join("auth.json.lock");
    let holder = fs::read_to_string(&official).unwrap();
    assert!(
        holder.starts_with(&format!("{}:", std::process::id())),
        "{holder}"
    );
    assert!(
        !sb.home
            .path()
            .join(".grok")
            .join("auth.json.skillstar.lock")
            .exists(),
        "Grok's own lock is the one that keeps the CLI out"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn a_cli_without_an_official_lock_gets_a_private_one() {
    let sb = sandbox().await;
    let custody = sb.custody("codex");
    let _lease = custody.lock().unwrap();

    assert!(
        sb.home
            .path()
            .join(".codex")
            .join("auth.json.skillstar.lock")
            .exists()
    );
}

// ── target-level units (no filesystem) ───────────────────────────────────

#[test]
fn codex_access_token_covers_both_oauth_and_api_key_shapes() {
    let target = CodexTarget;
    assert_eq!(
        target
            .access_token(&json!({ "tokens": { "access_token": "at" } }))
            .as_deref(),
        Some("at")
    );
    assert_eq!(
        target
            .access_token(&json!({ "OPENAI_API_KEY": "sk-1" }))
            .as_deref(),
        Some("sk-1")
    );
    assert!(
        target
            .access_token(&json!({ "OPENAI_API_KEY": null }))
            .is_none()
    );
}

#[test]
fn opencode_expiry_is_read_as_milliseconds() {
    let target = OpenCodeTarget;
    let root =
        json!({ "opencode": { "type": "oauth", "access": "a", "expires": 1_700_000_000_000i64 } });
    assert_eq!(target.expires_at(&root), Some(1_700_000_000));
    assert!(
        target
            .expires_at(&json!({ "opencode": { "type": "api", "key": "k" } }))
            .is_none()
    );
}

#[test]
fn an_opencode_credential_has_no_identity_so_it_can_never_falsely_conflict() {
    let target = OpenCodeTarget;
    let identity = target.identity(&json!({ "opencode": { "type": "api", "key": "k" } }));
    assert!(identity.is_empty());
    assert!(
        !identity.conflicts(&super::target::subscription_identity(&subscription(
            "x", "opencode"
        )))
    );
}

#[test]
fn link_mode_names_are_stable() {
    assert_eq!(LinkMode::Symlink.as_str(), "symlink");
    assert_eq!(LinkMode::Copy.as_str(), "copy");
}
