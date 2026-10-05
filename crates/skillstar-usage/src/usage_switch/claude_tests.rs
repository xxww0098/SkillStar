//! Tests for the Claude Code switch adapter.
//!
//! The file store is pinned into a temp dir via `CLAUDE_CONFIG_DIR`, and the
//! keychain is always a fake runner — so both stores are testable on every
//! platform and nothing here touches a real `$HOME` or login keychain,
//! including on macOS where the keychain is the real store. Env-mutating
//! tests hold the crate's env lock.

use std::cell::RefCell;
use std::rc::Rc;

use super::*;
use crate::claude_credentials::credentials_file_path;
use crate::fetchers::oauth::common::SubscriptionBuilder;
use crate::test_support::EnvGuard;

struct Env {
    _temp: tempfile::TempDir,
    _guard: EnvGuard,
}

fn isolated(tag: &str) -> Env {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join(tag);
    std::fs::create_dir_all(&dir).unwrap();
    let guard = EnvGuard::set(&[
        ("CLAUDE_CONFIG_DIR", &dir),
        ("SKILLSTAR_DATA_DIR", &dir),
    ]);
    Env { _temp: temp, _guard: guard }
}

fn subscription(access: &str, refresh: Option<&str>) -> Subscription {
    let mut sub =
        SubscriptionBuilder::new("anthropic", "Claude", "USD", access, Some(1_900_000_000)).build();
    sub.refresh_token_encrypted = refresh.map(crypto::encrypt);
    sub.plan_tier = Some("PRO".into());
    sub
}

/// In-memory keychain standing in for `/usr/bin/security`. Records every
/// invocation; `read_back_override` simulates a store that serves different
/// bytes than were just written.
#[derive(Default)]
struct FakeKeychain {
    state: Rc<RefCell<FakeState>>,
}

#[derive(Default)]
struct FakeState {
    blob: Option<String>,
    fail_write: Option<String>,
    read_back_override: Option<String>,
    calls: Vec<Vec<String>>,
}

impl crate::claude_credentials::SecurityRunner for FakeKeychain {
    fn run(
        &self,
        args: &[String],
    ) -> crate::UsageResult<crate::claude_credentials::SecurityOutput> {
        use crate::claude_credentials::SecurityOutput;

        self.state.borrow_mut().calls.push(args.to_vec());
        let mut state = self.state.borrow_mut();
        match args.first().map(String::as_str) {
            Some("find-generic-password") => {
                let blob = state.read_back_override.clone().or_else(|| state.blob.clone());
                match blob {
                    Some(blob) => Ok(SecurityOutput {
                        success: true,
                        status_code: Some(0),
                        stdout: blob,
                        stderr: String::new(),
                    }),
                    None => Ok(SecurityOutput {
                        success: false,
                        status_code: Some(44),
                        stdout: String::new(),
                        stderr: "The specified item could not be found in the keychain.".into(),
                    }),
                }
            }
            Some("add-generic-password") => {
                if let Some(message) = &state.fail_write {
                    return Ok(SecurityOutput {
                        success: false,
                        status_code: Some(100),
                        stdout: String::new(),
                        stderr: message.clone(),
                    });
                }
                let hex = args
                    .iter()
                    .position(|arg| arg == "-X")
                    .and_then(|index| args.get(index + 1))
                    .cloned()
                    .unwrap_or_default();
                state.blob = Some(
                    (0..hex.len())
                        .step_by(2)
                        .map(|index| {
                            u8::from_str_radix(&hex[index..index + 2], 16).expect("valid hex") as char
                        })
                        .collect(),
                );
                Ok(SecurityOutput {
                    success: true,
                    status_code: Some(0),
                    stdout: String::new(),
                    stderr: String::new(),
                })
            }
            other => Err(crate::UsageError::Other(format!("fake 不认识该命令：{other:?}"))),
        }
    }
}

fn stored_blob(fake: &FakeKeychain) -> serde_json::Value {
    let blob = fake.state.borrow().blob.clone().expect("keychain holds a blob");
    serde_json::from_str(&blob).unwrap()
}

