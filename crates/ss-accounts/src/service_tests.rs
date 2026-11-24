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
        reset_credits: None,
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
        identity: None,
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

#[test]
fn retired_agent_records_are_deleted_on_the_first_list() {
    let _sandbox = Sandbox::new();
    let retired_desktop = store_account(
        "gemini-desktop",
        Identity {
            account_id: Some("user1|private-file-identity".to_string()),
            ..Identity::default()
        },
        token_set("local-reference", None, None),
    );
    // Gemini CLI 的账号支持已移除（D-119）：它的存量记录同样按下线客户端清掉。
    let retired_cli = store_account(
        "gemini-cli",
        email_identity("user@example.com"),
        token_set("at-retired", Some("rt-retired"), None),
    );
    let supported = store_account(
        "codex",
        codex_identity("active"),
        token_set("at", None, None),
    );

    assert!(crate::Agent::from_id("gemini-desktop").is_err());
    assert!(crate::Agent::from_id("gemini-cli").is_err());
    assert!(add_from_local_with("gemini-desktop", usage_transient).is_err());
    assert!(add_from_token_with("gemini-desktop", "token", usage_transient).is_err());
    assert!(add_from_local_with("gemini-cli", usage_transient).is_err());
    for retired in [&retired_desktop, &retired_cli] {
        assert!(matches!(
            refresh_with(&retired.id, oauth_rotates, usage_ok),
            Err(crate::AccountError::UnsupportedAgent(_))
        ));
    }

    let rows = list().expect("list");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, supported.id);

    // 下线客户端的记录连磁盘一起消失：既不在列表，也不在记录目录里。
    for retired in [&retired_desktop, &retired_cli] {
        assert!(
            matches!(
                store::get(&retired.id),
                Err(crate::AccountError::NotFound(_))
            ),
            "{} should be gone",
            retired.id
        );
        assert!(!store::record_path(&retired.id).exists());
        assert!(!store::index_path().exists() || !read_index_text().contains(&retired.id));
    }
    assert!(store::record_path(&supported.id).is_file());
}

