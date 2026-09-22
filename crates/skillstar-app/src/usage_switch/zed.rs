//! Zed account switching through the macOS login keychain.
//!
//! The live credential is an internet-password: server `https://zed.dev`,
//! account = the subscription's `oauth_account_id` (Zed user id), secret = the
//! access token. Activate runs `security add-internet-password -U` and pins
//! only after `find-internet-password` reads that same pair back. A failed
//! write or read-back does not move the pin.
//!
//! Every command carries `-s https://zed.dev`. Forget deletes that user id's
//! item only — not every account on the server, and not any other server.
//! `delete_internet_password` in `keychain_cli` loops until the server is
//! empty; this adapter does not call it.
//!
//! `available()` is false off macOS and while `SKILLSTAR_TOOL_SYNC_HOME` is
//! set. Reconcile then returns `None` instead of a fabricated link. The
//! command runner is the test seam: production spawns `/usr/bin/security`
//! only after the sandbox check, and tests never do. A live keychain write
//! accepted by Zed is not verified here. Zed stays out of `DesktopAppId`.

use std::path::Path;

use skillstar_usage::crypto;
use skillstar_usage::subscription::Subscription;
use skillstar_usage::{UsageError, UsageResult, storage, tool_paths};

use super::ide::IdeCredentialAdapter;
use super::{CliAccountState, SwitchOutcome};

pub(super) const CATALOG_ID: &str = "zed";

const SERVER: &str = "https://zed.dev";
const SANDBOX_ERROR: &str =
    "SKILLSTAR_TOOL_SYNC_HOME 已设置，拒绝访问 macOS internet-password keychain";
const MACOS_ONLY: &str = "internet-password keychain 仅支持 macOS";
const FOREIGN_SERVER: &str = "Zed 切号只允许改 https://zed.dev 的 internet-password";

pub(super) struct Adapter;

impl IdeCredentialAdapter for Adapter {
    fn catalog_id(&self) -> &'static str {
        CATALOG_ID
    }

    fn available(&self) -> bool {
        keychain_available(
            cfg!(target_os = "macos"),
            tool_paths::is_tool_sync_sandboxed(),
        )
    }

    fn activate(&self, sub_id: &str) -> UsageResult<(Subscription, SwitchOutcome)> {
        if !self.available() {
            let subscription = storage::get_subscription(sub_id)?;
            return Ok((subscription, failed(unavailable_reason())));
        }
        activate_with(sub_id, &SecurityCli)
    }

    fn sync(&self, sub: &Subscription) -> UsageResult<SwitchOutcome> {
        if !self.available() {
            return Ok(failed(unavailable_reason()));
        }
        sync_with(sub, &SecurityCli)
    }

    fn reconcile(&self) -> UsageResult<Option<CliAccountState>> {
        if !self.available() {
            return Ok(None);
        }
        reconcile_with(&SecurityCli).map(Some)
    }

    fn adopt_before_refresh(&self, sub: &mut Subscription) -> UsageResult<()> {
        if !self.available() {
            return Ok(());
        }
        adopt_with(sub, &SecurityCli)
    }

    fn forget(&self, sub_id: &str) -> UsageResult<()> {
        if !self.available() {
            return Ok(());
        }
        forget_with(sub_id, &SecurityCli)
    }
}

struct InternetPassword {
    account: String,
    password: String,
}

#[derive(Debug)]
struct CliOutput {
    success: bool,
    stdout: String,
    stderr: String,
}

trait CommandRunner {
    fn run(&self, args: &[String]) -> UsageResult<CliOutput>;
}

struct SecurityCli;

impl CommandRunner for SecurityCli {
    fn run(&self, args: &[String]) -> UsageResult<CliOutput> {
        run_security(args)
    }
}

fn keychain_available(macos: bool, sandboxed: bool) -> bool {
    macos && !sandboxed
}

fn unavailable_reason() -> &'static str {
    if tool_paths::is_tool_sync_sandboxed() {
        SANDBOX_ERROR
    } else {
        MACOS_ONLY
    }
}

fn config_path() -> &'static Path {
    Path::new(SERVER)
}

fn failed(reason: impl Into<String>) -> SwitchOutcome {
    SwitchOutcome::fail(CATALOG_ID, config_path(), reason)
}

fn succeeded() -> SwitchOutcome {
    SwitchOutcome::direct_ok(CATALOG_ID, config_path(), true)
}

fn activate_with(
    subscription_id: &str,
    cli: &dyn CommandRunner,
) -> UsageResult<(Subscription, SwitchOutcome)> {
    let subscription = storage::get_subscription(subscription_id)?;
    match write_verified(&subscription, cli) {
        Ok(outcome) => {
            storage::set_active_subscription(&subscription.catalog_id, &subscription.id)?;
            Ok((subscription, outcome))
        }
        Err(error) => Ok((subscription, failed(error.to_string()))),
    }
}

