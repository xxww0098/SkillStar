//! Antigravity IDE switch tests: the OAuth session in the real
//! `state.vscdb` under the sandbox home is the live source of truth.
//! Like the rest of `custody_tests`, every test runs against a sandboxed
//! `SKILLSTAR_DATA_DIR` + `SKILLSTAR_TOOL_SYNC_HOME` + temporary `HOME`;
//! nothing here may touch a real `$HOME` or the macOS login keychain.

use rusqlite::Connection;

use super::*;

fn antigravity_state_db(home: &Path) -> PathBuf {
    let root = if cfg!(target_os = "macos") {
        home.join("Library/Application Support/Antigravity IDE")
    } else if cfg!(target_os = "windows") {
        home.join("AppData/Roaming/Antigravity IDE")
    } else {
        home.join(".config/Antigravity IDE")
    };
    root.join("User/globalStorage/state.vscdb")
}

fn write_antigravity_state(home: &Path, access_token: &str, refresh_token: &str, email: &str) {
    let path = antigravity_state_db(home);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let conn = Connection::open(&path).unwrap();
    conn.execute(
        "CREATE TABLE IF NOT EXISTS ItemTable (key TEXT PRIMARY KEY, value TEXT)",
        [],
    )
    .unwrap();
    drop(conn);
    crate::vscdb::write_antigravity_oauth_token(
        &path,
        access_token,
        refresh_token,
        2_000_000_000,
        Some(email),
    )
    .unwrap();
}

fn read_antigravity_state(home: &Path) -> crate::vscdb::AntigravityOAuthSession {
    crate::vscdb::read_antigravity_oauth_session(&antigravity_state_db(home))
        .unwrap()
        .unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn antigravity_refresh_window_adopts_and_projects_the_live_session() {
    let sb = sandbox();
    let mut account = subscription("antigravity-alice", "antigravity");
    account.display_name = "alice@example.com".into();
    account.access_token_encrypted = Some(crypto::encrypt("alice-access"));
    account.refresh_token_encrypted = Some(crypto::encrypt("alice-refresh"));
    storage::upsert_subscription(account).unwrap();
    write_antigravity_state(
        sb.home.path(),
        "alice-access",
        "alice-refresh",
        "alice@example.com",
    );
    activate_subscription("antigravity-alice").await.unwrap();

    write_antigravity_state(
        sb.home.path(),
        "alice-access-rotated",
        "alice-refresh-rotated",
        "alice@example.com",
    );
    let lease = acquire_cli_refresh_lease("antigravity").await.unwrap();
    let mut row = storage::get_subscription("antigravity-alice").unwrap();
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
    let path = antigravity_state_db(sb.home.path());
    let key = "antigravityUnifiedStateSync.oauthToken";
    let encoded = crate::vscdb::read_item_string(&path, key).unwrap().unwrap();
    let mut blob =
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded).unwrap();
    let extension = [0x98, 0x06, 0x07]; // unknown field 99, varint 7
    blob.extend(extension);
    let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, blob);
    crate::vscdb::write_labeled_items(&path, "Antigravity", &[(key, &encoded)]).unwrap();
    row.access_token_encrypted = Some(crypto::encrypt("alice-access-from-skillstar"));
    row.refresh_token_encrypted = Some(crypto::encrypt("alice-refresh-from-skillstar"));
    let mut row = storage::patch_oauth_credentials(&row).unwrap();
    let outcome = sync_refreshed_active_subscription(&before_refresh, &mut row, &lease)
        .unwrap()
        .expect("active Antigravity account must be projected");

    assert!(outcome.success, "{:?}", outcome.error);
    let live = read_antigravity_state(sb.home.path());
    assert_eq!(live.access_token, "alice-access-from-skillstar");
    assert_eq!(live.refresh_token, "alice-refresh-from-skillstar");
    let encoded = crate::vscdb::read_item_string(&path, key).unwrap().unwrap();
    let blob = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded).unwrap();
    assert!(
        blob.windows(extension.len())
            .any(|bytes| bytes == extension),
        "rotation must preserve client-private protobuf fields"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn antigravity_switch_failure_does_not_move_the_active_pin() {
    let _sb = sandbox();
    let mut alice = subscription("antigravity-alice", "antigravity");
    alice.access_token_encrypted = Some(crypto::encrypt("alice-access"));
    alice.refresh_token_encrypted = Some(crypto::encrypt("alice-refresh"));
    storage::upsert_subscription(alice).unwrap();
    let mut bob = subscription("antigravity-bob", "antigravity");
    bob.access_token_encrypted = Some(crypto::encrypt("bob-access"));
    bob.refresh_token_encrypted = Some(crypto::encrypt("bob-refresh"));
    storage::upsert_subscription(bob).unwrap();
    storage::set_active_subscription("antigravity", "antigravity-alice").unwrap();

    let result = activate_subscription("antigravity-bob").await.unwrap();

    assert!(!result.switch_result.success);
    assert_eq!(
        storage::get_active_subscription("antigravity")
            .unwrap()
            .as_deref(),
        Some("antigravity-alice")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn antigravity_switch_changes_the_real_session_between_two_accounts() {
    let sb = sandbox();
    for (id, access, refresh, email) in [
        (
            "antigravity-alice",
            "alice-access",
            "alice-refresh",
            "alice@example.com",
        ),
        (
            "antigravity-bob",
            "bob-access",
            "bob-refresh",
            "bob@example.com",
        ),
    ] {
        let mut account = subscription(id, "antigravity");
        account.display_name = email.into();
        account.access_token_encrypted = Some(crypto::encrypt(access));
        account.refresh_token_encrypted = Some(crypto::encrypt(refresh));
        storage::upsert_subscription(account).unwrap();
    }
    write_antigravity_state(
        sb.home.path(),
        "alice-access",
        "alice-refresh",
        "alice@example.com",
    );

    activate_subscription("antigravity-alice").await.unwrap();
    let result = activate_subscription("antigravity-bob").await.unwrap();

    assert!(
        result.switch_result.success,
        "{:?}",
        result.switch_result.error
    );
    let live = read_antigravity_state(sb.home.path());
    assert_eq!(live.access_token, "bob-access");
    assert_eq!(live.refresh_token, "bob-refresh");
    assert_eq!(
        reconcile_cli_account("antigravity").await.unwrap(),
        Some(CliAccountState::LinkedTo {
            subscription_id: "antigravity-bob".into()
        })
    );
}
