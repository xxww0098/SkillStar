//! Where Claude Code keeps its credential blob, and how to address it.
//!
//! One module owns the addressing rules so the read-only login adoption
//! (`fetchers::oauth::anthropic`) and the switch adapter
//! (`usage_switch::claude`) can never disagree about which keychain item or
//! file is authoritative:
//!
//! - macOS: the generic-password item, account = `$USER` / `$LOGNAME` with
//!   fallback `claude-code-user`, service `Claude Code-credentials` — plus a
//!   `-<8 hex>` SHA-256 suffix of the config dir whenever `CLAUDE_CONFIG_DIR`
//!   is set, exactly how Claude Code scopes non-default installs.
//! - every platform: `<config dir>/.credentials.json` as the fallback store
//!   (authoritative off macOS; on macOS a stale mirror the CLI deletes after
//!   migrating).
//!
//! [D-083](docs/decisions.md) scopes the one keychain write SkillStar is
//! allowed: Claude Code's own item, above. All keychain IO shells out to
//! `/usr/bin/security` instead of linking `security-framework` — keychain ACL
//! grants bind to the calling binary's signature, and a rebuilt `skillstar`
//! would silently lose an entitlement granted to the old one. Writes go
//! through `add-generic-password -U -X <hex>` so the JSON blob never has to
//! survive shell quoting.
//!
//! `RealSecurity` refuses to run while `SKILLSTAR_TOOL_SYNC_HOME` is set (and
//! off macOS), so no test or sandboxed run can touch a real login keychain;
//! unit tests inject their own [`SecurityRunner`].

use std::path::PathBuf;

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{UsageError, UsageResult, tool_paths};

/// macOS keychain item Claude Code stores its credential blob under (before
/// any config-dir scoping suffix).
pub(crate) const KEYCHAIN_SERVICE_BASE: &str = "Claude Code-credentials";

/// Account label Claude Code falls back to when it cannot read the OS login
/// name.
const KEYCHAIN_ACCOUNT_FALLBACK: &str = "claude-code-user";

/// `security` exits with 44 when the item does not exist.
const EXIT_ITEM_NOT_FOUND: i32 = 44;

/// OS login name Claude Code keys its keychain item by.
pub(crate) fn keychain_account() -> String {
    for var in ["USER", "LOGNAME"] {
        if let Ok(value) = std::env::var(var) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }
    KEYCHAIN_ACCOUNT_FALLBACK.to_string()
}

/// The unscoped service name when no config dir override applies, and the
/// SHA-256-scoped one when it does — matching Claude Code's own derivation.
pub(crate) fn service_for_config_dir(env_config_dir: Option<&str>) -> String {
    let Some(dir) = env_config_dir.map(str::trim).filter(|dir| !dir.is_empty()) else {
        return KEYCHAIN_SERVICE_BASE.to_string();
    };
    let digest = Sha256::digest(dir.as_bytes());
    let suffix: String = digest.iter().take(4).map(|byte| format!("{byte:02x}")).collect();
    format!("{KEYCHAIN_SERVICE_BASE}-{suffix}")
}

/// Service name of Claude Code's keychain item for the current environment.
pub(crate) fn keychain_service() -> String {
    service_for_config_dir(std::env::var("CLAUDE_CONFIG_DIR").ok().as_deref())
}

/// `<config dir>/.credentials.json`, honoring `CLAUDE_CONFIG_DIR`.
pub(crate) fn credentials_file_path() -> PathBuf {
    if let Ok(dir) = std::env::var("CLAUDE_CONFIG_DIR") {
        let dir = dir.trim();
        if !dir.is_empty() {
            return PathBuf::from(dir).join(".credentials.json");
        }
    }
    skillstar_core::infra::paths::home_dir()
        .join(".claude")
        .join(".credentials.json")
}

// ── file store ───────────────────────────────────────────────────────────

