//! Codex multi-account management + quota fetching.
//!
//! Accounts are stored in `~/.skillstar/config/codex_accounts/` as individual JSON files.
//! Index: `~/.skillstar/config/codex_accounts_index.json`
//! Quota: fetched from `chatgpt.com/backend-api/wham/usage`

use anyhow::{Context, Result};
use base64::Engine;
use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::codex_oauth;
use crate::core::paths;

// ── Types ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexAccount {
    pub id: String,
    pub email: String,
    pub auth_mode: String, // "oauth" | "apikey"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub openai_api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_name: Option<String>,
    pub tokens: CodexTokens,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quota: Option<CodexQuota>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quota_error: Option<CodexQuotaError>,
    pub created_at: i64,
    pub last_used: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexTokens {
    pub id_token: String,
    pub access_token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexQuota {
    /// 主窗口(5h)配额剩余百分比 (0-100)
    pub hourly_percentage: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hourly_reset_time: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hourly_window_minutes: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hourly_window_present: Option<bool>,
    /// 次窗口(7d)配额剩余百分比 (0-100)
    pub weekly_percentage: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weekly_reset_time: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weekly_window_minutes: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weekly_window_present: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexQuotaError {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub message: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AccountIndex {
    version: String,
    accounts: Vec<AccountIndexEntry>,
    current_account_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AccountIndexEntry {
    id: String,
    email: String,
    plan_type: Option<String>,
    created_at: i64,
}

impl Default for AccountIndex {
    fn default() -> Self {
        Self {
            version: "1.0".to_string(),
            accounts: Vec::new(),
            current_account_id: None,
        }
    }
}

// ── Paths ───────────────────────────────────────────────────────────

fn accounts_dir() -> PathBuf {
    paths::config_dir().join("codex_accounts")
}

fn index_path() -> PathBuf {
    paths::config_dir().join("codex_accounts_index.json")
}

fn account_file_path(account_id: &str) -> PathBuf {
    accounts_dir().join(format!("{}.json", account_id))
}

// ── Index Operations ────────────────────────────────────────────────

fn read_index() -> Result<AccountIndex> {
    let path = index_path();
    if !path.exists() {
        return Ok(AccountIndex::default());
    }
    let content = std::fs::read_to_string(&path).context("Failed to read accounts index")?;
    serde_json::from_str(&content).context("Failed to parse accounts index")
}

fn save_index(index: &AccountIndex) -> Result<()> {
    let dir = paths::config_dir();
    std::fs::create_dir_all(&dir)?;
    let content = serde_json::to_string_pretty(index)?;
    super::atomic_write(&index_path(), content.as_bytes()).context("Failed to write accounts index")
}

// ── Account CRUD ────────────────────────────────────────────────────

pub fn save_account(account: &CodexAccount) -> Result<()> {
    let dir = accounts_dir();
    std::fs::create_dir_all(&dir)?;

    let content = serde_json::to_string_pretty(account)?;
    super::atomic_write(&account_file_path(&account.id), content.as_bytes())
        .context("Failed to write account")?;

    // Update index
    let mut index = read_index()?;
    let found = index.accounts.iter_mut().find(|a| a.id == account.id);
    if let Some(entry) = found {
        entry.email = account.email.clone();
        entry.plan_type = account.plan_type.clone();
    } else {
        index.accounts.push(AccountIndexEntry {
            id: account.id.clone(),
            email: account.email.clone(),
            plan_type: account.plan_type.clone(),
            created_at: account.created_at,
        });
    }
    save_index(&index)?;

    Ok(())
}

pub fn load_account(account_id: &str) -> Option<CodexAccount> {
    let path = account_file_path(account_id);
    if !path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

pub fn list_accounts() -> Vec<CodexAccount> {
    let index = read_index().unwrap_or_default();
    let mut accounts = Vec::new();
    for entry in &index.accounts {
        if let Some(account) = load_account(&entry.id) {
            accounts.push(account);
        }
    }
    accounts
}

pub fn get_current_account_id() -> Option<String> {
    read_index().ok()?.current_account_id
}

/// Clear the current OAuth account selection.
/// Used when an API provider takes over as the active auth method,
/// so OAuth and provider "当前" badges are mutually exclusive.
pub fn clear_current_account() -> Result<()> {
    let mut index = read_index()?;
    index.current_account_id = None;
    save_index(&index)
}

pub fn delete_account(account_id: &str) -> Result<()> {
    let path = account_file_path(account_id);
    if path.exists() {
        std::fs::remove_file(&path)?;
    }

    let mut index = read_index()?;
    index.accounts.retain(|a| a.id != account_id);
    if index.current_account_id.as_deref() == Some(account_id) {
        index.current_account_id = index.accounts.first().map(|a| a.id.clone());
    }
    save_index(&index)?;

    Ok(())
}

/// Switch the active Codex account. Writes tokens into `~/.codex/auth.json`.
/// Also clears any active provider card selection and manages provider pointers
/// in config.toml based on auth mode.
pub fn switch_account(account_id: &str) -> Result<CodexAccount> {
    let account =
        load_account(account_id).ok_or_else(|| anyhow::anyhow!("账号不存在: {}", account_id))?;

    let codex_home = super::codex::config_dir();

    // Update index
    let mut index = read_index()?;
    index.current_account_id = Some(account_id.to_string());
    save_index(&index)?;

    // Write to ~/.codex/auth.json so the Codex CLI can use this session
    write_tokens_to_codex_auth(&account)?;

    // Write macOS Keychain (new Codex CLI reads credentials from Keychain)
    #[cfg(target_os = "macos")]
    if let Err(e) = write_codex_keychain(&codex_home, &account) {
        tracing::warn!("Failed to write Codex keychain: {}", e);
    }

    // Clear the provider card's current selection — OAuth is now the sole auth method.
    // This ensures no provider card shows "当前" simultaneously.
    if let Err(e) = super::providers::clear_current("codex") {
        tracing::warn!("Failed to clear codex provider current: {}", e);
    }

    // Manage config.toml based on auth mode
    if let Ok(config_text) = super::codex::read_config_text() {
        use toml_edit::DocumentMut;
        match config_text.parse::<DocumentMut>() {
            Ok(mut doc) => {
                let mut changed = false;

                if account.auth_mode == "oauth" {
                    // OAuth mode: remove third-party provider pointers so Codex CLI
                    // falls back to the official endpoint with the OAuth session.
                    for key in [
                        "model_provider",
                        "openai_base_url",
                        "base_url",
                        "model_providers",
                        "disable_response_storage",
                    ] {
                        if doc.get(key).is_some() {
                            doc.remove(key);
                            changed = true;
                        }
                    }
                } else {
                    // API key mode: write/remove openai_base_url based on account config
                    if let Some(ref base_url) = account.api_base_url {
                        let trimmed = base_url.trim().trim_end_matches('/');
                        if !trimmed.is_empty() {
                            doc["openai_base_url"] = toml_edit::value(trimmed);
                            changed = true;
                        }
                    } else if doc.get("openai_base_url").is_some() {
                        doc.remove("openai_base_url");
                        changed = true;
                    }
                    // Remove stale third-party provider pointers
                    for key in [
                        "model_provider",
                        "base_url",
                        "model_providers",
                        "disable_response_storage",
                    ] {
                        if doc.get(key).is_some() {
                            doc.remove(key);
                            changed = true;
                        }
                    }
                }

                if changed {
                    if let Err(e) = super::codex::write_config(&doc.to_string()) {
                        tracing::warn!("Failed to update config.toml: {}", e);
                    }
                }
            }
            Err(e) => {
                return Err(anyhow::anyhow!(
                    "config.toml 解析失败，请先修复语法错误: {}",
                    e
                ));
            }
        }
    }

    // Update last_used
    let mut updated = account.clone();
    updated.last_used = chrono::Utc::now().timestamp();
    save_account(&updated)?;

    Ok(updated)
}

/// Build the auth.json value in the format Codex CLI expects.
/// OAuth: nested `tokens` object; API Key: `auth_mode` + `OPENAI_API_KEY`.
fn build_auth_json_value(account: &CodexAccount) -> Result<serde_json::Value> {
    if account.auth_mode != "oauth" {
        // API Key mode
        let api_key = account
            .openai_api_key
            .as_deref()
            .filter(|k| !k.trim().is_empty())
            .ok_or_else(|| anyhow::anyhow!("API Key account missing OPENAI_API_KEY"))?;
        return Ok(serde_json::json!({
            "auth_mode": "apikey",
            "OPENAI_API_KEY": api_key,
        }));
    }

    // OAuth mode — use nested tokens structure that Codex CLI expects
    let mut tokens_obj = serde_json::json!({
        "id_token": account.tokens.id_token,
        "access_token": account.tokens.access_token,
    });
    if let Some(ref rt) = account.tokens.refresh_token {
        tokens_obj["refresh_token"] = serde_json::Value::String(rt.clone());
    }
    if let Some(ref acct_id) = account.account_id {
        tokens_obj["account_id"] = serde_json::Value::String(acct_id.clone());
    }

    Ok(serde_json::json!({
        "OPENAI_API_KEY": null,
        "tokens": tokens_obj,
        "last_refresh": chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string(),
    }))
}

/// Write account tokens to ~/.codex/auth.json for Codex CLI to use.
/// Uses the nested `tokens` format that Codex CLI expects.
fn write_tokens_to_codex_auth(account: &CodexAccount) -> Result<()> {
    let codex_dir = super::codex::config_dir();
    std::fs::create_dir_all(&codex_dir)?;
    let auth_path = super::codex::auth_json_path();

    let auth_value = build_auth_json_value(account)?;
    let json_text = serde_json::to_string_pretty(&auth_value)?;
    super::atomic_write(&auth_path, json_text.as_bytes())
        .context("Failed to write Codex auth.json")?;

    tracing::info!(
        "[Codex切号] 已写入 auth.json: account_id={}, email={}, mode={}",
        account.id,
        account.email,
        account.auth_mode
    );

    Ok(())
}

/// Write Codex credentials to macOS Keychain.
/// New Codex CLI versions read credentials from Keychain first.
#[cfg(target_os = "macos")]
fn write_codex_keychain(codex_home: &Path, account: &CodexAccount) -> Result<()> {
    use sha2::Digest;

    if account.auth_mode != "oauth" {
        return Ok(());
    }

    let resolved = std::fs::canonicalize(codex_home).unwrap_or_else(|_| codex_home.to_path_buf());
    let digest = sha2::Sha256::digest(resolved.to_string_lossy().as_bytes());
    let digest_hex: String = digest.iter().map(|b| format!("{:02x}", b)).collect();
    let keychain_account = format!("cli|{}", &digest_hex[..16]);

    let payload = build_auth_json_value(account)?;
    let secret = serde_json::to_string(&payload).context("Failed to serialize keychain payload")?;

    let output = std::process::Command::new("security")
        .args([
            "add-generic-password",
            "-U",
            "-s",
            "Codex Auth",
            "-a",
            &keychain_account,
            "-w",
            &secret,
        ])
        .output()
        .context("Failed to execute security command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::warn!(
            "[Codex切号] Keychain write failed: status={}, stderr={}",
            output.status,
            stderr.trim()
        );
    } else {
        tracing::info!(
            "[Codex切号] Keychain updated: service=Codex Auth, account={}",
            keychain_account
        );
    }

    Ok(())
}

// ── JWT Parsing ─────────────────────────────────────────────────────

/// Extract email, user_id, plan_type, and account_id from an id_token JWT.
pub fn extract_user_info(
    id_token: &str,
) -> Result<(String, Option<String>, Option<String>, Option<String>)> {
    let parts: Vec<&str> = id_token.split('.').collect();
    if parts.len() < 2 {
        return Err(anyhow::anyhow!("Invalid JWT format"));
    }

    let payload_base64 = parts[1];
    let padded = format!(
        "{}{}",
        payload_base64,
        "=".repeat((4 - payload_base64.len() % 4) % 4)
    );
    let base64_str = padded.replace('-', "+").replace('_', "/");
    let payload_bytes = base64::engine::general_purpose::STANDARD
        .decode(&base64_str)
        .context("Failed to decode JWT payload")?;
    let payload_str = String::from_utf8(payload_bytes).context("JWT payload is not UTF-8")?;
    let payload: serde_json::Value =
        serde_json::from_str(&payload_str).context("Failed to parse JWT payload")?;

    let email = payload
        .get("email")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown@email.com")
        .to_string();

    let auth_data = payload
        .get("https://api.openai.com/auth")
        .and_then(|v| v.as_object());

    let user_id = auth_data
        .and_then(|a| a.get("chatgpt_user_id"))
        .and_then(|v| v.as_str())
        .or_else(|| payload.get("sub").and_then(|v| v.as_str()))
        .map(|s| s.to_string());

    let plan_type = auth_data
        .and_then(|a| a.get("chatgpt_plan_type"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let account_id = auth_data
        .and_then(|a| a.get("account_id"))
        .or_else(|| auth_data.and_then(|a| a.get("chatgpt_account_id")))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Ok((email, user_id, plan_type, account_id))
}

/// Create a new account from OAuth tokens.
pub fn create_account_from_tokens(tokens: codex_oauth::OAuthTokens) -> Result<CodexAccount> {
    let (email, user_id, plan_type, account_id) = extract_user_info(&tokens.id_token)?;
    let now = chrono::Utc::now().timestamp();
    let id = uuid::Uuid::new_v4().to_string();

    // Check for duplicate email
    let existing = list_accounts();
    for existing_account in &existing {
        if existing_account.email == email && existing_account.auth_mode == "oauth" {
            // Update existing account
            let mut updated = existing_account.clone();
            updated.tokens = CodexTokens {
                id_token: tokens.id_token,
                access_token: tokens.access_token,
                refresh_token: tokens.refresh_token,
            };
            updated.user_id = user_id;
            updated.plan_type = plan_type;
            updated.account_id = account_id;
            updated.last_used = now;
            save_account(&updated)?;
            return Ok(updated);
        }
    }

    let account = CodexAccount {
        id,
        email,
        auth_mode: "oauth".to_string(),
        openai_api_key: None,
        api_base_url: None,
        user_id,
        plan_type,
        account_id,
        account_name: None,
        tokens: CodexTokens {
            id_token: tokens.id_token,
            access_token: tokens.access_token,
            refresh_token: tokens.refresh_token,
        },
        quota: None,
        quota_error: None,
        created_at: now,
        last_used: now,
    };

    save_account(&account)?;
    Ok(account)
}

/// Create a new API key based account.
pub fn create_api_key_account(
    api_key: String,
    api_base_url: Option<String>,
) -> Result<CodexAccount> {
    let now = chrono::Utc::now().timestamp();
    let id = uuid::Uuid::new_v4().to_string();
    let email = format!("apikey-{}", &api_key[..api_key.len().min(8)]);

    let account = CodexAccount {
        id,
        email,
        auth_mode: "apikey".to_string(),
        openai_api_key: Some(api_key),
        api_base_url,
        user_id: None,
        plan_type: Some("API_KEY".to_string()),
        account_id: None,
        account_name: None,
        tokens: CodexTokens {
            id_token: String::new(),
            access_token: String::new(),
            refresh_token: None,
        },
        quota: None,
        quota_error: None,
        created_at: now,
        last_used: now,
    };

    save_account(&account)?;
    Ok(account)
}

// ── Quota Fetching ──────────────────────────────────────────────────

const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";

#[derive(Debug, Deserialize)]
struct WindowInfo {
    used_percent: Option<i32>,
    limit_window_seconds: Option<i64>,
    reset_after_seconds: Option<i64>,
    reset_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct RateLimitInfo {
    primary_window: Option<WindowInfo>,
    secondary_window: Option<WindowInfo>,
}

#[derive(Debug, Deserialize)]
struct UsageResponse {
    plan_type: Option<String>,
    rate_limit: Option<RateLimitInfo>,
}

fn normalize_remaining_percentage(window: &WindowInfo) -> i32 {
    let used = window.used_percent.unwrap_or(0).clamp(0, 100);
    100 - used
}

fn normalize_window_minutes(window: &WindowInfo) -> Option<i64> {
    let seconds = window.limit_window_seconds?;
    if seconds <= 0 {
        return None;
    }
    Some((seconds + 59) / 60)
}

fn normalize_reset_time(window: &WindowInfo) -> Option<i64> {
    if let Some(reset_at) = window.reset_at {
        return Some(reset_at);
    }
    let reset_after_seconds = window.reset_after_seconds?;
    if reset_after_seconds < 0 {
        return None;
    }
    Some(chrono::Utc::now().timestamp() + reset_after_seconds)
}

/// Fetch quota for an account using its access token.
async fn fetch_quota_internal(
    account: &CodexAccount,
) -> Result<(CodexQuota, Option<String>), String> {
    let client = reqwest::Client::new();

    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", account.tokens.access_token))
            .map_err(|e| format!("构建 Authorization 头失败: {}", e))?,
    );
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));

    if let Some(ref acct_id) = account.account_id {
        if !acct_id.is_empty() {
            if let Ok(hv) = HeaderValue::from_str(acct_id) {
                headers.insert("ChatGPT-Account-Id", hv);
            }
        }
    }

    tracing::info!(
        "Codex 配额请求: {} (account_id: {:?})",
        USAGE_URL,
        account.account_id
    );

    let response = client
        .get(USAGE_URL)
        .headers(headers)
        .send()
        .await
        .map_err(|e| format!("请求失败: {}", e))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| format!("读取响应失败: {}", e))?;

    if !status.is_success() {
        let body_preview = &body[..body.len().min(200)];
        return Err(format!("API 返回错误 {} - {}", status, body_preview));
    }

    let usage: UsageResponse =
        serde_json::from_str(&body).map_err(|e| format!("解析 JSON 失败: {}", e))?;

    let rate_limit = usage.rate_limit.as_ref();
    let primary_window = rate_limit.and_then(|r| r.primary_window.as_ref());
    let secondary_window = rate_limit.and_then(|r| r.secondary_window.as_ref());

    let (hourly_percentage, hourly_reset_time, hourly_window_minutes) =
        if let Some(primary) = primary_window {
            (
                normalize_remaining_percentage(primary),
                normalize_reset_time(primary),
                normalize_window_minutes(primary),
            )
        } else {
            (100, None, None)
        };

    let (weekly_percentage, weekly_reset_time, weekly_window_minutes) =
        if let Some(secondary) = secondary_window {
            (
                normalize_remaining_percentage(secondary),
                normalize_reset_time(secondary),
                normalize_window_minutes(secondary),
            )
        } else {
            (100, None, None)
        };

    let quota = CodexQuota {
        hourly_percentage,
        hourly_reset_time,
        hourly_window_minutes,
        hourly_window_present: Some(primary_window.is_some()),
        weekly_percentage,
        weekly_reset_time,
        weekly_window_minutes,
        weekly_window_present: Some(secondary_window.is_some()),
    };

    Ok((quota, usage.plan_type))
}

/// Refresh quota for an account, with auto token refresh.
pub async fn refresh_account_quota(account_id: &str) -> Result<CodexQuota, String> {
    let mut account =
        load_account(account_id).ok_or_else(|| format!("账号不存在: {}", account_id))?;

    if account.auth_mode == "apikey" {
        return Err("API Key 账号不支持刷新配额".to_string());
    }

    // Check token expiry and refresh
    if codex_oauth::is_token_expired(&account.tokens.access_token) {
        let refresh_token = account
            .tokens
            .refresh_token
            .clone()
            .ok_or("账号缺少 refresh_token")?;

        let new_tokens = codex_oauth::refresh_access_token(&refresh_token)
            .await
            .map_err(|e| format!("Token 刷新失败: {}", e))?;

        account.tokens = CodexTokens {
            id_token: new_tokens.id_token,
            access_token: new_tokens.access_token,
            refresh_token: new_tokens.refresh_token,
        };

        // Re-extract plan_type from new token
        if let Ok((_, _, new_plan_type, _)) = extract_user_info(&account.tokens.id_token) {
            if new_plan_type.is_some() {
                account.plan_type = new_plan_type;
            }
        }

        save_account(&account).map_err(|e| format!("保存 Token 失败: {}", e))?;
    }

    match fetch_quota_internal(&account).await {
        Ok((quota, plan_type)) => {
            if plan_type.is_some() {
                account.plan_type = plan_type;
            }
            account.quota = Some(quota.clone());
            account.quota_error = None;
            save_account(&account).map_err(|e| format!("保存配额失败: {}", e))?;
            Ok(quota)
        }
        Err(e) => {
            // If token invalidated, try refresh once more
            if e.to_lowercase().contains("401") || e.to_lowercase().contains("unauthorized") {
                if let Some(ref refresh_token) = account.tokens.refresh_token {
                    if let Ok(new_tokens) = codex_oauth::refresh_access_token(refresh_token).await {
                        account.tokens = CodexTokens {
                            id_token: new_tokens.id_token,
                            access_token: new_tokens.access_token,
                            refresh_token: new_tokens.refresh_token,
                        };
                        save_account(&account).map_err(|e| format!("保存 Token 失败: {}", e))?;

                        match fetch_quota_internal(&account).await {
                            Ok((quota, plan_type)) => {
                                if plan_type.is_some() {
                                    account.plan_type = plan_type;
                                }
                                account.quota = Some(quota.clone());
                                account.quota_error = None;
                                save_account(&account)
                                    .map_err(|e| format!("保存配额失败: {}", e))?;
                                return Ok(quota);
                            }
                            Err(retry_err) => {
                                account.quota_error = Some(CodexQuotaError {
                                    code: None,
                                    message: retry_err.clone(),
                                    timestamp: chrono::Utc::now().timestamp(),
                                });
                                let _ = save_account(&account);
                                return Err(retry_err);
                            }
                        }
                    }
                }
            }

            account.quota_error = Some(CodexQuotaError {
                code: None,
                message: e.clone(),
                timestamp: chrono::Utc::now().timestamp(),
            });
            let _ = save_account(&account);
            Err(e)
        }
    }
}

/// Refresh all account quotas concurrently.
pub async fn refresh_all_quotas() -> Vec<(String, Result<CodexQuota, String>)> {
    let accounts: Vec<_> = list_accounts()
        .into_iter()
        .filter(|a| a.auth_mode != "apikey")
        .collect();

    let mut tasks = Vec::new();
    for account in accounts {
        let account_id = account.id.clone();
        tasks.push(tokio::spawn(async move {
            let result = refresh_account_quota(&account_id).await;
            (account_id, result)
        }));
    }

    let mut results = Vec::new();
    for task in tasks {
        match task.await {
            Ok(item) => results.push(item),
            Err(e) => tracing::error!("Quota refresh task panic: {}", e),
        }
    }
    results
}
