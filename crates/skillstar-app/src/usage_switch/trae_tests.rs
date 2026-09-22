use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};
use skillstar_usage::subscription::Subscription;
use skillstar_usage::trae_platform::TraePlatformKind;
use skillstar_usage::{crypto, storage, tool_paths};
use tempfile::TempDir;

use super::READBACK_FAIL_ENV;
use crate::test_support::{ENV_LOCK, EnvGuard};
use crate::usage_switch::{
    CliAccountState, acquire_cli_refresh_lease, activate_subscription,
    adopt_active_cli_session_before_refresh, forget_subscription_session, reconcile_cli_account,
    sync_refreshed_active_subscription,
};

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
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        // SAFETY: ENV_LOCK is still held. Later fields drop after this guard.
        unsafe {
            match self.previous.take() {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }
}

struct Sandbox {
    _env: EnvGuard,
    home: TempDir,
    real_home: Option<PathBuf>,
    _data: TempDir,
    _lock: tokio::sync::MutexGuard<'static, ()>,
}

async fn sandbox() -> Sandbox {
    let lock = ENV_LOCK.lock().await;
    let real_home = dirs::home_dir();
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
    Sandbox {
        _env: env,
        home,
        real_home,
        _data: data,
        _lock: lock,
    }
}

fn storage_path(home: &Path, kind: TraePlatformKind) -> PathBuf {
    let path = tool_paths::trae_storage_path_for(kind).expect("trae storage path");
    assert!(
        path.starts_with(home),
        "storage.json left the sandbox: {}",
        path.display()
    );
    assert!(path.ends_with("storage.json"), "{}", path.display());
    assert!(
        path.components()
            .any(|component| component.as_os_str() == kind.app_support_dir_name()),
        "{}",
        path.display()
    );
    assert!(!path.starts_with(home.join("poison-appdata")));
    assert!(!path.starts_with(home.join("poison-xdg")));
    path
}

fn assert_not_real_home(sb: &Sandbox, path: &Path) {
    let Some(real) = sb.real_home.as_ref() else {
        return;
    };
    assert_ne!(sb.home.path(), real.as_path());
    assert!(
        !path.starts_with(real.join("Library").join("Application Support")),
        "{path:?} escaped into {real:?}"
    );
    assert!(!path.starts_with(real.join(".config")), "{path:?}");
    assert!(
        !path.starts_with(real.join("AppData")),
        "{path:?} escaped into {real:?}"
    );
}

fn write_json(path: &Path, value: &Value) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn decrypted(path: &Path, key: &str) -> Value {
    let root = read_json(path);
    super::opened_value(root.get(key).unwrap_or(&Value::Null))
        .unwrap_or_else(|| panic!("{key} did not decrypt"))
}

fn device_key() -> String {
    format!("{}99", super::DEVICE_PREFIX)
}

fn seeded_root() -> Value {
    let mut map = Map::new();
    map.insert(
        super::DEFAULT_AUTH_KEY.to_string(),
        json!({
            "kept": "inside",
            "accessToken": "old-access-token",
            "refreshToken": "old-refresh-token",
            "userId": "old-user",
            "email": "old@example.com",
            "account": {"scope": "marscode", "email": "old@example.com", "uid": "old-user"}
        }),
    );
    map.insert(
        device_key(),
        json!({"privateKeyPEM": "old-private", "publicKeyPEM": "old-public"}),
    );
    map.insert(
        super::USERTAG_KEY.to_string(),
        Value::String("tag-keep".into()),
    );
    map.insert(
        "iCubeServerData://icube.cloudide".into(),
        Value::String("server-keep".into()),
    );
    map.insert(
        "iCubeEntitlementInfo://icube.cloudide".into(),
        Value::String("ent-keep".into()),
    );
    map.insert("unrelated".into(), json!({"ok": true}));
    Value::Object(map)
}

