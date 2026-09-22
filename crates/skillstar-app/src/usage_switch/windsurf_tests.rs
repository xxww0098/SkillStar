use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine;
use rusqlite::Connection;
use serde_json::Value;
use skillstar_usage::subscription::Subscription;
use skillstar_usage::tool_store::safe_storage::{self, KeyMaterial};
use skillstar_usage::{crypto, storage};
use tempfile::TempDir;

use super::{
    API_SERVER_PLAIN_KEY, API_SERVER_SECRET_KEY, AUTH_STATUS_KEY, EXTENSION_STATE_KEY,
    READBACK_FAIL_ENV, SAFE_STORAGE_PASSWORD_ENV, SELECTED_AUTH_KEY, SESSIONS_SECRET_KEY,
    decrypt_stored, host_key,
};
use crate::test_support::{ENV_LOCK, EnvGuard};
use crate::usage_switch::{
    CliAccountState, acquire_cli_refresh_lease, activate_subscription, forget_subscription_session,
    reconcile_cli_account, sync_refreshed_active_subscription,
};

const PASSWORD: &str = "injected-password";

struct EnvVarGuard {
    key: &'static str,
    previous: Option<OsString>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let previous = std::env::var_os(key);
        // SAFETY: the caller holds ENV_LOCK until this guard drops.
        unsafe { std::env::set_var(key, value) };
        Self { key, previous }
    }

    fn clear(key: &'static str) -> Self {
        let previous = std::env::var_os(key);
        // SAFETY: the caller holds ENV_LOCK until this guard drops.
        unsafe { std::env::remove_var(key) };
        Self { key, previous }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        // SAFETY: ENV_LOCK is still held. Field order drops this guard first.
        unsafe {
            match self.previous.take() {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }
}

struct Sandbox {
    _password: EnvVarGuard,
    _env: EnvGuard,
    home: TempDir,
    _data: TempDir,
    _lock: tokio::sync::MutexGuard<'static, ()>,
}

async fn sandbox(password: bool) -> Sandbox {
    let lock = ENV_LOCK.lock().await;
    let data = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let poison_appdata = home.path().join("poison-appdata");
    let poison_xdg = home.path().join("poison-xdg");
    let env = EnvGuard::set(&[
        ("SKILLSTAR_DATA_DIR", data.path()),
        ("SKILLSTAR_TOOL_SYNC_HOME", home.path()),
        ("HOME", home.path()),
        ("USERPROFILE", home.path()),
        ("APPDATA", &poison_appdata),
        ("XDG_CONFIG_HOME", &poison_xdg),
    ]);
    let password = if password {
        EnvVarGuard::set(SAFE_STORAGE_PASSWORD_ENV, PASSWORD)
    } else {
        EnvVarGuard::clear(SAFE_STORAGE_PASSWORD_ENV)
    };
    Sandbox {
        _password: password,
        _env: env,
        home,
        _data: data,
        _lock: lock,
    }
}

fn db_path(home: &Path) -> PathBuf {
    let resolved = skillstar_usage::tool_paths::windsurf_state_db_path().expect("windsurf db");
    assert!(
        resolved.starts_with(home),
        "windsurf db escaped the sandbox: {}",
        resolved.display()
    );
    assert!(
        resolved
            .components()
            .any(|component| component.as_os_str() == "Windsurf"),
        "{}",
        resolved.display()
    );
    resolved
}

fn seed_db(home: &Path) -> PathBuf {
    let path = db_path(home);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let conn = Connection::open(&path).unwrap();
    conn.execute(
        "CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT)",
        [],
    )
    .unwrap();
    for (key, value) in [
        ("unrelated", "keep"),
        ("windsurf_auth-ada-usages", r#"{"cached":true}"#),
        (EXTENSION_STATE_KEY, r#"{"installationId":"inst-1"}"#),
    ] {
        conn.execute(
            "INSERT INTO ItemTable (key, value) VALUES (?1, ?2)",
            (key, value),
        )
        .unwrap();
    }
    path
}

fn item(path: &Path, key: &str) -> Option<String> {
    let conn = Connection::open(path).unwrap();
    conn.query_row("SELECT value FROM ItemTable WHERE key = ?1", [key], |row| {
        row.get(0)
    })
    .ok()
}

fn set_item(path: &Path, key: &str, value: &str) {
    let conn = Connection::open(path).unwrap();
    conn.execute(
        "INSERT OR REPLACE INTO ItemTable (key, value) VALUES (?1, ?2)",
        (key, value),
    )
    .unwrap();
}

fn keys(path: &Path) -> Vec<String> {
    let conn = Connection::open(path).unwrap();
    let mut statement = conn
        .prepare("SELECT key FROM ItemTable ORDER BY key")
        .unwrap();
    statement
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(|row| row.unwrap())
        .collect()
}

fn assert_cache_untouched(path: &Path) {
    assert_eq!(item(path, "unrelated").as_deref(), Some("keep"));
    assert_eq!(
        item(path, "windsurf_auth-ada-usages").as_deref(),
        Some(r#"{"cached":true}"#)
    );
    let names = keys(path);
    assert!(
        names
            .iter()
            .all(|key| !key.starts_with("windsurf_auth-") || key == "windsurf_auth-ada-usages"),
        "{names:?}"
    );
    assert!(!names.iter().any(|key| key == "windsurf_auth-ada@wind.dev"));
    assert!(
        !names
            .iter()
            .any(|key| key == "windsurf_auth-ada@wind.dev-usages")
    );
}

fn empty_row(id: &str) -> Subscription {
    Subscription {
        id: id.into(),
        catalog_id: "windsurf".into(),
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

/// Test fixture builder for a full Windsurf account row.
#[allow(clippy::too_many_arguments)]
fn save_account(
    id: &str,
    email: &str,
    api_key: &str,
    auth_token: &str,
    refresh: &str,
    auth1: &str,
    url: &str,
    proto: &str,
) {
    let mut row = empty_row(id);
    row.display_name = email.into();
    row.oauth_account_id = Some(email.into());
    row.access_token_encrypted = Some(crypto::encrypt(auth_token));
    row.refresh_token_encrypted = Some(crypto::encrypt(refresh));
    row.provider_state_encrypted = Some(crypto::encrypt(
        &serde_json::json!({
            "apiKey": api_key,
            "apiServerUrl": url,
            "auth1Token": auth1,
            "userStatusProtoBinaryBase64": proto,
        })
        .to_string(),
    ));
    storage::upsert_subscription(row).unwrap();
}

fn status(path: &Path) -> Value {
    serde_json::from_str(&item(path, AUTH_STATUS_KEY).expect("auth status")).unwrap()
}

fn key_material() -> KeyMaterial {
    host_key(PASSWORD)
}

#[tokio::test(flavor = "current_thread")]
async fn missing_database_is_missing_not_an_absent_adapter() {
    let sb = sandbox(true).await;
    let path = db_path(sb.home.path());
    assert!(!path.exists());
    assert_eq!(
        reconcile_cli_account("windsurf").await.unwrap(),
        Some(CliAccountState::Missing)
    );
}

#[tokio::test(flavor = "current_thread")]
async fn switch_writes_auth_keys_and_keeps_the_usage_cache() {
    let sb = sandbox(true).await;
    let path = seed_db(sb.home.path());
    save_account(
        "windsurf-ada",
        "ada@wind.dev",
        "sk-ws-alice-key",
        "session-ada",
        "refresh-ada",
        "auth1-ada",
        "https://server.example.test",
        "cHJvdG8tYWRh",
    );
    save_account(
        "windsurf-bob",
        "bob@wind.dev",
        "sk-ws-bob-key",
        "session-bob",
        "refresh-bob",
        "auth1-bob",
        "https://server.bob.test",
        "cHJvdG8tYm9i",
    );

    let result = activate_subscription("windsurf-ada").await.unwrap();
    assert!(
        result.switch_result.success,
        "{:?}",
        result.switch_result.error
    );
    assert!(!result.switch_result.keychain_updated);
    let backup = result.switch_result.backup_path.expect("backup");
    assert!(Path::new(&backup).is_file(), "{backup}");
    assert!(backup.starts_with(sb.home.path().to_string_lossy().as_ref()));
    assert!(item(Path::new(&backup), AUTH_STATUS_KEY).is_none());
    assert_eq!(
        storage::get_active_subscription("windsurf")
            .unwrap()
            .as_deref(),
        Some("windsurf-ada")
    );

    let live = status(&path);
    assert_eq!(live["apiKey"], "sk-ws-alice-key");
    assert_eq!(live["authToken"], "session-ada");
    assert_eq!(live["refreshToken"], "refresh-ada");
    assert_eq!(live["auth1Token"], "auth1-ada");
    assert_eq!(live["email"], "ada@wind.dev");
    assert_eq!(live["apiServerUrl"], "https://server.example.test");
    assert_eq!(live["userStatusProtoBinaryBase64"], "cHJvdG8tYWRh");
    assert_eq!(live["authMethod"], "auth1");
    assert_eq!(
        item(&path, API_SERVER_PLAIN_KEY).as_deref(),
        Some("https://server.example.test")
    );
    assert_eq!(
        item(&path, SELECTED_AUTH_KEY).as_deref(),
        Some("ada@wind.dev")
    );
    assert_eq!(
        decrypt_stored(&item(&path, API_SERVER_SECRET_KEY).unwrap()).unwrap(),
        "https://server.example.test"
    );
    let sessions: Value =
        serde_json::from_str(&decrypt_stored(&item(&path, SESSIONS_SECRET_KEY).unwrap()).unwrap())
            .unwrap();
    assert_eq!(sessions[0]["accessToken"], "sk-ws-alice-key");
    assert_eq!(sessions[0]["account"]["label"], "ada@wind.dev");
    let extension: Value =
        serde_json::from_str(&item(&path, EXTENSION_STATE_KEY).unwrap()).unwrap();
    assert_eq!(extension["installationId"], "inst-1");
    assert_eq!(extension["apiServerUrl"], "https://server.example.test");
    assert_cache_untouched(&path);
    assert_eq!(
        reconcile_cli_account("windsurf").await.unwrap(),
        Some(CliAccountState::LinkedTo {
            subscription_id: "windsurf-ada".into()
        })
    );

    let switched = activate_subscription("windsurf-bob").await.unwrap();
    assert!(
        switched.switch_result.success,
        "{:?}",
        switched.switch_result.error
    );
    assert_eq!(status(&path)["apiKey"], "sk-ws-bob-key");
    assert_eq!(status(&path)["refreshToken"], "refresh-bob");
    assert_eq!(
        item(&path, API_SERVER_PLAIN_KEY).as_deref(),
        Some("https://server.bob.test")
    );
    assert_cache_untouched(&path);
    let extension: Value =
        serde_json::from_str(&item(&path, EXTENSION_STATE_KEY).unwrap()).unwrap();
    assert_eq!(extension["installationId"], "inst-1");
    assert_eq!(
        reconcile_cli_account("windsurf").await.unwrap(),
        Some(CliAccountState::LinkedTo {
            subscription_id: "windsurf-bob".into()
        })
    );
}

#[tokio::test(flavor = "current_thread")]
async fn reconcile_is_diverged_when_the_file_changes_under_the_pin() {
    let sb = sandbox(true).await;
    let path = seed_db(sb.home.path());
    save_account(
        "windsurf-ada",
        "ada@wind.dev",
        "sk-ws-alice-key",
        "session-ada",
        "refresh-ada",
        "auth1-ada",
        "https://server.example.test",
        "cHJvdG8tYWRh",
    );
    activate_subscription("windsurf-ada").await.unwrap();
    set_item(
        &path,
        AUTH_STATUS_KEY,
        r#"{"apiKey":"sk-ws-someone-else","authToken":"other-session","refreshToken":"other-refresh","auth1Token":"other-auth1","email":"ada@wind.dev","apiServerUrl":"https://server.example.test"}"#,
    );

    assert_eq!(
        reconcile_cli_account("windsurf").await.unwrap(),
        Some(CliAccountState::Diverged)
    );
    assert_eq!(
        storage::get_active_subscription("windsurf")
            .unwrap()
            .as_deref(),
        Some("windsurf-ada"),
        "diverged file does not move the pin"
    );
    assert_cache_untouched(&path);
}

#[tokio::test(flavor = "current_thread")]
async fn forget_clears_auth_keys_and_leaves_the_database() {
    let sb = sandbox(true).await;
    let path = seed_db(sb.home.path());
    save_account(
        "windsurf-ada",
        "ada@wind.dev",
        "sk-ws-alice-key",
        "session-ada",
        "refresh-ada",
        "auth1-ada",
        "https://server.example.test",
        "cHJvdG8tYWRh",
    );
    save_account(
        "windsurf-bob",
        "bob@wind.dev",
        "sk-ws-bob-key",
        "session-bob",
        "refresh-bob",
        "auth1-bob",
        "https://server.bob.test",
        "cHJvdG8tYm9i",
    );
    activate_subscription("windsurf-ada").await.unwrap();

    forget_subscription_session("windsurf", "windsurf-bob").unwrap();
    assert_eq!(status(&path)["apiKey"], "sk-ws-alice-key");
    assert_cache_untouched(&path);

    forget_subscription_session("windsurf", "windsurf-ada").unwrap();
    assert!(path.is_file(), "forget must not delete the database");
    assert!(item(&path, AUTH_STATUS_KEY).is_none());
    assert!(item(&path, SESSIONS_SECRET_KEY).is_none());
    assert!(item(&path, API_SERVER_SECRET_KEY).is_none());
    assert!(item(&path, API_SERVER_PLAIN_KEY).is_none());
    assert!(item(&path, SELECTED_AUTH_KEY).is_none());
    let extension: Value =
        serde_json::from_str(&item(&path, EXTENSION_STATE_KEY).unwrap()).unwrap();
    assert_eq!(extension["installationId"], "inst-1");
    assert!(extension.get("apiServerUrl").is_none());
    assert_cache_untouched(&path);
    assert_eq!(
        reconcile_cli_account("windsurf").await.unwrap(),
        Some(CliAccountState::Missing)
    );
}

#[tokio::test(flavor = "current_thread")]
async fn switch_failure_does_not_move_the_pin_or_the_database() {
    let sb = sandbox(false).await;
    save_account(
        "windsurf-ada",
        "ada@wind.dev",
        "sk-ws-alice-key",
        "session-ada",
        "refresh-ada",
        "auth1-ada",
        "https://server.example.test",
        "cHJvdG8tYWRh",
    );
    save_account(
        "windsurf-bob",
        "bob@wind.dev",
        "sk-ws-bob-key",
        "session-bob",
        "refresh-bob",
        "auth1-bob",
        "https://server.bob.test",
        "cHJvdG8tYm9i",
    );
    storage::set_active_subscription("windsurf", "windsurf-ada").unwrap();

    let missing = activate_subscription("windsurf-bob").await.unwrap();
    assert!(!missing.switch_result.success);
    assert!(
        missing
            .switch_result
            .error
            .as_ref()
            .unwrap()
            .contains("state.vscdb")
    );
    assert!(!db_path(sb.home.path()).exists());
    assert_eq!(
        storage::get_active_subscription("windsurf")
            .unwrap()
            .as_deref(),
        Some("windsurf-ada")
    );

    let path = seed_db(sb.home.path());
    let locked = activate_subscription("windsurf-bob").await.unwrap();
    assert!(!locked.switch_result.success, "no injected password");
    let error = locked.switch_result.error.unwrap();
    assert!(error.contains("不会读取系统钥匙串"), "{error}");
    assert!(item(&path, AUTH_STATUS_KEY).is_none());
    assert_cache_untouched(&path);
    assert_eq!(
        storage::get_active_subscription("windsurf")
            .unwrap()
            .as_deref(),
        Some("windsurf-ada")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn sync_projects_a_refreshed_subscription() {
    let sb = sandbox(true).await;
    let path = seed_db(sb.home.path());
    save_account(
        "windsurf-ada",
        "ada@wind.dev",
        "sk-ws-alice-key",
        "session-ada",
        "refresh-ada",
        "auth1-ada",
        "https://server.example.test",
        "cHJvdG8tYWRh",
    );
    activate_subscription("windsurf-ada").await.unwrap();

    let lease = acquire_cli_refresh_lease("windsurf").await.unwrap();
    let mut row = storage::get_subscription("windsurf-ada").unwrap();
    row.refresh_token_encrypted = Some(crypto::encrypt("refresh-rotated"));
    let mut provider: Value = serde_json::from_str(&crypto::decrypt(
        row.provider_state_encrypted.as_deref().unwrap(),
    ))
    .unwrap();
    provider["auth1Token"] = Value::String("auth1-rotated".into());
    row.provider_state_encrypted = Some(crypto::encrypt(&provider.to_string()));
    let mut row = storage::patch_oauth_credentials(&row).unwrap();
    let outcome = sync_refreshed_active_subscription(&mut row, &lease)
        .unwrap()
        .expect("active windsurf row is projected");

    assert!(outcome.success, "{:?}", outcome.error);
    assert_eq!(status(&path)["refreshToken"], "refresh-rotated");
    assert_eq!(status(&path)["auth1Token"], "auth1-rotated");
    assert_eq!(status(&path)["apiKey"], "sk-ws-alice-key");
    assert_cache_untouched(&path);
    assert_eq!(
        storage::get_active_subscription("windsurf")
            .unwrap()
            .as_deref(),
        Some("windsurf-ada")
    );
    assert_eq!(
        reconcile_cli_account("windsurf").await.unwrap(),
        Some(CliAccountState::LinkedTo {
            subscription_id: "windsurf-ada".into()
        })
    );
}

#[tokio::test(flavor = "current_thread")]
async fn readback_failure_restores_the_backup_and_skips_the_pin() {
    let sb = sandbox(true).await;
    let path = seed_db(sb.home.path());
    save_account(
        "windsurf-ada",
        "ada@wind.dev",
        "sk-ws-alice-key",
        "session-ada",
        "refresh-ada",
        "auth1-ada",
        "https://server.example.test",
        "cHJvdG8tYWRh",
    );
    save_account(
        "windsurf-bob",
        "bob@wind.dev",
        "sk-ws-bob-key",
        "session-bob",
        "refresh-bob",
        "auth1-bob",
        "https://server.bob.test",
        "cHJvdG8tYm9i",
    );
    storage::set_active_subscription("windsurf", "windsurf-ada").unwrap();
    let _fail = EnvVarGuard::set(READBACK_FAIL_ENV, "1");

    let result = activate_subscription("windsurf-bob").await.unwrap();
    assert!(!result.switch_result.success);
    assert!(result.switch_result.error.unwrap().contains("回读校验失败"));
    assert!(item(&path, AUTH_STATUS_KEY).is_none());
    assert_eq!(
        item(&path, EXTENSION_STATE_KEY).as_deref(),
        Some(r#"{"installationId":"inst-1"}"#)
    );
    assert_cache_untouched(&path);
    assert!(
        fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(Result::ok)
            .any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("state.vscdb.bak.")
            }),
        "backup is taken before the write"
    );
    assert_eq!(
        storage::get_active_subscription("windsurf")
            .unwrap()
            .as_deref(),
        Some("windsurf-ada")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn cockpit_buffer_sessions_link_when_auth_status_has_no_api_key() {
    let sb = sandbox(true).await;
    let path = seed_db(sb.home.path());
    save_account(
        "windsurf-ada",
        "ada@wind.dev",
        "sk-ws-from-buffer",
        "session-ada",
        "refresh-ada",
        "auth1-ada",
        "https://server.example.test",
        "cHJvdG8tYWRh",
    );
    let sessions = r#"[{"accessToken":"sk-ws-from-buffer","account":{"label":"ada@wind.dev","id":"ada@wind.dev"},"scopes":[]}]"#;
    let encoded = safe_storage::encrypt_secret(&key_material(), sessions.as_bytes()).unwrap();
    let raw = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .unwrap();
    let buffer = serde_json::json!({"type": "Buffer", "data": raw}).to_string();
    set_item(&path, AUTH_STATUS_KEY, r#"{"email":"ada@wind.dev"}"#);
    set_item(&path, SESSIONS_SECRET_KEY, &buffer);

    assert_eq!(
        reconcile_cli_account("windsurf").await.unwrap(),
        Some(CliAccountState::LinkedTo {
            subscription_id: "windsurf-ada".into()
        })
    );

    let _wrong = EnvVarGuard::set(SAFE_STORAGE_PASSWORD_ENV, "wrong-password");
    assert_eq!(
        reconcile_cli_account("windsurf").await.unwrap(),
        Some(CliAccountState::Diverged),
        "locked secret material is not Missing"
    );
}
