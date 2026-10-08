//! Cursor IDE switch tests: the real `state.vscdb` under the sandbox home
//! is the live source of truth, so every switch writes into it and every
//! claim is read back out of it.
//! Like the rest of `custody_tests`, every test runs against a sandboxed
//! `SKILLSTAR_DATA_DIR` + `SKILLSTAR_TOOL_SYNC_HOME` + temporary `HOME`;
//! nothing here may touch a real `$HOME` or the macOS login keychain.

use rusqlite::Connection;

use super::*;

fn cursor_state_db(home: &Path) -> PathBuf {
    let root = if cfg!(target_os = "macos") {
        home.join("Library/Application Support/Cursor")
    } else if cfg!(target_os = "windows") {
        home.join("AppData/Roaming/Cursor")
    } else {
        home.join(".config/Cursor")
    };
    root.join("User/globalStorage/state.vscdb")
}

fn write_cursor_state(home: &Path, access_token: &str, refresh_token: &str, email: &str) {
    let path = cursor_state_db(home);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let conn = Connection::open(path).unwrap();
    conn.execute(
        "CREATE TABLE IF NOT EXISTS ItemTable (key TEXT PRIMARY KEY, value TEXT)",
        [],
    )
    .unwrap();
    for (key, value) in [
        ("cursorAuth/accessToken", access_token),
        ("cursorAuth/refreshToken", refresh_token),
        ("cursorAuth/cachedEmail", email),
        ("cursor/accessToken", access_token),
        ("cursor/email", email),
    ] {
        conn.execute(
            "INSERT OR REPLACE INTO ItemTable (key, value) VALUES (?1, ?2)",
            (key, value),
        )
        .unwrap();
    }
}

fn read_cursor_state(home: &Path, key: &str) -> String {
    let conn = Connection::open(cursor_state_db(home)).unwrap();
    conn.query_row("SELECT value FROM ItemTable WHERE key = ?1", [key], |row| {
        row.get(0)
    })
    .unwrap()
}