fn account_id(kind: TraePlatformKind, who: &str) -> String {
    format!("{}-{who}", kind.catalog_id())
}

fn access_of(kind: TraePlatformKind, who: &str) -> String {
    format!("{}-access-{who}", kind.catalog_id())
}

fn refresh_of(kind: TraePlatformKind, who: &str) -> String {
    format!("{}-refresh-{who}", kind.catalog_id())
}

fn uid_of(kind: TraePlatformKind, who: &str) -> String {
    format!("uid-{who}-{}", kind.catalog_id())
}

fn private_pem(kind: TraePlatformKind) -> String {
    format!("private-{}", kind.catalog_id())
}

fn public_pem(kind: TraePlatformKind) -> String {
    format!("public-{}", kind.catalog_id())
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

fn save_account(kind: TraePlatformKind, who: &str, name: &str, with_secret: bool) {
    let mut row = empty_row(&account_id(kind, who), kind.catalog_id());
    row.display_name = name.into();
    if with_secret {
        row.oauth_account_id = Some(uid_of(kind, who));
        row.oauth_region = Some(if kind.is_cn() { "cn" } else { "sg" }.into());
        row.access_token_encrypted = Some(crypto::encrypt(&access_of(kind, who)));
        row.refresh_token_encrypted = Some(crypto::encrypt(&refresh_of(kind, who)));
        row.access_token_expires_at = Some(1_793_368_047);
        row.provider_state_encrypted = Some(crypto::encrypt(
            &json!({
                "deviceKeyPair": {
                    "privateKeyPEM": private_pem(kind),
                    "publicKeyPEM": public_pem(kind),
                },
                "clientId": kind.auth_client_id(),
                "loginHost": kind.default_login_host(),
                "authDomain": kind.auth_domain(),
            })
            .to_string(),
        ));
    }
    storage::upsert_subscription(row).unwrap();
}

fn pin(catalog_id: &str) -> Option<String> {
    storage::get_active_subscription(catalog_id).unwrap()
}

fn backups(path: &Path) -> Vec<PathBuf> {
    let prefix = format!("{}.bak.", path.file_name().unwrap().to_string_lossy());
    fs::read_dir(path.parent().unwrap())
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|candidate| {
            candidate
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(&prefix))
        })
        .collect()
}

fn auth_root(auth: Value, unrelated: &str) -> Value {
    let mut map = Map::new();
    map.insert(super::DEFAULT_AUTH_KEY.to_string(), auth);
    map.insert("unrelated".into(), Value::String(unrelated.into()));
    Value::Object(map)
}

fn assert_no_tmp(path: &Path) {
    let leftovers: Vec<_> = fs::read_dir(path.parent().unwrap())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp"))
        .map(|entry| entry.path())
        .collect();
    assert!(leftovers.is_empty(), "temp files survived: {leftovers:?}");
}

fn assert_untouched_neighbors(paths: &[PathBuf], index: usize, snapshot: &[Vec<u8>]) {
    for (other, bytes) in paths.iter().zip(snapshot) {
        if other == &paths[index] {
            continue;
        }
        assert_eq!(fs::read(other).unwrap(), *bytes, "{}", other.display());
    }
}

