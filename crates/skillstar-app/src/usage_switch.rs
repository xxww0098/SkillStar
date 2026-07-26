//! Account switching: write a subscription's credentials into the real CLI
//! config files so the active account takes effect in `codex` / `opencode` /
//! `grok`.
//!
//! This is the SkillStar analogue of cockpit-tools' Codex account switch:
//! instead of injecting into an IDE's `state.vscdb`, we rewrite the CLI's own
//! credential files (`~/.codex/auth.json` + macOS keychain, opencode's config,
//! `~/.grok/auth.json`).
//!
//! Domain glue lives in `skillstar-app` (the top-level aggregator crate)
//! because it bridges two crates: `skillstar-usage` (subscription + crypto)
//! and `skillstar-models` (`tool_sync` path resolution + backup/merge
//! helpers). Hosting it here keeps the Tauri command layer thin and avoids a
//! `skillstar-usage → skillstar-models` dependency edge (the aggregator
//! already depends on both).
//!
//! ## Supported catalog → CLI mapping
//!
//! | catalog_id | CLI        | auth_mode | what gets written                       |
//! |------------|------------|-----------|-----------------------------------------|
//! | `codex`    | Codex CLI  | OAuth     | `~/.codex/auth.json` `tokens` + keychain|
//! | `opencode` | OpenCode   | ApiKey    | `~/.config/opencode/opencode.json`      |
//! | `xai`      | Grok CLI   | OAuth     | `~/.grok/auth.json` OIDC scope entry    |
//!
//! Other catalogs (cursor, antigravity, …) are IDEs, not CLIs, and use
//! entirely different switching mechanisms — out of scope here, surfaced as
//! [`SwitchOutcome::Unsupported`].

mod grok;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use skillstar_models::tool_sync::{
    create_rolling_backup, resolve_codex_auth_path, resolve_opencode_config_path,
};
use skillstar_usage::subscription::Subscription;
use skillstar_usage::{UsageResult, crypto, storage};

/// macOS keychain service name used by the real Codex CLI. Must match exactly
/// or the CLI will keep reading the stale credential.
const CODEX_KEYCHAIN_SERVICE: &str = "Codex Auth";

/// Outcome of a single account-switch attempt. Always serialised to the DTO
/// so the UI can show success / failure / "not a CLI provider" distinctly.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchOutcome {
    /// CLI tool id that was targeted (`"codex"` / `"opencode"` / `"grok"`).
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
    matches!(catalog_id, "codex" | "opencode" | "xai")
}

/// Result returned by the activation facade. Tauri receives the freshly
/// loaded/patched subscription plus the provider-specific CLI outcome, while
/// every ordering invariant remains inside this module.
#[derive(Debug, Clone)]
pub struct ActivationResult {
    pub subscription: Subscription,
    pub switch_result: SwitchOutcome,
}

/// Opaque provider lease held while a Usage refresh may rotate credentials.
/// For Grok this is the official `auth.json.lock` also used by the CLI.
pub struct CliRefreshLease {
    grok: Option<grok::GrokRefreshLease>,
}

pub async fn acquire_cli_refresh_lease(catalog_id: &str) -> UsageResult<CliRefreshLease> {
    let grok = if catalog_id == "xai" {
        Some(grok::acquire_refresh_lease().await?)
    } else {
        None
    };
    Ok(CliRefreshLease { grok })
}

/// Project an already-persisted credential rotation into an active CLI while
/// the caller still holds [`CliRefreshLease`] and the catalog transaction.
pub fn sync_refreshed_active_subscription(
    subscription: &mut Subscription,
    lease: &CliRefreshLease,
) -> UsageResult<Option<SwitchOutcome>> {
    if subscription.catalog_id == "xai"
        && storage::get_active_subscription("xai")?.as_deref() == Some(subscription.id.as_str())
    {
        let grok = lease.grok.as_ref().ok_or_else(|| {
            skillstar_usage::UsageError::Other("active Grok refresh is missing auth lease".into())
        })?;
        return grok::sync_refreshed_active(subscription, grok).map(Some);
    }
    Ok(None)
}

/// After taking the provider lease but before issuing a refresh request, adopt
/// a newer active CLI-side token generation if Grok already rotated it.
pub fn adopt_active_cli_session_before_refresh(
    subscription: &mut Subscription,
    lease: &CliRefreshLease,
) -> UsageResult<()> {
    if subscription.catalog_id == "xai" {
        let grok = lease.grok.as_ref().ok_or_else(|| {
            skillstar_usage::UsageError::Other("active Grok refresh is missing auth lease".into())
        })?;
        grok::adopt_active_before_refresh(subscription, grok)?;
    }
    Ok(())
}

/// Pin a subscription and activate its real CLI credentials under the same
/// per-catalog serialization domain used by Usage refreshes.
pub async fn activate_subscription(subscription_id: &str) -> UsageResult<ActivationResult> {
    let probe = storage::get_subscription(subscription_id)?;
    let lock_catalog_id = probe.catalog_id.clone();
    let catalog_id = lock_catalog_id.clone();
    let subscription_id = subscription_id.to_string();
    skillstar_usage::refresh_guard::with_catalog_lock(&lock_catalog_id, || async move {
        if catalog_id == "xai" {
            let (subscription, switch_result) = grok::activate(&subscription_id).await?;
            return Ok(ActivationResult {
                subscription,
                switch_result,
            });
        }

        let subscription = storage::get_subscription(&subscription_id)?;
        storage::set_active_subscription(&subscription.catalog_id, &subscription.id)?;
        let switch_result = switch_non_grok(&subscription);
        Ok(ActivationResult {
            subscription,
            switch_result,
        })
    })
    .await?
}

