//! Anthropic Claude subscription quota fetcher.
//!
//! Unlike the other OAuth fetchers there is **no browser leg and no token
//! refresh here**, and that is deliberate:
//!
//! - Claude Code already owns a login (macOS keychain `Claude Code-credentials`,
//!   `~/.claude/.credentials.json` elsewhere) and rotates it on its own.
//! - Anthropic's refresh token is single-use, so whichever side spends it
//!   revokes the other. Minting our own pair would log the user's CLI out.
//!   We therefore only ever *read* that store and adopt whatever pair it holds
//!   — the "adopt the fresher pair" rule, never "win the rotation race".
//! - Nothing is ever written back: no keychain read-modify-write, so the
//!   `mcpOAuth` block sharing that keychain item cannot be clobbered.
//!
//! `start_login` consequently completes synchronously: it adopts the local
//! credential and resolves the pending-login channel immediately, so the
//! existing dialog flow works unchanged.
//!
//! Quota endpoint: `GET https://api.anthropic.com/api/oauth/usage` with
//! `Authorization: Bearer <access_token>` + `anthropic-beta: oauth-2025-04-20`.
//! It is not publicly documented, so [`parse_usage`] degrades **per window**:
//! anything it cannot read becomes a missing bar, never a failed account.

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;
use std::sync::LazyLock;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

use super::common::SubscriptionBuilder;
use crate::oauth::pending_state;
use crate::request::{Req, RequestError};
use crate::storage;
use crate::subscription::{Subscription, SubscriptionUsage, UsageWindow};
use crate::{UsageError, UsageResult};

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const BETA_HEADER: &str = "oauth-2025-04-20";
const ACCOUNT_URL: &str = "https://claude.ai/settings/usage";
const LABEL: &str = "Claude oauth/usage";
const CATALOG_ID: &str = "anthropic";

/// Title strings a fresh login is allowed to overwrite (see `common.rs`).
const ANTHROPIC_TITLE_PLACEHOLDERS: &[&str] = &["Claude", "Anthropic"];

/// Minimum spacing between two calls to `api.anthropic.com`. Multiple accounts
/// queue on the same gate, so a five-account refresh paces itself instead of
/// bursting.
const HOST_MIN_GAP: Duration = Duration::from_secs(5);

/// macOS keychain item Claude Code stores its credential blob under.
const KEYCHAIN_SERVICE: &str = "Claude Code-credentials";
/// Account label Claude Code falls back to when it cannot read the OS login name.
const KEYCHAIN_ACCOUNT_FALLBACK: &str = "claude-code-user";

// ── credential store ────────────────────────────────────────────────────

/// The one field group SkillStar consumes from Claude Code's credential blob.
///
/// The same JSON object also carries `mcpOAuth` and several account-scoped
/// keys; they are deliberately left unparsed so this type can never be used to
/// round-trip (and therefore drop) them.
#[derive(Debug, Clone, Deserialize, Default, PartialEq)]
struct ClaudeCredentials {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: Option<ClaudeOAuth>,
}

#[derive(Debug, Clone, Deserialize, Default, PartialEq)]
struct ClaudeOAuth {
    #[serde(rename = "accessToken", default)]
    access_token: Option<String>,
    /// **Epoch milliseconds** — Claude Code differs from every other provider
    /// in this crate, which all use epoch seconds. Convert with
    /// [`ClaudeOAuth::expires_at_seconds`], never by using this field directly.
    #[serde(rename = "expiresAt", default)]
    expires_at_ms: Option<i64>,
    #[serde(rename = "subscriptionType", default)]
    subscription_type: Option<String>,
}

impl ClaudeOAuth {
    fn access_token(&self) -> Option<&str> {
        self.access_token
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
    }

    /// `expiresAt` normalized to the epoch **seconds** every `Subscription`
    /// timestamp in this crate uses.
    fn expires_at_seconds(&self) -> Option<i64> {
        self.expires_at_ms.filter(|ms| *ms > 0).map(|ms| ms / 1_000)
    }

    /// `max` / `pro` → `MAX` / `PRO`, matching the other fetchers' plan names.
    fn plan_name(&self) -> Option<String> {
        self.subscription_type
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_ascii_uppercase())
    }
}