#[tokio::test(flavor = "current_thread")]
async fn missing_storage_is_missing_and_does_not_create_a_file() {
    let sb = sandbox().await;
    let paths: Vec<_> = TraePlatformKind::ALL
        .into_iter()
        .map(|kind| storage_path(sb.home.path(), kind))
        .collect();
    for (index, path) in paths.iter().enumerate() {
        assert_not_real_home(&sb, path);
        for other in paths.iter().skip(index + 1) {
            assert_ne!(path, other);
            assert!(!path.starts_with(other));
            assert!(!other.starts_with(path));
        }
        assert!(!path.exists());
    }

    for kind in TraePlatformKind::ALL {
        let path = storage_path(sb.home.path(), kind);
        assert_eq!(
            reconcile_cli_account(kind.catalog_id()).await.unwrap(),
            Some(CliAccountState::Missing),
            "{}",
            kind.catalog_id()
        );
        save_account(kind, "ada", "ada@example.com", true);
        storage::set_active_subscription(kind.catalog_id(), &account_id(kind, "ada")).unwrap();
        let missing = activate_subscription(&account_id(kind, "ada"))
            .await
            .unwrap();
        assert!(!missing.switch_result.success);
        let error = missing.switch_result.error.unwrap();
        assert!(error.contains("storage.json"), "{error}");
        assert!(error.contains(kind.display_name()), "{error}");
        assert!(!path.exists(), "{}", path.display());
        assert_eq!(
            pin(kind.catalog_id()).as_deref(),
            Some(account_id(kind, "ada").as_str())
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn switch_round_trips_auth_keys_for_global_and_cn() {
    let sb = sandbox().await;
    let kinds = TraePlatformKind::ALL;
    assert!(kinds.contains(&TraePlatformKind::Trae));
    assert!(kinds.contains(&TraePlatformKind::TraeCn));
    let paths: Vec<_> = kinds
        .into_iter()
        .map(|kind| storage_path(sb.home.path(), kind))
        .collect();
    for (kind, path) in kinds.into_iter().zip(paths.iter()) {
        assert_not_real_home(&sb, path);
        write_json(path, &seeded_root());
        save_account(kind, "ada", "ada@example.com", true);
        save_account(kind, "bob", "Bob", true);
        storage::set_active_subscription(kind.catalog_id(), &account_id(kind, "ada")).unwrap();
    }

    for (index, kind) in kinds.into_iter().enumerate() {
        let path = &paths[index];
        let snapshot: Vec<_> = paths.iter().map(|path| fs::read(path).unwrap()).collect();
        let result = activate_subscription(&account_id(kind, "bob"))
            .await
            .unwrap();
        assert!(
            result.switch_result.success,
            "{}: {:?}",
            kind.catalog_id(),
            result.switch_result.error
        );
        assert!(!result.switch_result.keychain_updated);
        assert_eq!(result.switch_result.tool_id, kind.catalog_id());
        assert_eq!(result.switch_result.config_path, path.display().to_string());
        let backup = result.switch_result.backup_path.expect("backup");
        assert!(Path::new(&backup).starts_with(sb.home.path()), "{backup}");
        assert!(
            fs::read_to_string(&backup)
                .unwrap()
                .contains("old-access-token"),
            "backup is the file from before the auth key was replaced"
        );
        assert_untouched_neighbors(&paths, index, &snapshot);
        assert_no_tmp(path);

        let raw = fs::read_to_string(path).unwrap();
        assert!(
            !raw.contains("old-access-token"),
            "previous access token must not remain in plaintext"
        );
        let stored = read_json(path);
        let cipher = stored[super::DEFAULT_AUTH_KEY].as_str().unwrap();
        assert!(
            !cipher.starts_with('{'),
            "iCube auth value must be byte_crypto base64, got {cipher}"
        );
        let auth = decrypted(path, super::DEFAULT_AUTH_KEY);
        let token = access_of(kind, "bob");
        let uid = uid_of(kind, "bob");
        assert_eq!(auth["platformId"], kind.provider_key());
        assert_eq!(auth["platformName"], kind.display_name());
        assert_eq!(auth["authClientId"], kind.auth_client_id());
        assert_eq!(auth["clientId"], kind.auth_client_id());
        assert_eq!(auth["authDomain"], kind.auth_domain());
        assert_eq!(auth["loginHost"], kind.default_login_host());
        assert_eq!(auth["host"], kind.default_login_host());
        assert_eq!(auth["accessToken"], token);
        assert_eq!(auth["token"], token);
        assert_eq!(auth["refreshToken"], refresh_of(kind, "bob"));
        assert_eq!(auth["userId"], uid);
        assert_eq!(auth["loginRegion"], if kind.is_cn() { "cn" } else { "sg" });
        assert_eq!(auth["expiresAt"], 1_793_368_047);
        assert_eq!(
            chrono::DateTime::parse_from_rfc3339(auth["expiredAt"].as_str().unwrap())
                .unwrap()
                .timestamp(),
            1_793_368_047
        );
        assert_eq!(auth["kept"], "inside");
        assert_eq!(auth["account"]["scope"], "marscode");
        assert_eq!(auth["account"]["username"], "Bob");
        assert_eq!(auth["account"]["uid"], uid);
        assert!(
            auth.get("email").is_none(),
            "nickname must not be stored as email"
        );
        assert!(auth["account"].get("email").is_none());
        assert_eq!(auth["deviceKeyPair"]["privateKeyPEM"], private_pem(kind));
        assert_eq!(auth["deviceKeyPair"]["publicKeyPEM"], public_pem(kind));

        let device = decrypted(path, &device_key());
        assert_eq!(device["privateKeyPEM"], private_pem(kind));
        assert_eq!(device["publicKeyPEM"], public_pem(kind));
        assert_eq!(stored[super::USERTAG_KEY], "tag-keep");
        assert_eq!(stored["iCubeServerData://icube.cloudide"], "server-keep");
        assert_eq!(stored["iCubeEntitlementInfo://icube.cloudide"], "ent-keep");
        assert_eq!(stored["unrelated"]["ok"], true);
        assert_eq!(
            pin(kind.catalog_id()).as_deref(),
            Some(account_id(kind, "bob").as_str())
        );
        assert_eq!(
            reconcile_cli_account(kind.catalog_id()).await.unwrap(),
            Some(CliAccountState::LinkedTo {
                subscription_id: account_id(kind, "bob"),
            })
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn reconcile_reports_missing_diverged_and_linked_for_global_and_cn() {
    let sb = sandbox().await;
    for kind in [TraePlatformKind::Trae, TraePlatformKind::TraeCn] {
        let path = storage_path(sb.home.path(), kind);
        assert_not_real_home(&sb, &path);
        write_json(&path, &json!({"unrelated": "keep"}));
        assert_eq!(
            reconcile_cli_account(kind.catalog_id()).await.unwrap(),
            Some(CliAccountState::Missing)
        );
        save_account(kind, "ada", "ada@example.com", true);
        save_account(kind, "bob", "Bob", true);
        write_json(
            &path,
            &auth_root(
                json!({"accessToken": "someone-else", "userId": "stranger"}),
                "keep",
            ),
        );
        assert_eq!(
            reconcile_cli_account(kind.catalog_id()).await.unwrap(),
            Some(CliAccountState::Diverged)
        );

        write_json(&path, &auth_root(json!("AAAA"), "keep"));
        assert_eq!(
            reconcile_cli_account(kind.catalog_id()).await.unwrap(),
            Some(CliAccountState::Diverged),
            "undecryptable auth is not Missing"
        );

        write_json(
            &path,
            &auth_root(
                json!({
                    "accessToken": "rotated-access",
                    "refreshToken": "rotated-refresh",
                    "userId": uid_of(kind, "ada"),
                    "expiresAt": 1_700_000_050,
                }),
                "keep",
            ),
        );
        assert_eq!(
            reconcile_cli_account(kind.catalog_id()).await.unwrap(),
            Some(CliAccountState::LinkedTo {
                subscription_id: account_id(kind, "ada"),
            })
        );
        let absorbed = storage::get_subscription(&account_id(kind, "ada")).unwrap();
        assert_eq!(
            crypto::decrypt(absorbed.access_token_encrypted.as_deref().unwrap()),
            "rotated-access"
        );
        assert_eq!(
            crypto::decrypt(absorbed.refresh_token_encrypted.as_deref().unwrap()),
            "rotated-refresh"
        );
        assert_eq!(absorbed.access_token_expires_at, Some(1_700_000_050));

        let root = read_json(&path);
        let mut cleared = root.as_object().unwrap().clone();
        cleared.remove(super::DEFAULT_AUTH_KEY);
        write_json(&path, &Value::Object(cleared));
        assert_eq!(
            reconcile_cli_account(kind.catalog_id()).await.unwrap(),
            Some(CliAccountState::Missing)
        );
        assert_eq!(read_json(&path)["unrelated"], "keep");
    }
}

#[tokio::test(flavor = "current_thread")]
async fn readback_failure_restores_the_backup_and_skips_the_pin() {
    let sb = sandbox().await;
    for kind in [TraePlatformKind::Trae, TraePlatformKind::TraeCn] {
        let path = storage_path(sb.home.path(), kind);
        write_json(&path, &seeded_root());
        save_account(kind, "ada", "ada@example.com", true);
        save_account(kind, "bob", "Bob", true);
        storage::set_active_subscription(kind.catalog_id(), &account_id(kind, "ada")).unwrap();
        let before = fs::read(&path).unwrap();
        let _fail = EnvVarGuard::set(READBACK_FAIL_ENV, "1");
        let result = activate_subscription(&account_id(kind, "bob"))
            .await
            .unwrap();
        assert!(!result.switch_result.success);
        let error = result.switch_result.error.unwrap();
        assert!(error.contains("回读校验失败"), "{error}");
        assert!(error.contains(kind.display_name()), "{error}");
        assert_eq!(fs::read(&path).unwrap(), before);
        assert!(
            !backups(&path).is_empty(),
            "backup is taken before the write"
        );
        assert_no_tmp(&path);
        assert_eq!(
            pin(kind.catalog_id()).as_deref(),
            Some(account_id(kind, "ada").as_str())
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn forget_removes_matching_auth_keys_and_leaves_the_rest() {
    let sb = sandbox().await;
    let kinds = [TraePlatformKind::Trae, TraePlatformKind::TraeCn];
    let paths: Vec<_> = kinds
        .into_iter()
        .map(|kind| storage_path(sb.home.path(), kind))
        .collect();
    for (kind, path) in kinds.into_iter().zip(paths.iter()) {
        write_json(path, &seeded_root());
        save_account(kind, "ada", "ada@example.com", true);
        save_account(kind, "bob", "Bob", true);
        let switched = activate_subscription(&account_id(kind, "bob"))
            .await
            .unwrap();
        assert!(
            switched.switch_result.success,
            "{:?}",
            switched.switch_result.error
        );
    }

    for (index, kind) in kinds.into_iter().enumerate() {
        let path = &paths[index];
        let snapshot: Vec<_> = paths.iter().map(|path| fs::read(path).unwrap()).collect();
        forget_subscription_session(kind.catalog_id(), &account_id(kind, "ada")).unwrap();
        assert!(
            read_json(path).get(super::DEFAULT_AUTH_KEY).is_some(),
            "another card must not log the IDE out"
        );
        assert_eq!(
            decrypted(path, super::DEFAULT_AUTH_KEY)["accessToken"],
            access_of(kind, "bob")
        );
        forget_subscription_session(kind.catalog_id(), &account_id(kind, "bob")).unwrap();
        let root = read_json(path);
        assert!(root.get(super::DEFAULT_AUTH_KEY).is_none());
        assert!(root.get(device_key()).is_none());
        assert_eq!(root[super::USERTAG_KEY], "tag-keep");
        assert_eq!(root["iCubeServerData://icube.cloudide"], "server-keep");
        assert_eq!(root["iCubeEntitlementInfo://icube.cloudide"], "ent-keep");
        assert_eq!(root["unrelated"]["ok"], true);
        assert!(path.is_file(), "forget does not delete storage.json");
        assert_untouched_neighbors(&paths, index, &snapshot);
        assert_eq!(
            reconcile_cli_account(kind.catalog_id()).await.unwrap(),
            Some(CliAccountState::Missing)
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn invalid_json_and_a_missing_token_do_not_rewrite_the_file() {
    let sb = sandbox().await;
    for kind in [TraePlatformKind::Trae, TraePlatformKind::TraeCn] {
        let path = storage_path(sb.home.path(), kind);
        write_json(&path, &seeded_root());
        let id = account_id(kind, "empty");
        let mut row = empty_row(&id, kind.catalog_id());
        row.display_name = "Empty".into();
        storage::upsert_subscription(row).unwrap();
        save_account(kind, "ada", "ada@example.com", true);
        storage::set_active_subscription(kind.catalog_id(), &account_id(kind, "ada")).unwrap();
        let before = fs::read(&path).unwrap();
        let missing_token = activate_subscription(&id).await.unwrap();
        assert!(!missing_token.switch_result.success);
        assert!(
            missing_token
                .switch_result
                .error
                .unwrap()
                .contains("缺少令牌")
        );
        assert_eq!(fs::read(&path).unwrap(), before);
        assert!(backups(&path).is_empty());
        assert_eq!(
            pin(kind.catalog_id()).as_deref(),
            Some(account_id(kind, "ada").as_str())
        );

        fs::write(&path, b"not-json").unwrap();
        let garbage = fs::read(&path).unwrap();
        let invalid = activate_subscription(&account_id(kind, "ada"))
            .await
            .unwrap();
        assert!(!invalid.switch_result.success);
        assert!(
            invalid
                .switch_result
                .error
                .unwrap()
                .contains("不是 JSON 对象")
        );
        assert_eq!(fs::read(&path).unwrap(), garbage);
        assert!(backups(&path).is_empty());

        fs::write(&path, b"[1,2]").unwrap();
        let array = fs::read(&path).unwrap();
        let invalid = activate_subscription(&account_id(kind, "ada"))
            .await
            .unwrap();
        assert!(!invalid.switch_result.success);
        assert_eq!(fs::read(&path).unwrap(), array);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn empty_object_creates_the_default_auth_key_and_no_device_slot() {
    let sb = sandbox().await;
    let kind = TraePlatformKind::Trae;
    let path = storage_path(sb.home.path(), kind);
    assert_not_real_home(&sb, &path);
    write_json(&path, &json!({"unrelated": true}));
    save_account(kind, "bob", "Bob", true);
    let result = activate_subscription(&account_id(kind, "bob"))
        .await
        .unwrap();
    assert!(
        result.switch_result.success,
        "{:?}",
        result.switch_result.error
    );
    let root = read_json(&path);
    assert!(root.get(super::DEFAULT_AUTH_KEY).is_some());
    assert!(
        root.as_object()
            .unwrap()
            .keys()
            .all(|key| !key.starts_with(super::DEVICE_PREFIX))
    );
    assert!(root.get(super::USERTAG_KEY).is_none());
    assert_eq!(root["unrelated"], true);
    assert_eq!(
        decrypted(&path, super::DEFAULT_AUTH_KEY)["accessToken"],
        access_of(kind, "bob")
    );
    assert_eq!(
        decrypted(&path, super::DEFAULT_AUTH_KEY)["platformId"],
        "trae"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn cn_custom_auth_key_is_the_only_one_written() {
    let sb = sandbox().await;
    let kind = TraePlatformKind::TraeCn;
    let path = storage_path(sb.home.path(), kind);
    assert_not_real_home(&sb, &path);
    let custom = "iCubeAuthInfo://custom.ide";
    let mut root = Map::new();
    root.insert(
        custom.to_string(),
        json!({"accessToken": "old-access-token", "kept": "inside"}),
    );
    root.insert("unrelated".into(), json!({"ok": true}));
    root.insert(
        super::USERTAG_KEY.to_string(),
        Value::String("tag-keep".into()),
    );
    write_json(&path, &Value::Object(root));
    save_account(kind, "bob", "Bob", true);
    let result = activate_subscription(&account_id(kind, "bob"))
        .await
        .unwrap();
    assert!(
        result.switch_result.success,
        "{:?}",
        result.switch_result.error
    );
    let root = read_json(&path);
    assert!(root.get(super::DEFAULT_AUTH_KEY).is_none());
    assert_eq!(
        decrypted(&path, custom)["accessToken"],
        access_of(kind, "bob")
    );
    assert_eq!(decrypted(&path, custom)["kept"], "inside");
    assert_eq!(decrypted(&path, custom)["platformName"], "Trae CN");
    assert_eq!(root["unrelated"]["ok"], true);
    assert_eq!(root[super::USERTAG_KEY], "tag-keep");

    forget_subscription_session(kind.catalog_id(), &account_id(kind, "bob")).unwrap();
    let root = read_json(&path);
    assert!(root.get(custom).is_none());
    assert_eq!(root["unrelated"]["ok"], true);
    assert_eq!(root[super::USERTAG_KEY], "tag-keep");
    assert_eq!(
        reconcile_cli_account(kind.catalog_id()).await.unwrap(),
        Some(CliAccountState::Missing)
    );
}

#[tokio::test(flavor = "current_thread")]
async fn adopt_and_sync_follow_the_live_auth_key() {
    let sb = sandbox().await;
    for kind in [TraePlatformKind::Trae, TraePlatformKind::TraeCn] {
        let path = storage_path(sb.home.path(), kind);
        write_json(&path, &seeded_root());
        save_account(kind, "bob", "Bob", true);
        let switched = activate_subscription(&account_id(kind, "bob"))
            .await
            .unwrap();
        assert!(
            switched.switch_result.success,
            "{:?}",
            switched.switch_result.error
        );

        let lease = acquire_cli_refresh_lease(kind.catalog_id()).await.unwrap();
        let rotated = format!("{}-rotated", access_of(kind, "bob"));
        let mut row = storage::get_subscription(&account_id(kind, "bob")).unwrap();
        row.access_token_encrypted = Some(crypto::encrypt(&rotated));
        let mut row = storage::patch_oauth_credentials(&row).unwrap();
        let outcome = sync_refreshed_active_subscription(&mut row, &lease)
            .unwrap()
            .expect("active row is projected");
        assert!(outcome.success, "{:?}", outcome.error);
        assert_eq!(
            decrypted(&path, super::DEFAULT_AUTH_KEY)["accessToken"],
            rotated
        );
        assert_eq!(decrypted(&path, super::DEFAULT_AUTH_KEY)["kept"], "inside");
        assert_eq!(
            read_json(&path)["iCubeServerData://icube.cloudide"],
            "server-keep"
        );
        assert_eq!(
            pin(kind.catalog_id()).as_deref(),
            Some(account_id(kind, "bob").as_str())
        );

        let adopted = format!("{rotated}-from-ide");
        let mut root = read_json(&path);
        root[super::DEFAULT_AUTH_KEY] = json!({
            "accessToken": adopted,
            "userId": uid_of(kind, "bob"),
            "refreshToken": "refresh-from-ide",
        });
        write_json(&path, &root);
        let mut row = storage::get_subscription(&account_id(kind, "bob")).unwrap();
        adopt_active_cli_session_before_refresh(&mut row, &lease).unwrap();
        assert_eq!(
            crypto::decrypt(row.access_token_encrypted.as_deref().unwrap()),
            adopted
        );
        let stored = storage::get_subscription(&account_id(kind, "bob")).unwrap();
        assert_eq!(
            crypto::decrypt(stored.refresh_token_encrypted.as_deref().unwrap()),
            "refresh-from-ide"
        );
        assert_eq!(
            pin(kind.catalog_id()).as_deref(),
            Some(account_id(kind, "bob").as_str())
        );
        assert_eq!(read_json(&path)[super::USERTAG_KEY], "tag-keep");
    }
}
