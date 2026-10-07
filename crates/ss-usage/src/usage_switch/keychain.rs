//! macOS keychain read for the Codex CLI.
//!
//! The CLI reads the keychain before `auth.json`. Custody reads that blob so a
//! token rotated in the keychain still reconciles. SkillStar does not write
//! the keychain; credentials stay in local encrypted JSON.
//!
//! Shelling out to `/usr/bin/security` rather than linking `security-framework`
//! is deliberate: keychain ACLs are bound to the calling binary, and a
//! recompiled SkillStar would lose its own authorization.
#![cfg(target_os = "macos")]

use std::path::Path;
use std::process::Command;

use serde_json::Value;
use sha2::{Digest, Sha256};

/// Keychain service name used by the real Codex CLI.
const SERVICE: &str = "Codex Auth";

/// Keychain custody is skipped whenever tool-config paths are sandboxed.
///
/// A sandboxed home already produces a *different* keychain account label, so
/// a test could not clobber a developer's real Codex login — but it would
/// still litter their login keychain and can raise an authorization prompt
/// mid-suite. Sandboxed home, sandboxed credentials: no second store.
fn enabled() -> bool {
    std::env::var_os(ss_core::infra::paths::TOOL_SYNC_HOME_ENV).is_none_or(|value| value.is_empty())
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
