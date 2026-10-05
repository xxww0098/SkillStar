//! Claude Code account switching through the credentials **file**.
//!
//! Claude Code's authoritative store is the macOS keychain on macOS and
//! `~/.claude/.credentials.json` (or `$CLAUDE_CONFIG_DIR/.credentials.json`)
//! everywhere else. [D-072](docs/decisions.md) forbids SkillStar from writing
//! the system keychain, so on macOS this adapter is **unavailable**: switching
//! there would mean rewriting `Claude Code-credentials`, and writing the file
//! nobody reads would be a lie the UI shows as a successful switch. The
//! macOS path stays read-only adoption (see `fetchers::oauth::anthropic`).
//!
//! Where the file *is* the store, switching is a read-modify-write that
//! replaces only the `claudeAiOauth` key and preserves every other key in
//! the JSON (the same blob carries `mcpOAuth` and account-scoped entries),
//! followed by a read-back. The pin moves only after the read-back matches.
//!
//! Anthropic's refresh token is single-use, and on this platform Claude Code
//! itself is the one spending it: the live file is fresher than any snapshot,
//! so `adopt_before_refresh` absorbs the live generation into the row before
//! a refresh, and reconcile repairs the pin from the file — never the other
//! way round.

use std::path::PathBuf;

use crate::crypto;
use crate::subscription::Subscription;
use crate::{UsageError, UsageResult, storage};

use super::ide::IdeCredentialAdapter;
use super::{CliAccountState, SwitchOutcome};

pub(super) const CATALOG_ID: &str = "anthropic";

const MACOS_UNAVAILABLE: &str = "macOS 上 Claude Code 的凭证存于系统钥匙串；受 D-072（禁止写入系统钥匙串）约束，本平台不提供 Claude 账号切换，可继续使用只读绑定";

pub(super) struct Adapter;

impl IdeCredentialAdapter for Adapter {
    fn catalog_id(&self) -> &'static str {
        CATALOG_ID
    }

    /// The file store is addressable off macOS. `SKILLSTAR_TOOL_SYNC_HOME`
    /// redirects the home the file resolves under, so the sandbox is safe and
    /// stays available (unlike a global store such as the keychain).
    fn available(&self) -> bool {
        !cfg!(target_os = "macos")
    }

    fn activate(&self, sub_id: &str) -> UsageResult<(Subscription, SwitchOutcome)> {
        let subscription = storage::get_subscription(sub_id)?;
        if !self.available() {
            return Ok((subscription, failed(MACOS_UNAVAILABLE)));
        }
        match write_verified(&subscription) {
            Ok(()) => {
                storage::set_active_subscription(&subscription.catalog_id, &subscription.id)?;
                Ok((subscription, succeeded()))
            }
            Err(error) => Ok((subscription, failed(error.to_string()))),
        }
    }

    fn sync(&self, sub: &Subscription) -> UsageResult<SwitchOutcome> {
        if !self.available() {
            return Ok(failed(MACOS_UNAVAILABLE));
        }
        write_verified(sub).map(|()| succeeded()).or_else(|error| Ok(failed(error.to_string())))
    }

    fn reconcile(&self) -> UsageResult<Option<CliAccountState>> {
        if !self.available() {
            return Ok(None);
        }
        reconcile_file().map(Some)
    }

    fn adopt_before_refresh(&self, sub: &mut Subscription) -> UsageResult<()> {
        if !self.available() || sub.catalog_id != CATALOG_ID {
            return Ok(());
        }
        // Live-first: whatever Claude Code rotated into the file wins over the
        // row's copy, exactly like `fetch_inner` already does on every read.
        if let Some(live) = read_live_oauth()
            && token_of(sub).as_deref() != live.access_token.as_deref()
        {
            {
                let mut updated = sub.clone();
                updated.access_token_encrypted = live.access_token.map(|t| crypto::encrypt(&t));
                if let Some(expires_s) = live.expires_at_seconds {
                    updated.access_token_expires_at = Some(expires_s);
                }
                *sub = storage::patch_oauth_credentials(&updated)?;
            }
        }
        Ok(())
    }

    fn forget(&self, _sub_id: &str) -> UsageResult<()> {
        // No snapshot to drop, and deleting the live credential would log the
        // CLI out entirely — forgetting a card must not log anybody out.
        Ok(())
    }
}

