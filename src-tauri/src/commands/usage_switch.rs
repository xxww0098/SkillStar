//! Account switching: write a subscription's credentials into the real CLI
//! config files so the active account takes effect in `codex` / `zcode` /
//! `opencode`.
//!
//! This is the SkillStar analogue of cockpit-tools' Codex account switch:
//! instead of injecting into an IDE's `state.vscdb`, we rewrite the CLI's own
//! credential files (`~/.codex/auth.json` + macOS keychain, `~/.zcode/...`,
//! opencode's config).
//!
//! Domain glue lives here (Tauri layer) because it bridges two crates:
//! `skillstar-usage` (subscription + crypto) and `skillstar-models`
//! (`tool_sync` path resolution + backup/merge helpers). Keeping it out of
//! either crate avoids a usage→models dependency edge.
//!
//! ## Supported catalog → CLI mapping
//!
//! | catalog_id | CLI        | auth_mode | what gets written                       |
//! |------------|------------|-----------|-----------------------------------------|
//! | `codex`    | Codex CLI  | OAuth     | `~/.codex/auth.json` `tokens` + keychain|
//! | `zcode`    | ZCode      | ApiKey    | `~/.zcode/v2/config.json` (opencode schema) |
//! | `opencode` | OpenCode   | ApiKey    | `~/.config/opencode/opencode.json`      |
//!
//! Other catalogs (cursor, antigravity, trae, …) are IDEs, not CLIs, and use
//! entirely different switching mechanisms — out of scope here, surfaced as
//! [`SwitchOutcome::Unsupported`].

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use skillstar_models::tool_sync::{
    create_rolling_backup, resolve_codex_auth_path, resolve_opencode_config_path,
    resolve_zcode_config_path,
};
use skillstar_usage::crypto;
use skillstar_usage::subscription::Subscription;

/// macOS keychain service name used by the real Codex CLI. Must match exactly
/// or the CLI will keep reading the stale credential.
const CODEX_KEYCHAIN_SERVICE: &str = "Codex Auth";

/// Outcome of a single account-switch attempt. Always serialised to the DTO
/// so the UI can show success / failure / "not a CLI provider" distinctly.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchOutcome {
    /// CLI tool id that was targeted (`"codex"` / `"zcode"` / `"opencode"`).
    pub tool_id: String,
    /// Resolved config file that was (or would be) written.
    pub config_path: String,
    /// Path to the rolling backup created before the write, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup_path: Option<String>,
    /// `true` on macOS when the keychain entry was updated (Codex only).
    #[serde(default)]
    pub keychain_updated: bool,
    /// `true` when the write fully succeeded.
    pub success: bool,
    /// Human-readable error when `success` is false.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl SwitchOutcome {
    fn ok(tool_id: &str, config_path: PathBuf, backup: Option<PathBuf>, keychain: bool) -> Self {
        Self {
            tool_id: tool_id.to_string(),
            config_path: config_path.to_string_lossy().to_string(),
            backup_path: backup.map(|p| p.to_string_lossy().to_string()),
            keychain_updated: keychain,
            success: true,
            error: None,
        }
    }

    fn fail(tool_id: &str, config_path: PathBuf, error: impl Into<String>) -> Self {
        Self {
            tool_id: tool_id.to_string(),
            config_path: config_path.to_string_lossy().to_string(),
            backup_path: None,
            keychain_updated: false,
            success: false,
            error: Some(error.into()),
        }
    }

    fn unsupported(catalog_id: &str) -> Self {
        Self {
            tool_id: catalog_id.to_string(),
            config_path: String::new(),
            backup_path: None,
            keychain_updated: false,
            success: false,
            error: None,
        }
    }
}

/// Whether a catalog id maps to a CLI whose credentials SkillStar can switch.
/// Surfaced to the UI so it can hide the "sync to CLI" affordance for IDEs.
pub fn supports_cli_switch(catalog_id: &str) -> bool {
    matches!(catalog_id, "codex" | "zcode" | "opencode")
}

/// Switch `subscription`'s credentials into its CLI config files.
///
/// Never panics; every failure path returns a [`SwitchOutcome`] with `success:
/// false` so the caller (the Tauri command) can still pin the account as
/// active locally while surfacing the CLI-sync failure to the user.
pub fn switch_subscription_to_cli(sub: &Subscription) -> SwitchOutcome {
    match sub.catalog_id.as_str() {
        "codex" => switch_codex(sub),
        "zcode" => switch_zcode(sub),
        "opencode" => switch_opencode(sub),
        other => SwitchOutcome::unsupported(other),
    }
}

// ── Codex ────────────────────────────────────────────────────────────────