/// Read the credentials file as a whole JSON blob. `Ok(None)` = absent; an
/// empty file reads as an empty object (Claude Code tolerates that shape).
pub(crate) fn read_file_blob() -> UsageResult<Option<Value>> {
    match std::fs::read_to_string(credentials_file_path()) {
        Ok(text) if text.trim().is_empty() => Ok(Some(Value::Object(Default::default()))),
        Ok(text) => serde_json::from_str(&text)
            .map(Some)
            .map_err(|error| UsageError::Other(format!("解析 Claude 凭证文件失败：{error}"))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(UsageError::Other(format!("读取 Claude 凭证文件失败：{error}"))),
    }
}

/// Production read of the live blob: keychain first on macOS (where the file
/// is only a stale mirror), file everywhere else. Lenient by design — a
/// missing login is `None`, and callers degrade, they do not fail.
pub(crate) fn read_live_blob() -> Option<Value> {
    #[cfg(target_os = "macos")]
    if let Ok(Some(blob)) = read_keychain_blob(&RealSecurity) {
        return Some(blob);
    }
    read_file_blob().ok().flatten()
}

// ── keychain IO ──────────────────────────────────────────────────────────

#[derive(Debug)]
pub(crate) struct SecurityOutput {
    pub(crate) success: bool,
    pub(crate) status_code: Option<i32>,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

/// Test seam over `/usr/bin/security`. Production uses [`RealSecurity`];
/// tests inject a fake so no unit test ever addresses a real keychain.
pub(crate) trait SecurityRunner {
    fn run(&self, args: &[String]) -> UsageResult<SecurityOutput>;
}

/// Spawns `/usr/bin/security`. Refuses off macOS and while the tool-sync
/// sandbox is active — the two states where the real login keychain must not
/// be reached (read or write) from this process.
pub(crate) struct RealSecurity;

impl SecurityRunner for RealSecurity {
    fn run(&self, args: &[String]) -> UsageResult<SecurityOutput> {
        ensure_keychain_addressable()?;
        let output = std::process::Command::new("/usr/bin/security")
            .args(args)
            .output()
            .map_err(|error| UsageError::Other(format!("执行 security 失败：{error}")))?;
        Ok(SecurityOutput {
            success: output.status.success(),
            status_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

/// Read the credential blob from Claude Code's keychain item. `Ok(None)` =
/// the item does not exist yet (a fresh machine, or a pre-migration install).
pub(crate) fn read_keychain_blob(runner: &dyn SecurityRunner) -> UsageResult<Option<Value>> {
    let output = runner.run(&find_args())?;
    if !output.success {
        return if is_not_found(&output) {
            Ok(None)
        } else {
            Err(command_error("读取 Claude Code 钥匙串失败", &output))
        };
    }
    let text = output.stdout.trim();
    if text.is_empty() {
        return Ok(None);
    }
    serde_json::from_str(text)
        .map(Some)
        .map_err(|error| UsageError::Other(format!("解析 Claude Code 钥匙串失败：{error}")))
}

/// Write the whole credential blob into Claude Code's keychain item, creating
/// or updating it. Only the switch adapter calls this, under the D-083
/// exception; the fetcher stays read-only.
pub(crate) fn write_keychain_blob(runner: &dyn SecurityRunner, blob: &Value) -> UsageResult<()> {
    let text = serde_json::to_string(blob).map_err(|error| UsageError::Other(error.to_string()))?;
    let hex: String = text.as_bytes().iter().map(|byte| format!("{byte:02x}")).collect();
    let output = runner.run(&write_args(&hex))?;
    if output.success {
        Ok(())
    } else {
        Err(command_error("写入 Claude Code 钥匙串失败", &output))
    }
}

fn find_args() -> Vec<String> {
    vec![
        "find-generic-password".into(),
        "-a".into(),
        keychain_account(),
        "-s".into(),
        keychain_service(),
        "-w".into(),
    ]
}

fn write_args(hex: &str) -> Vec<String> {
    vec![
        "add-generic-password".into(),
        "-U".into(),
        "-a".into(),
        keychain_account(),
        "-s".into(),
        keychain_service(),
        "-X".into(),
        hex.into(),
    ]
}

fn ensure_keychain_addressable() -> UsageResult<()> {
    if !cfg!(target_os = "macos") {
        return Err(UsageError::Other("Claude Code 钥匙串存储仅支持 macOS".into()));
    }
    if tool_paths::is_tool_sync_sandboxed() {
        return Err(UsageError::Other(
            "SKILLSTAR_TOOL_SYNC_HOME 已设置，拒绝访问 macOS 钥匙串".into(),
        ));
    }
    Ok(())
}

fn is_not_found(output: &SecurityOutput) -> bool {
    output.status_code == Some(EXIT_ITEM_NOT_FOUND)
        || output.stderr.contains("could not be found")
}

fn command_error(action: &str, output: &SecurityOutput) -> UsageError {
    let stderr = output.stderr.trim();
    if stderr.is_empty() {
        UsageError::Other(action.into())
    } else {
        UsageError::Other(format!("{action}：{stderr}"))
    }
}

#[cfg(test)]
#[path = "claude_credentials_tests.rs"]
mod tests;
