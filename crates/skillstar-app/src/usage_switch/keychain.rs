//! macOS keychain custody for the Codex CLI — the one store a symlink cannot
//! reach.
//!
//! On macOS the CLI reads the keychain **first** and treats `auth.json` as a
//! fallback, so pointing the file at a snapshot without also updating the
//! keychain would be a switch that changes nothing. Two consequences drive
//! this module:
//!
//! * **Write is read-modify-write.** `security add-generic-password -U`
//!   replaces the entire secret, so blindly writing our own object would drop
//!   any key the CLI keeps alongside it. (Codex's blob holds one auth object
//!   today; Claude Code's holds every MCP-server login, which is the failure
//!   this rule exists to prevent.)
//! * **Read feeds reconciliation.** A CLI that rotated its token through the
//!   keychain never touched the snapshot, so the snapshot only stays fresh if
//!   the keychain is read back into it.
//!
//! Shelling out to `/usr/bin/security` rather than linking `security-framework`
//! is deliberate: keychain ACLs are bound to the calling binary, and a
//! recompiled SkillStar would lose its own authorization.
#![cfg(target_os = "macos")]

use std::path::Path;
use std::process::Command;

use serde_json::Value;
use sha2::{Digest, Sha256};

use super::error::ExternalStoreError;

/// Keychain service name used by the real Codex CLI.
const SERVICE: &str = "Codex Auth";

/// Keychain custody is skipped whenever tool-config paths are sandboxed.
///
/// A sandboxed home already produces a *different* keychain account label, so
/// a test could not clobber a developer's real Codex login — but it would
/// still litter their login keychain and can raise an authorization prompt
/// mid-suite. Sandboxed home, sandboxed credentials: no second store.
fn enabled() -> bool {
    std::env::var_os(skillstar_models::tool_sync::TOOL_SYNC_HOME_ENV)
        .is_none_or(|value| value.is_empty())
}

/// The account label the CLI looks up: `cli|<sha256(canonical CODEX_HOME)[..16]>`.
/// Namespacing by home is why `CODEX_HOME` has to be honoured when resolving
/// the live path — a different home is a different keychain entry.
fn account_label(codex_home: &Path) -> String {
    let resolved = std::fs::canonicalize(codex_home).unwrap_or_else(|_| codex_home.to_path_buf());
    let digest = Sha256::digest(resolved.to_string_lossy().as_bytes());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!("cli|{}", &hex[..16])
}

/// The credential blob the CLI is actually authenticating with, if any.
pub(super) fn read(codex_home: &Path) -> Option<Value> {
    if !enabled() {
        return None;
    }
    let output = Command::new("security")
        .args(["find-generic-password", "-s", SERVICE, "-a"])
        .arg(account_label(codex_home))
        .arg("-w")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let raw = String::from_utf8(output.stdout).ok()?;
    let value: Value = serde_json::from_str(raw.trim()).ok()?;
    value.is_object().then_some(value)
}

/// Merge `root`'s keys into the stored blob and write it back. Returns whether
/// anything was written.
pub(super) fn write_merged(codex_home: &Path, root: &Value) -> Result<bool, ExternalStoreError> {
    if !enabled() {
        return Ok(false);
    }
    let mut merged = read(codex_home).unwrap_or_else(|| Value::Object(Default::default()));
    let (Some(target), Some(incoming)) = (merged.as_object_mut(), root.as_object()) else {
        return Err(ExternalStoreError::NotAnObject);
    };
    for (key, value) in incoming {
        target.insert(key.clone(), value.clone());
    }

    let payload = serde_json::to_string(&merged)
        .map_err(|source| ExternalStoreError::Serialize { source })?;
    let output = Command::new("security")
        .args(["add-generic-password", "-U", "-s", SERVICE, "-a"])
        .arg(account_label(codex_home))
        .arg("-w")
        .arg(&payload)
        .output()
        .map_err(|source| ExternalStoreError::Spawn { source })?;
    if output.status.success() {
        return Ok(true);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(ExternalStoreError::Write {
        detail: if stderr.trim().is_empty() {
            "未知错误".to_string()
        } else {
            stderr.trim().to_string()
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pinned against the CLI's own `compute_store_key`: service `Codex Auth`,
    /// account `cli|<first 16 hex of sha256(canonical home)>`.
    #[test]
    fn account_label_matches_the_cli_namespacing_scheme() {
        let temp = tempfile::tempdir().unwrap();
        let label = account_label(temp.path());
        assert!(label.starts_with("cli|"), "{label}");
        assert_eq!(label.len(), "cli|".len() + 16);
        assert_eq!(label, account_label(temp.path()), "must be stable");
    }

    #[test]
    fn account_label_is_per_home() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        assert_ne!(account_label(a.path()), account_label(b.path()));
    }
}