fn switch_codex(sub: &Subscription) -> SwitchOutcome {
    let auth_path = match resolve_codex_auth_path() {
        Ok(p) => p,
        Err(e) => {
            return SwitchOutcome::fail("codex", PathBuf::from("~/.codex/auth.json"), e.to_string())
        }
    };

    // api_key mode: only OPENAI_API_KEY is needed.
    if let Some(api_key_cipher) = sub.api_key_encrypted.as_deref() {
        let api_key = crypto::decrypt(api_key_cipher);
        if !api_key.is_empty() {
            return write_codex_auth_file(&auth_path, codex_api_key_auth_value(&api_key), true);
        }
    }

    // OAuth mode: need the full tokens block.
    let access_token = sub
        .access_token_encrypted
        .as_deref()
        .map(crypto::decrypt)
        .filter(|t| !t.is_empty());
    let Some(access_token) = access_token else {
        return SwitchOutcome::fail(
            "codex",
            auth_path,
            "Codex OAuth 切号缺少 access_token，请重新登录该账号补充凭证",
        );
    };

    let id_token = sub
        .id_token_encrypted
        .as_deref()
        .map(crypto::decrypt)
        .filter(|t| !t.is_empty());
    let Some(id_token) = id_token else {
        return SwitchOutcome::fail(
            "codex",
            auth_path,
            "Codex OAuth 切号缺少 id_token，请重新登录该账号补充凭证",
        );
    };

    let refresh_token = sub
        .refresh_token_encrypted
        .as_deref()
        .map(crypto::decrypt);
    let account_id = sub.oauth_account_id.clone();

    let auth_value = codex_oauth_auth_value(&id_token, &access_token, refresh_token, account_id);
    let mut outcome = write_codex_auth_file(&auth_path, auth_value, true);

    // On macOS the Codex CLI reads credentials from the keychain, not
    // auth.json (auth.json is only a fallback). Failing to update the
    // keychain means the switch silently no-ops, so we must do it and report.
    #[cfg(target_os = "macos")]
    if outcome.success {
        match write_codex_keychain(&auth_path, &sub.catalog_id) {
            Ok(()) => outcome.keychain_updated = true,
            Err(e) => {
                outcome.success = false;
                outcome.error = Some(format!("auth.json 已更新，但 macOS keychain 写入失败：{e}"));
            }
        }
    }
    let _ = &sub.catalog_id; // keep borrow used on all platforms
    outcome
}

/// Build the `~/.codex/auth.json` JSON value for an OAuth account, mirroring
/// the schema the real Codex CLI writes (and cockpit-tools reproduces):
/// `{ "OPENAI_API_KEY": null, "tokens": { id_token, access_token,
/// refresh_token, account_id }, "last_refresh": <iso8601> }`.
///
/// Separated from I/O so it is trivially unit-testable.
fn codex_oauth_auth_value(
    id_token: &str,
    access_token: &str,
    refresh_token: Option<String>,
    account_id: Option<String>,
) -> serde_json::Value {
    let tokens = serde_json::json!({
        "id_token": id_token,
        "access_token": access_token,
        "refresh_token": refresh_token.unwrap_or_default(),
        "account_id": account_id,
    });
    serde_json::json!({
        "OPENAI_API_KEY": serde_json::Value::Null,
        "tokens": tokens,
        "last_refresh": chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string(),
    })
}

/// api_key-mode auth.json: `{"OPENAI_API_KEY": "<key>"}`.
fn codex_api_key_auth_value(api_key: &str) -> serde_json::Value {
    serde_json::json!({ "OPENAI_API_KEY": api_key })
}

/// Atomically write the auth.json value, after a rolling backup. `with_backup`
/// is false in unit tests where there is no prior file to back up.
fn write_codex_auth_file(
    auth_path: &Path,
    value: serde_json::Value,
    with_backup: bool,
) -> SwitchOutcome {
    let backup = if with_backup && auth_path.exists() {
        create_rolling_backup(auth_path).ok()
    } else {
        None
    };

    // We replace the whole tokens/OPENAI_API_KEY block rather than merge, so
    // write pretty JSON directly (merge_json_write would preserve stale keys).
    let content = match serde_json::to_string_pretty(&value) {
        Ok(c) => c,
        Err(e) => return SwitchOutcome::fail("codex", auth_path.to_path_buf(), e.to_string()),
    };
    if let Err(e) = atomic_write(auth_path, &content) {
        return SwitchOutcome::fail("codex", auth_path.to_path_buf(), e);
    }
    SwitchOutcome::ok("codex", auth_path.to_path_buf(), backup, false)
}

