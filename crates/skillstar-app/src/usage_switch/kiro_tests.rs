use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde_json::{Value, json};
use skillstar_usage::subscription::Subscription;
use skillstar_usage::{crypto, storage};

use super::{BUILDER_ID_START_URL, READBACK_FAIL_ENV, USAGE_DB_KEY, client_id_hash};
use crate::test_support::{ENV_LOCK, EnvGuard};
use crate::usage_switch::{
    CliAccountState, acquire_cli_refresh_lease, activate_subscription,
    adopt_active_cli_session_before_refresh, forget_subscription_session, reconcile_cli_account,
    sync_refreshed_active_subscription,
};

const EXPIRES: i64 = 1_700_000_000;
const ENTERPRISE_URL: &str = "https://acme.awsapps.com/start";

struct EnvVarGuard {
    key: &'static str,
    previous: Option<std::ffi::OsString>,
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
    _env: EnvGuard,
    home: tempfile::TempDir,
    _data: tempfile::TempDir,
    _lock: tokio::sync::MutexGuard<'static, ()>,
}

async fn sandbox() -> Sandbox {
    let lock = ENV_LOCK.lock().await;
    // `dirs` follows HOME. Read it before the sandbox replaces HOME.
    let real = dirs::home_dir().expect("real home");
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
    let sandbox = Sandbox {
        _env: env,
        home,
        _data: data,
        _lock: lock,
    };
    assert_sandboxed(sandbox.home.path(), &real);
    sandbox
}

fn assert_sandboxed(home: &Path, real: &Path) {
    let auth = auth_path();
    let cache = cache_dir();
    let data = skillstar_usage::tool_paths::kiro_data_dir().expect("kiro dir");
    assert_eq!(
        auth,
        home.join(".aws")
            .join("sso")
            .join("cache")
            .join("kiro-auth-token.json")
    );
    assert!(cache.starts_with(home), "{}", cache.display());
    assert!(data.starts_with(home), "{}", data.display());
    assert!(
        data.components()
            .any(|component| component.as_os_str() == "Kiro"),
        "{}",
        data.display()
    );
    assert_ne!(home, real);
    assert!(!auth.starts_with(real), "{}", auth.display());
    assert!(!data.starts_with(real), "{}", data.display());
    assert!(!auth.starts_with(home.join("poison-appdata")));
    assert!(!data.starts_with(home.join("poison-xdg")));
}

fn auth_path() -> PathBuf {
    skillstar_usage::tool_paths::aws_sso_cache_dir().join("kiro-auth-token.json")
}

fn cache_dir() -> PathBuf {
    skillstar_usage::tool_paths::aws_sso_cache_dir()
}

fn profile_path() -> PathBuf {
    skillstar_usage::tool_paths::kiro_data_dir()
        .unwrap()
        .join("User")
        .join("globalStorage")
        .join("kiro.kiroagent")
        .join("profile.json")
}

fn db_path() -> PathBuf {
    skillstar_usage::tool_paths::kiro_data_dir()
        .unwrap()
        .join("User")
        .join("globalStorage")
        .join("state.vscdb")
}

fn registration_path(start_url: &str) -> PathBuf {
    cache_dir().join(format!("{}.json", client_id_hash(start_url)))
}

fn seed_cache_decoy() {
    fs::create_dir_all(cache_dir()).unwrap();
    fs::write(
        cache_dir().join("unrelated.json"),
        r#"{"accessToken":"aws-cli-other","clientId":"not-kiro"}"#,
    )
    .unwrap();
}

fn seed_db() -> PathBuf {
    let path = db_path();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let conn = Connection::open(&path).unwrap();
    conn.execute(
        "CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO ItemTable (key, value) VALUES (?1, ?2)",
        ("unrelated", "keep"),
    )
    .unwrap();
    path
}

