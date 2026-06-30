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
//! Other catalogs (cursor, antigravity, trae, …) are IDEs, not CLIs, and use
//! entirely different switching mechanisms — out of scope here, surfaced as
//! [`SwitchOutcome::Unsupported`].

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use skillstar_models::tool_sync::{
    create_rolling_backup, resolve_codex_auth_path, resolve_grok_auth_path,
    resolve_opencode_config_path,
};
use skillstar_usage::crypto;
use skillstar_usage::subscription::Subscription;

/// macOS keychain service name used by the real Codex CLI. Must match exactly
/// or the CLI will keep reading the stale credential.
const CODEX_KEYCHAIN_SERVICE: &str = "Codex Auth";

/// OAuth client id of the xAI Grok Build CLI. The `~/.grok/auth.json` OIDC
/// credential is keyed by `https://auth.x.ai::<this id>`, and the CLI's own
/// installer (`x.ai/cli/install.sh`) reads exactly that scope. Resolved from
/// the Grok usage fetcher (`skillstar-usage::fetchers::oauth::xai::client_id`)
/// so an env / config override of `SKILLSTAR_XAI_CLIENT_ID` stays consistent
/// across login and switch — otherwise the switch would write a key the CLI
/// never reads.
fn grok_oidc_client_id() -> &'static str {
    skillstar_usage::fetchers::oauth::xai::client_id()
}

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

/// Switch `subscription`'s credentials into its CLI config files.
///
/// Never panics; every failure path returns a [`SwitchOutcome`] with `success:
/// false` so the caller (the Tauri command) can still pin the account as
/// active locally while surfacing the CLI-sync failure to the user.
pub fn switch_subscription_to_cli(sub: &Subscription) -> SwitchOutcome {
    match sub.catalog_id.as_str() {
        "codex" => switch_codex(sub),
        "opencode" => switch_opencode(sub),
        "xai" => switch_xai(sub),
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
    match write_opencode_provider(&config_path, &api_key) {
        Ok(backup) => SwitchOutcome::ok("opencode", config_path, backup, false),
        Err(e) => SwitchOutcome::fail("opencode", config_path, e),
    }
}

/// Provider block key SkillStar owns inside `provider.<key>` of
/// `~/.config/opencode/opencode.json`. Mirrors
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

    let output = serde_json::to_string_pretty(&root)
        .map_err(|e| format!("序列化失败：{e}"))?;
    atomic_write(config_path, &output)?;
    Ok(backup)
}

// ── Grok (xAI) ─────────────────────────────────────────────────────────────

/// Switch the active Grok account by rewriting `~/.grok/auth.json`.
///
/// The Grok Build CLI authenticates from the OIDC scope entry keyed by
/// `https://auth.x.ai::<client-id>`; flipping which account the CLI uses is a
/// matter of swapping that entry's `key` (bearer token), `refresh_token`, and
/// `expires_at`. We never touch any sibling top-level scope (e.g. the legacy
/// `https://accounts.x.ai/sign-in` session), so a partial install survives.
fn switch_xai(sub: &Subscription) -> SwitchOutcome {
    let auth_path = match resolve_grok_auth_path() {
        Ok(p) => p,
        Err(e) => {
            return SwitchOutcome::fail("grok", PathBuf::from("~/.grok/auth.json"), e.to_string())
        }
    };

    // Grok is OAuth-only: the CLI reads the bearer token from `key`. Without a
    // non-empty access token the switch would write a credential the CLI
    // rejects, so fail loudly instead of silently no-opping.
    let access_token = sub
        .access_token_encrypted
        .as_deref()
        .map(crypto::decrypt)
        .filter(|t| !t.is_empty());
    let Some(access_token) = access_token else {
        return SwitchOutcome::fail(
            "grok",
            auth_path,
            "Grok 切号缺少 access_token，请重新登录该账号补充凭证",
        );
    };
    let refresh_token = sub
        .refresh_token_encrypted
        .as_deref()
        .map(crypto::decrypt)
        .filter(|t| !t.is_empty());

    let existing: serde_json::Value = if auth_path.exists() {
        std::fs::read_to_string(&auth_path)
            .ok()
            .and_then(|c| serde_json::from_str(&c).ok())
            .unwrap_or_else(|| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    let merged = merge_grok_auth(existing, sub, &access_token, refresh_token.as_deref());

    let backup = if auth_path.exists() {
        create_rolling_backup(&auth_path).ok()
    } else {
        None
    };
    let content = match serde_json::to_string_pretty(&merged) {
        Ok(c) => c,
        Err(e) => return SwitchOutcome::fail("grok", auth_path, e.to_string()),
    };
    if let Err(e) = atomic_write(&auth_path, &content) {
        return SwitchOutcome::fail("grok", auth_path, e);
    }
    SwitchOutcome::ok("grok", auth_path, backup, false)
}

/// The OIDC scope key the Grok CLI reads: `https://auth.x.ai::<client-id>`.
fn grok_oidc_scope_key() -> String {
    format!("https://auth.x.ai::{}", grok_oidc_client_id())
}

/// Best-effort email for the target Grok account. Login stores the display
/// name as `Grok · <email>` and/or the email in `oauth_account_id`.
fn grok_email(sub: &Subscription) -> Option<String> {
    if let Some(rest) = sub.display_name.strip_prefix("Grok · ") {
        let trimmed = rest.trim();
        if trimmed.contains('@') {
            return Some(trimmed.to_string());
        }
    }
    sub.oauth_account_id
        .as_deref()
        .map(str::trim)
        .filter(|id| id.contains('@'))
        .map(str::to_string)
}

/// Best-effort user id. The xAI id_token `sub` claim (stored as
/// `oauth_account_id`) is the account UUID when it is not an email.
fn grok_user_id(sub: &Subscription) -> Option<String> {
    sub.oauth_account_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty() && !id.contains('@'))
        .map(str::to_string)
}

/// Format an epoch-seconds expiry as the RFC3339 microsecond string the Grok
/// CLI writes (`2026-06-29T04:57:46.000000Z`).
fn grok_expires_at(secs: i64) -> Option<String> {
    chrono::DateTime::<chrono::Utc>::from_timestamp(secs, 0)
        .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string())
}