/// Compute the keychain account label the Codex CLI looks up on macOS:
/// `cli|<first 16 hex chars of sha256(canonicalized ~/.codex)>`. Matches
/// cockpit-tools (`build_codex_keychain_account`) and the CLI itself.
#[cfg(target_os = "macos")]
fn codex_keychain_account_label(codex_home: &Path) -> String {
    let resolved = std::fs::canonicalize(codex_home).unwrap_or_else(|_| codex_home.to_path_buf());
    let mut hasher = Sha256::new();
    hasher.update(resolved.to_string_lossy().as_bytes());
    let digest = hasher.finalize();
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!("cli|{}", &hex[..16])
}

/// Store the same auth.json payload into the macOS keychain under the Codex
/// CLI's service/account, via the `security` CLI.
#[cfg(target_os = "macos")]
fn write_codex_keychain(auth_path: &Path, _catalog_id: &str) -> Result<(), String> {
    let codex_home = auth_path
        .parent()
        .ok_or_else(|| "无法定位 ~/.codex 目录".to_string())?;
    let payload = std::fs::read_to_string(auth_path)
        .map_err(|e| format!("读取 auth.json 失败：{e}"))?;
    let account = codex_keychain_account_label(codex_home);

    let output = std::process::Command::new("security")
        .arg("add-generic-password")
        .arg("-U")
        .arg("-s")
        .arg(CODEX_KEYCHAIN_SERVICE)
        .arg("-a")
        .arg(&account)
        .arg("-w")
        .arg(&payload)
        .output()
        .map_err(|e| format!("执行 security 命令失败：{e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "security 写入失败：{}",
            if stderr.trim().is_empty() {
                "未知错误"
            } else {
                stderr.trim()
            }
        ));
    }
    Ok(())
}

// ── ZCode ────────────────────────────────────────────────────────────────

fn switch_zcode(sub: &Subscription) -> SwitchOutcome {
    let config_path = match resolve_zcode_config_path() {
        Ok(p) => p,
        Err(e) => {
            return SwitchOutcome::fail(
                "zcode",
                PathBuf::from("~/.zcode/v2/config.json"),
                e.to_string(),
            )
        }
    };
    let Some(api_key_cipher) = sub.api_key_encrypted.as_deref() else {
        return SwitchOutcome::fail("zcode", config_path, "ZCode 账号缺少 API Key");
    };
    let api_key = crypto::decrypt(api_key_cipher);
    if api_key.is_empty() {
        return SwitchOutcome::fail("zcode", config_path, "ZCode API Key 为空");
    }
    match write_opencode_provider(&config_path, &api_key, "zcode") {
        Ok(backup) => SwitchOutcome::ok("zcode", config_path, backup, false),
        Err(e) => SwitchOutcome::fail("zcode", config_path, e),
    }
}

// ── OpenCode ─────────────────────────────────────────────────────────────

fn switch_opencode(sub: &Subscription) -> SwitchOutcome {
    let config_path = match resolve_opencode_config_path() {
        Ok(p) => p,
        Err(e) => {
            return SwitchOutcome::fail(
                "opencode",
                PathBuf::from("~/.config/opencode/opencode.json"),
                e.to_string(),
            )
        }
    };
    let Some(api_key_cipher) = sub.api_key_encrypted.as_deref() else {
        return SwitchOutcome::fail("opencode", config_path, "OpenCode 账号缺少 API Key");
    };
    let api_key = crypto::decrypt(api_key_cipher);
    if api_key.is_empty() {
        return SwitchOutcome::fail("opencode", config_path, "OpenCode API Key 为空");
    }
    match write_opencode_provider(&config_path, &api_key, "opencode") {
        Ok(backup) => SwitchOutcome::ok("opencode", config_path, backup, false),
        Err(e) => SwitchOutcome::fail("opencode", config_path, e),
    }
}

/// Provider block key SkillStar owns inside `provider.<key>` of both
/// `~/.zcode/v2/config.json` and `~/.config/opencode/opencode.json`. Mirrors
/// `tool_sync::OPENCODE_MANAGED_PROVIDER_KEY` (which is `pub(crate)` there).
const MANAGED_PROVIDER_KEY: &str = "skillstar";

