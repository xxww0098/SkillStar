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

use super::READBACK_FAIL_ENV;
use crate::test_support::{ENV_LOCK, EnvGuard};
use crate::usage_switch::{
    CliAccountState, acquire_cli_refresh_lease, activate_subscription,
    adopt_active_cli_session_before_refresh, forget_subscription_session, reconcile_cli_account,
    reconcile_cli_accounts, sync_refreshed_active_subscription,
};

const PASSWORD: &str = "injected-password";
/// Exact ItemTable keys. `accessToken` is a prefix of `accessTokencn`, so a
/// `contains` check is the bug these literals exist to reject.
const GLOBAL_ITEM_KEY: &str = r#"secret://{"extensionId":"tencent-cloud.coding-copilot","key":"planning-genie.new.accessToken"}"#;
const CN_ITEM_KEY: &str = r#"secret://{"extensionId":"tencent-cloud.coding-copilot","key":"planning-genie.new.accessTokencn"}"#;

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
    _global_password: EnvVarGuard,
    _cn_password: EnvVarGuard,
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
    let (global_password, cn_password) = if password {
        (
            EnvVarGuard::set(super::GLOBAL.password_env, PASSWORD),
            EnvVarGuard::set(super::CN.password_env, PASSWORD),
        )
    } else {
        (
            EnvVarGuard::clear(super::GLOBAL.password_env),
            EnvVarGuard::clear(super::CN.password_env),
        )
    };
    Sandbox {
        _global_password: global_password,
        _cn_password: cn_password,
        _env: env,
        home,
        _data: data,
        _lock: lock,
    }
}

#[derive(Clone, Copy)]
struct Spec {
    profile: &'static super::Profile,
    item_key: &'static str,
    other_key: &'static str,
    app_dir: &'static str,
}

fn specs() -> [Spec; 2] {
    [
        Spec {
            profile: &super::GLOBAL,
            item_key: GLOBAL_ITEM_KEY,
            other_key: CN_ITEM_KEY,
            app_dir: "CodeBuddy",
        },
        Spec {
            profile: &super::CN,
            item_key: CN_ITEM_KEY,
            other_key: GLOBAL_ITEM_KEY,
            app_dir: "CodeBuddy CN",
        },
    ]
}

fn db_path(home: &Path, spec: &Spec) -> PathBuf {
    let resolved = (spec.profile.state_db)().expect("codebuddy db");
    assert!(
        resolved.starts_with(home),
        "db escaped the sandbox: {}",
        resolved.display()
    );
    assert!(
        resolved
            .components()
            .any(|component| component.as_os_str() == spec.app_dir),
        "{}",
        resolved.display()
    );
    assert!(resolved.ends_with("state.vscdb"), "{}", resolved.display());
    resolved
}

