use std::collections::BTreeMap;
use std::sync::Mutex;

use skillstar_usage::subscription::Subscription;
use skillstar_usage::{crypto, storage};

use super::{
    CATALOG_ID, CommandRunner, SANDBOX_ERROR, SecurityCli, activate_with, add_args, adopt_with,
    delete_args, find_account_args, find_args, forget_with, keychain_available, parse_account,
    reconcile_with, require_zed_server, sync_with,
};
use crate::test_support::{ENV_LOCK, EnvGuard};
use crate::usage_switch::CliAccountState;
use crate::usage_switch::ide::IdeCredentialAdapter;

const SERVER: &str = "https://zed.dev";

struct FakeState {
    items: BTreeMap<String, String>,
    current: Option<String>,
    calls: Vec<Vec<String>>,
    fail_add: bool,
    readback: Option<String>,
}

struct FakeCli {
    state: Mutex<FakeState>,
}

impl FakeCli {
    fn new() -> Self {
        Self {
            state: Mutex::new(FakeState {
                items: BTreeMap::new(),
                current: None,
                calls: Vec::new(),
                fail_add: false,
                readback: None,
            }),
        }
    }

    fn account(self, account: &str, password: &str) -> Self {
        let mut state = self.lock();
        let make_current = state.current.is_none();
        state
            .items
            .insert(account.to_string(), password.to_string());
        if make_current {
            state.current = Some(account.to_string());
        }
        drop(state);
        self
    }

    fn fail_add(self) -> Self {
        self.lock().fail_add = true;
        self
    }

    fn readback(self, password: &str) -> Self {
        self.lock().readback = Some(password.to_string());
        self
    }

    fn calls(&self) -> Vec<Vec<String>> {
        self.lock().calls.clone()
    }

    fn password(&self, account: &str) -> Option<String> {
        self.lock().items.get(account).cloned()
    }

    fn accounts(&self) -> Vec<String> {
        self.lock().items.keys().cloned().collect()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, FakeState> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }
}

impl CommandRunner for FakeCli {
    fn run(&self, args: &[String]) -> skillstar_usage::UsageResult<super::CliOutput> {
        let mut state = self.lock();
        state.calls.push(args.to_vec());
        let command = args.first().map(String::as_str).unwrap_or("");
        match command {
            "add-internet-password" => {
                if state.fail_add {
                    return Ok(denied("write denied"));
                }
                let account = flag(args, "-a").unwrap_or("").to_string();
                let password = flag(args, "-w").unwrap_or("").to_string();
                state.items.insert(account.clone(), password);
                state.current = Some(account);
                Ok(ok(""))
            }
            "find-internet-password" => {
                let account = match flag(args, "-a") {
                    Some(account) => account.to_string(),
                    None => match state.current.clone() {
                        Some(account) => account,
                        None => return Ok(missing()),
                    },
                };
                let Some(password) = state.items.get(&account).cloned() else {
                    return Ok(missing());
                };
                if args.iter().any(|arg| arg == "-w") {
                    let shown = state.readback.clone().unwrap_or(password);
                    return Ok(ok(&format!("{shown}\n")));
                }
                Ok(ok(&metadata(&account)))
            }
            "delete-internet-password" => {
                let account = flag(args, "-a").unwrap_or("");
                if state.items.remove(account).is_none() {
                    return Ok(missing());
                }
                if state.current.as_deref() == Some(account) {
                    state.current = state.items.keys().next().cloned();
                }
                Ok(ok(""))
            }
            other => Ok(denied(&format!("unexpected command {other}"))),
        }
    }
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|arg| arg == name)
        .and_then(|index| args.get(index + 1))
        .map(String::as_str)
}

fn ok(stdout: &str) -> super::CliOutput {
    super::CliOutput {
        success: true,
        stdout: stdout.to_string(),
        stderr: String::new(),
    }
}

fn missing() -> super::CliOutput {
    super::CliOutput {
        success: false,
        stdout: String::new(),
        stderr: "security: The specified item could not be found in the keychain.\n".into(),
    }
}

fn denied(stderr: &str) -> super::CliOutput {
    super::CliOutput {
        success: false,
        stdout: String::new(),
        stderr: stderr.to_string(),
    }
}

fn metadata(account: &str) -> String {
    format!(
        "keychain: \"test.keychain\"\nclass: \"inet\"\n    \"acct\"<blob>=\"{account}\"\n    \"svce\"<blob>=\"{SERVER}\"\n"
    )
}

fn only_zed_server(calls: &[Vec<String>]) {
    for call in calls {
        let server = flag(call, "-s").unwrap_or("");
        assert_eq!(server, SERVER, "{call:?}");
        assert!(
            call.iter()
                .all(|arg| !arg.contains("anthropic") && !arg.contains("claude")),
            "{call:?}"
        );
    }
}