fn parse_credentials(blob: &str) -> Option<ClaudeOAuth> {
    serde_json::from_str::<ClaudeCredentials>(blob)
        .ok()?
        .claude_ai_oauth
        .filter(|oauth| oauth.access_token().is_some())
}

/// OS login name Claude Code keys its keychain item by.
fn keychain_account() -> String {
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

fn credentials_file_path() -> std::path::PathBuf {
    skillstar_core::infra::paths::home_dir()
        .join(".claude")
        .join(".credentials.json")
}

/// Read Claude Code's credential blob: keychain first on macOS (where the file
/// is only a stale mirror the CLI deletes after migrating), file everywhere else.
fn read_local_credentials() -> Option<ClaudeOAuth> {
    #[cfg(target_os = "macos")]
    {
        if let Some(oauth) = read_keychain_credentials() {
            return Some(oauth);
        }
    }
    let blob = std::fs::read_to_string(credentials_file_path()).ok()?;
    parse_credentials(&blob)
}

/// Shell out to `/usr/bin/security` rather than linking `security-framework`:
/// keychain ACL grants are bound to the *calling binary's* signature, and a
/// rebuilt `skillstar` would silently lose an entitlement granted to the old one.
#[cfg(target_os = "macos")]
fn read_keychain_credentials() -> Option<ClaudeOAuth> {
    let output = std::process::Command::new("/usr/bin/security")
        .arg("find-generic-password")
        .arg("-s")
        .arg(KEYCHAIN_SERVICE)
        .arg("-a")
        .arg(keychain_account())
        .arg("-w")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_credentials(String::from_utf8_lossy(&output.stdout).trim())
}

// ── login (local adoption) ──────────────────────────────────────────────

/// "Log in" by adopting Claude Code's existing credential.
///
/// Resolves the pending-login channel before returning, so the dialog's
/// `await_oauth_completion` succeeds immediately and no browser round trip is
/// needed. Failure surfaces here (not as a stuck "waiting" panel), because the
/// user's fix — run `claude` once — is local.
pub async fn start_login(
    _region: Option<&str>,
    target_subscription_id: Option<&str>,
) -> UsageResult<super::OAuthStartInfo> {
    let target = target_subscription_id.map(str::to_string);
    // Same serialization domain as a refresh: adopting writes the row.
    let subscription = crate::refresh_guard::with_catalog_lock(CATALOG_ID, || async {
        adopt_local_credentials(target.as_deref()).await
    })
    .await??;

    let pending_id = pending_state::register(CATALOG_ID, None, ACCOUNT_URL.to_string());
    if let Some(tx) = pending_state::take_sender(&pending_id) {
        let _ = tx.send(Ok(subscription));
    }
    Ok(super::OAuthStartInfo::browser(
        ACCOUNT_URL.to_string(),
        pending_id,
    ))
}

async fn adopt_local_credentials(
    target_subscription_id: Option<&str>,
) -> UsageResult<Subscription> {
    let oauth = read_local_credentials().ok_or_else(|| {
        UsageError::Other(
            "未找到 Claude Code 登录态：请先在终端运行一次 `claude` 完成登录，再回来绑定。".into(),
        )
    })?;
    let access_token = oauth
        .access_token()
        .ok_or_else(|| UsageError::Other("Claude 本地凭证缺少 accessToken".into()))?
        .to_string();

    let mut sub = SubscriptionBuilder::new(
        CATALOG_ID,
        "Claude",
        "USD",
        &access_token,
        oauth.expires_at_seconds(),
    )
    .build();
    // No refresh token is stored on purpose: see the module docs — spending it
    // would revoke Claude Code's own session.
    sub.plan_tier = oauth.plan_name();

    if let Some(existing) = super::common::reauth_target(CATALOG_ID, target_subscription_id) {
        super::common::carry_over_user_metadata(&mut sub, &existing, ANTHROPIC_TITLE_PLACEHOLDERS);
    }

    if let Ok(usage) = fetch_with_token(&sub.id, &access_token, oauth.plan_name()).await {
        storage::save_usage_snapshot(usage).ok();
    }
    storage::upsert_subscription(sub)
        .map_err(|e| UsageError::Other(format!("Claude 订阅保存失败：{e}")))
}

// ── refresh ─────────────────────────────────────────────────────────────

super::common::impl_oauth_fetch!();

async fn fetch_inner(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    // Claude Code's store is authoritative and always fresher than our copy,
    // so read it every time and adopt what it holds. Only when it is gone do we
    // fall back to the token captured at bind time.
    let local = read_local_credentials();
    let (access_token, plan_name) = match local.as_ref().and_then(|o| o.access_token()) {
        Some(token) => {
            subscription.access_token_encrypted = Some(crate::crypto::encrypt(token));
            let oauth = local.as_ref().expect("access_token implies credentials");
            subscription.access_token_expires_at = oauth.expires_at_seconds();
            if let Some(plan) = oauth.plan_name() {
                subscription.plan_tier = Some(plan);
            }
            (token.to_string(), oauth.plan_name())
        }
        None => (
            crate::fetchers::decrypt_required(
                &subscription.access_token_encrypted,
                "access_token",
            )?,
            subscription.plan_tier.clone(),
        ),
    };

    fetch_with_token(&subscription.id, &access_token, plan_name).await
}

/// Serializes every call to `api.anthropic.com` and spaces them by
/// [`HOST_MIN_GAP`]. Held across the request so the gap really is *between
/// adjacent requests*, not merely between their start times.
static HOST_GATE: LazyLock<Mutex<Option<Instant>>> = LazyLock::new(|| Mutex::new(None));

async fn with_host_gap<T>(work: impl std::future::Future<Output = T>) -> T {
    let mut last = HOST_GATE.lock().await;
    if let Some(previous) = *last {
        let elapsed = previous.elapsed();
        if elapsed < HOST_MIN_GAP {
            tokio::time::sleep(HOST_MIN_GAP - elapsed).await;
        }
    }
    let output = work.await;
    *last = Some(Instant::now());
    output
}

async fn fetch_with_token(
    subscription_id: &str,
    access_token: &str,
    plan_name: Option<String>,
) -> UsageResult<SubscriptionUsage> {
    let client = crate::fetchers::http_client()?;
    let response = with_host_gap(
        Req::get(&client, USAGE_URL)
            .bearer(access_token)
            .header("anthropic-beta", BETA_HEADER)
            .header("Accept", "application/json")
            .timeout(Duration::from_secs(20))
            .send(),
    )
    .await
    .map_err(classify_transport)?;

    // The single grading table from `docs/features/usage/README.md`: 401 is the
    // only auth failure, 429/5xx stay retryable via `UsageError::http_status`.
    if response.is_auth_error() {
        return Err(UsageError::AuthRequired);
    }
    if !response.is_success() {
        return Err(UsageError::http_status(
            LABEL,
            response.status,
            &response.body,
        ));
    }

    let body: Value = serde_json::from_str(&response.body)
        .map_err(|e| UsageError::Fetcher(format!("Claude 解析 usage: {e}")))?;
    let windows = parse_usage(&body);

    Ok(SubscriptionUsage {
        subscription_id: subscription_id.to_string(),
        fetched_at: Utc::now().timestamp(),
        plan_name,
        hourly: windows.hourly,
        weekly: windows.weekly,
        monthly: None,
        balance: None,
        credits: Vec::new(),
        error: None,
        api_keys: Vec::new(),
        deepseek_analytics: None,
    })
}

/// `Req::send` only fails at the transport layer, but route it through the
/// shared `is_transient` verdict anyway so a future non-transport variant can
/// never silently become a "provider outage".
fn classify_transport(error: RequestError) -> UsageError {
    if error.is_transient() {
        UsageError::transport(LABEL, error)
    } else {
        UsageError::Fetcher(format!("{LABEL}: {error}"))
    }
}

// ── response parsing ────────────────────────────────────────────────────

/// The whole quota model: one rate-limit window is a percent plus a reset time.
#[derive(Debug, Default)]
struct ParsedUsage {
    hourly: Option<UsageWindow>,
    weekly: Option<UsageWindow>,
}

/// Read the usage payload, preferring the newer `limits[]` array over the older
/// top-level `five_hour` / `seven_day` objects.
///
/// Every extraction is independent and optional. An unreadable window (unknown
/// `kind`, missing percent, a `used_dollars` that is a string this week and a
/// number next week) drops that one bar; it never fails the account, because
/// this endpoint is undocumented and its shape has already migrated once.
fn parse_usage(body: &Value) -> ParsedUsage {
    let mut session = None;
    let mut weekly_all = None;
    let mut scoped: Vec<UsageWindow> = Vec::new();

    if let Some(limits) = body.get("limits").and_then(Value::as_array) {
        for limit in limits {
            match limit.get("kind").and_then(Value::as_str) {
                Some("session") if session.is_none() => {
                    session = limit_window(limit, "5h".to_string());
                }
                Some("weekly_all") if weekly_all.is_none() => {
                    weekly_all = limit_window(limit, "7d".to_string());
                }
                Some("weekly_scoped") => {
                    if let Some(label) = scope_label(limit) {
                        scoped.extend(limit_window(limit, label));
                    }
                }
                // Unknown / repeated / absent kinds are skipped, not fatal.
                _ => {}
            }
        }
    }

    let hourly = session.or_else(|| top_level_window(body, "five_hour", "5h"));
    let mut weekly = weekly_all.or_else(|| top_level_window(body, "seven_day", "7d"));

    // Per-model weekly limits hang off the weekly bar as a breakdown. With no
    // weekly total to hang them on, the first one becomes the bar itself (the
    // slot name never reaches the UI — its `label` does).
    match weekly.as_mut() {
        Some(parent) => {
            // A scoped limit that resets with its parent carries no new
            // information, and the breakdown rows each draw their own reset
            // chip — so three identical countdowns would stack on one card.
            // A genuinely different reset survives and stays visible.
            for child in &mut scoped {
                if child.reset_at == parent.reset_at {
                    child.reset_at = None;
                }
            }
            parent.breakdown = scoped;
        }
        None if !scoped.is_empty() => {
            let mut head = scoped.remove(0);
            head.breakdown = scoped;
            weekly = Some(head);
        }
        None => {}
    }

    ParsedUsage { hourly, weekly }
}

fn limit_window(limit: &Value, label: String) -> Option<UsageWindow> {
    let percent = lenient_percent(limit.get("percent"))
        .or_else(|| lenient_percent(limit.get("utilization")))?;
    Some(percent_window(
        label,
        percent,
        lenient_reset_at(limit.get("resets_at")),
    ))
}

fn top_level_window(body: &Value, key: &str, label: &str) -> Option<UsageWindow> {
    let window = body.get(key)?;
    let percent = lenient_percent(window.get("utilization"))
        .or_else(|| lenient_percent(window.get("percent")))?;
    Some(percent_window(
        label.to_string(),
        percent,
        lenient_reset_at(window.get("resets_at")),
    ))
}

/// Display name of a `weekly_scoped` limit's model. Without one there is no
/// honest label for the bar, so the caller drops that window.
fn scope_label(limit: &Value) -> Option<String> {
    let model = limit.get("scope")?.get("model")?;
    ["display_name", "name"]
        .iter()
        .find_map(|key| model.get(*key).and_then(Value::as_str))
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
}

fn percent_window(label: String, percent: i32, reset_at: Option<i64>) -> UsageWindow {
    UsageWindow {
        label,
        used: percent as i64,
        total: Some(100),
        percent: Some(percent),
        reset_at,
        breakdown: Vec::new(),
    }
}

/// A 0–100 share that may arrive as a number or a numeric string.
fn lenient_percent(value: Option<&Value>) -> Option<i32> {
    let raw = match value? {
        Value::Number(number) => number.as_f64()?,
        Value::String(text) => text.trim().parse::<f64>().ok()?,
        _ => return None,
    };
    if !raw.is_finite() {
        return None;
    }
    Some(raw.round().clamp(0.0, 100.0) as i32)
}

/// Epoch seconds from an RFC 3339 string, epoch seconds, or epoch milliseconds.
fn lenient_reset_at(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::String(text) => DateTime::parse_from_rfc3339(text.trim())
            .ok()
            .map(|parsed| parsed.timestamp()),
        Value::Number(number) => {
            let raw = number.as_i64()?;
            // Anything past year 10000 in seconds is really milliseconds.
            Some(if raw > 253_402_300_799 {
                raw / 1_000
            } else {
                raw
            })
        }
        _ => None,
    }
}

#[cfg(test)]
#[path = "anthropic_tests.rs"]
mod tests;