fn read_index_text() -> String {
    std::fs::read_to_string(store::index_path()).unwrap_or_default()
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
fn a_transient_quota_failure_without_a_prior_snapshot_records_the_error() {
    let _sandbox = Sandbox::new();
    let stored = store_account(
        "codex",
        codex_identity("acc-no-snap"),
        token_set("at-t", Some("rt-t"), None),
    );

    let view = refresh_with(&stored.id, oauth_rotates, usage_transient).expect("still a view");
    assert!(!view.requires_reauth);
    let usage = view.usage.expect("an error-only snapshot");
    assert!(usage.windows.is_empty());
    assert!(usage.error.is_some(), "the card has to surface why quota failed");
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
fn a_switch_reports_no_client_restart() {
    let _sandbox = Sandbox::new();
    let stored = store_account(
        "codex",
        codex_identity("acc-shape"),
        token_set("at-shape", Some("rt-shape"), None),
    );

    let outcome = activate_with(&stored.id, oauth_rotates, usage_ok).expect("activate");
    // 切号只写凭据，客户端进程不动：结果里没有 clientRestart 之类的字段（D-121）。
    let json = serde_json::to_value(&outcome).expect("json");
    let mut keys: Vec<&str> = json
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, ["account", "previous", "target"]);
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

#[test]
fn grok_views_drop_cached_legacy_monthly_window_while_keeping_other_agents_intact() {
    let _sandbox = Sandbox::new();
    let legacy_month = UsageWindow {
        key: "month".to_string(),
        label: "Monthly credits".to_string(),
        remaining_percent: 0.0,
        used: Some(138.0),
        total: Some(0.0),
        unit: UsageUnit::UsdCents,
        resets_at: None,
    };
    let weekly = UsageWindow {
        key: "week".to_string(),
        label: "Weekly credits".to_string(),
        remaining_percent: 93.0,
        used: Some(7.0),
        total: Some(100.0),
        unit: UsageUnit::Percent,
        resets_at: None,
    };
    let grok = store_account(
        "grok",
        email_identity("grok@example.com"),
        token_set("at-grok", Some("rt-grok"), None),
    );
    store::update(&grok.id, |row| {
        row.usage = Some(UsageSnapshot {
            fetched_at: 1_700_000_000,
            windows: vec![legacy_month.clone(), weekly.clone()],
            plan: Some("SuperGrok Heavy".to_string()),
            credits: None,
            error: None,
            gcp_tos: None,
            reset_credits: None,
        });
        Ok(())
    })
    .expect("seed grok usage");

    let kiro = store_account(
        "kiro",
        email_identity("kiro@example.com"),
        token_set("at-kiro", Some("rt-kiro"), None),
    );
    store::update(&kiro.id, |row| {
        row.usage = Some(UsageSnapshot {
            fetched_at: 1_700_000_000,
            windows: vec![UsageWindow {
                key: "month".to_string(),
                label: "Credits".to_string(),
                remaining_percent: 80.0,
                used: Some(10.0),
                total: Some(50.0),
                unit: UsageUnit::Count,
                resets_at: None,
            }],
            plan: Some("Pro".to_string()),
            credits: None,
            error: None,
            gcp_tos: None,
            reset_credits: None,
        });
        Ok(())
    })
    .expect("seed kiro usage");

    let views = list().expect("list");
    let grok_view = views.iter().find(|view| view.id == grok.id).expect("grok");
    let kiro_view = views.iter().find(|view| view.id == kiro.id).expect("kiro");

    let grok_keys: Vec<&str> = grok_view
        .usage
        .as_ref()
        .expect("grok usage")
        .windows
        .iter()
        .map(|window| window.key.as_str())
        .collect();
    assert_eq!(grok_keys, vec!["week"]);

    let kiro_keys: Vec<&str> = kiro_view
        .usage
        .as_ref()
        .expect("kiro usage")
        .windows
        .iter()
        .map(|window| window.key.as_str())
        .collect();
    assert_eq!(kiro_keys, vec!["month"]);
}

#[test]
fn refresh_upgrades_placeholder_name_from_usage_identity_without_clobbering_renames() {
    let _sandbox = Sandbox::new();
    let placeholder = store_account(
        "zcode",
        Identity {
            account_id: Some("27741789126905948".to_string()),
            ..Identity::default()
        },
        token_set("at-z", None, None),
    );
    assert_eq!(placeholder.display_name, "zcode");

    let renamed = store_account(
        "zcode",
        Identity {
            display_name: Some("My ZCode".to_string()),
            account_id: Some("99999".to_string()),
            ..Identity::default()
        },
        token_set("at-z2", None, None),
    );

    fn usage_with_identity(
        _agent: &str,
        creds: &Credentials,
    ) -> Result<UsageOutcome, crate::usage::UsageError> {
        Ok(UsageOutcome {
            snapshot: snapshot(80.0),
            tokens: creds.tokens.clone(),
            provider_state: None,
            identity: Some(Identity {
                display_name: Some("xxww0098".to_string()),
                account_id: Some("27741789126905948".to_string()),
                email: None,
                plan: None,
            }),
        })
    }

    let updated = refresh_with(&placeholder.id, oauth_rotates, usage_with_identity).expect("refresh");
    assert_eq!(updated.display_name, "xxww0098");
    assert_eq!(updated.account_id.as_deref(), Some("27741789126905948"));

    let kept = refresh_with(&renamed.id, oauth_rotates, usage_with_identity).expect("refresh");
    assert_eq!(kept.display_name, "My ZCode");
}

#[test]
fn antigravity_import_and_refresh_fill_missing_google_identity() {
    let _sandbox = Sandbox::new();
    fn usage_with_google_identity(
        _agent: &str,
        creds: &Credentials,
    ) -> Result<UsageOutcome, crate::usage::UsageError> {
        assert!(creds.needs_identity);
        Ok(UsageOutcome {
            snapshot: snapshot(80.0),
            tokens: creds.tokens.clone(),
            provider_state: None,
            identity: Some(Identity {
                display_name: Some("Google User".to_string()),
                email: Some("user@example.com".to_string()),
                ..Identity::default()
            }),
        })
    }
    fn usage_without_lookup(
        _agent: &str,
        creds: &Credentials,
    ) -> Result<UsageOutcome, crate::usage::UsageError> {
        assert!(!creds.needs_identity);
        usage_ok(_agent, creds)
    }

    let email_fallback = store_account(
        "antigravity",
        email_identity("user@example.com"),
        token_set("at-email-fallback", None, None),
    );
    let imported = add_from_token_with("antigravity", "at-import", usage_with_google_identity)
        .expect("import");
    assert_eq!(imported.id, email_fallback.id);
    assert_eq!(imported.display_name, "Google User");
    assert_eq!(imported.email.as_deref(), Some("user@example.com"));
    refresh_with(&imported.id, oauth_rotates, usage_without_lookup).expect("known identity");

    let old = store_account(
        "antigravity", Identity::default(), token_set("at-old", None, None),
    );
    let updated =
        refresh_with(&old.id, oauth_rotates, usage_with_google_identity).expect("refresh");
    assert_eq!(updated.display_name, "Google User");
    assert_eq!(updated.email.as_deref(), Some("user@example.com"));

    let email_only = store_account(
        "antigravity",
        email_identity("user@example.com"),
        token_set("at-email", None, None),
    );
    let updated =
        refresh_with(&email_only.id, oauth_rotates, usage_with_google_identity).expect("refresh");
    assert_eq!(updated.display_name, "Google User");

    let renamed = store_account(
        "antigravity",
        Identity {
            display_name: Some("My account".to_string()),
            ..Identity::default()
        },
        token_set("at-renamed", None, None),
    );
    let updated =
        refresh_with(&renamed.id, oauth_rotates, usage_with_google_identity).expect("refresh");
    assert_eq!(updated.display_name, "My account");
    assert_eq!(updated.email.as_deref(), Some("user@example.com"));
}

#[test]
fn imported_and_existing_accounts_recover_names_from_token_claims() {
    let _sandbox = Sandbox::new();
    let token = jwt_with_claims(json!({ "name": "Example User", "email": "dev@example.com" }));
    let payload = json!({ "access_token": "at", "id_token": token }).to_string();
    let imported = add_from_token_with("codex", &payload, usage_ok).expect("import");
    assert_eq!(imported.display_name, "Example User");
    assert_eq!(imported.email.as_deref(), Some("dev@example.com"));

    let mut tokens = token_set("at-existing", None, None);
    tokens.id_token = Some(token);
    let existing = store_account("codex", email_identity("dev@example.com"), tokens.clone());
    let updated = refresh_with(&existing.id, oauth_rotates, usage_transient).expect("refresh");
    assert_eq!(updated.display_name, "Example User");
    assert_eq!(updated.email.as_deref(), Some("dev@example.com"));

    let renamed = store_account(
        "codex",
        Identity {
            display_name: Some("My account".to_string()),
            email: Some("dev@example.com".to_string()),
            ..Identity::default()
        },
        tokens,
    );
    let kept = refresh_with(&renamed.id, oauth_rotates, usage_ok).expect("refresh");
    assert_eq!(kept.display_name, "My account");
}

#[test]
fn cursor_requests_a_missing_profile_without_overwriting_custom_names() {
    let _sandbox = Sandbox::new();
    fn usage_with_profile(
        agent: &str,
        creds: &Credentials,
    ) -> Result<UsageOutcome, crate::usage::UsageError> {
        assert_eq!(agent, "cursor");
        assert!(creds.needs_identity);
        Ok(UsageOutcome {
            snapshot: snapshot(80.0),
            tokens: creds.tokens.clone(),
            provider_state: None,
            identity: Some(Identity {
                display_name: Some("Cursor User".to_string()),
                email: Some("user@example.com".to_string()),
                ..Identity::default()
            }),
        })
    }
    let existing = store_account("cursor", Identity::default(), token_set("at", None, None));
    let updated = refresh_with(&existing.id, oauth_rotates, usage_with_profile).expect("refresh");
    assert_eq!(updated.display_name, "Cursor User");
    assert_eq!(updated.email.as_deref(), Some("user@example.com"));
    let renamed = store_account(
        "cursor",
        Identity {
            display_name: Some("My account".to_string()),
            ..Identity::default()
        },
        token_set("at-renamed", None, None),
    );
    let updated = refresh_with(&renamed.id, oauth_rotates, usage_with_profile).expect("refresh");
    assert_eq!(updated.display_name, "My account");
    assert_eq!(updated.email.as_deref(), Some("user@example.com"));
}

#[test]
fn account_view_exposes_provider_account_id() {
    let _sandbox = Sandbox::new();
    let zcode = store_account(
        "zcode",
        Identity {
            display_name: Some("xxww0098".to_string()),
            account_id: Some("27741789126905948".to_string()),
            ..Identity::default()
        },
        token_set("at-z", None, None),
    );

    let rows = list().expect("list");
    let zcode_view = rows.iter().find(|row| row.id == zcode.id).expect("zcode");
    assert_eq!(zcode_view.account_id.as_deref(), Some("27741789126905948"));

}

#[test]
fn refresh_heals_cursor_ciphertext_tokens_from_local_plaintext_mirror() {
    let _sandbox = Sandbox::new();
    let plain_jwt = jwt_with_claims(json!({
        "sub": "grok|user_01A0000000000000000000000",
        "exp": 1_900_000_000_i64
    }));
    // Seed the client database where the heal path reads it on this OS
    // (Application Support on macOS, .config on Linux, AppData on Windows).
    let db_path =
        crate::switch::paths::cursor_state_db_path(&crate::switch::PathRoots::from_env());
    std::fs::create_dir_all(db_path.parent().expect("parent")).expect("dir");
    let conn = rusqlite::Connection::open(&db_path).expect("vscdb");
    conn.execute_batch("CREATE TABLE ItemTable (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB);")
        .expect("schema");
    for (k, v) in [
        ("cursorAuth/accessToken", "cursor-safe-storage:v1:encryptedAt"),
        ("cursorAuth/refreshToken", "cursor-safe-storage:v1:encryptedRt"),
        ("cursorAuth/cachedEmail", "shefkie@cursor.example"),
        ("cursorAuth/authId", "grok|user_01A0000000000000000000000"),
        ("cursor.accessToken", plain_jwt.as_str()),
        ("cursor.email", "shefkie@cursor.example"),
    ] {
        conn.execute(
            "INSERT OR REPLACE INTO ItemTable (key, value) VALUES (?1, ?2)",
            [k, v],
        )
        .expect("seed");
    }

    let stored = store_account(
        "cursor",
        Identity {
            email: Some("shefkie@cursor.example".to_string()),
            display_name: Some("shefkie@cursor.example".to_string()),
            account_id: Some("grok|user_01A0000000000000000000000".to_string()),
            plan: None,
        },
        token_set(
            "cursor-safe-storage:v1:encryptedAt",
            Some("cursor-safe-storage:v1:encryptedRt"),
            None,
        ),
    );

    let view = refresh_with(&stored.id, oauth_rejects, usage_ok).expect("refresh heals ciphertext");
    assert_eq!(view.plan.as_deref(), Some("max"));
    assert_eq!(view.usage.expect("snapshot").windows[0].remaining_percent, 62.0);
    let healed = store::get(&stored.id).expect("reloaded");
    let tokens = healed.tokens().expect("tokens");
    assert_eq!(tokens.access_token, plain_jwt);
    assert_eq!(tokens.refresh_token, None);
}

#[test]
fn a_paste_with_an_unreadable_quota_records_the_diagnostic_reason() {
    let _sandbox = Sandbox::new();
    fn usage_unreadable(
        _agent: &str,
        _creds: &Credentials,
    ) -> Result<UsageOutcome, crate::usage::UsageError> {
        Err(crate::usage::UsageError::Unreadable(
            "上游已停用个人账号的 Cloud Code 配额通道".to_string(),
        ))
    }

    let view = add_from_token_with("antigravity", "ya29.consumer", usage_unreadable)
        .expect("stored with diagnostic");
    assert!(!view.requires_reauth);
    let usage = view.usage.expect("diagnostic snapshot recorded");
    assert!(usage.windows.is_empty());
    assert!(
        usage
            .error
            .as_deref()
            .is_some_and(|msg| msg.contains("Cloud Code")),
        "unexpected error: {:?}",
        usage.error
    );
}

#[test]
fn activate_does_not_stall_behind_another_accounts_quota_http_refresh() {
    use std::sync::{Mutex, OnceLock};

    type GateChannel = (std::sync::mpsc::Sender<()>, std::sync::mpsc::Receiver<()>);
    static GATE: OnceLock<Mutex<Option<GateChannel>>> = OnceLock::new();

    let sandbox = Sandbox::new();
    let future_exp = Some(super::now_secs() + 3_600);
    let account_a = store_account(
        "codex",
        codex_identity("acc-a"),
        token_set("at-a", Some("rt-a"), future_exp),
    );
    let account_b = store_account(
        "codex",
        codex_identity("acc-b"),
        token_set("at-b", Some("rt-b"), future_exp),
    );
    sandbox.write_file(
        &sandbox.tool_path(&[".codex", "auth.json"]),
        json!({ "tokens": { "access_token": "at-a", "account_id": "acc-a" } }).to_string(),
    );

    let (entered_tx, entered_rx) = std::sync::mpsc::channel::<()>();
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    *GATE.get_or_init(|| Mutex::new(None)).lock().unwrap() = Some((entered_tx, release_rx));

    fn blocking_usage(
        _agent: &str,
        creds: &Credentials,
    ) -> Result<UsageOutcome, crate::usage::UsageError> {
        if let Some((entered, release)) = GATE.get().and_then(|m| m.lock().unwrap().take()) {
            let _ = entered.send(());
            let _ = release.recv();
        }
        Ok(UsageOutcome {
            snapshot: snapshot(70.0),
            tokens: creds.tokens.clone(),
            provider_state: None,
            identity: None,
        })
    }

    let id_a = account_a.id.clone();
    let bg = std::thread::spawn(move || refresh_with(&id_a, oauth_rotates, blocking_usage));
    entered_rx.recv().expect("account A entered quota read");

    // While account A is blocked inside quota HTTP, activating account B (same agent)
    // must succeed without waiting on AgentRefreshLock or failing with "busy".
    let outcome = activate_with(&account_b.id, oauth_rotates, usage_ok)
        .expect("activate B must not contend with A's quota HTTP");
    assert!(outcome.account.is_active);

    let _ = release_tx.send(());
    bg.join().unwrap().expect("refresh A finishes");
}

#[test]
fn kiro_local_import_preserves_client_secret_and_activate_rollback_restores_registration() {
    let sandbox = Sandbox::new();
    let roots = crate::switch::PathRoots::from_env();
    let start_url = crate::oauth::agents::kiro::BUILDER_ID_START_URL;
    let old_state = crate::oauth::agents::kiro::KiroState {
        client_id: Some("cid-a".to_string()),
        client_secret: Some("csec-a".to_string()),
        region: Some("us-east-1".to_string()),
        start_url: Some(start_url.to_string()),
        profile_arn: Some("arn:aws:codewhisperer:us-east-1:1:profile/A".to_string()),
    }
    .to_value()
    .expect("old state");
    let tokens_a = token_set("at-kiro-a", Some("rt-kiro-a"), Some(super::now_secs() + 3_600));
    crate::switch::kiro::write(
        &roots,
        &crate::switch::SwitchRequest {
            agent: "kiro",
            tokens: &tokens_a,
            provider_state: Some(&old_state),
            display_name: "a@example.com",
            account_id: Some("user-a"),
            email: Some("a@example.com"),
        },
    )
    .expect("seed local kiro");

    let imported = add_from_local_with("kiro", usage_ok).expect("local import");
    let imported_record = store::get(&imported.id).expect("stored a");
    let imported_state = crate::oauth::agents::kiro::KiroState::parse(
        imported_record.provider_state().expect("ps").as_ref(),
    );
    assert_eq!(imported_state.client_secret.as_deref(), Some("csec-a"));

    let new_state = crate::oauth::agents::kiro::KiroState {
        client_id: Some("cid-b".to_string()),
        client_secret: Some("csec-b".to_string()),
        region: Some("us-east-1".to_string()),
        start_url: Some(start_url.to_string()),
        profile_arn: Some("arn:aws:codewhisperer:us-east-1:1:profile/B".to_string()),
    }
    .to_value()
    .expect("new state");
    let account_b = Account::from_login(
        "kiro",
        &Identity {
            display_name: Some("b@example.com".to_string()),
            email: Some("b@example.com".to_string()),
            account_id: Some("user-b".to_string()),
            plan: None,
        },
        &token_set("at-kiro-b", Some("rt-kiro-b"), Some(super::now_secs() + 3_600)),
        Some(&new_state),
        super::now_secs(),
    )
    .expect("build b");
    let stored_b = store::upsert_replacing_identity(account_b).expect("store b");

    // Force `kiro::write_all` to fail after overwriting `<sha1>.json` and `kiro-auth-token.json`
    // by making `profile.json` a directory.
    let profile_path = crate::switch::paths::kiro_profile_path(&roots);
    if profile_path.exists() {
        std::fs::remove_file(&profile_path).expect("remove seeded profile");
    }
    std::fs::create_dir_all(&profile_path).expect("dir blocking profile.json");

    let err = activate_with(&stored_b.id, oauth_rotates, usage_ok)
        .expect_err("directory profile.json should force switch failure and rollback");
    assert!(matches!(err, crate::AccountError::Io(_) | crate::AccountError::Client(_)));

    let reg_hash = crate::switch::kiro_store::client_id_hash(start_url);
    let reg_path = sandbox.tool_path(&[".aws", "sso", "cache", &format!("{reg_hash}.json")]);
    let reg_after: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&reg_path).expect("reg exists")).expect("json");
    assert_eq!(reg_after["clientId"], "cid-a");
    assert_eq!(reg_after["clientSecret"], "csec-a");
    let local_after = crate::switch::kiro::read(&roots).expect("read").expect("restored");
    assert_eq!(local_after.tokens.access_token, "at-kiro-a");
}
