//! Claude Code account switching through its own credential store.
//!
//! The authoritative store is the macOS keychain item on macOS (see
//! `claude_credentials` for the addressing rules) and
//! `~/.claude/.credentials.json` (or `$CLAUDE_CONFIG_DIR/.credentials.json`)
//! everywhere else. [D-083](docs/decisions.md) grants exactly one keychain
//! write — Claude Code's own item — as a scoped exception to D-072; no other
//! keychain item is ever read or written.
//!
//! Switching is a read-modify-write that replaces only the `claudeAiOauth`
//! key and preserves every other key in the blob (the same JSON carries
//! `mcpOAuth` and account-scoped entries), followed by a read-back. The pin
//! moves only after the read-back matches, and a verified macOS switch also
//! deletes the stale plaintext mirror, exactly like the CLI's own migration.
//!
//! Anthropic's refresh token is single-use, and Claude Code itself is the one
//! spending it: the live store is fresher than any snapshot, so
//! `adopt_before_refresh` absorbs the live generation into the row before a
//! refresh, and reconcile repairs the pin from the store — never the other
//! way round.

use crate::crypto;
use crate::subscription::Subscription;
use crate::{UsageError, UsageResult, storage, tool_paths};

use super::ide::IdeCredentialAdapter;
use super::{CliAccountState, SwitchOutcome};
use crate::claude_credentials::{self, SecurityRunner};

pub(super) const CATALOG_ID: &str = "anthropic";

const SANDBOX_UNAVAILABLE: &str =
    "SKILLSTAR_TOOL_SYNC_HOME 已设置，拒绝访问 macOS 钥匙串；沙箱内不提供 Claude 账号切换";

pub(super) struct Adapter;

impl IdeCredentialAdapter for Adapter {
    fn catalog_id(&self) -> &'static str {
        CATALOG_ID
    }

    /// The live store is addressable. On macOS that is the login keychain —
    /// a global store, so it stays off inside the `SKILLSTAR_TOOL_SYNC_HOME`
    /// sandbox (and every real keychain IO re-checks). Off macOS the file
    /// store resolves under `CLAUDE_CONFIG_DIR` / the redirected home, so it
    /// stays available in that sandbox.
    fn available(&self) -> bool {
        if cfg!(target_os = "macos") {
            !tool_paths::is_tool_sync_sandboxed()
        } else {
            true
        }
    }

    fn activate(&self, sub_id: &str) -> UsageResult<(Subscription, SwitchOutcome)> {
        if !self.available() {
            let subscription = storage::get_subscription(sub_id)?;
            return Ok((subscription, failed(keychain_display(), SANDBOX_UNAVAILABLE)));
        }
        if cfg!(target_os = "macos") {
            activate_keychain(sub_id, &claude_credentials::RealSecurity)
        } else {
            activate_file(sub_id)
        }
    }

    fn sync(&self, sub: &Subscription) -> UsageResult<SwitchOutcome> {
        if !self.available() {
            return Ok(failed(keychain_display(), SANDBOX_UNAVAILABLE));
        }
        let display = if cfg!(target_os = "macos") {
            keychain_display()
        } else {
            file_display()
        };
        let written = if cfg!(target_os = "macos") {
            write_verified_keychain(sub, &claude_credentials::RealSecurity).map(|()| true)
        } else {
            write_verified_file(sub).map(|()| false)
        };
        match written {
            Ok(keychain_updated) => Ok(succeeded(display, keychain_updated)),
            Err(error) => Ok(failed(display, error.to_string())),
        }
    }

    fn reconcile(&self) -> UsageResult<Option<CliAccountState>> {
        if !self.available() {
            return Ok(None);
        }
        let live = if cfg!(target_os = "macos") {
            read_live_oauth_keychain(&claude_credentials::RealSecurity)
        } else {
            read_live_oauth_file()
        };
        reconcile_with(live).map(Some)
    }

    fn adopt_before_refresh(&self, sub: &mut Subscription) -> UsageResult<()> {
        if !self.available() || sub.catalog_id != CATALOG_ID {
            return Ok(());
        }
        let live = if cfg!(target_os = "macos") {
            read_live_oauth_keychain(&claude_credentials::RealSecurity)
        } else {
            read_live_oauth_file()
        };
        absorb_live(sub, live)
    }

    fn forget(&self, _sub_id: &str) -> UsageResult<()> {
        // No snapshot to drop, and deleting the live credential would log the
        // CLI out entirely — forgetting a card must not log anybody out.
        Ok(())
    }
}

// ── shared blob model ────────────────────────────────────────────────────