fn seed_db(path: &Path, rows: &[(&str, &str)]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let conn = Connection::open(path).unwrap();
    conn.execute(
        "CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT)",
        [],
    )
    .unwrap();
    for (key, value) in rows {
        conn.execute(
            "INSERT INTO ItemTable (key, value) VALUES (?1, ?2)",
            [key, value],
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

fn account_id(spec: &Spec, who: &str) -> String {
    format!("{}-{who}", spec.profile.catalog_id)
}

fn token_of(spec: &Spec, who: &str) -> String {
    format!("{}-token-{who}-0123456789", spec.profile.catalog_id)
}

fn uid_of(spec: &Spec, who: &str) -> String {
    format!("uid-{who}-{}", spec.profile.catalog_id)
}

fn refresh_of(spec: &Spec, who: &str) -> String {
    format!("{}-refresh-{who}-0123456789", spec.profile.catalog_id)
}

fn empty_row(id: &str, catalog_id: &str) -> Subscription {
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
    spec: &Spec,
    who: &str,
    name: &str,
    refresh: Option<&str>,
    expires: Option<i64>,
    enterprise: bool,
) {
    let mut row = empty_row(&account_id(spec, who), spec.profile.catalog_id);
    row.display_name = name.into();
    row.oauth_account_id = Some(uid_of(spec, who));
    row.access_token_encrypted = Some(crypto::encrypt(&token_of(spec, who)));
    row.refresh_token_encrypted = refresh.map(crypto::encrypt);
    row.access_token_expires_at = expires;
    row.oauth_region = Some(
        if spec.profile.catalog_id == "codebuddy" {
            "global"
        } else {
            "cn"
        }
        .into(),
    );
    if enterprise {
        row.provider_state_encrypted = Some(crypto::encrypt(
            r#"{"enterpriseId":"ent","enterpriseName":"Acme","domain":"acme.example"}"#,
        ));
    }
    storage::upsert_subscription(row).unwrap();
}

fn session(path: &Path, spec: &Spec) -> Value {
    let stored = item(path, spec.item_key).expect("secret");
    assert!(
        !stored.starts_with('{'),
        "secret:// must be ciphertext, got {stored}"
    );
    let plain = super::decrypt_stored(spec.profile, &stored).expect("decrypt");
    assert!(
        !plain.contains("planning-genie"),
        "auth blob must not carry the item key: {plain}"
    );
    serde_json::from_str(&plain).expect("session json")
}

fn seal(plain: &str) -> String {
    safe_storage::encrypt_secret(&super::host_key(PASSWORD), plain.as_bytes()).unwrap()
}

fn pin(catalog_id: &str) -> Option<String> {
    storage::get_active_subscription(catalog_id).unwrap()
}

#[test]
fn secret_keys_pin_the_cn_suffix_instead_of_a_second_write() {
    assert_eq!(super::GLOBAL.item_key(), GLOBAL_ITEM_KEY);
    assert_eq!(super::CN.item_key(), CN_ITEM_KEY);
    assert_eq!(super::GLOBAL.secret_key, "planning-genie.new.accessToken");
    assert_eq!(super::CN.secret_key, "planning-genie.new.accessTokencn");
    assert_eq!(
        super::CN.secret_key.strip_prefix(super::GLOBAL.secret_key),
        Some("cn")
    );
    assert_eq!(super::GLOBAL.session_id, "Tencent-Cloud.genie-ide");
    assert_eq!(super::CN.session_id, "Tencent-Cloud.genie-ide-cn");
    assert!(!GLOBAL_ITEM_KEY.contains("accessTokencn"));
    assert!(!CN_ITEM_KEY.contains("accessToken-cn"));
    assert!(!CN_ITEM_KEY.contains("accessToken.cn"));
    assert_ne!(GLOBAL_ITEM_KEY, CN_ITEM_KEY);
    assert_ne!(super::GLOBAL.password_env, super::CN.password_env);
    assert_ne!(super::GLOBAL.catalog_id, super::CN.catalog_id);
}

#[tokio::test(flavor = "current_thread")]
async fn missing_database_is_missing_not_an_absent_adapter() {
    let sb = sandbox(true).await;
    let paths: Vec<_> = specs()
        .iter()
        .map(|spec| db_path(sb.home.path(), spec))
        .collect();
    assert_ne!(paths[0], paths[1]);
    assert!(!paths[1].starts_with(&paths[0]));
    assert!(!paths[0].starts_with(&paths[1]));
    for path in &paths {
        assert!(!path.exists());
    }

    let states = reconcile_cli_accounts().await.unwrap();
    for spec in specs() {
        assert_eq!(
            reconcile_cli_account(spec.profile.catalog_id)
                .await
                .unwrap(),
            Some(CliAccountState::Missing),
            "{}",
            spec.profile.catalog_id
        );
        assert_eq!(
            states.get(spec.profile.catalog_id),
            Some(&CliAccountState::Missing),
            "{}",
            spec.profile.catalog_id
        );
        save_account(&spec, "ada", "ada@example.com", None, None, false);
        storage::set_active_subscription(spec.profile.catalog_id, &account_id(&spec, "ada"))
            .unwrap();
        let missing = activate_subscription(&account_id(&spec, "ada"))
            .await
            .unwrap();
        assert!(!missing.switch_result.success);
        let error = missing.switch_result.error.unwrap();
        assert!(error.contains("state.vscdb"), "{error}");
        assert!(error.contains(spec.profile.display_name), "{error}");
        assert_eq!(
            pin(spec.profile.catalog_id).as_deref(),
            Some(account_id(&spec, "ada").as_str())
        );
    }
    for path in &paths {
        assert!(
            !path.exists(),
            "a missing database is not created or copied"
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn switch_writes_only_its_own_secret_and_leaves_the_other_database() {
    let sb = sandbox(true).await;
    let specs = specs();
    let paths: Vec<_> = specs
        .iter()
        .map(|spec| db_path(sb.home.path(), spec))
        .collect();
    for (spec, path) in specs.iter().zip(paths.iter()) {
        seed_db(
            path,
            &[
                ("unrelated", spec.profile.catalog_id),
                ("codebuddy.sidebar", r#"{"open":true}"#),
                (spec.other_key, "decoy"),
                (spec.item_key, "stale-secret"),
            ],
        );
        save_account(&spec, "ada", "ada@example.com", None, None, false);
        save_account(
            &spec,
            "bob",
            "Bob",
            Some(&refresh_of(spec, "bob")),
            Some(1_793_368_047),
            true,
        );
        storage::set_active_subscription(spec.profile.catalog_id, &account_id(spec, "ada"))
            .unwrap();
    }

    for (index, spec) in specs.iter().enumerate() {
        let path = &paths[index];
        let other_before = fs::read(&paths[1 - index]).unwrap();
        let result = activate_subscription(&account_id(spec, "bob"))
            .await
            .unwrap();
        assert!(
            result.switch_result.success,
            "{}: {:?}",
            spec.profile.catalog_id, result.switch_result.error
        );
        assert!(!result.switch_result.keychain_updated);
        assert_eq!(result.switch_result.tool_id, spec.profile.catalog_id);
        assert_eq!(result.switch_result.config_path, path.display().to_string());
        let backup = result.switch_result.backup_path.expect("backup");
        assert!(Path::new(&backup).starts_with(sb.home.path()), "{backup}");
        assert!(
            fs::read(&backup)
                .unwrap()
                .windows(12)
                .any(|window| window == b"stale-secret"),
            "backup is taken before the auth key is replaced"
        );
        assert_eq!(fs::read(&paths[1 - index]).unwrap(), other_before);
        assert_eq!(item(path, spec.other_key).as_deref(), Some("decoy"));
        assert_eq!(
            item(path, "unrelated").as_deref(),
            Some(spec.profile.catalog_id)
        );
        assert_eq!(
            item(path, "codebuddy.sidebar").as_deref(),
            Some(r#"{"open":true}"#)
        );
        assert!(item(path, spec.item_key).as_deref().unwrap() != "stale-secret");

        let body = session(path, spec);
        let token = token_of(spec, "bob");
        let uid = uid_of(spec, "bob");
        assert_eq!(body["id"], spec.profile.session_id);
        assert_eq!(body["token"], token);
        assert_eq!(body["accessToken"], format!("{uid}+{token}"));
        assert_eq!(body["refreshToken"], refresh_of(spec, "bob"));
        assert_eq!(body["expiresAt"], 1_793_368_047);
        assert_eq!(body["domain"], "acme.example");
        assert_eq!(body["converted"], true);
        assert_eq!(body["account"]["uid"], uid);
        assert_eq!(body["account"]["id"], uid);
        assert_eq!(body["account"]["nickname"], "Bob");
        assert_eq!(body["account"]["label"], "Bob");
        assert_eq!(body["account"]["enterpriseId"], "ent");
        assert_eq!(body["account"]["enterpriseName"], "Acme");
        assert_eq!(body["account"]["pluginEnabled"], true);
        assert_eq!(body["account"]["lastLogin"], true);
        assert_eq!(body["auth"]["accessToken"], token);
        assert_eq!(body["auth"]["refreshToken"], refresh_of(spec, "bob"));
        assert_eq!(body["auth"]["tokenType"], "Bearer");
        assert_eq!(body["auth"]["domain"], "acme.example");
        assert_eq!(body["auth"]["expiresAt"], 1_793_368_047);
        assert_eq!(body["auth"]["expiresIn"], 1_793_368_047);
        assert_eq!(body["auth"]["refreshExpiresIn"], 0);
        assert!(body["auth"]["lastRefreshTime"].as_i64().unwrap() > 0);
        assert_eq!(
            pin(spec.profile.catalog_id).as_deref(),
            Some(account_id(spec, "bob").as_str())
        );
        assert_eq!(
            reconcile_cli_account(spec.profile.catalog_id)
                .await
                .unwrap(),
            Some(CliAccountState::LinkedTo {
                subscription_id: account_id(spec, "bob"),
            })
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn empty_uid_does_not_prefix_the_access_token() {
    let sb = sandbox(true).await;
    for spec in specs() {
        let path = db_path(sb.home.path(), &spec);
        seed_db(&path, &[("unrelated", "keep"), (spec.other_key, "decoy")]);
        let id = account_id(&spec, "bare");
        let mut row = empty_row(&id, spec.profile.catalog_id);
        row.display_name = "bare@example.com".into();
        row.access_token_encrypted = Some(crypto::encrypt(&token_of(&spec, "bare")));
        storage::upsert_subscription(row).unwrap();
        let result = activate_subscription(&id).await.unwrap();
        assert!(
            result.switch_result.success,
            "{:?}",
            result.switch_result.error
        );
        let body = session(&path, &spec);
        let token = token_of(&spec, "bare");
        assert_eq!(body["accessToken"], token);
        assert_eq!(body["token"], token);
        assert_eq!(body["account"]["uid"], "");
        assert_eq!(body["account"]["nickname"], "");
        assert_eq!(body["refreshToken"], "");
        assert_eq!(body["expiresAt"], 0);
        assert_eq!(body["domain"], "");
        assert_eq!(body["account"]["enterpriseId"], "");
        assert_eq!(item(&path, spec.other_key).as_deref(), Some("decoy"));
        assert_eq!(
            reconcile_cli_account(spec.profile.catalog_id)
                .await
                .unwrap(),
            Some(CliAccountState::LinkedTo {
                subscription_id: id.clone(),
            })
        );
        let stored = storage::get_subscription(&id).unwrap();
        assert!(stored.access_token_expires_at.is_none());
        assert!(stored.refresh_token_encrypted.is_none());
        assert!(stored.oauth_account_id.is_none());
    }
}

#[tokio::test(flavor = "current_thread")]
async fn reconcile_reports_each_state_for_both_catalogs() {
    let sb = sandbox(true).await;
    for spec in specs() {
        let path = db_path(sb.home.path(), &spec);
        seed_db(&path, &[("unrelated", "keep")]);
        assert_eq!(
            reconcile_cli_account(spec.profile.catalog_id)
                .await
                .unwrap(),
            Some(CliAccountState::Missing)
        );
        save_account(
            &spec,
            "ada",
            "ada@example.com",
            Some(&refresh_of(&spec, "ada")),
            Some(50),
            false,
        );
        save_account(&spec, "bob", "Bob", None, None, false);
        set_item(
            &path,
            spec.item_key,
            r#"{"accessToken":"someone-else-token-0123456789"}"#,
        );
        assert_eq!(
            reconcile_cli_account(spec.profile.catalog_id)
                .await
                .unwrap(),
            Some(CliAccountState::Diverged)
        );

        let plain = serde_json::json!({
            "token": token_of(&spec, "ada"),
            "uid": uid_of(&spec, "ada"),
        })
        .to_string();
        set_item(&path, spec.item_key, &plain);
        assert_eq!(
            reconcile_cli_account(spec.profile.catalog_id)
                .await
                .unwrap(),
            Some(CliAccountState::LinkedTo {
                subscription_id: account_id(&spec, "ada"),
            })
        );

        let buffer = {
            let encoded = seal(&plain);
            let raw = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .unwrap();
            serde_json::json!({ "type": "Buffer", "data": raw }).to_string()
        };
        set_item(&path, spec.item_key, &buffer);
        assert_eq!(
            reconcile_cli_account(spec.profile.catalog_id)
                .await
                .unwrap(),
            Some(CliAccountState::LinkedTo {
                subscription_id: account_id(&spec, "ada"),
            })
        );

        storage::set_active_subscription(spec.profile.catalog_id, &account_id(&spec, "ada"))
            .unwrap();
        let switched = activate_subscription(&account_id(&spec, "bob"))
            .await
            .unwrap();
        assert!(
            switched.switch_result.success,
            "{:?}",
            switched.switch_result.error
        );
        assert_eq!(
            pin(spec.profile.catalog_id).as_deref(),
            Some(account_id(&spec, "bob").as_str())
        );

        set_item(
            &path,
            spec.item_key,
            &serde_json::json!({
                "token": token_of(&spec, "ada"),
                "uid": uid_of(&spec, "ada"),
            })
            .to_string(),
        );
        assert_eq!(
            reconcile_cli_account(spec.profile.catalog_id)
                .await
                .unwrap(),
            Some(CliAccountState::LinkedTo {
                subscription_id: account_id(&spec, "ada"),
            }),
            "the file wins over the pin"
        );
        assert_eq!(
            pin(spec.profile.catalog_id).as_deref(),
            Some(account_id(&spec, "bob").as_str())
        );
        assert_eq!(
            storage::get_subscription(&account_id(&spec, "ada"))
                .unwrap()
                .access_token_expires_at,
            Some(50)
        );

        let rotated = format!("{}-rotated", token_of(&spec, "bob"));
        let rotated_refresh = format!("{}-rotated", refresh_of(&spec, "bob"));
        let rotated_plain = serde_json::json!({
            "accessToken": format!("{}+{rotated}", uid_of(&spec, "bob")),
            "refreshToken": rotated_refresh,
            "account": { "uid": uid_of(&spec, "bob") },
        })
        .to_string();
        set_item(&path, spec.item_key, &seal(&rotated_plain));
        let lease = acquire_cli_refresh_lease(spec.profile.catalog_id)
            .await
            .unwrap();
        let mut row = storage::get_subscription(&account_id(&spec, "bob")).unwrap();
        adopt_active_cli_session_before_refresh(&mut row, &lease).unwrap();
        assert_eq!(
            crypto::decrypt(row.access_token_encrypted.as_deref().unwrap()),
            rotated
        );
        assert_eq!(
            reconcile_cli_account(spec.profile.catalog_id)
                .await
                .unwrap(),
            Some(CliAccountState::LinkedTo {
                subscription_id: account_id(&spec, "bob"),
            })
        );
        let absorbed = storage::get_subscription(&account_id(&spec, "bob")).unwrap();
        assert_eq!(
            crypto::decrypt(absorbed.access_token_encrypted.as_deref().unwrap()),
            rotated
        );
        assert_eq!(
            crypto::decrypt(absorbed.refresh_token_encrypted.as_deref().unwrap()),
            rotated_refresh
        );
        assert_eq!(
            pin(spec.profile.catalog_id).as_deref(),
            Some(account_id(&spec, "bob").as_str())
        );

        let _wrong = EnvVarGuard::set(spec.profile.password_env, "wrong-password");
        assert_eq!(
            reconcile_cli_account(spec.profile.catalog_id)
                .await
                .unwrap(),
            Some(CliAccountState::Diverged),
            "locked secret material is not Missing"
        );
        drop(_wrong);

        delete_item(&path, spec.item_key);
        assert_eq!(
            reconcile_cli_account(spec.profile.catalog_id)
                .await
                .unwrap(),
            Some(CliAccountState::Missing)
        );
        assert!(path.is_file());
        assert_eq!(item(&path, "unrelated").as_deref(), Some("keep"));
    }
}

#[tokio::test(flavor = "current_thread")]
async fn forget_clears_only_the_matching_auth_key() {
    let sb = sandbox(true).await;
    let specs = specs();
    let paths: Vec<_> = specs
        .iter()
        .map(|spec| db_path(sb.home.path(), spec))
        .collect();
    for (spec, path) in specs.iter().zip(paths.iter()) {
        seed_db(path, &[("unrelated", "keep"), (spec.other_key, "decoy")]);
        save_account(spec, "ada", "ada@example.com", None, None, false);
        save_account(spec, "bob", "Bob", None, None, false);
        activate_subscription(&account_id(spec, "ada"))
            .await
            .unwrap();
    }
    for (index, spec) in specs.iter().enumerate() {
        let path = &paths[index];
        let other_before = fs::read(&paths[1 - index]).unwrap();
        forget_subscription_session(spec.profile.catalog_id, &account_id(spec, "bob")).unwrap();
        assert!(
            item(path, spec.item_key).is_some(),
            "other card must not log the IDE out"
        );
        forget_subscription_session(spec.profile.catalog_id, &account_id(spec, "ada")).unwrap();
        assert!(item(path, spec.item_key).is_none());
        assert_eq!(item(path, spec.other_key).as_deref(), Some("decoy"));
        assert_eq!(item(path, "unrelated").as_deref(), Some("keep"));
        assert!(path.is_file(), "forget does not delete the database");
        assert_eq!(fs::read(&paths[1 - index]).unwrap(), other_before);
        assert_eq!(
            reconcile_cli_account(spec.profile.catalog_id)
                .await
                .unwrap(),
            Some(CliAccountState::Missing)
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn switch_without_a_password_does_not_move_the_pin() {
    let sb = sandbox(false).await;
    for spec in specs() {
        let path = db_path(sb.home.path(), &spec);
        seed_db(
            &path,
            &[(spec.item_key, "stale-secret"), ("unrelated", "keep")],
        );
        save_account(&spec, "ada", "ada@example.com", None, None, false);
        save_account(&spec, "bob", "Bob", None, None, false);
        storage::set_active_subscription(spec.profile.catalog_id, &account_id(&spec, "ada"))
            .unwrap();
        let before = fs::read(&path).unwrap();
        let locked = activate_subscription(&account_id(&spec, "bob"))
            .await
            .unwrap();
        assert!(!locked.switch_result.success);
        let error = locked.switch_result.error.unwrap();
        assert!(error.contains("不会读取系统钥匙串"), "{error}");
        assert!(error.contains(spec.profile.display_name), "{error}");
        assert_eq!(fs::read(&path).unwrap(), before);
        assert_eq!(
            pin(spec.profile.catalog_id).as_deref(),
            Some(account_id(&spec, "ada").as_str())
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn cn_switch_does_not_use_the_global_password() {
    let sb = sandbox(true).await;
    let global = specs()[0];
    let cn = specs()[1];
    let global_path = db_path(sb.home.path(), &global);
    let cn_path = db_path(sb.home.path(), &cn);
    seed_db(&global_path, &[("unrelated", "g")]);
    seed_db(&cn_path, &[(cn.item_key, "stale-cn"), ("unrelated", "c")]);
    save_account(&global, "ada", "Ada", None, None, false);
    save_account(&cn, "ada", "Ada", None, None, false);
    save_account(&cn, "bob", "Bob", None, None, false);
    storage::set_active_subscription(cn.profile.catalog_id, &account_id(&cn, "ada")).unwrap();
    let _cleared = EnvVarGuard::clear(cn.profile.password_env);
    let global_result = activate_subscription(&account_id(&global, "ada"))
        .await
        .unwrap();
    assert!(
        global_result.switch_result.success,
        "{:?}",
        global_result.switch_result.error
    );
    let cn_before = fs::read(&cn_path).unwrap();
    let cn_result = activate_subscription(&account_id(&cn, "bob"))
        .await
        .unwrap();
    assert!(!cn_result.switch_result.success);
    let error = cn_result.switch_result.error.unwrap();
    assert!(error.contains("不会读取系统钥匙串"), "{error}");
    assert!(error.contains("CodeBuddy CN"), "{error}");
    assert_eq!(fs::read(&cn_path).unwrap(), cn_before);
    assert_eq!(item(&cn_path, cn.item_key).as_deref(), Some("stale-cn"));
    assert_eq!(
        pin(cn.profile.catalog_id).as_deref(),
        Some(account_id(&cn, "ada").as_str())
    );
    assert_eq!(
        pin(global.profile.catalog_id).as_deref(),
        Some(account_id(&global, "ada").as_str())
    );
}

#[tokio::test(flavor = "current_thread")]
async fn readback_failure_restores_the_backup_and_skips_the_pin() {
    let sb = sandbox(true).await;
    let specs = specs();
    let paths: Vec<_> = specs
        .iter()
        .map(|spec| db_path(sb.home.path(), spec))
        .collect();
    for (spec, path) in specs.iter().zip(paths.iter()) {
        seed_db(
            path,
            &[
                (spec.item_key, "stale-secret"),
                (spec.other_key, "decoy"),
                ("unrelated", "keep"),
            ],
        );
        save_account(spec, "ada", "ada@example.com", None, None, false);
        save_account(spec, "bob", "Bob", None, None, false);
        storage::set_active_subscription(spec.profile.catalog_id, &account_id(spec, "ada"))
            .unwrap();
    }
    for (index, spec) in specs.iter().enumerate() {
        let path = &paths[index];
        let other_before = fs::read(&paths[1 - index]).unwrap();
        let _fail = EnvVarGuard::set(READBACK_FAIL_ENV, "1");
        let result = activate_subscription(&account_id(spec, "bob"))
            .await
            .unwrap();
        assert!(!result.switch_result.success);
        assert!(result.switch_result.error.unwrap().contains("回读校验失败"));
        assert_eq!(item(path, spec.item_key).as_deref(), Some("stale-secret"));
        assert_eq!(item(path, spec.other_key).as_deref(), Some("decoy"));
        assert_eq!(item(path, "unrelated").as_deref(), Some("keep"));
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
        assert_eq!(fs::read(&paths[1 - index]).unwrap(), other_before);
        assert_eq!(
            pin(spec.profile.catalog_id).as_deref(),
            Some(account_id(spec, "ada").as_str())
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn missing_token_does_not_move_the_pin_or_the_database() {
    let sb = sandbox(true).await;
    for spec in specs() {
        let path = db_path(sb.home.path(), &spec);
        seed_db(&path, &[(spec.item_key, "stale-secret")]);
        let id = account_id(&spec, "empty");
        let mut row = empty_row(&id, spec.profile.catalog_id);
        row.display_name = "Empty".into();
        storage::upsert_subscription(row).unwrap();
        save_account(&spec, "ada", "ada@example.com", None, None, false);
        storage::set_active_subscription(spec.profile.catalog_id, &account_id(&spec, "ada"))
            .unwrap();
        let before = fs::read(&path).unwrap();
        let result = activate_subscription(&id).await.unwrap();
        assert!(!result.switch_result.success);
        assert!(result.switch_result.error.unwrap().contains("access_token"));
        assert_eq!(fs::read(&path).unwrap(), before);
        assert_eq!(
            pin(spec.profile.catalog_id).as_deref(),
            Some(account_id(&spec, "ada").as_str())
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn sync_projects_a_rotated_token_without_moving_the_pin() {
    let sb = sandbox(true).await;
    let specs = specs();
    let paths: Vec<_> = specs
        .iter()
        .map(|spec| db_path(sb.home.path(), spec))
        .collect();
    for (spec, path) in specs.iter().zip(paths.iter()) {
        seed_db(path, &[("unrelated", "keep"), (spec.other_key, "decoy")]);
        save_account(
            spec,
            "ada",
            "Ada",
            Some(&refresh_of(spec, "ada")),
            Some(50),
            true,
        );
        activate_subscription(&account_id(spec, "ada"))
            .await
            .unwrap();
    }
    for (index, spec) in specs.iter().enumerate() {
        let path = &paths[index];
        let other_before = fs::read(&paths[1 - index]).unwrap();
        let lease = acquire_cli_refresh_lease(spec.profile.catalog_id)
            .await
            .unwrap();
        let rotated = format!("{}-rotated", token_of(spec, "ada"));
        let mut row = storage::get_subscription(&account_id(spec, "ada")).unwrap();
        row.access_token_encrypted = Some(crypto::encrypt(&rotated));
        let mut row = storage::patch_oauth_credentials(&row).unwrap();
        let outcome = sync_refreshed_active_subscription(&mut row, &lease)
            .unwrap()
            .expect("active row is projected");
        assert!(outcome.success, "{:?}", outcome.error);
        let body = session(path, spec);
        assert_eq!(body["token"], rotated);
        assert_eq!(
            body["accessToken"],
            format!("{}+{rotated}", uid_of(spec, "ada"))
        );
        assert_eq!(body["id"], spec.profile.session_id);
        assert_eq!(body["account"]["nickname"], "Ada");
        assert_eq!(body["account"]["enterpriseId"], "ent");
        assert_eq!(body["refreshToken"], refresh_of(spec, "ada"));
        assert_eq!(item(path, spec.other_key).as_deref(), Some("decoy"));
        assert_eq!(item(path, "unrelated").as_deref(), Some("keep"));
        assert_eq!(fs::read(&paths[1 - index]).unwrap(), other_before);
        assert_eq!(
            pin(spec.profile.catalog_id).as_deref(),
            Some(account_id(spec, "ada").as_str())
        );
    }
}