// ── file store ───────────────────────────────────────────────────────────

#[test]
fn file_activate_replaces_only_claude_ai_oauth_and_pins() {
    let _env = isolated("activate");
    // A foreign key the write must preserve (the mcpOAuth precedent).
    std::fs::write(
        credentials_file_path(),
        r#"{"mcpOAuth":{"github":"gho_x"},"claudeAiOauth":{"accessToken":"old"}}"#,
    )
    .unwrap();

    let sub = subscription("token-a", Some("refresh-a"));
    storage::upsert_subscription(sub.clone()).unwrap();
    let (saved, outcome) = activate_file(&sub.id).unwrap();
    assert!(outcome.success, "{outcome:?}");
    assert_eq!(saved.id, sub.id);
    assert_eq!(
        storage::get_active_subscription("anthropic").unwrap().as_deref(),
        Some(sub.id.as_str())
    );

    let text = std::fs::read_to_string(credentials_file_path()).unwrap();
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        value["mcpOAuth"]["github"].as_str(),
        Some("gho_x"),
        "foreign keys survive the merge"
    );
    assert_eq!(value["claudeAiOauth"]["accessToken"].as_str(), Some("token-a"));
    assert_eq!(value["claudeAiOauth"]["refreshToken"].as_str(), Some("refresh-a"));
    assert_eq!(value["claudeAiOauth"]["subscriptionType"].as_str(), Some("pro"));
    // expiresAt is Claude Code's milliseconds.
    assert_eq!(value["claudeAiOauth"]["expiresAt"].as_i64(), Some(1_900_000_000_000));
}

#[test]
fn file_reconcile_reads_the_store_not_the_pin() {
    let _env = isolated("reconcile");

    // Missing store → Missing.
    assert_eq!(
        reconcile_with(read_live_oauth_file()).unwrap(),
        CliAccountState::Missing
    );

    let sub = subscription("token-b", None);
    storage::upsert_subscription(sub.clone()).unwrap();
    storage::set_active_subscription("anthropic", &sub.id).unwrap();

    // A store with a different token → Diverged.
    std::fs::write(
        credentials_file_path(),
        r#"{"claudeAiOauth":{"accessToken":"someone-else"}}"#,
    )
    .unwrap();
    assert_eq!(
        reconcile_with(read_live_oauth_file()).unwrap(),
        CliAccountState::Diverged
    );

    // The pinned row's token → LinkedTo.
    activate_file(&sub.id).unwrap();
    assert_eq!(
        reconcile_with(read_live_oauth_file()).unwrap(),
        CliAccountState::LinkedTo {
            subscription_id: sub.id.clone()
        }
    );
}

#[test]
fn file_adopt_absorbs_the_live_generation() {
    let _env = isolated("adopt");

    let mut sub = subscription("stale-token", None);
    storage::upsert_subscription(sub.clone()).unwrap();
    storage::set_active_subscription("anthropic", &sub.id).unwrap();

    // Claude Code rotated: the file holds a newer access token.
    std::fs::write(
        credentials_file_path(),
        r#"{"claudeAiOauth":{"accessToken":"fresh-token","expiresAt":1999999999999}}"#,
    )
    .unwrap();

    absorb_live(&mut sub, read_live_oauth_file()).unwrap();
    assert_eq!(token_of(&sub).as_deref(), Some("fresh-token"));
    assert_eq!(sub.access_token_expires_at, Some(1_999_999_999));
}

// ── keychain store ───────────────────────────────────────────────────────