// ── file store ───────────────────────────────────────────────────────────

/// The `claudeAiOauth` slice SkillStar writes; every other key in the file is
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

fn credentials_path() -> PathBuf {
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

fn read_file_json() -> UsageResult<Option<serde_json::Value>> {
    match std::fs::read_to_string(credentials_path()) {
        Ok(text) if text.trim().is_empty() => Ok(Some(serde_json::Value::Object(Default::default()))),
        Ok(text) => serde_json::from_str(&text)
            .map(Some)
            .map_err(|error| UsageError::Other(format!("解析 Claude 凭证文件失败：{error}"))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(UsageError::Other(format!("读取 Claude 凭证文件失败：{error}"))),
    }
}

fn read_live_oauth() -> Option<LiveOAuth> {
    let value = read_file_json().ok()??;
    let oauth = value.get("claudeAiOauth")?.clone();
    let parsed: ClaudeOAuthFile = serde_json::from_value(oauth).ok()?;
    let access_token = parsed
        .access_token
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string);
    Some(LiveOAuth {
        access_token,
        expires_at_seconds: parsed.expires_at_seconds(),
    })
}

/// Replace only the `claudeAiOauth` key, keeping every other key (notably
/// `mcpOAuth`) exactly as stored.
fn write_oauth(oauth: &ClaudeOAuthFile) -> UsageResult<()> {
    let mut root = read_file_json()?.unwrap_or_else(|| serde_json::Value::Object(Default::default()));
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
    let path = credentials_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| UsageError::Other(format!("创建 Claude 配置目录失败：{error}")))?;
    }
    std::fs::write(&path, format!("{text}\n"))
        .map_err(|error| UsageError::Other(format!("写入 Claude 凭证文件失败：{error}")))
}

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
        expires_at_ms: subscription.access_token_expires_at.map(|s| s * 1000),
        subscription_type: subscription
            .plan_tier
            .clone()
            .map(|tier| tier.to_ascii_lowercase()),
    })
}

fn write_verified(subscription: &Subscription) -> UsageResult<()> {
    if subscription.catalog_id != CATALOG_ID {
        return Err(UsageError::Other(
            "Claude 切换收到了其它 catalog 的订阅".into(),
        ));
    }
    let oauth = oauth_of(subscription)?;
    write_oauth(&oauth)?;
    // Read back: the switch is real only if the file now serves this row.
    let live = read_live_oauth()
        .ok_or_else(|| UsageError::Other("Claude 凭证文件回读失败，切换未生效".into()))?;
    if live.access_token.as_deref() != oauth.access_token.as_deref() {
        return Err(UsageError::Other(
            "Claude 凭证文件回读与写入不一致，切换未生效".into(),
        ));
    }
    Ok(())
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

fn reconcile_file() -> UsageResult<CliAccountState> {
    let Some(live) = read_live_oauth() else {
        return Ok(CliAccountState::Missing);
    };
    let Some(live_token) = live.access_token else {
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

fn config_path_display() -> String {
    credentials_path().to_string_lossy().to_string()
}

fn failed(reason: impl Into<String>) -> SwitchOutcome {
    SwitchOutcome {
        tool_id: CATALOG_ID.to_string(),
        config_path: config_path_display(),
        backup_path: None,
        keychain_updated: false,
        link_mode: None,
        success: false,
        error: Some(reason.into()),
    }
}

fn succeeded() -> SwitchOutcome {
    SwitchOutcome {
        tool_id: CATALOG_ID.to_string(),
        config_path: config_path_display(),
        backup_path: None,
        keychain_updated: false,
        link_mode: None,
        success: true,
        error: None,
    }
}

#[cfg(test)]
#[path = "claude_tests.rs"]
mod tests;