/// The `claudeAiOauth` slice SkillStar writes; every other key in the blob is
/// preserved verbatim.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
struct ClaudeOAuthFile {
    #[serde(rename = "accessToken", skip_serializing_if = "Option::is_none")]
    access_token: Option<String>,
    #[serde(rename = "refreshToken", skip_serializing_if = "Option::is_none")]
    refresh_token: Option<String>,
    /// Epoch **milliseconds**, Claude Code's own unit.
    #[serde(rename = "expiresAt", skip_serializing_if = "Option::is_none")]
    expires_at_ms: Option<i64>,
    #[serde(rename = "subscriptionType", skip_serializing_if = "Option::is_none")]
    subscription_type: Option<String>,
}

impl ClaudeOAuthFile {
    fn expires_at_seconds(&self) -> Option<i64> {
        self.expires_at_ms.filter(|ms| *ms > 0).map(|ms| ms / 1_000)
    }
}

struct LiveOAuth {
    access_token: Option<String>,
    expires_at_seconds: Option<i64>,
}

fn live_oauth_from_blob(blob: &serde_json::Value) -> Option<LiveOAuth> {
    let parsed: ClaudeOAuthFile = serde_json::from_value(blob.get("claudeAiOauth")?.clone()).ok()?;
    let access_token = parsed
        .access_token
        .as_deref()
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::to_string);
    Some(LiveOAuth {
        access_token,
        expires_at_seconds: parsed.expires_at_seconds(),
    })
}

// ── file store (authoritative off macOS) ─────────────────────────────────

fn activate_file(subscription_id: &str) -> UsageResult<(Subscription, SwitchOutcome)> {
    let subscription = storage::get_subscription(subscription_id)?;
    match write_verified_file(&subscription) {
        Ok(()) => {
            storage::set_active_subscription(&subscription.catalog_id, &subscription.id)?;
            Ok((subscription, succeeded(file_display(), false)))
        }
        Err(error) => Ok((subscription, failed(file_display(), error.to_string()))),
    }
}

fn read_live_oauth_file() -> Option<LiveOAuth> {
    claude_credentials::read_file_blob()
        .ok()
        .flatten()
        .as_ref()
        .and_then(live_oauth_from_blob)
}

/// Replace only the `claudeAiOauth` key, keeping every other key (notably
/// `mcpOAuth`) exactly as stored.
fn write_oauth_file(oauth: &ClaudeOAuthFile) -> UsageResult<()> {
    let mut root = claude_credentials::read_file_blob()?
        .unwrap_or_else(|| serde_json::Value::Object(Default::default()));
    if !root.is_object() {
        return Err(UsageError::Other(
            "Claude 凭证文件顶层不是 JSON 对象，拒绝改写".into(),
        ));
    }
    root.as_object_mut()
        .expect("checked above")
        .insert(
            "claudeAiOauth".to_string(),
            serde_json::to_value(oauth).map_err(|error| UsageError::Other(error.to_string()))?,
        );
    let text = serde_json::to_string_pretty(&root)
        .map_err(|error| UsageError::Other(error.to_string()))?;
    let path = claude_credentials::credentials_file_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| UsageError::Other(format!("创建 Claude 配置目录失败：{error}")))?;
    }
    std::fs::write(&path, format!("{text}\n"))
        .map_err(|error| UsageError::Other(format!("写入 Claude 凭证文件失败：{error}")))
}

fn write_verified_file(subscription: &Subscription) -> UsageResult<()> {
    let oauth = oauth_of(subscription)?;
    write_oauth_file(&oauth)?;
    // Read back: the switch is real only if the file now serves this row.
    let live = read_live_oauth_file()
        .ok_or_else(|| UsageError::Other("Claude 凭证文件回读失败，切换未生效".into()))?;
    if live.access_token.as_deref() != oauth.access_token.as_deref() {
        return Err(UsageError::Other(
            "Claude 凭证文件回读与写入不一致，切换未生效".into(),
        ));
    }
    Ok(())
}

// ── keychain store (authoritative on macOS, D-083 exception) ─────────────

fn activate_keychain(
    subscription_id: &str,
    runner: &dyn SecurityRunner,
) -> UsageResult<(Subscription, SwitchOutcome)> {
    let subscription = storage::get_subscription(subscription_id)?;
    match write_verified_keychain(&subscription, runner) {
        Ok(()) => {
            storage::set_active_subscription(&subscription.catalog_id, &subscription.id)?;
            Ok((subscription, succeeded(keychain_display(), true)))
        }
        Err(error) => Ok((subscription, failed(keychain_display(), error.to_string()))),
    }
}

/// Keychain-first live read — the same order login adoption reads with — so
/// reconcile and adopt see exactly what the CLI sees.
fn read_live_oauth_keychain(runner: &dyn SecurityRunner) -> Option<LiveOAuth> {
    if let Ok(Some(blob)) = claude_credentials::read_keychain_blob(runner)
        && let Some(live) = live_oauth_from_blob(&blob)
    {
        return Some(live);
    }
    read_live_oauth_file()
}