struct Sandbox {
    _env: EnvGuard,
    _data: tempfile::TempDir,
    _lock: tokio::sync::MutexGuard<'static, ()>,
}

async fn sandbox() -> Sandbox {
    let lock = ENV_LOCK.lock().await;
    let data = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let env = EnvGuard::set(&[
        ("SKILLSTAR_DATA_DIR", data.path()),
        ("SKILLSTAR_TOOL_SYNC_HOME", home.path()),
    ]);
    Sandbox {
        _env: env,
        _data: data,
        _lock: lock,
    }
}

fn save(id: &str, user_id: &str, token: &str) {
    let mut row = empty_row(id);
    row.oauth_account_id = Some(user_id.into());
    row.access_token_encrypted = Some(crypto::encrypt(token));
    storage::upsert_subscription(row).unwrap();
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

fn pin() -> Option<String> {
    storage::get_active_subscription(CATALOG_ID).unwrap()
}

#[test]
fn availability_requires_macos_without_a_sandbox() {
    assert!(!keychain_available(false, false));
    assert!(!keychain_available(false, true));
    assert!(!keychain_available(true, true));
    assert!(keychain_available(true, false));
}

#[test]
fn argv_addresses_only_the_zed_server() {
    assert_eq!(
        add_args("user-1", "tok"),
        [
            "add-internet-password",
            "-U",
            "-a",
            "user-1",
            "-s",
            SERVER,
            "-w",
            "tok",
        ]
    );
    assert_eq!(
        find_args("user-1", false),
        ["find-internet-password", "-a", "user-1", "-s", SERVER]
    );
    assert_eq!(
        find_args("user-1", true),
        ["find-internet-password", "-a", "user-1", "-s", SERVER, "-w"]
    );
    assert_eq!(
        find_account_args(),
        ["find-internet-password", "-s", SERVER]
    );
    assert_eq!(
        delete_args("user-1"),
        ["delete-internet-password", "-a", "user-1", "-s", SERVER]
    );
    assert!(require_zed_server(&add_args("user-1", "tok")).is_ok());
    let foreign = vec![
        "delete-internet-password".into(),
        "-s".into(),
        "https://api.anthropic.com".into(),
    ];
    assert!(require_zed_server(&foreign).is_err());
    assert!(require_zed_server(&["add-internet-password".into()]).is_err());
}

#[test]
fn parses_the_account_blob_without_spawning_security() {
    let text = "\
keychain: \"/Library/Keychains/login.keychain-db\"
class: \"inet\"
    \"acct\"<blob>=\"user-123\"
    \"svce\"<blob>=\"https://zed.dev\"
";
    assert_eq!(parse_account(text).as_deref(), Some("user-123"));
    assert_eq!(parse_account("no account line"), None);
    assert_eq!(parse_account("\"acct\"<blob>=\"   \""), None);
}

#[test]
fn zed_is_switchable_and_not_an_instance() {
    assert!(crate::usage_switch::supports_switch(CATALOG_ID));
    assert!(crate::instances::DesktopAppId::parse("zed").is_err());
}

#[tokio::test(flavor = "current_thread")]
async fn security_cli_refuses_in_the_sandbox_before_spawn() {
    let _sb = sandbox().await;
    let error = SecurityCli
        .run(&add_args("user-1", "secret"))
        .expect_err("sandbox");
    assert_eq!(error.to_string(), SANDBOX_ERROR);
}

#[tokio::test(flavor = "current_thread")]
async fn sandboxed_adapter_omits_reconcile_and_does_not_move_the_pin() {
    let _sb = sandbox().await;
    save("zed-a", "user-a", "token-a");
    save("zed-b", "user-b", "token-b");
    storage::set_active_subscription(CATALOG_ID, "zed-a").unwrap();
    assert!(!super::Adapter.available());
    assert!(super::Adapter.reconcile().unwrap().is_none());
    let (subscription, outcome) = super::Adapter.activate("zed-b").unwrap();
    assert_eq!(subscription.id, "zed-b");
    assert!(!outcome.success);
    assert!(!outcome.keychain_updated);
    assert_eq!(outcome.error.as_deref(), Some(SANDBOX_ERROR));
    assert_eq!(pin().as_deref(), Some("zed-a"));
    super::Adapter.forget("zed-a").unwrap();
    assert_eq!(pin().as_deref(), Some("zed-a"));
}

#[tokio::test(flavor = "current_thread")]
async fn unsandboxed_availability_is_macos_only() {
    let _lock = ENV_LOCK.lock().await;
    let previous = std::env::var_os("SKILLSTAR_TOOL_SYNC_HOME");
    unsafe { std::env::remove_var("SKILLSTAR_TOOL_SYNC_HOME") };
    struct Restore(Option<std::ffi::OsString>);
    impl Drop for Restore {
        fn drop(&mut self) {
            unsafe {
                match self.0.take() {
                    Some(value) => std::env::set_var("SKILLSTAR_TOOL_SYNC_HOME", value),
                    None => std::env::remove_var("SKILLSTAR_TOOL_SYNC_HOME"),
                }
            }
        }
    }
    let _restore = Restore(previous);
    assert_eq!(super::Adapter.available(), cfg!(target_os = "macos"));
}

#[tokio::test(flavor = "current_thread")]
async fn activate_pins_only_when_the_readback_matches() {
    let _sb = sandbox().await;
    save("zed-a", "user-a", "token-a");
    save("zed-b", " user-b ", " token-b ");
    storage::set_active_subscription(CATALOG_ID, "zed-a").unwrap();
    let cli = FakeCli::new().account("user-a", "old-a");
    let (_subscription, outcome) = activate_with("zed-b", &cli).unwrap();
    assert!(outcome.success);
    assert!(outcome.keychain_updated);
    assert_eq!(outcome.config_path, SERVER);
    assert_eq!(pin().as_deref(), Some("zed-b"));
    assert_eq!(cli.password("user-a").as_deref(), Some("old-a"));
    assert_eq!(cli.password("user-b").as_deref(), Some("token-b"));
    let calls = cli.calls();
    only_zed_server(&calls);
    assert!(
        calls
            .iter()
            .all(|call| call[0] != "delete-internet-password"),
        "{calls:?}"
    );
    assert_eq!(calls[0][0], "add-internet-password");
    assert!(calls[0].contains(&"-U".to_string()));
    assert_eq!(flag(&calls[0], "-a"), Some("user-b"));
    assert_eq!(flag(&calls[0], "-w"), Some("token-b"));
    assert_eq!(
        reconcile_with(&cli).unwrap(),
        CliAccountState::LinkedTo {
            subscription_id: "zed-b".into(),
        }
    );
}

#[tokio::test(flavor = "current_thread")]
async fn readback_mismatch_and_add_failure_leave_the_pin() {
    let _sb = sandbox().await;
    save("zed-a", "user-a", "token-a");
    save("zed-b", "user-b", "token-b");
    storage::set_active_subscription(CATALOG_ID, "zed-a").unwrap();

    let mismatch = FakeCli::new().readback("not-the-token");
    let (_subscription, outcome) = activate_with("zed-b", &mismatch).unwrap();
    assert!(!outcome.success);
    assert!(!outcome.keychain_updated);
    assert!(
        outcome
            .error
            .as_deref()
            .unwrap()
            .contains("回读与写入不一致")
    );
    assert_eq!(pin().as_deref(), Some("zed-a"));
    assert_eq!(mismatch.password("user-b").as_deref(), Some("token-b"));
    only_zed_server(&mismatch.calls());

    let denied = FakeCli::new().fail_add();
    let (_subscription, outcome) = activate_with("zed-b", &denied).unwrap();
    assert!(!outcome.success);
    assert!(outcome.error.as_deref().unwrap().contains("写入"));
    assert_eq!(pin().as_deref(), Some("zed-a"));
    assert!(denied.password("user-b").is_none());
    assert_eq!(denied.calls().len(), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn missing_credentials_do_not_touch_the_runner_or_the_pin() {
    let _sb = sandbox().await;
    save("zed-a", "user-a", "token-a");
    storage::set_active_subscription(CATALOG_ID, "zed-a").unwrap();
    let mut bare = empty_row("zed-empty");
    bare.oauth_account_id = Some("user-empty".into());
    storage::upsert_subscription(bare).unwrap();
    let cli = FakeCli::new();
    let (_subscription, outcome) = activate_with("zed-empty", &cli).unwrap();
    assert!(!outcome.success);
    assert!(outcome.error.as_deref().unwrap().contains("access_token"));
    assert!(cli.calls().is_empty());
    assert_eq!(pin().as_deref(), Some("zed-a"));

    let mut no_user = empty_row("zed-nouser");
    no_user.access_token_encrypted = Some(crypto::encrypt("token"));
    storage::upsert_subscription(no_user).unwrap();
    let (_subscription, outcome) = activate_with("zed-nouser", &cli).unwrap();
    assert!(outcome.error.as_deref().unwrap().contains("user_id"));
    assert!(cli.calls().is_empty());
    assert_eq!(pin().as_deref(), Some("zed-a"));
}

#[tokio::test(flavor = "current_thread")]
async fn reconcile_is_missing_linked_or_diverged_against_the_pin() {
    let _sb = sandbox().await;
    save("zed-a", "user-a", "token-a");
    save("zed-b", "user-b", "token-b");

    let empty = FakeCli::new();
    assert_eq!(reconcile_with(&empty).unwrap(), CliAccountState::Missing);
    storage::set_active_subscription(CATALOG_ID, "zed-a").unwrap();
    assert_eq!(reconcile_with(&empty).unwrap(), CliAccountState::Missing);

    let linked = FakeCli::new().account("user-a", "token-a");
    assert_eq!(
        reconcile_with(&linked).unwrap(),
        CliAccountState::LinkedTo {
            subscription_id: "zed-a".into(),
        }
    );

    let other = FakeCli::new().account("user-b", "token-b");
    assert_eq!(reconcile_with(&other).unwrap(), CliAccountState::Diverged);

    let rotated = FakeCli::new().account("user-a", "token-rotated");
    assert_eq!(reconcile_with(&rotated).unwrap(), CliAccountState::Diverged);

    storage::clear_active_subscription(CATALOG_ID).unwrap();
    assert_eq!(reconcile_with(&linked).unwrap(), CliAccountState::Diverged);
    only_zed_server(&linked.calls());
}

#[tokio::test(flavor = "current_thread")]
async fn sync_writes_the_refreshed_token_without_moving_the_pin() {
    let _sb = sandbox().await;
    save("zed-a", "user-a", "token-a");
    save("zed-b", "user-b", "token-b");
    storage::set_active_subscription(CATALOG_ID, "zed-a").unwrap();
    save("zed-a", "user-a", "token-a-refreshed");
    let row = storage::get_subscription("zed-a").unwrap();
    let cli = FakeCli::new().account("user-a", "token-a");
    let outcome = sync_with(&row, &cli).unwrap();
    assert!(outcome.success);
    assert!(outcome.keychain_updated);
    assert_eq!(cli.password("user-a").as_deref(), Some("token-a-refreshed"));
    assert_eq!(pin().as_deref(), Some("zed-a"));
    assert!(
        cli.calls()
            .iter()
            .all(|call| call[0] != "delete-internet-password")
    );
    only_zed_server(&cli.calls());
}

#[tokio::test(flavor = "current_thread")]
async fn forget_deletes_that_account_and_leaves_other_items() {
    let _sb = sandbox().await;
    save("zed-a", "user-a", "token-a");
    save("zed-b", "user-b", "token-b");
    storage::set_active_subscription(CATALOG_ID, "zed-b").unwrap();
    let cli = FakeCli::new()
        .account("user-b", "token-b")
        .account("user-a", "token-a");
    forget_with("zed-a", &cli).unwrap();
    assert!(cli.password("user-a").is_none());
    assert_eq!(cli.password("user-b").as_deref(), Some("token-b"));
    assert_eq!(pin().as_deref(), Some("zed-b"));
    let calls = cli.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0],
        vec![
            "delete-internet-password".to_string(),
            "-a".into(),
            "user-a".into(),
            "-s".into(),
            SERVER.into(),
        ]
    );
    forget_with("missing", &cli).unwrap();
    forget_with("zed-a", &cli).unwrap();
    assert_eq!(cli.accounts(), vec!["user-b".to_string()]);
    assert_eq!(cli.calls().len(), 2);
}

#[tokio::test(flavor = "current_thread")]
async fn adopt_takes_the_same_accounts_newer_token_only() {
    let _sb = sandbox().await;
    save("zed-a", "user-a", "token-a");
    let mut row = storage::get_subscription("zed-a").unwrap();
    let other = FakeCli::new().account("user-b", "token-b");
    adopt_with(&mut row, &other).unwrap();
    assert_eq!(
        crypto::decrypt(row.access_token_encrypted.as_deref().unwrap()),
        "token-a"
    );
    assert!(
        other
            .calls()
            .iter()
            .all(|call| flag(call, "-a") == Some("user-a"))
    );

    let same = FakeCli::new().account("user-a", "token-from-zed");
    adopt_with(&mut row, &same).unwrap();
    assert_eq!(
        crypto::decrypt(row.access_token_encrypted.as_deref().unwrap()),
        "token-from-zed"
    );
    assert_eq!(
        crypto::decrypt(
            storage::get_subscription("zed-a")
                .unwrap()
                .access_token_encrypted
                .as_deref()
                .unwrap()
        ),
        "token-from-zed"
    );
    assert!(
        same.calls()
            .iter()
            .all(|call| call[0] == "find-internet-password")
    );
}
