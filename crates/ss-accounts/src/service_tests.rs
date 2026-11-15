//! Facade tests: the narrow refresh, what a rejected credential does to a row,
//! and a switch that only moves after the client's own disk agrees.
//!
//! Provider calls are injected, so nothing here touches a network.

use serde_json::json;
use crate::oauth::{Identity, RefreshOutcome, TokenSet};
use crate::usage::{Credentials, UsageOutcome, UsageSnapshot, UsageUnit, UsageWindow};

use super::{
    activate_with, add_from_local_with, add_from_token_with, list, refresh_with, remove,
};
use crate::record::Account;
use crate::store;
use crate::test_support::{Sandbox, jwt_with_claims};

// ---------------------------------------------------------------------------
// Injected provider calls
// ---------------------------------------------------------------------------

fn snapshot(remaining_percent: f32) -> UsageSnapshot {
    UsageSnapshot {
        fetched_at: 1_700_000_000,
        windows: vec![UsageWindow {
            key: "five_hour".to_string(),
            label: "5 hour".to_string(),
            remaining_percent,
            used: None,
            total: None,
            unit: UsageUnit::Percent,
            resets_at: None,
        }],
        plan: Some("max".to_string()),
        credits: None,
        error: None,
        gcp_tos: None,
    }
}

fn rotated() -> TokenSet {
    TokenSet {
        access_token: "at-rotated".to_string(),
        refresh_token: Some("rt-rotated".to_string()),
        id_token: None,
        expires_at: Some(super::now_secs() + 3_600),
        token_type: "Bearer".to_string(),
    }
}

fn oauth_rotates(
    _agent: &str,
    _tokens: &TokenSet,
    _state: Option<&serde_json::Value>,
) -> Result<RefreshOutcome, crate::oauth::OAuthError> {
    Ok(RefreshOutcome {
        tokens: rotated(),
        provider_state: None,
    })
}

fn oauth_rejects(
    _agent: &str,
    _tokens: &TokenSet,
    _state: Option<&serde_json::Value>,
) -> Result<RefreshOutcome, crate::oauth::OAuthError> {
    Err(crate::oauth::OAuthError::AuthRequired(
        "the refresh token was revoked".to_string(),
    ))
}

fn usage_ok(_agent: &str, creds: &Credentials) -> Result<UsageOutcome, crate::usage::UsageError> {
    Ok(UsageOutcome {
        snapshot: snapshot(62.0),
        tokens: creds.tokens.clone(),
        provider_state: None,
    })
}

fn usage_transient(
    _agent: &str,
    _creds: &Credentials,
) -> Result<UsageOutcome, crate::usage::UsageError> {
    Err(crate::usage::UsageError::Transient("429 too many requests".to_string()))
}