fn sync_with(subscription: &Subscription, cli: &dyn CommandRunner) -> UsageResult<SwitchOutcome> {
    write_verified(subscription, cli).or_else(|error| Ok(failed(error.to_string())))
}

fn reconcile_with(cli: &dyn CommandRunner) -> UsageResult<CliAccountState> {
    let Some(item) = read_current(cli)? else {
        return Ok(CliAccountState::Missing);
    };
    let Some(subscription) = pinned_row()? else {
        return Ok(CliAccountState::Diverged);
    };
    if item_matches(&subscription, &item) {
        Ok(CliAccountState::LinkedTo {
            subscription_id: subscription.id,
        })
    } else {
        Ok(CliAccountState::Diverged)
    }
}

fn adopt_with(subscription: &mut Subscription, cli: &dyn CommandRunner) -> UsageResult<()> {
    if subscription.catalog_id != CATALOG_ID {
        return Ok(());
    }
    let Some(account) = account_of(subscription) else {
        return Ok(());
    };
    let Some(item) = read_account(cli, &account)? else {
        return Ok(());
    };
    if item.account != account {
        return Ok(());
    }
    if token_of(subscription).as_deref() == Some(item.password.as_str()) {
        return Ok(());
    }
    let mut updated = subscription.clone();
    updated.access_token_encrypted = Some(crypto::encrypt(&item.password));
    *subscription = storage::patch_oauth_credentials(&updated)?;
    Ok(())
}

fn forget_with(subscription_id: &str, cli: &dyn CommandRunner) -> UsageResult<()> {
    let subscription = match storage::get_subscription(subscription_id) {
        Ok(subscription) => subscription,
        Err(UsageError::NotFound(_)) => return Ok(()),
        Err(error) => return Err(error),
    };
    if subscription.catalog_id != CATALOG_ID {
        return Ok(());
    }
    let Some(account) = account_of(&subscription) else {
        return Ok(());
    };
    delete_account(cli, &account)
}

fn write_verified(
    subscription: &Subscription,
    cli: &dyn CommandRunner,
) -> UsageResult<SwitchOutcome> {
    if subscription.catalog_id != CATALOG_ID {
        return Err(UsageError::Other(
            "Zed 切换收到了其它 catalog 的订阅".into(),
        ));
    }
    let account = account_of(subscription)
        .ok_or_else(|| UsageError::Other("Zed 账号缺少 user_id，切换未生效".into()))?;
    let token = token_of(subscription)
        .ok_or_else(|| UsageError::Other("Zed 账号缺少 access_token，切换未生效".into()))?;
    add_account(cli, &account, &token)?;
    match read_account(cli, &account)? {
        Some(item) if item.account == account && item.password == token => Ok(succeeded()),
        _ => Err(UsageError::Other(
            "Zed 钥匙串回读与写入不一致，切换未生效".into(),
        )),
    }
}

fn pinned_row() -> UsageResult<Option<Subscription>> {
    let Some(id) = storage::get_active_subscription(CATALOG_ID)? else {
        return Ok(None);
    };
    match storage::get_subscription(&id) {
        Ok(subscription) if subscription.catalog_id == CATALOG_ID => Ok(Some(subscription)),
        Ok(_) | Err(UsageError::NotFound(_)) => Ok(None),
        Err(error) => Err(error),
    }
}

fn item_matches(subscription: &Subscription, item: &InternetPassword) -> bool {
    account_of(subscription).as_deref() == Some(item.account.as_str())
        && token_of(subscription).as_deref() == Some(item.password.as_str())
}

