//! `/usr/bin/security` **internet-password** wrapper.
//!
//! This is a different keychain item class from the generic-password helpers
//! used for Claude, Codex, and Antigravity. Do not merge them.
//!
//! When `SKILLSTAR_TOOL_SYNC_HOME` is set, every operation returns a sandbox
//! error and does not spawn `security`. A live keychain round-trip is out of
//! scope for this module.

#![cfg_attr(not(test), allow(dead_code))]

use std::process::Output;

use crate::{UsageError, UsageResult};

const SANDBOX_ERROR: &str =
    "SKILLSTAR_TOOL_SYNC_HOME 已设置，拒绝访问 macOS internet-password keychain";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InternetPassword {
    pub account: String,
    pub password: String,
}

/// Account label from `security find-internet-password -s <server>` with no
/// `-a`. Zed's item is stored under the user id, which the caller does not
/// know yet. Sandbox refuses before `security` is spawned.
pub(crate) fn find_internet_password_account(server: &str) -> UsageResult<Option<String>> {
    ensure_keychain_allowed()?;
    if server.trim().is_empty() {
        return Err(UsageError::Other(
            "internet-password 的 server 不能为空".into(),
        ));
    }
    let meta = spawn_security(&["find-internet-password", "-s", server])?;
    if !meta.status.success() {
        let stderr = String::from_utf8_lossy(&meta.stderr);
        if stderr.contains("could not be found") {
            return Ok(None);
        }
        return Err(UsageError::Other(format!(
            "读取 internet-password 失败：status={} stderr={}",
            meta.status,
            stderr.trim()
        )));
    }
    let meta_text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&meta.stdout),
        String::from_utf8_lossy(&meta.stderr)
    );
    parse_internet_password_account(&meta_text)
        .map(Some)
        .ok_or_else(|| UsageError::Other("解析 internet-password 账号失败".into()))
}

pub(crate) fn find_internet_password(
    server: &str,
    account: &str,
) -> UsageResult<Option<InternetPassword>> {
    ensure_keychain_allowed()?;
    let account = account.trim();
    if server.trim().is_empty() || account.is_empty() {
        return Err(UsageError::Other(
            "internet-password 的 server 和 account 不能为空".into(),
        ));
    }
    let meta = spawn_security(&find_internet_password_argv(account, server, false))?;
    if !meta.status.success() {
        let stderr = String::from_utf8_lossy(&meta.stderr);
        if stderr.contains("could not be found") {
            return Ok(None);
        }
        return Err(UsageError::Other(format!(
            "读取 internet-password 失败：status={} stderr={}",
            meta.status,
            stderr.trim()
        )));
    }

    let password_output = spawn_security(&find_internet_password_argv(account, server, true))?;
    if !password_output.status.success() {
        let stderr = String::from_utf8_lossy(&password_output.stderr);
        return Err(UsageError::Other(format!(
            "读取 internet-password 密码失败：status={} stderr={}",
            password_output.status,
            stderr.trim()
        )));
    }

    let meta_text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&meta.stdout),
        String::from_utf8_lossy(&meta.stderr)
    );
    let account = parse_internet_password_account(&meta_text)
        .ok_or_else(|| UsageError::Other("解析 internet-password 账号失败".into()))?;
    let password = String::from_utf8_lossy(&password_output.stdout)
        .trim()
        .to_string();
    if password.is_empty() {
        return Err(UsageError::Other("internet-password 密码为空".into()));
    }
    Ok(Some(InternetPassword { account, password }))
}

