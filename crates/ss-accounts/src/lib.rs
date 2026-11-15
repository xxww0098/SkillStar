//! ss-accounts - the accounts workbench capability: records, encrypted storage,
//! login flows, quota reads, and switching what a client is actually serving.
//!
//! Owns the account record and its on-disk layout, OAuth login and token
//! refresh (`oauth`), quota reads (`usage`), add/remove, the refresh
//! orchestration, the per-agent switch writers, and the reconcile read.
//!
//! Credentials never leave this crate: the UI only ever sees [AccountView], and
//! no type here prints a plaintext token through Debug or Serialize.

mod agents;
mod identity;
pub(crate) mod oauth;
mod service;
mod token_import;
pub(crate) mod usage;

pub mod crypto;
pub mod error;
pub mod reconcile;
pub mod record;
pub mod store;
pub mod switch;

pub use error::{AccountError, AccountResult};
pub use oauth::{LoginFlow, LoginStart, StartOptions};
pub use reconcile::AgentLive;
pub use switch::SwitchOutcome;
pub use usage::{Credits, UsageSnapshot, UsageUnit, UsageWindow};

/// The agents this crate manages accounts for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Agent {
    Codex,
    ClaudeCode,
    Cursor,
    Antigravity,
    GeminiCli,
    Grok,
    Kiro,
    Zcode,
    DevinDesktop,
}

impl Agent {
    /// Every supported agent.
    pub fn supported() -> &'static [Agent] {
        &[
            Self::Codex,
            Self::ClaudeCode,
            Self::Cursor,
            Self::Antigravity,
            Self::GeminiCli,
            Self::Grok,
            Self::Kiro,
            Self::Zcode,
            Self::DevinDesktop,
        ]
    }

    /// The ss-skills agent id (the account's agent field).
    pub fn id(self) -> &'static str {
        agents::spec(self).id
    }

    /// 面向 UI 展示的品牌名称。
    pub fn display_name(self) -> &'static str {
        agents::spec(self).display_name
    }

    /// 是否支持标准 OAuth 登录链路（`false` 时 UI 默认引导本地导入或粘贴令牌）。
    pub fn supports_oauth(self) -> bool {
        agents::spec(self).oauth.supported
    }

    /// 本机登录凭据的存储位置提示，供添加弹窗「本地导入」引导使用。
    pub fn local_source(self) -> &'static str {
        agents::spec(self).local_source
    }

    /// Parse an agent id; unknown ids are an error, not a default.
    pub fn from_id(agent: &str) -> AccountResult<Self> {
        Self::supported()
            .iter()
            .copied()
            .find(|candidate| candidate.id() == agent)
            .ok_or_else(|| AccountError::UnsupportedAgent(agent.to_string()))
    }
}

/// What the UI may see of an account.
///
/// No credential, no ciphertext, no fingerprint: this is the whole of what
/// leaves the crate, and it serialises to JSON the UI renders as-is.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountView {
    pub id: String,
    pub agent: String,
    pub display_name: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub plan: Option<String>,
    pub created_at: i64,
    #[serde(default)]
    pub last_used_at: Option<i64>,
    pub requires_reauth: bool,
    /// The client currently serves this account (disk truth).
    pub is_active: bool,
    #[serde(default)]
    pub usage: Option<UsageSnapshot>,
}

/// The result of switching a client to one account.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivateOutcome {
    pub account: AccountView,
    /// The account the client served before the switch, if any.
    #[serde(default)]
    pub previous: Option<String>,
    pub target: String,
}

/// Every stored account, oldest first.
pub fn list() -> AccountResult<Vec<AccountView>> {
    service::list()
}

/// Refresh one account's tokens and quota, then return its fresh view.
pub fn refresh(id: &str) -> AccountResult<AccountView> {
    service::refresh(id)
}

/// Make the account's client serve it.
pub fn activate(id: &str) -> AccountResult<ActivateOutcome> {
    service::activate(id)
}

/// Forget one account (the client is left as it is).
pub fn remove(id: &str) -> AccountResult<()> {
    service::remove(id)
}

/// What each client is serving right now.
pub fn reconcile() -> AccountResult<Vec<AgentLive>> {
    reconcile::live_all()
}

/// Add an account from a pasted token or credential JSON.
pub fn add_from_token(agent: &str, payload: &str) -> AccountResult<AccountView> {
    service::add_from_token(agent, payload)
}

/// Add an account from the machine's own client credential.
pub fn add_from_local(agent: &str) -> AccountResult<AccountView> {
    service::add_from_local(agent)
}

/// Start an OAuth login for the agent.
pub fn start_login(agent: &str, opts: &StartOptions) -> AccountResult<LoginStart> {
    Agent::from_id(agent)?;
    Ok(oauth::start(agent, opts)?)
}

/// Feed a pasted callback into a pending login.
pub fn submit_login_callback(login_id: &str, text: &str) -> AccountResult<()> {
    Ok(oauth::submit_callback(login_id, text)?)
}

/// Finish a pending login and store the account; an existing row for the same
/// agent + identity is replaced in place.
pub fn finish_login(login_id: &str) -> AccountResult<AccountView> {
    service::finish_login(login_id)
}

/// Drop a pending login.
pub fn cancel_login(login_id: &str) {
    oauth::cancel(login_id);
}

#[cfg(test)]
pub(crate) mod test_support;
