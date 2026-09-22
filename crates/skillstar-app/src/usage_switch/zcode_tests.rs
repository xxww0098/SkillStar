use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use skillstar_usage::subscription::Subscription;
use skillstar_usage::tool_store::enc_v1::{decrypt_enc_v1, encrypt_enc_v1, zcode_credential_key};
use skillstar_usage::{crypto, storage};

use super::{CATALOG_ID, READBACK_FAIL_ENV};
use crate::test_support::{ENV_LOCK, EnvGuard};
use crate::usage_switch::CliAccountState;
use crate::usage_switch::ide::IdeCredentialAdapter;

struct Sandbox {
    _env: EnvGuard,
    _data: tempfile::TempDir,
    home: tempfile::TempDir,
    secret: Option<std::ffi::OsString>,
    _lock: tokio::sync::MutexGuard<'static, ()>,
}

impl Sandbox {
    fn home(&self) -> &Path {
        self.home.path()
    }

    fn root(&self) -> PathBuf {
        self.home().join(".zcode")
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        // SAFETY: ENV_LOCK is still held; fields drop after this.
        unsafe {
            match self.secret.take() {
                Some(value) => std::env::set_var("ZCODE_CREDENTIAL_SECRET", value),
                None => std::env::remove_var("ZCODE_CREDENTIAL_SECRET"),
            }
        }
    }
}

async fn sandbox() -> Sandbox {
    let lock = ENV_LOCK.lock().await;
    let data = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let secret = std::env::var_os("ZCODE_CREDENTIAL_SECRET");
    // SAFETY: this test holds ENV_LOCK until the sandbox drops.
    unsafe { std::env::remove_var("ZCODE_CREDENTIAL_SECRET") };
    let env = EnvGuard::set(&[
        ("SKILLSTAR_DATA_DIR", data.path()),
        ("SKILLSTAR_TOOL_SYNC_HOME", home.path()),
    ]);
    Sandbox {
        _env: env,
        _data: data,
        home,
        secret,
        _lock: lock,
    }
}

struct ReadbackFail(Option<std::ffi::OsString>);

impl ReadbackFail {
    fn arm() -> Self {
        let previous = std::env::var_os(READBACK_FAIL_ENV);
        // SAFETY: caller holds the sandbox ENV_LOCK.
        unsafe { std::env::set_var(READBACK_FAIL_ENV, "1") };
        Self(previous)
    }
}

impl Drop for ReadbackFail {
    fn drop(&mut self) {
        // SAFETY: sandbox still holds ENV_LOCK.
        unsafe {
            match self.0.take() {
                Some(value) => std::env::set_var(READBACK_FAIL_ENV, value),
                None => std::env::remove_var(READBACK_FAIL_ENV),
            }
        }
    }
}

