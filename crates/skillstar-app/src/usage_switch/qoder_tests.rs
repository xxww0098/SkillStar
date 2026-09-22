use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use base64::Engine;
use rusqlite::Connection;
use serde_json::Value;
use skillstar_usage::subscription::Subscription;
use skillstar_usage::tool_store::safe_storage;
use skillstar_usage::{crypto, storage};
use tempfile::TempDir;

use super::{
    CREDIT_PLAIN, CREDIT_SECRET, READBACK_FAIL_ENV, SAFE_STORAGE_PASSWORD_ENV, USER_INFO_PLAIN,
    USER_INFO_SECRET, USER_PLAN_PLAIN, USER_PLAN_SECRET, decrypt_stored, host_key,
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
    let resolved = skillstar_usage::tool_paths::qoder_state_db_path().expect("qoder db");
    assert!(
        resolved.starts_with(home),
        "qoder db escaped the sandbox: {}",
        resolved.display()
    );
    assert!(
        resolved
            .components()
            .any(|component| component.as_os_str() == "Qoder"),
        "{}",
        resolved.display()
    );
    resolved
}

fn seed_db(path: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let conn = Connection::open(path).unwrap();
    conn.execute(
        "CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT)",
        [],
    )
    .unwrap();
    for (key, value) in [
        ("unrelated", "keep"),
        ("qoder.sidebar", r#"{"open":true}"#),
        (
            USER_INFO_PLAIN,
            r#"{"token":"stale-token-value-0123456789","email":"stale@qoder.dev"}"#,
        ),
        (USER_PLAN_PLAIN, r#"{"plan":"Stale"}"#),
    ] {
        conn.execute(
            "INSERT INTO ItemTable (key, value) VALUES (?1, ?2)",
            (key, value),
        )
        .unwrap();
    }
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

fn delete_item(path: &Path, key: &str) {
    let conn = Connection::open(path).unwrap();
    conn.execute("DELETE FROM ItemTable WHERE key = ?1", [key])
        .unwrap();
}

fn assert_unrelated(path: &Path) {
    assert_eq!(item(path, "unrelated").as_deref(), Some("keep"));
    assert_eq!(
        item(path, "qoder.sidebar").as_deref(),
        Some(r#"{"open":true}"#)
    );
}

fn empty_row(id: &str) -> Subscription {
    Subscription {
        id: id.into(),
        catalog_id: "qoder".into(),
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

fn save_account(
    id: &str,
    email: &str,
    token: &str,
    refresh: Option<&str>,
    user_id: &str,
    plan: Option<&str>,
    machine_token: Option<&str>,
) {
    let mut row = empty_row(id);
    row.display_name = email.into();
    row.oauth_account_id = Some(user_id.into());
    row.access_token_encrypted = Some(crypto::encrypt(token));
    row.refresh_token_encrypted = refresh.map(crypto::encrypt);
    row.plan_tier = plan.map(str::to_string);
    if let Some(machine_token) = machine_token {
        row.provider_state_encrypted = Some(crypto::encrypt(
            &serde_json::json!({
                "machineToken": machine_token,
                "machineId": "mid-ada",
                "cosy_version": "1.2.3",
            })
            .to_string(),
        ));
    }
    storage::upsert_subscription(row).unwrap();
}

fn user_info(path: &Path) -> Value {
    let plain = decrypt_stored(&item(path, USER_INFO_SECRET).expect("userInfo")).unwrap();
    serde_json::from_str(&plain).unwrap()
}

fn json_item(path: &Path, key: &str) -> Value {
    serde_json::from_str(&decrypt_stored(&item(path, key).expect(key)).unwrap()).unwrap()
}

fn qoder_root(preferred: &Path) -> PathBuf {
    preferred
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

#[tokio::test(flavor = "current_thread")]
async fn missing_database_is_missing_not_an_absent_adapter() {
    let sb = sandbox(true).await;
    let path = db_path(sb.home.path());
    assert!(!path.exists());
    assert_eq!(
        reconcile_cli_account("qoder").await.unwrap(),
        Some(CliAccountState::Missing)
    );

    save_account(
        "qoder-ada",
        "ada@qoder.dev",
        "qoder-token-ada-0123456789",
        None,
        "user-ada",
        None,
        None,
    );
    storage::set_active_subscription("qoder", "qoder-ada").unwrap();
    let missing = activate_subscription("qoder-ada").await.unwrap();
    assert!(!missing.switch_result.success);
    assert!(
        missing
            .switch_result
            .error
            .as_deref()
            .unwrap()
            .contains("state.vscdb"),
        "{:?}",
        missing.switch_result.error
    );
    assert!(
        !path.exists(),
        "a missing database is not created or copied"
    );
    assert_eq!(
        storage::get_active_subscription("qoder")
            .unwrap()
            .as_deref(),
        Some("qoder-ada")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn switch_writes_secret_auth_keys_and_keeps_unrelated_rows() {
    let sb = sandbox(true).await;
    let path = db_path(sb.home.path());
    seed_db(&path);
    save_account(
        "qoder-ada",
        "ada@qoder.dev",
        "qoder-token-ada-0123456789",
        Some("qoder-refresh-ada-0123456789"),
        "user-ada",
        Some("Pro Plus"),
        Some("mt-ada"),
    );
    save_account(
        "qoder-bob",
        "bob@qoder.dev",
        "qoder-token-bob-0123456789",
        Some("qoder-refresh-bob-0123456789"),
        "user-bob",
        None,
        None,
    );

    let result = activate_subscription("qoder-ada").await.unwrap();
    assert!(
        result.switch_result.success,
        "{:?}",
        result.switch_result.error
    );
    assert!(!result.switch_result.keychain_updated);
    let backup = result.switch_result.backup_path.expect("backup");
    assert!(Path::new(&backup).is_file(), "{backup}");
    assert!(Path::new(&backup).starts_with(sb.home.path()), "{backup}");
    assert_eq!(
        storage::get_active_subscription("qoder")
            .unwrap()
            .as_deref(),
        Some("qoder-ada")
    );

    let info = user_info(&path);
    assert_eq!(info["token"], "qoder-token-ada-0123456789");
    assert_eq!(info["refreshToken"], "qoder-refresh-ada-0123456789");
    assert_eq!(info["email"], "ada@qoder.dev");
    assert_eq!(info["id"], "user-ada");
    assert_eq!(info["machineToken"], "mt-ada");
    assert_eq!(info["machineId"], "mid-ada");
    assert_eq!(info["cosy_version"], "1.2.3");
    assert!(info.get("name").is_none());
    assert!(!item(&path, USER_INFO_SECRET).unwrap().contains('@'));
    let plan = json_item(&path, USER_PLAN_SECRET);
    assert_eq!(plan["plan"], "Pro Plus");
    assert_eq!(plan["tier"], "Pro Plus");
    assert_eq!(json_item(&path, CREDIT_SECRET), serde_json::json!({}));
    assert!(item(&path, USER_INFO_PLAIN).is_none());
    assert!(item(&path, USER_PLAN_PLAIN).is_none());
    assert!(item(&path, CREDIT_PLAIN).is_none());
    assert_unrelated(&path);
    assert_eq!(
        reconcile_cli_account("qoder").await.unwrap(),
        Some(CliAccountState::LinkedTo {
            subscription_id: "qoder-ada".into()
        })
    );

    let switched = activate_subscription("qoder-bob").await.unwrap();
    assert!(
        switched.switch_result.success,
        "{:?}",
        switched.switch_result.error
    );
    let info = user_info(&path);
    assert_eq!(info["token"], "qoder-token-bob-0123456789");
    assert_eq!(info["email"], "bob@qoder.dev");
    assert_eq!(info["id"], "user-bob");
    assert!(info.get("machineToken").is_none());
    assert_eq!(json_item(&path, USER_PLAN_SECRET), serde_json::json!({}));
    assert_eq!(json_item(&path, CREDIT_SECRET), serde_json::json!({}));
    assert_unrelated(&path);
    assert_eq!(
        reconcile_cli_account("qoder").await.unwrap(),
        Some(CliAccountState::LinkedTo {
            subscription_id: "qoder-bob".into()
        })
    );
}

#[tokio::test(flavor = "current_thread")]
async fn alternate_candidate_is_the_live_database() {
    let sb = sandbox(true).await;
    let preferred = db_path(sb.home.path());
    assert!(!preferred.exists());
    let alternate = qoder_root(&preferred)
        .join("globalStorage")
        .join("state.vscdb");
    seed_db(&alternate);
    let resolved = skillstar_usage::tool_paths::qoder_state_db_path().unwrap();
    assert_eq!(resolved, alternate);

    save_account(
        "qoder-ada",
        "ada@qoder.dev",
        "qoder-token-ada-0123456789",
        None,
        "user-ada",
        None,
        None,
    );
    let result = activate_subscription("qoder-ada").await.unwrap();
    assert!(
        result.switch_result.success,
        "{:?}",
        result.switch_result.error
    );
    assert_eq!(
        result.switch_result.config_path,
        alternate.display().to_string()
    );
    assert!(
        !preferred.exists(),
        "do not copy the alternate db onto the preferred path"
    );
    assert_eq!(user_info(&alternate)["email"], "ada@qoder.dev");
    assert_unrelated(&alternate);
}

#[tokio::test(flavor = "current_thread")]
async fn reconcile_follows_user_info_email() {
    let sb = sandbox(true).await;
    let path = db_path(sb.home.path());
    seed_db(&path);
    save_account(
        "qoder-ada",
        "Ada@Qoder.dev",
        "qoder-token-ada-0123456789",
        Some("qoder-refresh-ada-0123456789"),
        "user-ada",
        None,
        None,
    );
    save_account(
        "qoder-bob",
        "bob@qoder.dev",
        "qoder-token-bob-0123456789",
        None,
        "user-bob",
        None,
        None,
    );
    delete_item(&path, USER_INFO_PLAIN);
    delete_item(&path, USER_PLAN_PLAIN);
    let secret = safe_storage::encrypt_secret(
        &host_key(PASSWORD),
        br#"{"token":"qoder-token-ada-0123456789","email":"secret@qoder.dev"}"#,
    )
    .unwrap();
    set_item(&path, USER_INFO_SECRET, &secret);
    set_item(
        &path,
        USER_INFO_PLAIN,
        r#"{"token":"other-token-value-0123456789","email":"ada@qoder.dev"}"#,
    );

    assert_eq!(
        reconcile_cli_account("qoder").await.unwrap(),
        Some(CliAccountState::LinkedTo {
            subscription_id: "qoder-ada".into()
        }),
        "plaintext userInfo wins over secret://"
    );
    let absorbed = storage::get_subscription("qoder-ada").unwrap();
    assert_eq!(
        crypto::decrypt(absorbed.access_token_encrypted.as_deref().unwrap()),
        "other-token-value-0123456789"
    );

    delete_item(&path, USER_INFO_PLAIN);
    let other = safe_storage::encrypt_secret(
        &host_key(PASSWORD),
        br#"{"token":"qoder-token-ada-0123456789","email":"other@qoder.dev"}"#,
    )
    .unwrap();
    set_item(&path, USER_INFO_SECRET, &other);
    storage::set_active_subscription("qoder", "qoder-ada").unwrap();
    assert_eq!(
        reconcile_cli_account("qoder").await.unwrap(),
        Some(CliAccountState::Diverged)
    );
    assert_eq!(
        storage::get_active_subscription("qoder")
            .unwrap()
            .as_deref(),
        Some("qoder-ada"),
        "diverged file does not move the pin"
    );

    set_item(
        &path,
        USER_INFO_SECRET,
        &safe_storage::encrypt_secret(
            &host_key(PASSWORD),
            br#"{"token":"qoder-token-ada-0123456789"}"#,
        )
        .unwrap(),
    );
    assert_eq!(
        reconcile_cli_account("qoder").await.unwrap(),
        Some(CliAccountState::Diverged),
        "a token without email is not Missing"
    );
    assert_unrelated(&path);
}

#[tokio::test(flavor = "current_thread")]
async fn forget_clears_auth_keys_and_leaves_the_database() {
    let sb = sandbox(true).await;
    let path = db_path(sb.home.path());
    seed_db(&path);
    save_account(
        "qoder-ada",
        "ada@qoder.dev",
        "qoder-token-ada-0123456789",
        None,
        "user-ada",
        Some("Pro"),
        None,
    );
    save_account(
        "qoder-bob",
        "bob@qoder.dev",
        "qoder-token-bob-0123456789",
        None,
        "user-bob",
        None,
        None,
    );
    activate_subscription("qoder-ada").await.unwrap();

    forget_subscription_session("qoder", "qoder-bob").unwrap();
    assert_eq!(user_info(&path)["email"], "ada@qoder.dev");
    assert_unrelated(&path);

    forget_subscription_session("qoder", "qoder-ada").unwrap();
    assert!(path.is_file(), "forget must not delete the database");
    assert!(item(&path, USER_INFO_SECRET).is_none());
    assert!(item(&path, USER_PLAN_SECRET).is_none());
    assert!(item(&path, CREDIT_SECRET).is_none());
    assert!(item(&path, USER_INFO_PLAIN).is_none());
    assert!(item(&path, USER_PLAN_PLAIN).is_none());
    assert!(item(&path, CREDIT_PLAIN).is_none());
    assert_unrelated(&path);
    assert_eq!(
        reconcile_cli_account("qoder").await.unwrap(),
        Some(CliAccountState::Missing)
    );
}

#[tokio::test(flavor = "current_thread")]
async fn switch_without_a_password_does_not_move_the_pin() {
    let sb = sandbox(false).await;
    let path = db_path(sb.home.path());
    seed_db(&path);
    save_account(
        "qoder-ada",
        "ada@qoder.dev",
        "qoder-token-ada-0123456789",
        None,
        "user-ada",
        None,
        None,
    );
    save_account(
        "qoder-bob",
        "bob@qoder.dev",
        "qoder-token-bob-0123456789",
        None,
        "user-bob",
        None,
        None,
    );
    storage::set_active_subscription("qoder", "qoder-ada").unwrap();

    let locked = activate_subscription("qoder-bob").await.unwrap();
    assert!(!locked.switch_result.success);
    let error = locked.switch_result.error.unwrap();
    assert!(error.contains("不会读取系统钥匙串"), "{error}");
    assert_eq!(
        item(&path, USER_INFO_PLAIN).as_deref(),
        Some(r#"{"token":"stale-token-value-0123456789","email":"stale@qoder.dev"}"#)
    );
    assert!(item(&path, USER_INFO_SECRET).is_none());
    assert_unrelated(&path);
    assert_eq!(
        storage::get_active_subscription("qoder")
            .unwrap()
            .as_deref(),
        Some("qoder-ada")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn readback_failure_restores_the_backup_and_skips_the_pin() {
    let sb = sandbox(true).await;
    let path = db_path(sb.home.path());
    seed_db(&path);
    save_account(
        "qoder-ada",
        "ada@qoder.dev",
        "qoder-token-ada-0123456789",
        None,
        "user-ada",
        None,
        None,
    );
    save_account(
        "qoder-bob",
        "bob@qoder.dev",
        "qoder-token-bob-0123456789",
        None,
        "user-bob",
        None,
        None,
    );
    storage::set_active_subscription("qoder", "qoder-ada").unwrap();
    let _fail = EnvVarGuard::set(READBACK_FAIL_ENV, "1");

    let result = activate_subscription("qoder-bob").await.unwrap();
    assert!(!result.switch_result.success);
    assert!(result.switch_result.error.unwrap().contains("回读校验失败"));
    assert_eq!(
        item(&path, USER_INFO_PLAIN).as_deref(),
        Some(r#"{"token":"stale-token-value-0123456789","email":"stale@qoder.dev"}"#)
    );
    assert!(item(&path, USER_INFO_SECRET).is_none());
    assert_unrelated(&path);
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
        storage::get_active_subscription("qoder")
            .unwrap()
            .as_deref(),
        Some("qoder-ada")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn sync_updates_the_token_and_keeps_an_existing_quota_blob() {
    let sb = sandbox(true).await;
    let path = db_path(sb.home.path());
    seed_db(&path);
    save_account(
        "qoder-ada",
        "ada@qoder.dev",
        "qoder-token-ada-0123456789",
        Some("qoder-refresh-ada-0123456789"),
        "user-ada",
        Some("Pro Plus"),
        Some("mt-ada"),
    );
    activate_subscription("qoder-ada").await.unwrap();
    let marker =
        safe_storage::encrypt_secret(&host_key(PASSWORD), br#"{"used":3,"total":10}"#).unwrap();
    set_item(&path, CREDIT_SECRET, &marker);
    set_item(&path, CREDIT_PLAIN, r#"{"used":9}"#);

    let lease = acquire_cli_refresh_lease("qoder").await.unwrap();
    let mut row = storage::get_subscription("qoder-ada").unwrap();
    row.access_token_encrypted = Some(crypto::encrypt("qoder-token-rotated-0123456789"));
    let mut row = storage::patch_oauth_credentials(&row).unwrap();
    let outcome = sync_refreshed_active_subscription(&mut row, &lease)
        .unwrap()
        .expect("active qoder row is projected");

    assert!(outcome.success, "{:?}", outcome.error);
    assert_eq!(user_info(&path)["token"], "qoder-token-rotated-0123456789");
    assert_eq!(user_info(&path)["email"], "ada@qoder.dev");
    assert_eq!(user_info(&path)["machineToken"], "mt-ada");
    assert_eq!(item(&path, CREDIT_SECRET).as_deref(), Some(marker.as_str()));
    assert_eq!(item(&path, CREDIT_PLAIN).as_deref(), Some(r#"{"used":9}"#));
    assert_eq!(json_item(&path, USER_PLAN_SECRET)["plan"], "Pro Plus");
    assert_unrelated(&path);
    assert_eq!(
        storage::get_active_subscription("qoder")
            .unwrap()
            .as_deref(),
        Some("qoder-ada")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn cockpit_buffer_secret_links_by_email_and_a_wrong_password_diverges() {
    let sb = sandbox(true).await;
    let path = db_path(sb.home.path());
    seed_db(&path);
    delete_item(&path, USER_INFO_PLAIN);
    delete_item(&path, USER_PLAN_PLAIN);
    save_account(
        "qoder-ada",
        "ada@qoder.dev",
        "qoder-token-ada-0123456789",
        None,
        "user-ada",
        None,
        None,
    );
    let encoded = safe_storage::encrypt_secret(
        &host_key(PASSWORD),
        br#"{"securityOauthToken":"qoder-token-ada-0123456789","email":"ada@qoder.dev"}"#,
    )
    .unwrap();
    let raw = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .unwrap();
    let buffer = serde_json::json!({ "type": "Buffer", "data": raw }).to_string();
    set_item(&path, USER_INFO_SECRET, &buffer);

    assert_eq!(
        reconcile_cli_account("qoder").await.unwrap(),
        Some(CliAccountState::LinkedTo {
            subscription_id: "qoder-ada".into()
        })
    );

    let _wrong = EnvVarGuard::set(SAFE_STORAGE_PASSWORD_ENV, "wrong-password");
    assert_eq!(
        reconcile_cli_account("qoder").await.unwrap(),
        Some(CliAccountState::Diverged),
        "locked secret material is not Missing"
    );
}