#[test]
fn keychain_activate_merges_the_item_verifies_and_pins() {
    let _env = isolated("kc-activate");
    let fake = FakeKeychain::default();
    // The item already holds a foreign key the merge must preserve, and the
    // plaintext mirror is stale from before the CLI's keychain migration.
    fake.state.borrow_mut().blob =
        Some(r#"{"mcpOAuth":{"github":"gho_x"},"claudeAiOauth":{"accessToken":"old"}}"#.into());
    std::fs::write(
        credentials_file_path(),
        r#"{"claudeAiOauth":{"accessToken":"stale-mirror"}}"#,
    )
    .unwrap();

    let sub = subscription("token-a", Some("refresh-a"));
    storage::upsert_subscription(sub.clone()).unwrap();
    let (saved, outcome) = activate_keychain(&sub.id, &fake).unwrap();
    assert!(outcome.success, "{outcome:?}");
    assert!(outcome.keychain_updated);
    assert_eq!(saved.id, sub.id);
    assert_eq!(
        storage::get_active_subscription("anthropic").unwrap().as_deref(),
        Some(sub.id.as_str())
    );

    let blob = stored_blob(&fake);
    assert_eq!(
        blob["mcpOAuth"]["github"].as_str(),
        Some("gho_x"),
        "foreign keys survive the keychain merge"
    );
    assert_eq!(blob["claudeAiOauth"]["accessToken"].as_str(), Some("token-a"));
    assert_eq!(blob["claudeAiOauth"]["refreshToken"].as_str(), Some("refresh-a"));
    assert_eq!(blob["claudeAiOauth"]["subscriptionType"].as_str(), Some("pro"));
    assert_eq!(blob["claudeAiOauth"]["expiresAt"].as_i64(), Some(1_900_000_000_000));
    // The verified switch deletes the stale plaintext mirror, as the CLI's
    // own migration does.
    assert!(!credentials_file_path().exists());
}

#[test]
fn keychain_activate_migrates_file_siblings_when_the_item_is_absent() {
    let _env = isolated("kc-migrate");
    let fake = FakeKeychain::default();
    std::fs::write(
        credentials_file_path(),
        r#"{"mcpOAuth":{"github":"gho_x"},"claudeAiOauth":{"accessToken":"old"}}"#,
    )
    .unwrap();

    let sub = subscription("token-m", None);
    storage::upsert_subscription(sub.clone()).unwrap();
    let (_, outcome) = activate_keychain(&sub.id, &fake).unwrap();
    assert!(outcome.success, "{outcome:?}");

    // The pre-migration file's siblings move into the item instead of being
    // dropped by the fresh blob.
    let blob = stored_blob(&fake);
    assert_eq!(blob["mcpOAuth"]["github"].as_str(), Some("gho_x"));
    assert_eq!(blob["claudeAiOauth"]["accessToken"].as_str(), Some("token-m"));
}

#[test]
fn keychain_readback_mismatch_does_not_pin() {
    let _env = isolated("kc-mismatch");
    let fake = FakeKeychain::default();
    fake.state.borrow_mut().read_back_override =
        Some(r#"{"claudeAiOauth":{"accessToken":"someone-else"}}"#.into());

    let sub = subscription("token-x", None);
    storage::upsert_subscription(sub.clone()).unwrap();
    let (_, outcome) = activate_keychain(&sub.id, &fake).unwrap();
    assert!(!outcome.success);
    assert!(outcome.error.unwrap().contains("回读与写入不一致"));
    assert_eq!(storage::get_active_subscription("anthropic").unwrap(), None);
}

#[test]
fn keychain_write_failure_does_not_pin_or_delete_the_file() {
    let _env = isolated("kc-locked");
    let fake = FakeKeychain::default();
    fake.state.borrow_mut().fail_write = Some("Keychain is locked.".into());
    std::fs::write(
        credentials_file_path(),
        r#"{"claudeAiOauth":{"accessToken":"stale-mirror"}}"#,
    )
    .unwrap();

    let sub = subscription("token-x", None);
    storage::upsert_subscription(sub.clone()).unwrap();
    let (_, outcome) = activate_keychain(&sub.id, &fake).unwrap();
    assert!(!outcome.success);
    assert!(outcome.error.unwrap().contains("Keychain is locked."));
    assert_eq!(storage::get_active_subscription("anthropic").unwrap(), None);
    // Nothing verified → the plaintext mirror must still be there.
    assert!(credentials_file_path().exists());
}

#[test]
fn keychain_reconcile_reads_the_item_first_and_falls_back_to_the_file() {
    let _env = isolated("kc-reconcile");
    let fake = FakeKeychain::default();

    // No item and no file → Missing.
    assert_eq!(
        reconcile_with(read_live_oauth_keychain(&fake)).unwrap(),
        CliAccountState::Missing
    );

    let sub = subscription("token-k", None);
    storage::upsert_subscription(sub.clone()).unwrap();
    storage::set_active_subscription("anthropic", &sub.id).unwrap();

    // Another account's item → Diverged.
    fake.state.borrow_mut().blob =
        Some(r#"{"claudeAiOauth":{"accessToken":"someone-else"}}"#.into());
    assert_eq!(
        reconcile_with(read_live_oauth_keychain(&fake)).unwrap(),
        CliAccountState::Diverged
    );

    // No item, but the pre-migration file holds the pinned row → LinkedTo.
    fake.state.borrow_mut().blob = None;
    std::fs::write(
        credentials_file_path(),
        r#"{"claudeAiOauth":{"accessToken":"token-k"}}"#,
    )
    .unwrap();
    assert_eq!(
        reconcile_with(read_live_oauth_keychain(&fake)).unwrap(),
        CliAccountState::LinkedTo {
            subscription_id: sub.id.clone()
        }
    );
}

#[test]
fn keychain_adopt_absorbs_the_live_generation() {
    let _env = isolated("kc-adopt");
    let fake = FakeKeychain::default();
    fake.state.borrow_mut().blob =
        Some(r#"{"claudeAiOauth":{"accessToken":"fresh-token","expiresAt":1999999999999}}"#.into());

    let mut sub = subscription("stale-token", None);
    storage::upsert_subscription(sub.clone()).unwrap();
    storage::set_active_subscription("anthropic", &sub.id).unwrap();

    absorb_live(&mut sub, read_live_oauth_keychain(&fake)).unwrap();
    assert_eq!(token_of(&sub).as_deref(), Some("fresh-token"));
    assert_eq!(sub.access_token_expires_at, Some(1_999_999_999));
}

// ── adapter gating ───────────────────────────────────────────────────────

#[test]
fn forget_never_touches_the_live_store() {
    let _env = isolated("forget");
    std::fs::write(
        credentials_file_path(),
        r#"{"claudeAiOauth":{"accessToken":"keep"}}"#,
    )
    .unwrap();
    Adapter.forget("anyone").unwrap();
    assert!(credentials_file_path().exists());
}

#[cfg(target_os = "macos")]
#[test]
fn macos_keychain_gates_on_the_tool_sync_sandbox() {
    let temp = tempfile::tempdir().unwrap();
    let sandbox = temp.path().join("sandbox");
    std::fs::create_dir_all(&sandbox).unwrap();
    {
        let _guard = EnvGuard::set(&[
            ("SKILLSTAR_TOOL_SYNC_HOME", &sandbox),
            ("SKILLSTAR_DATA_DIR", &sandbox),
        ]);
        assert!(!Adapter.available(), "sandboxed runs must not touch the keychain");
        assert!(claude_credentials::RealSecurity
            .run(&["find-generic-password".to_string()])
            .is_err());
        let sub = subscription("token-s", None);
        storage::upsert_subscription(sub.clone()).unwrap();
        let (_, outcome) = Adapter.activate(&sub.id).unwrap();
        assert!(!outcome.success);
        assert!(outcome.error.unwrap().contains("SKILLSTAR_TOOL_SYNC_HOME"));
    }
    // Unsandboxed on macOS the store is the login keychain and is available.
    assert!(Adapter.available());
}

#[cfg(not(target_os = "macos"))]
#[test]
fn the_file_store_is_available_and_the_adapter_uses_it() {
    assert!(Adapter.available());
    let _env = isolated("adapter-file");
    let sub = subscription("token-d", None);
    storage::upsert_subscription(sub.clone()).unwrap();
    let (_, outcome) = Adapter.activate(&sub.id).unwrap();
    assert!(outcome.success, "{outcome:?}");
}