fn usage_rejects(_agent: &str, _creds: &Credentials) -> Result<UsageOutcome, crate::usage::UsageError> {
    Err(crate::usage::UsageError::AuthRequired("401 unauthorized".to_string()))
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn token_set(access: &str, refresh: Option<&str>, expires_at: Option<i64>) -> TokenSet {
    TokenSet {
        access_token: access.to_string(),
        refresh_token: refresh.map(str::to_string),
        id_token: None,
        expires_at,
        token_type: "Bearer".to_string(),
    }
}

fn codex_identity(account_id: &str) -> Identity {
    Identity {
        email: None,
        display_name: None,
        plan: None,
        account_id: Some(account_id.to_string()),
    }
}

fn email_identity(email: &str) -> Identity {
    Identity {
        email: Some(email.to_string()),
        display_name: Some(email.to_string()),
        plan: None,
        account_id: None,
    }
}

fn store_account(agent: &str, identity: Identity, tokens: TokenSet) -> Account {
    let account = Account::from_login(agent, &identity, &tokens, None, 1_000).expect("record");
    store::upsert(&account).expect("store");
    account
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn list_marks_the_account_the_client_is_serving() {
    let sandbox = Sandbox::new();
    let first = store_account(
        "codex",
        codex_identity("acc-a"),
        token_set("at-a", Some("rt-a"), None),
    );
    let second = store_account(
        "codex",
        codex_identity("acc-b"),
        token_set("at-b", Some("rt-b"), None),
    );
    // What the Codex CLI itself holds.
    sandbox.write_file(
        &sandbox.tool_path(&[".codex", "auth.json"]),
        json!({ "tokens": { "access_token": "at-b", "account_id": "acc-b" } }).to_string(),
    );

    let views = list().expect("list");
    assert_eq!(views.len(), 2);
    let first_view = views.iter().find(|view| view.id == first.id).expect("first");
    let second_view = views.iter().find(|view| view.id == second.id).expect("second");
    assert!(!first_view.is_active);
    assert!(second_view.is_active);
    assert_eq!(second_view.agent, "codex");

    // Nothing the UI receives carries a credential.
    let printed = serde_json::to_string(&views).expect("json");
    assert!(!printed.contains("at-b"), "{printed}");
    assert!(!printed.contains("rt-b"), "{printed}");
}

#[test]
fn refresh_rotates_the_tokens_and_never_clobbers_a_rename() {
    let _sandbox = Sandbox::new();
    let stored = store_account(
        "codex",
        codex_identity("acc-r"),
        token_set("at-old", Some("rt-old"), Some(1)),
    );
    store::update(&stored.id, |row| {
        row.display_name = "Work laptop".to_string();
        Ok(())
    })
    .expect("rename");

    let view = refresh_with(&stored.id, oauth_rotates, usage_ok).expect("refresh");
    assert_eq!(view.display_name, "Work laptop");
    assert_eq!(view.last_used_at, None);
    assert!(!view.requires_reauth);
    assert_eq!(view.plan.as_deref(), Some("max"));
    let usage = view.usage.expect("snapshot");
    assert_eq!(usage.windows[0].remaining_percent, 62.0);

    let reloaded = store::get(&stored.id).expect("row");
    assert_eq!(reloaded.display_name, "Work laptop");
    assert_eq!(reloaded.created_at, stored.created_at);
    assert_eq!(reloaded.tokens().expect("tokens").access_token, "at-rotated");
}

#[test]
fn a_revoked_grant_keeps_the_row_and_flags_it() {
    let _sandbox = Sandbox::new();
    let stored = store_account(
        "codex",
        codex_identity("acc-x"),
        token_set("at-x", Some("rt-x"), Some(1)),
    );

    let error = refresh_with(&stored.id, oauth_rejects, usage_ok).expect_err("revoked");
    assert!(matches!(error, crate::AccountError::AuthRequired(_)));

    let reloaded = store::get(&stored.id).expect("the row is kept");
    assert!(reloaded.requires_reauth);
    assert_eq!(
        reloaded.tokens().expect("tokens").access_token,
        "at-x",
        "a refused refresh does not throw the credential away"
    );
    assert_eq!(list().expect("list").len(), 1);
}

#[test]
fn a_transient_quota_failure_keeps_the_last_snapshot_and_says_why() {
    let _sandbox = Sandbox::new();
    let stored = store_account(
        "codex",
        codex_identity("acc-t"),
        token_set("at-t", Some("rt-t"), None),
    );
    store::update(&stored.id, |row| {
        row.usage = Some(snapshot(40.0));
        Ok(())
    })
    .expect("seed a snapshot");

    let view = refresh_with(&stored.id, oauth_rotates, usage_transient).expect("still a view");
    assert!(!view.requires_reauth);
    let usage = view.usage.expect("the last good snapshot");
    assert_eq!(usage.windows[0].remaining_percent, 40.0);
    assert!(usage.error.is_some(), "the page has to know why it is stale");
}

#[test]
fn a_rejected_quota_read_flags_the_account_for_a_new_login() {
    let _sandbox = Sandbox::new();
    let stored = store_account(
        "claude-code",
        email_identity("dev@example.com"),
        token_set("at-c", Some("rt-c"), None),
    );

    let error = refresh_with(&stored.id, oauth_rotates, usage_rejects).expect_err("401");
    assert!(matches!(error, crate::AccountError::AuthRequired(_)));
    assert!(store::get(&stored.id).expect("row").requires_reauth);
    assert!(list().expect("list")[0].requires_reauth);
}

#[test]
fn activate_writes_the_client_and_moves_last_used_at() {
    let sandbox = Sandbox::new();
    let stored = store_account(
        "codex",
        codex_identity("acc-w"),
        token_set("at-w", Some("rt-w"), None),
    );

    let outcome = activate_with(&stored.id, oauth_rotates, usage_ok).expect("activate");
    assert_eq!(outcome.previous, None);
    assert!(outcome.target.ends_with(".codex/auth.json"));
    assert!(outcome.account.is_active);
    let used_at = outcome.account.last_used_at.expect("last_used_at");
    assert_eq!(
        store::get(&stored.id).expect("row").last_used_at,
        Some(used_at)
    );

    let auth: serde_json::Value = serde_json::from_slice(
        &std::fs::read(sandbox.tool_path(&[".codex", "auth.json"])).expect("auth.json"),
    )
    .expect("json");
    assert_eq!(auth["tokens"]["access_token"], "at-w");
    assert_eq!(auth["tokens"]["account_id"], "acc-w");

    // The client's own file now names this row, so the card is checked.
    let live = crate::reconcile().expect("reconcile");
    let codex = live.iter().find(|agent| agent.agent == "codex").expect("codex");
    assert_eq!(codex.account_id.as_deref(), Some(stored.id.as_str()));
}

#[test]
fn activate_reports_the_account_the_client_served_before() {
    let _sandbox = Sandbox::new();
    let first = store_account(
        "codex",
        codex_identity("acc-first"),
        token_set("at-first", Some("rt-first"), None),
    );
    let second = store_account(
        "codex",
        codex_identity("acc-second"),
        token_set("at-second", Some("rt-second"), None),
    );

    let outcome = activate_with(&first.id, oauth_rotates, usage_ok).expect("first switch");
    assert_eq!(outcome.previous, None);

    let outcome = activate_with(&second.id, oauth_rotates, usage_ok).expect("second switch");
    assert_eq!(outcome.previous.as_deref(), Some(first.id.as_str()));
    assert!(store::get(&second.id).expect("row").last_used_at.is_some());
}

#[test]
fn a_failed_switch_leaves_the_row_and_the_client_alone() {
    let _sandbox = Sandbox::new();
    // Cursor has no state.vscdb here, so the write cannot even start.
    let stored = store_account(
        "cursor",
        email_identity("dev@example.com"),
        token_set("at-cur", Some("rt-cur"), None),
    );

    let error = activate_with(&stored.id, oauth_rotates, usage_ok).expect_err("no database");
    assert!(matches!(error, crate::AccountError::Client(_)));
    assert_eq!(store::get(&stored.id).expect("row").last_used_at, None);
    assert!(!crate::switch::credential_path("cursor").expect("path").exists());
}

#[test]
fn a_switch_the_client_refuses_rolls_the_old_session_back() {
    let _sandbox = Sandbox::new();
    let path = crate::switch::credential_path("cursor").expect("path");
    std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
    rusqlite::Connection::open(&path)
        .expect("state.vscdb")
        .execute_batch("CREATE TABLE ItemTable (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB);")
        .expect("schema");

    let first = store_account(
        "cursor",
        email_identity("one@example.com"),
        token_set("at-one", Some("rt-one"), None),
    );
    let second = store_account(
        "cursor",
        email_identity("two@example.com"),
        token_set("at-two", Some("rt-two"), None),
    );
    let first_outcome = activate_with(&first.id, oauth_rotates, usage_ok).expect("first switch");
    assert!(first_outcome.account.last_used_at.is_some());

    // Cursor (or another process) refuses a session that differs from the one
    // it already holds; putting the old rows back must still be allowed.
    rusqlite::Connection::open(&path)
        .expect("state.vscdb")
        .execute_batch(
            "CREATE TRIGGER reject_auth BEFORE INSERT ON ItemTable
             WHEN NEW.key = 'cursorAuth/accessToken'
              AND NEW.value <> (SELECT value FROM ItemTable WHERE key = 'cursorAuth/accessToken')
             BEGIN SELECT RAISE(ABORT, 'fixture refuses a different session'); END;",
        )
        .expect("trigger");

    let error = activate_with(&second.id, oauth_rotates, usage_ok).expect_err("refused");
    assert!(matches!(error, crate::AccountError::Client(_)));
    assert_eq!(store::get(&second.id).expect("row").last_used_at, None);

    // The client still serves the first account, and reconcile says so.
    let live = crate::reconcile().expect("reconcile");
    let cursor = live.iter().find(|agent| agent.agent == "cursor").expect("cursor");
    assert_eq!(cursor.account_id.as_deref(), Some(first.id.as_str()));
}

#[test]
fn adding_a_pasted_token_stores_it_once_and_never_echoes_it() {
    let _sandbox = Sandbox::new();
    let payload = json!({
        "access_token": "at-paste",
        "refresh_token": "rt-paste",
        "email": "dev@example.com"
    })
    .to_string();

    let view = add_from_token_with("codex", &payload, usage_ok).expect("add");
    assert_eq!(view.email.as_deref(), Some("dev@example.com"));
    assert_eq!(view.usage.as_ref().map(|usage| usage.windows.len()), Some(1));
    let printed = serde_json::to_string(&view).expect("json");
    assert!(!printed.contains("at-paste"), "{printed}");
    assert!(!printed.contains("rt-paste"), "{printed}");

    let stored = store::get(&view.id).expect("row");
    assert_eq!(stored.tokens().expect("tokens").access_token, "at-paste");

    // The same account pasted again is the same card, not a second one.
    let again = add_from_token_with("codex", &payload, usage_ok).expect("add again");
    assert_eq!(again.id, view.id);
    assert_eq!(list().expect("list").len(), 1);
}

#[test]
fn a_paste_the_provider_rejects_is_not_stored() {
    let _sandbox = Sandbox::new();
    let error = add_from_token_with("codex", "at-bad", usage_rejects).expect_err("rejected");
    assert!(matches!(error, crate::AccountError::AuthRequired(_)));
    assert!(list().expect("list").is_empty());
}

#[test]
fn a_paste_with_an_unavailable_quota_endpoint_is_still_stored() {
    let _sandbox = Sandbox::new();
    let view = add_from_token_with("codex", "at-busy", usage_transient).expect("stored");
    assert!(view.usage.is_none());
    assert!(!view.requires_reauth);
    assert_eq!(list().expect("list").len(), 1);
}

#[test]
fn importing_the_machines_codex_session_stores_it() {
    let sandbox = Sandbox::new();
    let id_token = jwt_with_claims(json!({ "email": "dev@example.com" }));
    let auth = json!({
        "OPENAI_API_KEY": null,
        "tokens": {
            "access_token": "at-local",
            "refresh_token": "rt-local",
            "id_token": id_token,
            "account_id": "acc-local"
        }
    });
    sandbox.write_file(
        &sandbox.tool_path(&[".codex", "auth.json"]),
        auth.to_string(),
    );

    let view = add_from_local_with("codex", usage_ok).expect("import");
    assert_eq!(view.display_name, "dev@example.com");
    assert_eq!(view.email.as_deref(), Some("dev@example.com"));
    let stored = store::get(&view.id).expect("row");
    assert_eq!(stored.account_id.as_deref(), Some("acc-local"));
    assert_eq!(stored.tokens().expect("tokens").access_token, "at-local");
}

#[test]
fn the_facade_refuses_an_unknown_agent_and_a_missing_row() {
    let _sandbox = Sandbox::new();
    assert!(matches!(
        crate::add_from_token("chatgpt", "at"),
        Err(crate::AccountError::UnsupportedAgent(_))
    ));
    assert!(matches!(
        crate::add_from_local("chatgpt"),
        Err(crate::AccountError::UnsupportedAgent(_))
    ));
    assert!(matches!(
        crate::activate("no-such-account"),
        Err(crate::AccountError::NotFound(_))
    ));
    assert!(matches!(
        crate::refresh("no-such-account"),
        Err(crate::AccountError::NotFound(_))
    ));
}

#[test]
fn removing_a_row_leaves_the_client_serving() {
    let _sandbox = Sandbox::new();
    let stored = store_account(
        "codex",
        codex_identity("acc-del"),
        token_set("at-del", Some("rt-del"), None),
    );
    remove(&stored.id).expect("remove");
    assert!(list().expect("list").is_empty());
    assert!(matches!(
        store::get(&stored.id),
        Err(crate::AccountError::NotFound(_))
    ));
}