fn write_verified_keychain(
    subscription: &Subscription,
    runner: &dyn SecurityRunner,
) -> UsageResult<()> {
    if subscription.catalog_id != CATALOG_ID {
        return Err(UsageError::Other(
            "Claude 切换收到了其它 catalog 的订阅".into(),
        ));
    }
    let oauth = oauth_of(subscription)?;
    // Merge base: the keychain item first; when it is absent, the plaintext
    // file (the pre-migration store), so its `mcpOAuth` siblings survive the
    // migration into the item instead of being dropped.
    let mut root = match claude_credentials::read_keychain_blob(runner)? {
        Some(blob) => blob,
        None => claude_credentials::read_file_blob()?
            .unwrap_or_else(|| serde_json::Value::Object(Default::default())),
    };
    if !root.is_object() {
        return Err(UsageError::Other(
            "Claude 钥匙串凭证顶层不是 JSON 对象，拒绝改写".into(),
        ));
    }
    root.as_object_mut()
        .expect("checked above")
        .insert(
            "claudeAiOauth".to_string(),
            serde_json::to_value(&oauth).map_err(|error| UsageError::Other(error.to_string()))?,
        );
    claude_credentials::write_keychain_blob(runner, &root)?;
    // Read back: the switch is real only if the item now serves this row.
    let live = claude_credentials::read_keychain_blob(runner)?
        .as_ref()
        .and_then(live_oauth_from_blob)
        .ok_or_else(|| UsageError::Other("Claude 钥匙串回读失败，切换未生效".into()))?;
    if live.access_token.as_deref() != oauth.access_token.as_deref() {
        return Err(UsageError::Other(
            "Claude 钥匙串回读与写入不一致，切换未生效".into(),
        ));
    }
    // The plaintext file is a stale mirror once the item holds the login;
    // Claude Code deletes it after migrating, and so does a verified switch.
    let _ = std::fs::remove_file(claude_credentials::credentials_file_path());
    Ok(())
}

// ── row ↔ blob mapping and shared verdicts ───────────────────────────────

fn token_of(subscription: &Subscription) -> Option<String> {
    subscription
        .access_token_encrypted
        .as_deref()
        .map(crypto::decrypt)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn refresh_of(subscription: &Subscription) -> Option<String> {
    subscription
        .refresh_token_encrypted
        .as_deref()
        .map(crypto::decrypt)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn oauth_of(subscription: &Subscription) -> UsageResult<ClaudeOAuthFile> {
    let access_token = token_of(subscription).ok_or_else(|| {
        UsageError::Other("Claude 账号缺少 access_token，切换未生效".into())
    })?;
    Ok(ClaudeOAuthFile {
        access_token: Some(access_token),
        refresh_token: refresh_of(subscription),
        expires_at_ms: subscription.access_token_expires_at.map(|s| s * 1_000),
        subscription_type: subscription
            .plan_tier
            .clone()
            .map(|tier| tier.to_ascii_lowercase()),
    })
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

fn reconcile_with(live: Option<LiveOAuth>) -> UsageResult<CliAccountState> {
    let Some(live_token) = live.and_then(|live| live.access_token) else {
        return Ok(CliAccountState::Missing);
    };
    let Some(subscription) = pinned_row()? else {
        return Ok(CliAccountState::Diverged);
    };
    if token_of(&subscription).as_deref() == Some(live_token.as_str()) {
        Ok(CliAccountState::LinkedTo {
            subscription_id: subscription.id,
        })
    } else {
        Ok(CliAccountState::Diverged)
    }
}

/// Live-first: whatever Claude Code rotated into the store wins over the
/// row's copy, exactly like `fetch_inner` already does on every read.
fn absorb_live(subscription: &mut Subscription, live: Option<LiveOAuth>) -> UsageResult<()> {
    if let Some(live) = live
        && token_of(subscription).as_deref() != live.access_token.as_deref()
    {
        let mut updated = subscription.clone();
        updated.access_token_encrypted = live.access_token.map(|token| crypto::encrypt(&token));
        if let Some(expires_s) = live.expires_at_seconds {
            updated.access_token_expires_at = Some(expires_s);
        }
        *subscription = storage::patch_oauth_credentials(&updated)?;
    }
    Ok(())
}

fn file_display() -> String {
    claude_credentials::credentials_file_path().to_string_lossy().to_string()
}

fn keychain_display() -> String {
    format!("keychain:{}", claude_credentials::keychain_service())
}

fn failed(display: String, reason: impl Into<String>) -> SwitchOutcome {
    SwitchOutcome {
        tool_id: CATALOG_ID.to_string(),
        config_path: display,
        backup_path: None,
        keychain_updated: false,
        link_mode: None,
        success: false,
        error: Some(reason.into()),
    }
}

fn succeeded(display: String, keychain_updated: bool) -> SwitchOutcome {
    SwitchOutcome {
        tool_id: CATALOG_ID.to_string(),
        config_path: display,
        backup_path: None,
        keychain_updated,
        link_mode: None,
        success: true,
        error: None,
    }
}

#[cfg(test)]
#[path = "claude_tests.rs"]
mod tests;