pub(crate) fn add_internet_password(
    server: &str,
    account: &str,
    password: &str,
) -> UsageResult<()> {
    ensure_keychain_allowed()?;
    let account = account.trim();
    let password = password.trim();
    if server.trim().is_empty() || account.is_empty() || password.is_empty() {
        return Err(UsageError::Other(
            "internet-password 的 server、account、password 不能为空".into(),
        ));
    }
    let output = spawn_security(&[
        "add-internet-password",
        "-U",
        "-a",
        account,
        "-s",
        server,
        "-w",
        password,
    ])?;
    if output.status.success() {
        return Ok(());
    }
    Err(UsageError::Other(format!(
        "写入 internet-password 失败：status={} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    )))
}

pub(crate) fn delete_internet_password(server: &str) -> UsageResult<()> {
    ensure_keychain_allowed()?;
    loop {
        let output = spawn_security(&["delete-internet-password", "-s", server])?;
        if output.status.success() {
            continue;
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("could not be found") {
            return Ok(());
        }
        return Err(UsageError::Other(format!(
            "删除 internet-password 失败：status={} stderr={}",
            output.status,
            stderr.trim()
        )));
    }
}

/// Account label from `security find-internet-password` metadata
/// (`"acct"<blob>="..."`), matching cockpit's parser.
pub(crate) fn parse_internet_password_account(text: &str) -> Option<String> {
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

/// `security find-internet-password` argv. `password_only` adds `-w`.
/// Built here so tests can lock the flags without spawning `security`.
pub(crate) fn find_internet_password_argv<'a>(
    account: &'a str,
    server: &'a str,
    password_only: bool,
) -> Vec<&'a str> {
    let mut args = vec!["find-internet-password", "-a", account, "-s", server];
    if password_only {
        args.push("-w");
    }
    args
}

fn ensure_keychain_allowed() -> UsageResult<()> {
    if crate::tool_paths::is_tool_sync_sandboxed() {
        return Err(UsageError::Other(SANDBOX_ERROR.into()));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn spawn_security(args: &[&str]) -> UsageResult<Output> {
    ensure_keychain_allowed()?;
    std::process::Command::new("/usr/bin/security")
        .args(args)
        .output()
        .map_err(|error| UsageError::Other(format!("执行 security 失败：{error}")))
}

#[cfg(not(target_os = "macos"))]
fn spawn_security(args: &[&str]) -> UsageResult<Output> {
    ensure_keychain_allowed()?;
    let _ = args;
    Err(UsageError::Other(
        "internet-password keychain 仅支持 macOS".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EnvGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
        prev: Option<std::ffi::OsString>,
    }

    impl EnvGuard {
        fn sandbox(path: &std::path::Path) -> Self {
            let lock = crate::test_env_lock()
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let prev = std::env::var_os("SKILLSTAR_TOOL_SYNC_HOME");
            // SAFETY: serialized by the crate-wide test_env_lock.
            unsafe {
                std::env::set_var("SKILLSTAR_TOOL_SYNC_HOME", path);
            }
            Self { _lock: lock, prev }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            // SAFETY: still serialized by the crate-wide test_env_lock.
            unsafe {
                match &self.prev {
                    Some(value) => std::env::set_var("SKILLSTAR_TOOL_SYNC_HOME", value),
                    None => std::env::remove_var("SKILLSTAR_TOOL_SYNC_HOME"),
                }
            }
        }
    }

    #[test]
    fn sandboxed_internet_password_ops_do_not_spawn_security() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _guard = EnvGuard::sandbox(dir.path());
        assert!(crate::tool_paths::is_tool_sync_sandboxed());
        for error in [
            find_internet_password_account("https://zed.dev").expect_err("account"),
            find_internet_password("https://zed.dev", "user-1").expect_err("find"),
            add_internet_password("https://zed.dev", "user-1", "secret").expect_err("add"),
            delete_internet_password("https://zed.dev").expect_err("delete"),
        ] {
            assert_eq!(error.to_string(), SANDBOX_ERROR);
        }
    }

    #[test]
    fn find_argv_includes_account_and_server_without_spawning_security() {
        assert_eq!(
            find_internet_password_argv("user-1", "https://zed.dev", false).as_slice(),
            [
                "find-internet-password",
                "-a",
                "user-1",
                "-s",
                "https://zed.dev"
            ]
            .as_slice()
        );
        assert_eq!(
            find_internet_password_argv("user-1", "https://zed.dev", true).as_slice(),
            [
                "find-internet-password",
                "-a",
                "user-1",
                "-s",
                "https://zed.dev",
                "-w",
            ]
            .as_slice()
        );
    }

    #[test]
    fn parses_account_from_security_metadata() {
        let text = "\
keychain: \"/Library/Keychains/login.keychain-db\"
class: \"inet\"
    \"acct\"<blob>=\"user-123\"
    \"svce\"<blob>=\"https://zed.dev\"
";
        assert_eq!(
            parse_internet_password_account(text).as_deref(),
            Some("user-123")
        );
        assert_eq!(parse_internet_password_account("no account line"), None);
        assert_eq!(
            parse_internet_password_account("\"acct\"<blob>=\"   \""),
            None
        );
    }
}
