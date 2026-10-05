//! Tests for the Claude Code file-based switch adapter.
//!
//! `CLAUDE_CONFIG_DIR` pins the credentials file into a temp dir, so nothing
//! here touches a real `$HOME`. Env-mutating tests hold the crate's env lock.

use super::*;
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

#[test]
fn activate_replaces_only_claude_ai_oauth_and_verifies() {
    let _env = isolated("activate");
    // A foreign key the write must preserve (the mcpOAuth precedent).
    std::fs::write(
        credentials_path(),
        r#"{"mcpOAuth":{"github":"gho_x"},"claudeAiOauth":{"accessToken":"old"}}"#,
    )
    .unwrap();

    let sub = subscription("token-a", Some("refresh-a"));
    storage::upsert_subscription(sub.clone()).unwrap();
    let (saved, outcome) = Adapter.activate(&sub.id).unwrap();
    if cfg!(target_os = "macos") {
        // Off the file store the switch is refused by policy, not by luck.
        assert!(!outcome.success);
        assert!(outcome.error.unwrap().contains("D-072"));
        return;
    }
    assert!(outcome.success, "{outcome:?}");
    assert_eq!(saved.id, sub.id);
    assert_eq!(
        storage::get_active_subscription("anthropic").unwrap().as_deref(),
        Some(sub.id.as_str())
    );

    let text = std::fs::read_to_string(credentials_path()).unwrap();
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
fn reconcile_reads_the_file_not_the_pin() {
    let _env = isolated("reconcile");
    if cfg!(target_os = "macos") {
        assert_eq!(Adapter.reconcile().unwrap(), None, "off the file store");
        return;
    }

    // Missing file → Missing.
    assert_eq!(Adapter.reconcile().unwrap(), Some(CliAccountState::Missing));

    let sub = subscription("token-b", None);
    storage::upsert_subscription(sub.clone()).unwrap();
    storage::set_active_subscription("anthropic", &sub.id).unwrap();

    // A file with a different token → Diverged.
    std::fs::write(
        credentials_path(),
        r#"{"claudeAiOauth":{"accessToken":"someone-else"}}"#,
    )
    .unwrap();
    assert_eq!(Adapter.reconcile().unwrap(), Some(CliAccountState::Diverged));

    // The pinned row's token → LinkedTo.
    Adapter.activate(&sub.id).unwrap();
    assert_eq!(
        Adapter.reconcile().unwrap(),
        Some(CliAccountState::LinkedTo {
            subscription_id: sub.id.clone()
        })
    );
}

#[test]
fn adopt_before_refresh_absorbs_the_live_generation() {
    let _env = isolated("adopt");
    if cfg!(target_os = "macos") {
        // Off the file store, adopt is a no-op.
        let mut sub = subscription("stale-token", None);
        Adapter.adopt_before_refresh(&mut sub).unwrap();
        assert_eq!(token_of(&sub).as_deref(), Some("stale-token"));
        return;
    }

    let mut sub = subscription("stale-token", None);
    storage::upsert_subscription(sub.clone()).unwrap();
    storage::set_active_subscription("anthropic", &sub.id).unwrap();

    // Claude Code rotated: the file holds a newer access token.
    std::fs::write(
        credentials_path(),
        r#"{"claudeAiOauth":{"accessToken":"fresh-token","expiresAt":1999999999999}}"#,
    )
    .unwrap();

    Adapter.adopt_before_refresh(&mut sub).unwrap();
    assert_eq!(token_of(&sub).as_deref(), Some("fresh-token"));
    assert_eq!(sub.access_token_expires_at, Some(1_999_999_999));
}

#[test]
fn forget_never_touches_the_live_file() {
    let _env = isolated("forget");
    std::fs::write(credentials_path(), r#"{"claudeAiOauth":{"accessToken":"keep"}}"#).unwrap();
    Adapter.forget("anyone").unwrap();
    assert!(credentials_path().exists());
}

#[cfg(target_os = "macos")]
#[test]
fn macos_is_read_only_by_policy() {
    // D-072: no system-keychain writes, so the adapter must refuse rather
    // than flip a file the CLI does not read there.
    assert!(!Adapter.available());
}

#[cfg(not(target_os = "macos"))]
#[test]
fn the_file_store_is_available_off_macos() {
    assert!(Adapter.available());
}