fn cursor_account(id: &str, access_token: &str, refresh_token: &str, email: &str) -> Subscription {
    let mut sub = subscription(id, "cursor");
    sub.display_name = email.into();
    sub.access_token_encrypted = Some(crypto::encrypt(access_token));
    sub.refresh_token_encrypted = Some(crypto::encrypt(refresh_token));
    sub.oauth_account_id = Some(email.into());
    storage::upsert_subscription(sub).unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn cursor_without_local_state_reports_missing_instead_of_trusting_the_pin() {
    let _sb = sandbox();
    assert_eq!(
        reconcile_cli_account("cursor").await.unwrap(),
        Some(CliAccountState::Missing),
        "Cursor state.vscdb is the live source of truth"
    );
    assert_eq!(
        reconcile_cli_accounts().await.unwrap().get("cursor"),
        Some(&CliAccountState::Missing)
    );
}

#[tokio::test(flavor = "current_thread")]
async fn cursor_switch_writes_the_selected_account_into_state_vscdb() {
    let sb = sandbox();
    let mut alice = subscription("cursor-alice", "cursor");
    alice.display_name = "alice@example.com".into();
    alice.access_token_encrypted = Some(crypto::encrypt("alice-access"));
    alice.refresh_token_encrypted = Some(crypto::encrypt("alice-refresh"));
    alice.oauth_account_id = Some("alice@example.com".into());
    storage::upsert_subscription(alice).unwrap();
    write_cursor_state(
        sb.home.path(),
        "old-access",
        "old-refresh",
        "old@example.com",
    );

    let result = activate_subscription("cursor-alice").await.unwrap();

    assert!(
        result.switch_result.success,
        "Cursor 切号必须写入并回读 state.vscdb: {:?}",
        result.switch_result.error
    );
    assert_eq!(
        read_cursor_state(sb.home.path(), "cursorAuth/accessToken"),
        "alice-access"
    );
    assert_eq!(
        read_cursor_state(sb.home.path(), "cursorAuth/refreshToken"),
        "alice-refresh"
    );
    assert_eq!(
        storage::get_active_subscription("cursor")
            .unwrap()
            .as_deref(),
        Some("cursor-alice")
    );
    assert_eq!(
        reconcile_cli_account("cursor").await.unwrap(),
        Some(CliAccountState::LinkedTo {
            subscription_id: "cursor-alice".into()
        })
    );
}

#[tokio::test(flavor = "current_thread")]
async fn cursor_switch_changes_the_real_session_between_two_accounts() {
    let sb = sandbox();
    cursor_account(
        "cursor-alice",
        "alice-access",
        "alice-refresh",
        "alice@example.com",
    );
    cursor_account("cursor-bob", "bob-access", "bob-refresh", "bob@example.com");
    write_cursor_state(
        sb.home.path(),
        "alice-access",
        "alice-refresh",
        "alice@example.com",
    );

    activate_subscription("cursor-alice").await.unwrap();
    let result = activate_subscription("cursor-bob").await.unwrap();

    assert!(result.switch_result.success);
    assert_eq!(
        read_cursor_state(sb.home.path(), "cursorAuth/accessToken"),
        "bob-access"
    );
    assert_eq!(
        read_cursor_state(sb.home.path(), "cursorAuth/refreshToken"),
        "bob-refresh"
    );
    assert_eq!(
        reconcile_cli_account("cursor").await.unwrap(),
        Some(CliAccountState::LinkedTo {
            subscription_id: "cursor-bob".into()
        })
    );
}

#[tokio::test(flavor = "current_thread")]
async fn cursor_switch_failure_does_not_move_the_active_pin() {
    let _sb = sandbox();
    cursor_account(
        "cursor-alice",
        "alice-access",
        "alice-refresh",
        "alice@example.com",
    );
    cursor_account("cursor-bob", "bob-access", "bob-refresh", "bob@example.com");
    storage::set_active_subscription("cursor", "cursor-alice").unwrap();

    let result = activate_subscription("cursor-bob").await.unwrap();

    assert!(!result.switch_result.success);
    assert!(result.switch_result.error.is_some());
    assert_eq!(
        storage::get_active_subscription("cursor")
            .unwrap()
            .as_deref(),
        Some("cursor-alice")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn cursor_refresh_window_adopts_and_projects_the_live_session() {
    let sb = sandbox();
    cursor_account(
        "cursor-alice",
        "alice-access",
        "alice-refresh",
        "alice@example.com",
    );
    write_cursor_state(
        sb.home.path(),
        "alice-access",
        "alice-refresh",
        "alice@example.com",
    );
    activate_subscription("cursor-alice").await.unwrap();

    write_cursor_state(
        sb.home.path(),
        "alice-access-rotated",
        "alice-refresh-rotated",
        "alice@example.com",
    );
    let lease = acquire_cli_refresh_lease("cursor").await.unwrap();
    let mut row = storage::get_subscription("cursor-alice").unwrap();
    adopt_active_cli_session_before_refresh(&mut row, &lease).unwrap();
    assert_eq!(
        crypto::decrypt(row.access_token_encrypted.as_deref().unwrap()),
        "alice-access-rotated"
    );
    assert_eq!(
        crypto::decrypt(row.refresh_token_encrypted.as_deref().unwrap()),
        "alice-refresh-rotated"
    );

    let before_refresh = row.clone();
    assert!(
        sync_refreshed_active_subscription(&before_refresh, &mut row, &lease)
            .unwrap()
            .is_none()
    );
    row.access_token_encrypted = Some(crypto::encrypt("alice-access-from-skillstar"));
    row.refresh_token_encrypted = Some(crypto::encrypt("alice-refresh-from-skillstar"));
    let mut row = storage::patch_oauth_credentials(&row).unwrap();
    let outcome = sync_refreshed_active_subscription(&before_refresh, &mut row, &lease)
        .unwrap()
        .expect("active Cursor account must be projected");

    assert!(outcome.success, "{:?}", outcome.error);
    assert_eq!(
        read_cursor_state(sb.home.path(), "cursorAuth/accessToken"),
        "alice-access-from-skillstar"
    );
    assert_eq!(
        read_cursor_state(sb.home.path(), "cursorAuth/refreshToken"),
        "alice-refresh-from-skillstar"
    );

    // A login in the IDE while quota is in flight wins over the old pin.
    let before_refresh = row.clone();
    row.access_token_encrypted = Some(crypto::encrypt("alice-late-refresh"));
    write_cursor_state(
        sb.home.path(),
        "bob-access",
        "bob-refresh",
        "bob@example.com",
    );
    assert!(
        sync_refreshed_active_subscription(&before_refresh, &mut row, &lease)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        read_cursor_state(sb.home.path(), "cursorAuth/accessToken"),
        "bob-access"
    );
}