fn item(path: &Path, key: &str) -> Option<String> {
    let conn = Connection::open(path).unwrap();
    conn.query_row("SELECT value FROM ItemTable WHERE key = ?1", [key], |row| {
        row.get(0)
    })
    .ok()
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn mode_is_private(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(path).unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0, "{path:?} mode {mode:o}");
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

fn empty_row(id: &str) -> Subscription {
    Subscription {
        id: id.into(),
        catalog_id: "kiro".into(),
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

fn save_idc(
    id: &str,
    email: &str,
    user_id: &str,
    access: &str,
    refresh: &str,
    client_id: &str,
    client_secret: &str,
    start_url: &str,
    region: &str,
    profile_arn: &str,
) {
    let mut row = empty_row(id);
    row.display_name = email.into();
    row.oauth_account_id = Some(user_id.into());
    row.oauth_region = Some(region.into());
    row.access_token_encrypted = Some(crypto::encrypt(access));
    row.refresh_token_encrypted = Some(crypto::encrypt(refresh));
    row.access_token_expires_at = Some(EXPIRES);
    row.provider_state_encrypted = Some(crypto::encrypt(
        &json!({
            "clientId": client_id,
            "clientSecret": client_secret,
            "region": region,
            "startUrl": start_url,
            "profileArn": profile_arn,
        })
        .to_string(),
    ));
    storage::upsert_subscription(row).unwrap();
}

fn save_portal(id: &str, email: &str, user_id: &str, access: &str, refresh: &str) {
    let mut row = empty_row(id);
    row.display_name = email.into();
    row.oauth_account_id = Some(user_id.into());
    row.access_token_encrypted = Some(crypto::encrypt(access));
    row.refresh_token_encrypted = Some(crypto::encrypt(refresh));
    row.provider_state_encrypted = Some(crypto::encrypt(
        &json!({
            "provider": "Github",
            "region": "us-east-1",
            "profileArn": "arn:aws:codewhisperer:us-east-1:1:profile/portal",
        })
        .to_string(),
    ));
    storage::upsert_subscription(row).unwrap();
}

fn pinned() -> Option<String> {
    storage::get_active_subscription("kiro").unwrap()
}

fn decrypted(slot: Option<&str>) -> String {
    crypto::decrypt(slot.unwrap_or(""))
}

#[tokio::test(flavor = "current_thread")]
async fn missing_store_is_missing_not_an_absent_adapter() {
    let _sb = sandbox().await;
    assert!(!auth_path().exists());
    assert!(!db_path().exists());
    assert_eq!(
        reconcile_cli_account("kiro").await.unwrap(),
        Some(CliAccountState::Missing)
    );
}

#[tokio::test(flavor = "current_thread")]
async fn switch_writes_cache_profile_and_vscdb_without_touching_other_aws_files() {
    let sb = sandbox().await;
    assert_eq!(
        client_id_hash(BUILDER_ID_START_URL),
        "cc18142e2bfa693e309f59d910dcef90c3c47767"
    );
    seed_cache_decoy();
    let ada_registration = registration_path(BUILDER_ID_START_URL);
    fs::write(
        &ada_registration,
        r#"{"clientId":"ada-client","clientSecret":"old-secret","expiresAt":"2030-01-01T00:00:00Z"}"#,
    )
    .unwrap();
    let db = seed_db();
    save_idc(
        "kiro-ada",
        "ada@kiro.dev",
        "ada-user",
        "access-ada",
        "refresh-ada",
        "ada-client",
        "ada-secret",
        BUILDER_ID_START_URL,
        "us-east-1",
        "arn:aws:codewhisperer:us-east-1:1:profile/ada",
    );
    save_idc(
        "kiro-bob",
        "bob@kiro.dev",
        "bob-user",
        "access-bob",
        "refresh-bob",
        "bob-client",
        "bob-secret",
        ENTERPRISE_URL,
        "eu-central-1",
        "arn:aws:codewhisperer:eu-central-1:1:profile/bob",
    );
    storage::set_active_subscription("kiro", "kiro-ada").unwrap();

    let switched = activate_subscription("kiro-ada").await.unwrap();
    assert!(
        switched.switch_result.success,
        "{:?}",
        switched.switch_result.error
    );
    assert!(!switched.switch_result.keychain_updated);
    assert_eq!(pinned().as_deref(), Some("kiro-ada"));
    let backup = switched
        .switch_result
        .backup_path
        .expect("registration or token backup");
    assert!(Path::new(&backup).starts_with(sb.home.path()), "{backup}");

    let token = read_json(&auth_path());
    assert_eq!(token["accessToken"], "access-ada");
    assert_eq!(token["refreshToken"], "refresh-ada");
    assert_eq!(token["authMethod"], "IdC");
    assert_eq!(token["provider"], "BuilderId");
    assert_eq!(token["clientId"], "ada-client");
    assert_eq!(
        token["clientIdHash"],
        "cc18142e2bfa693e309f59d910dcef90c3c47767"
    );
    assert!(token.get("clientSecret").is_none());
    assert!(token.get("client_secret").is_none());
    assert_eq!(token["email"], "ada@kiro.dev");
    assert_eq!(token["userId"], "ada-user");
    assert_eq!(
        token["profileArn"],
        "arn:aws:codewhisperer:us-east-1:1:profile/ada"
    );
    let expires = chrono::DateTime::parse_from_rfc3339(token["expiresAt"].as_str().unwrap())
        .unwrap()
        .timestamp();
    assert_eq!(expires, EXPIRES);
    mode_is_private(&auth_path());

    let registration = read_json(&ada_registration);
    assert_eq!(registration["clientId"], "ada-client");
    assert_eq!(registration["clientSecret"], "ada-secret");
    assert_eq!(registration["expiresAt"], "2030-01-01T00:00:00Z");
    mode_is_private(&ada_registration);

    let profile = read_json(&profile_path());
    assert_eq!(profile["email"], "ada@kiro.dev");
    assert_eq!(profile["userId"], "ada-user");
    assert!(profile_path().starts_with(sb.home.path()));
    let usage: Value = serde_json::from_str(&item(&db, USAGE_DB_KEY).unwrap()).unwrap();
    assert_eq!(usage["userInfo"]["userId"], "ada-user");
    assert_eq!(item(&db, "unrelated").as_deref(), Some("keep"));
    assert_eq!(
        fs::read_to_string(cache_dir().join("unrelated.json")).unwrap(),
        r#"{"accessToken":"aws-cli-other","clientId":"not-kiro"}"#
    );
    assert_eq!(
        reconcile_cli_account("kiro").await.unwrap(),
        Some(CliAccountState::LinkedTo {
            subscription_id: "kiro-ada".into()
        })
    );

    let bob = activate_subscription("kiro-bob").await.unwrap();
    assert!(bob.switch_result.success, "{:?}", bob.switch_result.error);
    assert_eq!(read_json(&auth_path())["accessToken"], "access-bob");
    assert_eq!(read_json(&auth_path())["provider"], "Enterprise");
    assert_eq!(read_json(&auth_path())["region"], "eu-central-1");
    assert!(registration_path(ENTERPRISE_URL).is_file());
    assert_eq!(
        read_json(&registration_path(ENTERPRISE_URL))["clientId"],
        "bob-client"
    );
    assert_eq!(read_json(&ada_registration)["clientSecret"], "ada-secret");
    assert_eq!(item(&db, "unrelated").as_deref(), Some("keep"));
    assert_eq!(
        fs::read_to_string(cache_dir().join("unrelated.json")).unwrap(),
        r#"{"accessToken":"aws-cli-other","clientId":"not-kiro"}"#
    );
    assert_eq!(pinned().as_deref(), Some("kiro-bob"));
    assert_eq!(
        reconcile_cli_account("kiro").await.unwrap(),
        Some(CliAccountState::LinkedTo {
            subscription_id: "kiro-bob".into()
        })
    );
}

#[tokio::test(flavor = "current_thread")]
async fn portal_switch_does_not_write_a_registration_file() {
    let _sb = sandbox().await;
    seed_cache_decoy();
    let decoy = cache_dir().join("cc18142e2bfa693e309f59d910dcef90c3c47767.json");
    fs::write(
        &decoy,
        r#"{"clientId":"leave-me","clientSecret":"leave-secret"}"#,
    )
    .unwrap();
    assert!(!db_path().exists());
    save_portal(
        "kiro-portal",
        "pat@kiro.dev",
        "pat-user",
        "access-pat",
        "refresh-pat",
    );

    let switched = activate_subscription("kiro-portal").await.unwrap();
    assert!(
        switched.switch_result.success,
        "{:?}",
        switched.switch_result.error
    );
    let token = read_json(&auth_path());
    assert_eq!(token["accessToken"], "access-pat");
    assert_eq!(token["provider"], "Github");
    assert_eq!(token["authMethod"], "social");
    assert!(token.get("clientIdHash").is_none());
    assert!(token.get("clientSecret").is_none());
    assert_eq!(
        fs::read_to_string(&decoy).unwrap(),
        r#"{"clientId":"leave-me","clientSecret":"leave-secret"}"#
    );
    assert_eq!(
        fs::read_to_string(cache_dir().join("unrelated.json")).unwrap(),
        r#"{"accessToken":"aws-cli-other","clientId":"not-kiro"}"#
    );
    assert!(!db_path().exists(), "missing state.vscdb is not created");
    assert_eq!(read_json(&profile_path())["email"], "pat@kiro.dev");
}

#[tokio::test(flavor = "current_thread")]
async fn reconcile_is_diverged_when_the_live_token_changes_under_the_pin() {
    let _sb = sandbox().await;
    seed_cache_decoy();
    seed_db();
    save_idc(
        "kiro-ada",
        "ada@kiro.dev",
        "ada-user",
        "access-ada",
        "refresh-ada",
        "ada-client",
        "ada-secret",
        BUILDER_ID_START_URL,
        "us-east-1",
        "arn:aws:codewhisperer:us-east-1:1:profile/ada",
    );
    activate_subscription("kiro-ada").await.unwrap();
    let mut token = read_json(&auth_path());
    token["accessToken"] = json!("access-someone-else");
    token["refreshToken"] = json!("refresh-someone-else");
    token["userId"] = json!("other-user");
    token["user_id"] = json!("other-user");
    token["profileArn"] = json!("arn:aws:codewhisperer:us-east-1:1:profile/other");
    fs::write(auth_path(), serde_json::to_string_pretty(&token).unwrap()).unwrap();
    let profile = profile_path();
    let mut profile_json = read_json(&profile);
    profile_json["userId"] = json!("other-user");
    profile_json["arn"] = json!("arn:aws:codewhisperer:us-east-1:1:profile/other");
    fs::write(
        &profile,
        serde_json::to_string_pretty(&profile_json).unwrap(),
    )
    .unwrap();

    assert_eq!(
        reconcile_cli_account("kiro").await.unwrap(),
        Some(CliAccountState::Diverged)
    );
    assert_eq!(pinned().as_deref(), Some("kiro-ada"));
    assert_eq!(
        fs::read_to_string(cache_dir().join("unrelated.json")).unwrap(),
        r#"{"accessToken":"aws-cli-other","clientId":"not-kiro"}"#
    );
}

#[tokio::test(flavor = "current_thread")]
async fn reconcile_absorbs_a_rotated_token_when_the_user_id_still_matches() {
    let _sb = sandbox().await;
    seed_db();
    save_idc(
        "kiro-ada",
        "ada@kiro.dev",
        "ada-user",
        "access-ada",
        "refresh-ada",
        "ada-client",
        "ada-secret",
        BUILDER_ID_START_URL,
        "us-east-1",
        "arn:aws:codewhisperer:us-east-1:1:profile/ada",
    );
    activate_subscription("kiro-ada").await.unwrap();
    let mut token = read_json(&auth_path());
    token["accessToken"] = json!("access-rotated");
    token["refreshToken"] = json!("refresh-rotated");
    fs::write(auth_path(), serde_json::to_string_pretty(&token).unwrap()).unwrap();

    assert_eq!(
        reconcile_cli_account("kiro").await.unwrap(),
        Some(CliAccountState::LinkedTo {
            subscription_id: "kiro-ada".into()
        })
    );
    let row = storage::get_subscription("kiro-ada").unwrap();
    assert_eq!(
        decrypted(row.access_token_encrypted.as_deref()),
        "access-rotated"
    );
    assert_eq!(
        decrypted(row.refresh_token_encrypted.as_deref()),
        "refresh-rotated"
    );
    let state: Value =
        serde_json::from_str(&decrypted(row.provider_state_encrypted.as_deref())).unwrap();
    assert_eq!(state["clientSecret"], "ada-secret");
    assert_eq!(pinned().as_deref(), Some("kiro-ada"));
}

#[tokio::test(flavor = "current_thread")]
async fn adopt_ignores_a_live_session_that_belongs_to_someone_else() {
    let _sb = sandbox().await;
    seed_db();
    save_idc(
        "kiro-ada",
        "ada@kiro.dev",
        "ada-user",
        "access-ada",
        "refresh-ada",
        "ada-client",
        "ada-secret",
        BUILDER_ID_START_URL,
        "us-east-1",
        "arn:aws:codewhisperer:us-east-1:1:profile/ada",
    );
    save_idc(
        "kiro-bob",
        "bob@kiro.dev",
        "bob-user",
        "access-bob",
        "refresh-bob",
        "bob-client",
        "bob-secret",
        ENTERPRISE_URL,
        "eu-central-1",
        "arn:aws:codewhisperer:eu-central-1:1:profile/bob",
    );
    activate_subscription("kiro-ada").await.unwrap();
    activate_subscription("kiro-bob").await.unwrap();
    storage::set_active_subscription("kiro", "kiro-ada").unwrap();

    let lease = acquire_cli_refresh_lease("kiro").await.unwrap();
    let mut row = storage::get_subscription("kiro-ada").unwrap();
    adopt_active_cli_session_before_refresh(&mut row, &lease).unwrap();
    assert_eq!(
        decrypted(row.access_token_encrypted.as_deref()),
        "access-ada"
    );
    assert_eq!(read_json(&auth_path())["accessToken"], "access-bob");
}

#[tokio::test(flavor = "current_thread")]
async fn forget_removes_only_the_matching_kiro_files() {
    let sb = sandbox().await;
    seed_cache_decoy();
    seed_db();
    let evil = sb.home.path().join("evil.json");
    fs::write(&evil, "do-not-delete").unwrap();
    save_idc(
        "kiro-ada",
        "ada@kiro.dev",
        "ada-user",
        "access-ada",
        "refresh-ada",
        "ada-client",
        "ada-secret",
        BUILDER_ID_START_URL,
        "us-east-1",
        "arn:aws:codewhisperer:us-east-1:1:profile/ada",
    );
    save_idc(
        "kiro-bob",
        "bob@kiro.dev",
        "bob-user",
        "access-bob",
        "refresh-bob",
        "bob-client",
        "bob-secret",
        ENTERPRISE_URL,
        "eu-central-1",
        "arn:aws:codewhisperer:eu-central-1:1:profile/bob",
    );
    activate_subscription("kiro-ada").await.unwrap();
    activate_subscription("kiro-bob").await.unwrap();

    forget_subscription_session("kiro", "kiro-ada").unwrap();
    assert_eq!(read_json(&auth_path())["accessToken"], "access-bob");
    assert!(registration_path(BUILDER_ID_START_URL).is_file());
    assert!(registration_path(ENTERPRISE_URL).is_file());

    let mut token = read_json(&auth_path());
    token["clientIdHash"] = json!("../evil");
    fs::write(auth_path(), serde_json::to_string_pretty(&token).unwrap()).unwrap();
    forget_subscription_session("kiro", "kiro-bob").unwrap();

    assert!(!auth_path().exists());
    assert!(!registration_path(ENTERPRISE_URL).exists());
    assert!(registration_path(BUILDER_ID_START_URL).is_file());
    assert_eq!(
        fs::read_to_string(cache_dir().join("unrelated.json")).unwrap(),
        r#"{"accessToken":"aws-cli-other","clientId":"not-kiro"}"#
    );
    assert_eq!(fs::read_to_string(&evil).unwrap(), "do-not-delete");
    assert!(db_path().is_file());
    assert!(item(&db_path(), USAGE_DB_KEY).is_none());
    assert_eq!(item(&db_path(), "unrelated").as_deref(), Some("keep"));
    assert!(!profile_path().exists());
    assert_eq!(
        reconcile_cli_account("kiro").await.unwrap(),
        Some(CliAccountState::Missing)
    );
}

#[tokio::test(flavor = "current_thread")]
async fn missing_credentials_do_not_move_the_pin_or_create_files() {
    let _sb = sandbox().await;
    save_idc(
        "kiro-ada",
        "ada@kiro.dev",
        "ada-user",
        "access-ada",
        "refresh-ada",
        "ada-client",
        "ada-secret",
        BUILDER_ID_START_URL,
        "us-east-1",
        "arn:aws:codewhisperer:us-east-1:1:profile/ada",
    );
    let mut empty = empty_row("kiro-empty");
    empty.provider_state_encrypted = Some(crypto::encrypt(
        &json!({
            "clientId": "ada-client",
            "clientSecret": "ada-secret",
            "startUrl": BUILDER_ID_START_URL,
        })
        .to_string(),
    ));
    storage::upsert_subscription(empty).unwrap();
    storage::set_active_subscription("kiro", "kiro-ada").unwrap();

    let failed = activate_subscription("kiro-empty").await.unwrap();
    assert!(!failed.switch_result.success);
    assert!(failed.switch_result.error.unwrap().contains("access_token"));
    assert!(!auth_path().exists());
    assert!(!registration_path(BUILDER_ID_START_URL).exists());
    assert_eq!(pinned().as_deref(), Some("kiro-ada"));
}

#[tokio::test(flavor = "current_thread")]
async fn readback_failure_restores_the_backup_and_skips_the_pin() {
    let _sb = sandbox().await;
    seed_cache_decoy();
    let db = seed_db();
    let conn = Connection::open(&db).unwrap();
    conn.execute(
        "INSERT INTO ItemTable (key, value) VALUES (?1, ?2)",
        (USAGE_DB_KEY, r#"{"userInfo":{"userId":"old-user"}}"#),
    )
    .unwrap();
    fs::create_dir_all(auth_path().parent().unwrap()).unwrap();
    fs::write(
        auth_path(),
        r#"{"accessToken":"old-access","refreshToken":"old-refresh"}"#,
    )
    .unwrap();
    save_idc(
        "kiro-ada",
        "ada@kiro.dev",
        "ada-user",
        "access-ada",
        "refresh-ada",
        "ada-client",
        "ada-secret",
        BUILDER_ID_START_URL,
        "us-east-1",
        "arn:aws:codewhisperer:us-east-1:1:profile/ada",
    );
    save_idc(
        "kiro-bob",
        "bob@kiro.dev",
        "bob-user",
        "access-bob",
        "refresh-bob",
        "bob-client",
        "bob-secret",
        ENTERPRISE_URL,
        "eu-central-1",
        "arn:aws:codewhisperer:eu-central-1:1:profile/bob",
    );
    storage::set_active_subscription("kiro", "kiro-ada").unwrap();
    let _fail = EnvVarGuard::set(READBACK_FAIL_ENV, "1");

    let failed = activate_subscription("kiro-bob").await.unwrap();
    assert!(!failed.switch_result.success);
    assert!(failed.switch_result.error.unwrap().contains("回读校验失败"));
    assert_eq!(read_json(&auth_path())["accessToken"], "old-access");
    assert_eq!(read_json(&auth_path())["refreshToken"], "old-refresh");
    assert!(!registration_path(ENTERPRISE_URL).exists());
    assert_eq!(
        item(&db, USAGE_DB_KEY).as_deref(),
        Some(r#"{"userInfo":{"userId":"old-user"}}"#)
    );
    assert_eq!(item(&db, "unrelated").as_deref(), Some("keep"));
    assert_eq!(
        fs::read_to_string(cache_dir().join("unrelated.json")).unwrap(),
        r#"{"accessToken":"aws-cli-other","clientId":"not-kiro"}"#
    );
    assert!(
        fs::read_dir(cache_dir())
            .unwrap()
            .filter_map(Result::ok)
            .any(|entry| entry.file_name().to_string_lossy().contains(".bak.")),
        "backup is taken before the write"
    );
    assert_eq!(pinned().as_deref(), Some("kiro-ada"));
}

#[tokio::test(flavor = "current_thread")]
async fn sync_projects_a_refreshed_subscription_without_moving_the_pin() {
    let _sb = sandbox().await;
    seed_db();
    save_idc(
        "kiro-ada",
        "ada@kiro.dev",
        "ada-user",
        "access-ada",
        "refresh-ada",
        "ada-client",
        "ada-secret",
        BUILDER_ID_START_URL,
        "us-east-1",
        "arn:aws:codewhisperer:us-east-1:1:profile/ada",
    );
    save_idc(
        "kiro-bob",
        "bob@kiro.dev",
        "bob-user",
        "access-bob",
        "refresh-bob",
        "bob-client",
        "bob-secret",
        ENTERPRISE_URL,
        "eu-central-1",
        "arn:aws:codewhisperer:eu-central-1:1:profile/bob",
    );
    activate_subscription("kiro-ada").await.unwrap();

    let lease = acquire_cli_refresh_lease("kiro").await.unwrap();
    let mut inactive = storage::get_subscription("kiro-bob").unwrap();
    inactive.access_token_encrypted = Some(crypto::encrypt("access-bob-rotated"));
    assert!(
        sync_refreshed_active_subscription(&mut inactive, &lease)
            .unwrap()
            .is_none()
    );
    assert_eq!(read_json(&auth_path())["accessToken"], "access-ada");

    let mut row = storage::get_subscription("kiro-ada").unwrap();
    row.access_token_encrypted = Some(crypto::encrypt("access-rotated"));
    row.refresh_token_encrypted = Some(crypto::encrypt("refresh-rotated"));
    let mut row = storage::patch_oauth_credentials(&row).unwrap();
    let outcome = sync_refreshed_active_subscription(&mut row, &lease)
        .unwrap()
        .expect("active kiro row is projected");
    assert!(outcome.success, "{:?}", outcome.error);
    assert_eq!(read_json(&auth_path())["accessToken"], "access-rotated");
    assert_eq!(read_json(&auth_path())["refreshToken"], "refresh-rotated");
    assert!(read_json(&auth_path()).get("clientSecret").is_none());
    assert_eq!(
        read_json(&registration_path(BUILDER_ID_START_URL))["clientSecret"],
        "ada-secret"
    );
    assert_eq!(pinned().as_deref(), Some("kiro-ada"));
    assert_eq!(
        reconcile_cli_account("kiro").await.unwrap(),
        Some(CliAccountState::LinkedTo {
            subscription_id: "kiro-ada".into()
        })
    );
}