/// Merge the target account's credentials into the Grok `auth.json` root,
/// returning the new root. Pure (no I/O) so the merge semantics are unit
/// testable.
///
/// Behaviour:
/// - Reuses the existing OIDC entry's identity fields **only when it already
///   represents the same account** (matching email or user id), so re-syncing
///   the active account is lossless.
/// - When switching to a *different* account, builds a fresh entry from the
///   fields we hold, so stale identity never lingers next to a new token.
/// - Always overwrites `key` / `refresh_token` / `expires_at` and stamps the
///   OIDC issuer + client id so the CLI installer's scope lookup matches.
/// - Sibling top-level scopes are preserved untouched.
fn merge_grok_auth(
    mut root: serde_json::Value,
    sub: &Subscription,
    access_token: &str,
    refresh_token: Option<&str>,
) -> serde_json::Value {
    use serde_json::{Map, Value};

    if !root.is_object() {
        root = Value::Object(Map::new());
    }
    let scope = grok_oidc_scope_key();
    let target_email = grok_email(sub);
    let target_uid = grok_user_id(sub);

    let existing_entry = root
        .get(&scope)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let same_account = {
        let e_email = existing_entry.get("email").and_then(Value::as_str);
        let e_uid = existing_entry.get("user_id").and_then(Value::as_str);
        (target_email.is_some() && e_email == target_email.as_deref())
            || (target_uid.is_some() && e_uid == target_uid.as_deref())
    };

    let mut entry = if same_account {
        existing_entry
    } else {
        Map::new()
    };

    entry.insert("key".into(), Value::String(access_token.to_string()));
    match refresh_token {
        Some(rt) => {
            entry.insert("refresh_token".into(), Value::String(rt.to_string()));
        }
        None => {
            entry.remove("refresh_token");
        }
    }
    if let Some(exp) = sub.access_token_expires_at.and_then(grok_expires_at) {
        entry.insert("expires_at".into(), Value::String(exp));
    }
    entry.insert("auth_mode".into(), Value::String("oidc".into()));
    entry.insert(
        "oidc_issuer".into(),
        Value::String("https://auth.x.ai".into()),
    );
    entry.insert(
        "oidc_client_id".into(),
        Value::String(grok_oidc_client_id().into()),
    );
    if let Some(email) = target_email {
        entry
            .entry("email".to_string())
            .or_insert(Value::String(email));
    }
    if let Some(uid) = target_uid {
        entry
            .entry("user_id".to_string())
            .or_insert_with(|| Value::String(uid.clone()));
        entry
            .entry("principal_id".to_string())
            .or_insert(Value::String(uid));
    }

    if let Some(map) = root.as_object_mut() {
        map.insert(scope, Value::Object(entry));
    }
    root
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

    fn grok_sub(display_name: &str, account_id: Option<&str>) -> Subscription {
        Subscription {
            id: "grok-1".into(),
            catalog_id: "xai".into(),
            display_name: display_name.into(),
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
            access_token_expires_at: Some(1_782_000_000),
            id_token_encrypted: None,
            oauth_account_id: account_id.map(str::to_string),
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
        }
    }

    #[test]
    fn merge_grok_auth_writes_oidc_scope_with_token() {
        let sub = grok_sub("Grok · alice@example.com", Some("uid-alice"));
        let root = merge_grok_auth(serde_json::json!({}), &sub, "tok-abc", Some("rft-abc"));

        let scope = grok_oidc_scope_key();
        let entry = root.get(&scope).unwrap();
        assert_eq!(entry["key"], "tok-abc");
        assert_eq!(entry["refresh_token"], "rft-abc");
        assert_eq!(entry["auth_mode"], "oidc");
        assert_eq!(entry["oidc_issuer"], "https://auth.x.ai");
        assert_eq!(entry["oidc_client_id"], grok_oidc_client_id());
        assert_eq!(entry["email"], "alice@example.com");
        assert_eq!(entry["user_id"], "uid-alice");
        // expires_at is rendered as an RFC3339 microsecond string the CLI parses.
        assert!(entry["expires_at"].as_str().unwrap().ends_with('Z'));
    }

    #[test]
    fn merge_grok_auth_preserves_sibling_scopes() {
        let sub = grok_sub("Grok · alice@example.com", Some("uid-alice"));
        let existing = serde_json::json!({
            "https://accounts.x.ai/sign-in": { "key": "legacy-token" }
        });
        let root = merge_grok_auth(existing, &sub, "tok-new", Some("rft-new"));

        assert_eq!(
            root["https://accounts.x.ai/sign-in"]["key"],
            "legacy-token",
            "the legacy session scope must survive an OIDC switch"
        );
        assert_eq!(root[grok_oidc_scope_key()]["key"], "tok-new");
    }

    #[test]
    fn merge_grok_auth_switching_account_drops_stale_identity() {
        // Existing OIDC entry belongs to bob; we switch to alice. Bob's
        // identity must not leak onto alice's fresh token.
        let scope = grok_oidc_scope_key();
        let existing = serde_json::json!({
            scope.clone(): {
                "key": "bob-token",
                "refresh_token": "bob-refresh",
                "email": "bob@example.com",
                "user_id": "uid-bob",
                "first_name": "Bob",
                "team_id": "team-bob"
            }
        });
        let alice = grok_sub("Grok · alice@example.com", Some("uid-alice"));
        let root = merge_grok_auth(existing, &alice, "alice-token", Some("alice-refresh"));
        let entry = root.get(&scope).unwrap();

        assert_eq!(entry["key"], "alice-token");
        assert_eq!(entry["email"], "alice@example.com");
        assert_eq!(entry["user_id"], "uid-alice");
        assert!(
            entry.get("first_name").is_none(),
            "bob's display name must not linger on alice's entry"
        );
        assert!(entry.get("team_id").is_none(), "bob's team must not linger");
    }

    #[test]
    fn merge_grok_auth_resyncing_same_account_keeps_identity() {
        // Re-syncing the already-active account (matching email) should keep
        // its rich identity fields and only refresh the token material.
        let scope = grok_oidc_scope_key();
        let existing = serde_json::json!({
            scope.clone(): {
                "key": "old-token",
                "refresh_token": "old-refresh",
                "email": "alice@example.com",
                "user_id": "uid-alice",
                "first_name": "Alice",
                "team_id": "team-alice"
            }
        });
        let alice = grok_sub("Grok · alice@example.com", Some("uid-alice"));
        let root = merge_grok_auth(existing, &alice, "fresh-token", Some("fresh-refresh"));
        let entry = root.get(&scope).unwrap();

        assert_eq!(entry["key"], "fresh-token");
        assert_eq!(entry["refresh_token"], "fresh-refresh");
        assert_eq!(entry["first_name"], "Alice", "identity is preserved");
        assert_eq!(entry["team_id"], "team-alice");
    }

    #[test]
    fn merge_grok_auth_without_refresh_token_removes_stale_one() {
        let scope = grok_oidc_scope_key();
        let existing = serde_json::json!({
            scope.clone(): {
                "key": "old",
                "refresh_token": "stale-refresh",
                "email": "alice@example.com"
            }
        });
        let alice = grok_sub("Grok · alice@example.com", Some("uid-alice"));
        let root = merge_grok_auth(existing, &alice, "new", None);
        assert!(
            root[&scope].get("refresh_token").is_none(),
            "a missing refresh token must not keep a stale one around"
        );
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