/// Re-push the currently active subscription through the same provider seam
/// without changing the active pin.
pub async fn resync_active_subscription(catalog_id: &str) -> UsageResult<SwitchOutcome> {
    let lock_catalog_id = catalog_id.to_string();
    let catalog_id = lock_catalog_id.clone();
    skillstar_usage::refresh_guard::with_catalog_lock(&lock_catalog_id, || async move {
        let active = storage::get_active_subscription(&catalog_id)?.ok_or_else(|| {
            skillstar_usage::UsageError::Other(format!("catalog {catalog_id} 还没有设置活跃账号"))
        })?;
        if catalog_id == "xai" {
            return grok::resync(&active).await;
        }
        let subscription = storage::get_subscription(&active)?;
        Ok(switch_non_grok(&subscription))
    })
    .await?
}

/// Remove provider-private CLI session material before a subscription row is
/// deleted. Generic Usage storage never needs to know the Grok auth schema.
pub fn forget_subscription_session(catalog_id: &str, subscription_id: &str) -> UsageResult<()> {
    if catalog_id == "xai" {
        grok::forget(subscription_id)?;
    }
    Ok(())
}

fn switch_non_grok(sub: &Subscription) -> SwitchOutcome {
    match sub.catalog_id.as_str() {
        "codex" => switch_codex(sub),
        "opencode" => switch_opencode(sub),
        other => SwitchOutcome::unsupported(other),
    }
}

// ── Codex ────────────────────────────────────────────────────────────────

fn switch_codex(sub: &Subscription) -> SwitchOutcome {
    let auth_path = match resolve_codex_auth_path() {
        Ok(p) => p,
        Err(e) => {
            return SwitchOutcome::fail(
                "codex",
                PathBuf::from("~/.codex/auth.json"),
                e.to_string(),
            );
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

    let refresh_token = sub.refresh_token_encrypted.as_deref().map(crypto::decrypt);
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
    let payload =
        std::fs::read_to_string(auth_path).map_err(|e| format!("读取 auth.json 失败：{e}"))?;
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

// ── OpenCode ─────────────────────────────────────────────────────────────

fn switch_opencode(sub: &Subscription) -> SwitchOutcome {
    let config_path = match resolve_opencode_config_path() {
        Ok(p) => p,
        Err(e) => {
            return SwitchOutcome::fail(
                "opencode",
                PathBuf::from("~/.config/opencode/opencode.json"),
                e.to_string(),
            );
        }
    };
    let Some(api_key_cipher) = sub.api_key_encrypted.as_deref() else {
        return SwitchOutcome::fail("opencode", config_path, "OpenCode 账号缺少 API Key");
    };
    let api_key = crypto::decrypt(api_key_cipher);
    if api_key.is_empty() {
        return SwitchOutcome::fail("opencode", config_path, "OpenCode API Key 为空");
    }
    match write_opencode_provider(&config_path, &api_key) {
        Ok(backup) => SwitchOutcome::ok("opencode", config_path, backup, false),
        Err(e) => SwitchOutcome::fail("opencode", config_path, e),
    }
}

/// Provider block key SkillStar owns inside `provider.<key>` of
/// `~/.config/opencode/opencode.json`. Matches the legacy `skillstar` key that
/// `tool_sync::is_skillstar_managed_key` treats as managed.
const MANAGED_PROVIDER_KEY: &str = "skillstar";

/// Write/replace the `provider.skillstar` block (OpenCode schema) in the
/// given config file, preserving all other top-level + sibling provider
/// entries. Returns the backup path when a prior file existed.
///
/// This mirrors the OpenCode block shape written by
/// `tool_sync::sync_opencode_binding` but works off a raw decrypted api_key
/// instead of a full `ProviderEntryFlat` — account switching only ever needs
/// to swap the key, not the routing metadata.
fn write_opencode_provider(config_path: &Path, api_key: &str) -> Result<Option<PathBuf>, String> {
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
        let content = std::fs::read_to_string(config_path)
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

    let provider_block = serde_json::json!({
        "npm": "@ai-sdk/openai-compatible",
        "name": "SkillStar",
        "options": {
            "baseURL": "https://api.opencode.ai/v1",
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

    let output = serde_json::to_string_pretty(&root).map_err(|e| format!("序列化失败：{e}"))?;
    atomic_write(config_path, &output)?;
    Ok(backup)
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
        assert!(supports_cli_switch("opencode"));
        assert!(supports_cli_switch("xai"));
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
            cookie_jar_encrypted: None,
            cookie_session_expires_at: None,
            manual_quota: None,
            note: None,
            sort_index: 0,
            created_at: 0,
            updated_at: 0,
        };
        let outcome = switch_non_grok(&sub);
        assert!(!outcome.success);
        assert!(outcome.error.is_none(), "unsupported must have no error");
        assert_eq!(outcome.tool_id, "cursor");
    }

    #[test]
    fn codex_oauth_auth_value_has_tokens_block_and_null_api_key() {
        let v = codex_oauth_auth_value("idt", "acct", Some("rft".into()), Some("acc-1".into()));
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