/// Write/replace the `provider.skillstar` block (OpenCode schema) in the
/// given config file, preserving all other top-level + sibling provider
/// entries. Returns the backup path when a prior file existed.
///
/// This mirrors `tool_sync::sync_to_opencode_inner` but works off a raw
/// decrypted api_key instead of a full `ProviderEntryFlat` — account
/// switching only ever needs to swap the key, not the routing metadata.
fn write_opencode_provider(
    config_path: &Path,
    api_key: &str,
    tool_id: &str,
) -> Result<Option<PathBuf>, String> {
    let backup = if config_path.exists() {
        create_rolling_backup(config_path).ok()
    } else {
        None
    };
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败：{e}"))?;
    }

    // Read existing root (or seed the OpenCode schema), then merge the managed
    // provider block under `provider.<key>` without touching siblings.
    let mut root: serde_json::Value = if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)
            .map_err(|e| format!("读取 {} 失败：{e}", config_path.display()))?;
        serde_json::from_str(&content).unwrap_or_else(|_| {
            serde_json::json!({
                "$schema": "https://opencode.ai/config.json",
                "provider": {}
            })
        })
    } else {
        serde_json::json!({
            "$schema": "https://opencode.ai/config.json",
            "provider": {}
        })
    };

    let base_url = base_url_for_tool(tool_id);
    let provider_block = serde_json::json!({
        "npm": "@ai-sdk/openai-compatible",
        "name": "SkillStar",
        "options": {
            "baseURL": base_url,
            "apiKey": api_key,
        }
    });

    let root_obj = root
        .as_object_mut()
        .ok_or_else(|| "配置根不是 JSON 对象".to_string())?;
    let providers = root_obj
        .entry("provider".to_string())
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    if let Some(map) = providers.as_object_mut() {
        map.insert(MANAGED_PROVIDER_KEY.to_string(), provider_block);
    }

    let output = serde_json::to_string_pretty(&root)
        .map_err(|e| format!("序列化失败：{e}"))?;
    atomic_write(config_path, &output)?;
    Ok(backup)
}

/// Canonical base URL for each CLI's OpenAI-compatible endpoint. ZCode /
/// OpenCode accounts in the usage panel are routed to their vendor's endpoint.
fn base_url_for_tool(tool_id: &str) -> &'static str {
    match tool_id {
        "zcode" => "https://api.z.ai/api/paas/v4",
        _ => "https://api.opencode.ai/v1",
    }
}

// ── helpers ──────────────────────────────────────────────────────────────

/// Atomic file write: tmp file + rename, so a crash mid-write can't leave a
/// truncated CLI config.
fn atomic_write(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败：{e}"))?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, content).map_err(|e| format!("写入临时文件失败：{e}"))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("重命名失败：{e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supports_cli_switch_only_for_cli_catalogs() {
        assert!(supports_cli_switch("codex"));
        assert!(supports_cli_switch("zcode"));
        assert!(supports_cli_switch("opencode"));
        assert!(!supports_cli_switch("cursor"));
        assert!(!supports_cli_switch("antigravity"));
        assert!(!supports_cli_switch("deepseek"));
    }

    #[test]
    fn unsupported_catalog_returns_unsupported_outcome() {
        let sub = Subscription {
            id: "x".into(),
            catalog_id: "cursor".into(),
            display_name: "Cursor".into(),
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
            fingerprint_id: None,
            cookie_jar_encrypted: None,
            cookie_session_expires_at: None,
            manual_quota: None,
            note: None,
            sort_index: 0,
            created_at: 0,
            updated_at: 0,
        };
        let outcome = switch_subscription_to_cli(&sub);
        assert!(!outcome.success);
        assert!(outcome.error.is_none(), "unsupported must have no error");
        assert_eq!(outcome.tool_id, "cursor");
    }

    #[test]
    fn codex_oauth_auth_value_has_tokens_block_and_null_api_key() {
        let v = codex_oauth_auth_value(
            "idt",
            "acct",
            Some("rft".into()),
            Some("acc-1".into()),
        );
        assert!(v.get("OPENAI_API_KEY").unwrap().is_null());
        let tokens = v.get("tokens").unwrap();
        assert_eq!(tokens["id_token"], "idt");
        assert_eq!(tokens["access_token"], "acct");
        assert_eq!(tokens["refresh_token"], "rft");
        assert_eq!(tokens["account_id"], "acc-1");
        assert!(v.get("last_refresh").unwrap().is_string());
    }

    #[test]
    fn codex_oauth_auth_value_uses_empty_refresh_when_missing() {
        let v = codex_oauth_auth_value("idt", "acct", None, None);
        assert_eq!(v["tokens"]["refresh_token"], "");
        assert!(v["tokens"].get("account_id").unwrap().is_null());
    }

    #[test]
    fn codex_api_key_auth_value_just_openai_key() {
        let v = codex_api_key_auth_value("sk-test");
        assert_eq!(v["OPENAI_API_KEY"], "sk-test");
        assert!(v.get("tokens").is_none());
    }
}