fn account_of(subscription: &Subscription) -> Option<String> {
    subscription
        .oauth_account_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn token_of(subscription: &Subscription) -> Option<String> {
    subscription
        .access_token_encrypted
        .as_deref()
        .map(crypto::decrypt)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn add_account(cli: &dyn CommandRunner, account: &str, token: &str) -> UsageResult<()> {
    let output = invoke(cli, add_args(account, token))?;
    if output.success {
        return Ok(());
    }
    Err(command_error("写入 internet-password 失败", &output))
}

fn delete_account(cli: &dyn CommandRunner, account: &str) -> UsageResult<()> {
    let output = invoke(cli, delete_args(account))?;
    if output.success || not_found(&output.stderr) {
        return Ok(());
    }
    Err(command_error("删除 internet-password 失败", &output))
}

fn read_current(cli: &dyn CommandRunner) -> UsageResult<Option<InternetPassword>> {
    let meta = invoke(cli, find_account_args())?;
    if !meta.success {
        return if not_found(&meta.stderr) {
            Ok(None)
        } else {
            Err(command_error("读取 internet-password 失败", &meta))
        };
    }
    let account = parse_account(&format!("{}\n{}", meta.stdout, meta.stderr))
        .ok_or_else(|| UsageError::Other("解析 internet-password 账号失败".into()))?;
    read_account(cli, &account)
}

fn read_account(cli: &dyn CommandRunner, account: &str) -> UsageResult<Option<InternetPassword>> {
    let meta = invoke(cli, find_args(account, false))?;
    if !meta.success {
        return if not_found(&meta.stderr) {
            Ok(None)
        } else {
            Err(command_error("读取 internet-password 失败", &meta))
        };
    }
    let parsed = parse_account(&format!("{}\n{}", meta.stdout, meta.stderr))
        .ok_or_else(|| UsageError::Other("解析 internet-password 账号失败".into()))?;
    let password_output = invoke(cli, find_args(account, true))?;
    if !password_output.success {
        return Err(command_error(
            "读取 internet-password 密码失败",
            &password_output,
        ));
    }
    let password = password_output.stdout.trim().to_string();
    if password.is_empty() {
        return Err(UsageError::Other("internet-password 密码为空".into()));
    }
    Ok(Some(InternetPassword {
        account: parsed,
        password,
    }))
}

fn invoke(cli: &dyn CommandRunner, args: Vec<String>) -> UsageResult<CliOutput> {
    require_zed_server(&args)?;
    cli.run(&args)
}

fn add_args(account: &str, password: &str) -> Vec<String> {
    vec![
        "add-internet-password".into(),
        "-U".into(),
        "-a".into(),
        account.into(),
        "-s".into(),
        SERVER.into(),
        "-w".into(),
        password.into(),
    ]
}

fn find_args(account: &str, password_only: bool) -> Vec<String> {
    let mut args = vec![
        "find-internet-password".into(),
        "-a".into(),
        account.into(),
        "-s".into(),
        SERVER.into(),
    ];
    if password_only {
        args.push("-w".into());
    }
    args
}

fn find_account_args() -> Vec<String> {
    vec!["find-internet-password".into(), "-s".into(), SERVER.into()]
}

fn delete_args(account: &str) -> Vec<String> {
    vec![
        "delete-internet-password".into(),
        "-a".into(),
        account.into(),
        "-s".into(),
        SERVER.into(),
    ]
}

fn require_zed_server(args: &[String]) -> UsageResult<()> {
    let mut servers = 0;
    let mut index = 0;
    while index < args.len() {
        if args[index] == "-s" {
            let value = args.get(index + 1).map(String::as_str);
            if value != Some(SERVER) {
                return Err(UsageError::Other(FOREIGN_SERVER.into()));
            }
            servers += 1;
            index += 2;
            continue;
        }
        index += 1;
    }
    if servers == 1 {
        Ok(())
    } else {
        Err(UsageError::Other(FOREIGN_SERVER.into()))
    }
}

fn parse_account(text: &str) -> Option<String> {
    for line in text.lines() {
        let Some(rest) = line.split("\"acct\"<blob>=\"").nth(1) else {
            continue;
        };
        let Some(value) = rest.split('"').next() else {
            continue;
        };
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    None
}

fn not_found(stderr: &str) -> bool {
    stderr.contains("could not be found")
}

fn command_error(action: &str, output: &CliOutput) -> UsageError {
    let stderr = output.stderr.trim();
    if stderr.is_empty() {
        UsageError::Other(action.into())
    } else {
        UsageError::Other(format!("{action}：{stderr}"))
    }
}

fn ensure_keychain_allowed() -> UsageResult<()> {
    if tool_paths::is_tool_sync_sandboxed() {
        return Err(UsageError::Other(SANDBOX_ERROR.into()));
    }
    if !cfg!(target_os = "macos") {
        return Err(UsageError::Other(MACOS_ONLY.into()));
    }
    Ok(())
}

fn run_security(args: &[String]) -> UsageResult<CliOutput> {
    ensure_keychain_allowed()?;
    spawn_security(args)
}

fn spawn_security(args: &[String]) -> UsageResult<CliOutput> {
    ensure_keychain_allowed()?;
    let output = std::process::Command::new("/usr/bin/security")
        .args(args)
        .output()
        .map_err(|error| UsageError::Other(format!("执行 security 失败：{error}")))?;
    Ok(CliOutput {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

#[cfg(test)]
#[path = "zed_tests.rs"]
mod tests;