fn empty_row(id: &str) -> Subscription {
    Subscription {
        id: id.into(),
        catalog_id: CATALOG_ID.into(),
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

fn save_oauth(
    id: &str,
    provider: &str,
    user: &str,
    access: &str,
    refresh: Option<&str>,
    jwt: &str,
) {
    let mut row = empty_row(id);
    row.display_name = format!("{user}@example.com");
    row.oauth_region = Some(provider.into());
    row.oauth_account_id = Some(user.into());
    row.access_token_encrypted = Some(crypto::encrypt(access));
    row.refresh_token_encrypted = refresh.map(crypto::encrypt);
    row.id_token_encrypted = Some(crypto::encrypt(jwt));
    row.provider_state_encrypted = Some(crypto::encrypt(r#"{"kind":"oauth"}"#));
    storage::upsert_subscription(row).unwrap();
}

fn save_api(id: &str, provider: &str, api_key: &str) {
    let mut row = empty_row(id);
    row.oauth_region = Some(provider.into());
    row.api_key_encrypted = Some(crypto::encrypt(api_key));
    row.provider_state_encrypted = Some(crypto::encrypt(r#"{"kind":"api_key"}"#));
    storage::upsert_subscription(row).unwrap();
}

fn pin() -> Option<String> {
    storage::get_active_subscription(CATALOG_ID).unwrap()
}

fn load(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn reveal(value: &Value, field: &str) -> String {
    decrypt_enc_v1(
        &super::credential_key(),
        value.get(field).and_then(Value::as_str).unwrap(),
    )
    .unwrap()
}

fn assert_inside_sandbox(sb: &Sandbox, path: &Path) {
    assert!(path.starts_with(sb.home()), "{}", path.display());
    let real = skillstar_core::infra::paths::home_dir().join(".zcode");
    assert!(
        !path.starts_with(&real),
        "refusing to touch {}: {}",
        real.display(),
        path.display()
    );
}

fn write_json(path: &Path, value: &Value) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, format!("{value}\n")).unwrap();
}

#[test]
fn zcode_is_switchable_and_instance_launch_stays_pending() {
    assert!(crate::usage_switch::supports_switch(CATALOG_ID));
    assert!(super::Adapter.available());
    let app = crate::instances::DesktopAppId::parse("zcode").unwrap();
    assert_eq!(app.as_str(), "zcode");
    assert!(
        crate::instances::list_desktop_apps()
            .iter()
            .all(|row| row.id != app)
    );
}

#[tokio::test(flavor = "current_thread")]
async fn activate_pins_only_after_the_decrypt_readback_matches() {
    let sb = sandbox().await;
    save_oauth(
        "zcode-a",
        "zai",
        "user-a",
        "access-a",
        Some("refresh-a"),
        "jwt-a",
    );
    save_oauth(
        "zcode-b",
        "zai",
        "user-b",
        "access-b",
        Some("refresh-b"),
        "jwt-b",
    );
    storage::set_active_subscription(CATALOG_ID, "zcode-a").unwrap();
    let path = super::credentials_path();
    let other = encrypt_enc_v1(&super::credential_key(), "bigmodel-access").unwrap();
    write_json(
        &path,
        &json!({
            "preserved": "keep",
            "oauth:bigmodel:access_token": other,
        }),
    );
    let before = std::fs::read(&path).unwrap();

    let (subscription, outcome) = super::Adapter.activate("zcode-b").unwrap();
    assert_eq!(subscription.id, "zcode-b");
    assert!(outcome.success, "{outcome:?}");
    assert!(!outcome.keychain_updated);
    assert_eq!(outcome.config_path, path.display().to_string());
    assert!(outcome.backup_path.is_some());
    assert_eq!(pin().as_deref(), Some("zcode-b"));
    assert_inside_sandbox(&sb, &path);
    assert_eq!(
        super::credential_key(),
        zcode_credential_key(sb.home(), &super::os_username())
    );

    let written = load(&path);
    assert_eq!(written["preserved"], "keep");
    assert_eq!(written["oauth:bigmodel:access_token"], other);
    assert_eq!(reveal(&written, "oauth:active_provider"), "zai");
    assert_eq!(reveal(&written, "oauth:zai:access_token"), "access-b");
    assert_eq!(reveal(&written, "oauth:zai:refresh_token"), "refresh-b");
    assert_eq!(reveal(&written, "zcodejwttoken"), "jwt-b");
    let user: Value = serde_json::from_str(&reveal(&written, "oauth:zai:user_info")).unwrap();
    assert_eq!(user["user_id"], "user-b");
    assert_eq!(user["email"], "user-b@example.com");
    assert!(!super::config_path().exists());
    let settings = load(&super::settings_path());
    assert_eq!(settings["modelProviderFamilyModes"]["zai"], "oauth");

    let backup = PathBuf::from(outcome.backup_path.unwrap());
    assert_inside_sandbox(&sb, &backup);
    assert_eq!(std::fs::read(&backup).unwrap(), before);
    assert_eq!(
        super::Adapter.reconcile().unwrap(),
        Some(CliAccountState::LinkedTo {
            subscription_id: "zcode-b".into(),
        })
    );
}

#[tokio::test(flavor = "current_thread")]
async fn database_dir_is_read_from_setting_json_and_is_not_part_of_the_key() {
    let sb = sandbox().await;
    save_oauth("zcode-b", "bigmodel", "bm-1", "access-b", None, "jwt-b");
    let bootstrap = sb.root().join("v2");
    std::fs::create_dir_all(&bootstrap).unwrap();
    std::fs::write(
        bootstrap.join("settings.json"),
        r#"{"dataBaseDir":"/should/not/use"}"#,
    )
    .unwrap();
    let override_root = sb.home().join("override");
    let bootstrap_file = bootstrap.join("setting.json");
    write_json(
        &bootstrap_file,
        &json!({
            "dataBaseDir": override_root.to_string_lossy(),
            "localePreference": "zh",
        }),
    );
    let before = std::fs::read(&bootstrap_file).unwrap();

    let (subscription, outcome) = super::Adapter.activate("zcode-b").unwrap();
    assert!(outcome.success, "{outcome:?}");
    assert_eq!(subscription.oauth_region.as_deref(), Some("bigmodel"));
    let path = PathBuf::from(&outcome.config_path);
    assert_eq!(
        path,
        override_root
            .join(".zcode")
            .join("v2")
            .join("credentials.json")
    );
    assert_inside_sandbox(&sb, &path);
    assert!(!sb.root().join("v2").join("credentials.json").exists());
    assert_eq!(std::fs::read(&bootstrap_file).unwrap(), before);
    assert_eq!(load(&bootstrap_file)["localePreference"], "zh");
    assert_eq!(
        load(&bootstrap_file)["dataBaseDir"].as_str(),
        Some(override_root.to_string_lossy().as_ref())
    );
    let written = load(&path);
    assert_eq!(reveal(&written, "oauth:active_provider"), "bigmodel");
    assert_eq!(reveal(&written, "oauth:bigmodel:access_token"), "access-b");
    assert!(written.get("oauth:bigmodel:refresh_token").is_none());
    assert_ne!(
        super::credential_key(),
        zcode_credential_key(&override_root.join(".zcode"), &super::os_username())
    );
    let mode = load(&override_root.join(".zcode").join("v2").join("setting.json"));
    assert_eq!(mode["modelProviderFamilyModes"]["bigmodel"], "oauth");
    assert!(mode.get("dataBaseDir").is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn readback_failure_restores_the_backup_and_does_not_move_the_pin() {
    let sb = sandbox().await;
    save_oauth(
        "zcode-a",
        "zai",
        "user-a",
        "access-a",
        Some("refresh-a"),
        "jwt-a",
    );
    save_oauth(
        "zcode-b",
        "zai",
        "user-b",
        "access-b",
        Some("refresh-b"),
        "jwt-b",
    );
    storage::set_active_subscription(CATALOG_ID, "zcode-a").unwrap();
    let path = super::credentials_path();
    write_json(&path, &json!({"preserved": "original", "note": 1}));
    let original = std::fs::read(&path).unwrap();
    let settings = super::settings_path();
    write_json(
        &settings,
        &json!({"localePreference": "zh", "dataBaseDir": ""}),
    );
    let settings_before = std::fs::read(&settings).unwrap();

    let _fail = ReadbackFail::arm();
    let (_subscription, outcome) = super::Adapter.activate("zcode-b").unwrap();
    assert!(!outcome.success);
    assert!(outcome.error.unwrap().contains("回读与写入不一致"));
    assert_eq!(pin().as_deref(), Some("zcode-a"));
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert_eq!(std::fs::read(&settings).unwrap(), settings_before);
    assert_inside_sandbox(&sb, &path);
    drop(_fail);

    std::fs::remove_file(&path).unwrap();
    std::fs::remove_file(&settings).unwrap();
    let _fail = ReadbackFail::arm();
    let (_subscription, outcome) = super::Adapter.activate("zcode-b").unwrap();
    assert!(!outcome.success);
    assert_eq!(pin().as_deref(), Some("zcode-a"));
    assert!(
        !path.exists(),
        "a failed first write must not leave credentials.json"
    );
    assert!(!settings.exists());
}

#[tokio::test(flavor = "current_thread")]
async fn missing_secrets_and_garbage_json_do_not_replace_the_file() {
    let sb = sandbox().await;
    save_oauth("zcode-a", "zai", "user-a", "access-a", None, "jwt-a");
    storage::set_active_subscription(CATALOG_ID, "zcode-a").unwrap();
    let path = super::credentials_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"[]").unwrap();

    let mut bare = empty_row("zcode-empty");
    bare.oauth_region = Some("zai".into());
    bare.id_token_encrypted = Some(crypto::encrypt("jwt"));
    storage::upsert_subscription(bare).unwrap();
    let (_subscription, outcome) = super::Adapter.activate("zcode-empty").unwrap();
    assert!(outcome.error.unwrap().contains("access_token"));
    assert_eq!(std::fs::read(&path).unwrap(), b"[]");
    assert_eq!(pin().as_deref(), Some("zcode-a"));

    let mut no_jwt = empty_row("zcode-nojwt");
    no_jwt.oauth_region = Some("zai".into());
    no_jwt.access_token_encrypted = Some(crypto::encrypt("access"));
    storage::upsert_subscription(no_jwt).unwrap();
    let (_subscription, outcome) = super::Adapter.activate("zcode-nojwt").unwrap();
    assert!(outcome.error.unwrap().contains("JWT"));
    assert_eq!(std::fs::read(&path).unwrap(), b"[]");

    let mut bad = empty_row("zcode-bad");
    bad.oauth_region = Some("nope".into());
    bad.access_token_encrypted = Some(crypto::encrypt("access"));
    bad.id_token_encrypted = Some(crypto::encrypt("jwt"));
    storage::upsert_subscription(bad).unwrap();
    let (_subscription, outcome) = super::Adapter.activate("zcode-bad").unwrap();
    assert!(outcome.error.unwrap().contains("上游"));

    save_api("zcode-space", "zai", "has space");
    let (_subscription, outcome) = super::Adapter.activate("zcode-space").unwrap();
    assert!(outcome.error.unwrap().contains("空白"));
    assert!(!super::config_path().exists());
    assert_eq!(std::fs::read(&path).unwrap(), b"[]");
    assert_eq!(pin().as_deref(), Some("zcode-a"));
    assert_inside_sandbox(&sb, &path);

    let (_subscription, outcome) = super::Adapter.activate("zcode-a").unwrap();
    assert!(outcome.error.unwrap().contains("不是 JSON 对象"));
    assert_eq!(std::fs::read(&path).unwrap(), b"[]");
    assert_eq!(pin().as_deref(), Some("zcode-a"));
}

#[tokio::test(flavor = "current_thread")]
async fn reconcile_is_missing_linked_or_diverged() {
    let sb = sandbox().await;
    assert_eq!(
        super::Adapter.reconcile().unwrap(),
        Some(CliAccountState::Missing)
    );
    save_oauth(
        "zcode-a",
        "zai",
        "user-a",
        "access-a",
        Some("refresh-a"),
        "jwt-a",
    );
    save_oauth(
        "zcode-b",
        "zai",
        "user-b",
        "access-b",
        Some("refresh-b"),
        "jwt-b",
    );
    super::Adapter.activate("zcode-a").unwrap();
    assert_eq!(
        super::Adapter.reconcile().unwrap(),
        Some(CliAccountState::LinkedTo {
            subscription_id: "zcode-a".into(),
        })
    );

    let path = super::credentials_path();
    let mut written = load(&path);
    written["oauth:zai:access_token"] =
        Value::String(encrypt_enc_v1(&super::credential_key(), "access-b").unwrap());
    written["oauth:zai:refresh_token"] =
        Value::String(encrypt_enc_v1(&super::credential_key(), "refresh-b").unwrap());
    written["zcodejwttoken"] =
        Value::String(encrypt_enc_v1(&super::credential_key(), "jwt-b").unwrap());
    write_json(&path, &written);
    assert_eq!(
        super::Adapter.reconcile().unwrap(),
        Some(CliAccountState::LinkedTo {
            subscription_id: "zcode-b".into(),
        })
    );
    assert_eq!(pin().as_deref(), Some("zcode-a"));

    written = load(&path);
    written["oauth:zai:access_token"] =
        Value::String(encrypt_enc_v1(&super::credential_key(), "someone-else").unwrap());
    write_json(&path, &written);
    assert_eq!(
        super::Adapter.reconcile().unwrap(),
        Some(CliAccountState::Diverged)
    );
    std::fs::remove_file(&path).unwrap();
    write_json(
        &super::settings_path(),
        &json!({"modelProviderFamilyModes": {"zai": "oauth"}, "localePreference": "zh"}),
    );
    assert_eq!(
        super::Adapter.reconcile().unwrap(),
        Some(CliAccountState::Missing)
    );
    assert_inside_sandbox(&sb, &path);
}

#[tokio::test(flavor = "current_thread")]
async fn sync_rewrites_the_token_without_moving_the_pin() {
    let _sb = sandbox().await;
    save_oauth(
        "zcode-a",
        "zai",
        "user-a",
        "access-a",
        Some("refresh-a"),
        "jwt-a",
    );
    save_oauth("zcode-b", "zai", "user-b", "access-b", None, "jwt-b");
    super::Adapter.activate("zcode-a").unwrap();
    save_oauth(
        "zcode-a",
        "zai",
        "user-a",
        "access-a-next",
        Some("refresh-a-next"),
        "jwt-a-next",
    );
    let row = storage::get_subscription("zcode-a").unwrap();
    let outcome = super::Adapter.sync(&row).unwrap();
    assert!(outcome.success, "{outcome:?}");
    assert_eq!(pin().as_deref(), Some("zcode-a"));
    let written = load(&super::credentials_path());
    assert_eq!(reveal(&written, "oauth:zai:access_token"), "access-a-next");
    assert_eq!(
        reveal(&written, "oauth:zai:refresh_token"),
        "refresh-a-next"
    );
    assert_eq!(reveal(&written, "zcodejwttoken"), "jwt-a-next");
}

#[tokio::test(flavor = "current_thread")]
async fn forget_removes_this_accounts_fields_or_the_whole_file() {
    let sb = sandbox().await;
    save_oauth(
        "zcode-a",
        "zai",
        "user-a",
        "access-a",
        Some("refresh-a"),
        "jwt-a",
    );
    save_oauth("zcode-b", "bigmodel", "user-b", "access-b", None, "jwt-b");
    super::Adapter.activate("zcode-b").unwrap();
    let path = super::credentials_path();
    let mut written = load(&path);
    let kept = encrypt_enc_v1(&super::credential_key(), "access-a").unwrap();
    written["preserved"] = json!("keep");
    written["oauth:zai:access_token"] = Value::String(kept.clone());
    write_json(&path, &written);
    let settings = super::settings_path();
    let mut mode = load(&settings);
    mode["localePreference"] = json!("zh");
    write_json(&settings, &mode);
    let settings_before = std::fs::read(&settings).unwrap();

    super::Adapter.forget("zcode-b").unwrap();
    let left = load(&path);
    assert_eq!(left["preserved"], "keep");
    assert_eq!(left["oauth:zai:access_token"], kept);
    assert!(left.get("oauth:bigmodel:access_token").is_none());
    assert!(left.get("oauth:active_provider").is_none());
    assert!(left.get("zcodejwttoken").is_none());
    assert_eq!(std::fs::read(&settings).unwrap(), settings_before);
    assert_eq!(pin().as_deref(), Some("zcode-b"));

    write_json(
        &path,
        &json!({
            "oauth:active_provider": encrypt_enc_v1(&super::credential_key(), "zai").unwrap(),
            "oauth:zai:access_token": encrypt_enc_v1(&super::credential_key(), "access-a").unwrap(),
            "zcodejwttoken": encrypt_enc_v1(&super::credential_key(), "jwt-a").unwrap(),
            "oauth:zai:user_info": encrypt_enc_v1(&super::credential_key(), r#"{"user_id":"user-a"}"#).unwrap(),
        }),
    );
    super::Adapter.forget("zcode-a").unwrap();
    assert!(
        !path.exists(),
        "a file that only held this account is removed"
    );
    assert!(settings.is_file());
    super::Adapter.forget("missing").unwrap();
    super::Adapter.forget("zcode-a").unwrap();
    assert_inside_sandbox(&sb, &path);
}

#[tokio::test(flavor = "current_thread")]
async fn oauth_and_api_key_write_different_files() {
    let sb = sandbox().await;
    save_api("zcode-key", "zai", "sk-secret");
    save_oauth(
        "zcode-oauth",
        "bigmodel",
        "user-o",
        "access-o",
        Some("refresh-o"),
        "jwt-o",
    );
    let settings = super::settings_path();
    write_json(&settings, &json!({"localePreference": "zh"}));

    let (_subscription, outcome) = super::Adapter.activate("zcode-key").unwrap();
    assert!(outcome.success, "{outcome:?}");
    assert_eq!(pin().as_deref(), Some("zcode-key"));
    let config = super::config_path();
    assert_eq!(PathBuf::from(&outcome.config_path), config);
    assert_inside_sandbox(&sb, &config);
    assert!(!super::credentials_path().exists());
    let written = load(&config);
    assert_eq!(
        written["providers"]["builtin:zai"]["options"]["apiKey"],
        "sk-secret"
    );
    assert_eq!(written["providers"]["builtin:zai"]["enabled"], true);
    assert_eq!(load(&settings)["localePreference"], "zh");
    assert_eq!(load(&settings)["modelProviderFamilyModes"]["zai"], "apiKey");
    assert_eq!(
        super::Adapter.reconcile().unwrap(),
        Some(CliAccountState::LinkedTo {
            subscription_id: "zcode-key".into(),
        })
    );

    let config_before = std::fs::read(&config).unwrap();
    let (_subscription, outcome) = super::Adapter.activate("zcode-oauth").unwrap();
    assert!(outcome.success, "{outcome:?}");
    assert_eq!(std::fs::read(&config).unwrap(), config_before);
    let credentials = load(&super::credentials_path());
    assert_eq!(
        reveal(&credentials, "oauth:bigmodel:access_token"),
        "access-o"
    );
    assert!(credentials.get("oauth:zai:access_token").is_none());
    assert_eq!(load(&settings)["localePreference"], "zh");
    assert_eq!(
        load(&settings)["modelProviderFamilyModes"]["bigmodel"],
        "oauth"
    );
    assert_eq!(load(&settings)["modelProviderFamilyModes"]["zai"], "apiKey");
    assert_eq!(
        super::Adapter.reconcile().unwrap(),
        Some(CliAccountState::LinkedTo {
            subscription_id: "zcode-oauth".into(),
        })
    );

    save_oauth(
        "zcode-zai",
        "zai",
        "user-z",
        "access-z",
        Some("refresh-z"),
        "jwt-z",
    );
    super::Adapter.activate("zcode-zai").unwrap();
    assert_eq!(
        super::Adapter.reconcile().unwrap(),
        Some(CliAccountState::LinkedTo {
            subscription_id: "zcode-zai".into(),
        })
    );
    super::Adapter.activate("zcode-key").unwrap();
    assert_eq!(
        reveal(&load(&super::credentials_path()), "oauth:zai:access_token"),
        "access-z",
        "an API-key switch must not rewrite credentials.json"
    );
    assert_eq!(load(&settings)["modelProviderFamilyModes"]["zai"], "apiKey");
    assert_eq!(
        super::Adapter.reconcile().unwrap(),
        Some(CliAccountState::LinkedTo {
            subscription_id: "zcode-key".into(),
        })
    );

    let mut config_value = load(&config);
    config_value["theme"] = json!("dark");
    config_value["providers"]["builtin:bigmodel"] = json!({
        "enabled": true,
        "options": {"apiKey": "bm-key", "baseURL": "https://open.bigmodel.cn"},
    });
    write_json(&config, &config_value);
    super::Adapter.forget("zcode-key").unwrap();
    let left = load(&config);
    assert_eq!(left["theme"], "dark");
    assert!(left["providers"].get("builtin:zai").is_none());
    assert_eq!(
        left["providers"]["builtin:bigmodel"]["options"]["apiKey"],
        "bm-key"
    );
    assert_eq!(
        left["providers"]["builtin:bigmodel"]["options"]["baseURL"],
        "https://open.bigmodel.cn"
    );
    assert_eq!(load(&settings)["localePreference"], "zh");
}

#[tokio::test(flavor = "current_thread")]
async fn adopt_takes_the_same_users_newer_token_only() {
    let _sb = sandbox().await;
    save_oauth(
        "zcode-a",
        "zai",
        "user-a",
        "access-a",
        Some("refresh-a"),
        "jwt-a",
    );
    let path = super::credentials_path();
    let key = super::credential_key();
    write_json(
        &path,
        &json!({
            "oauth:active_provider": encrypt_enc_v1(&key, "zai").unwrap(),
            "oauth:zai:access_token": encrypt_enc_v1(&key, "access-from-zcode").unwrap(),
            "oauth:zai:refresh_token": encrypt_enc_v1(&key, "refresh-from-zcode").unwrap(),
            "zcodejwttoken": encrypt_enc_v1(&key, "jwt-from-zcode").unwrap(),
            "oauth:zai:user_info": encrypt_enc_v1(&key, r#"{"user_id":"user-b"}"#).unwrap(),
        }),
    );
    let mut row = storage::get_subscription("zcode-a").unwrap();
    super::adopt(&mut row).unwrap();
    assert_eq!(
        crypto::decrypt(row.access_token_encrypted.as_deref().unwrap()),
        "access-a"
    );

    let mut info = load(&path);
    info["oauth:zai:user_info"] =
        Value::String(encrypt_enc_v1(&key, r#"{"id":"user-a"}"#).unwrap());
    write_json(&path, &info);
    super::adopt(&mut row).unwrap();
    assert_eq!(
        crypto::decrypt(row.access_token_encrypted.as_deref().unwrap()),
        "access-from-zcode"
    );
    assert_eq!(
        crypto::decrypt(row.refresh_token_encrypted.as_deref().unwrap()),
        "refresh-from-zcode"
    );
    assert_eq!(
        crypto::decrypt(row.id_token_encrypted.as_deref().unwrap()),
        "jwt-from-zcode"
    );
    let stored = storage::get_subscription("zcode-a").unwrap();
    assert_eq!(
        crypto::decrypt(stored.access_token_encrypted.as_deref().unwrap()),
        "access-from-zcode"
    );
    assert_eq!(stored.oauth_account_id.as_deref(), Some("user-a"));
    assert_eq!(stored.display_name, "user-a@example.com");
}

#[tokio::test(flavor = "current_thread")]
async fn forget_ignores_a_different_token_for_the_same_provider() {
    let _sb = sandbox().await;
    save_oauth("zcode-a", "zai", "user-a", "access-a", None, "jwt-a");
    let path = super::credentials_path();
    let key = super::credential_key();
    write_json(
        &path,
        &json!({
            "preserved": "keep",
            "oauth:active_provider": encrypt_enc_v1(&key, "zai").unwrap(),
            "oauth:zai:access_token": encrypt_enc_v1(&key, "not-ours").unwrap(),
            "zcodejwttoken": encrypt_enc_v1(&key, "jwt-other").unwrap(),
        }),
    );
    let before = std::fs::read(&path).unwrap();
    super::Adapter.forget("zcode-a").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), before);
}
