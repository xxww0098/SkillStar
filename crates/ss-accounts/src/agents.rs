//! 受管 Agent 的单一静态描述表（SSOT）。
//!
//! 账号域（`oauth` / `usage` / `switch` / `token_import`）与 GUI 桥元数据
//! （展示名、`supportsOauth`、`localSource`）全部从 [spec] 派发，新增一家
//! agent 时只需按 `Agent` 枚举的穷尽分支在此登记一栏，不再在多个子域维护
//! 平行 `SUPPORTED_AGENTS` 列表与字符串 `match`。

use std::path::PathBuf;

use serde_json::Value;

use crate::error::AccountResult;
use crate::oauth::error::OAuthResult;
use crate::oauth::session::StartOptions;
use crate::oauth::{self, DEFAULT_REFRESH_SKEW_SECONDS, Identity, TokenSet};
use crate::switch::paths::PathRoots;
use crate::switch::{self, ClientSnapshot, LocalCredential, SwitchOutcome, SwitchRequest};
use crate::usage::credentials::{Credentials, UsageOutcome};
use crate::usage::error::UsageResult;
use crate::usage::fetch;
use crate::{Agent, identity};

/// 与 `Agent::supported()` 同序的受管 agent registry id 列表。
#[cfg(test)]
pub(crate) const SUPPORTED_IDS: [&str; 9] = [
    "codex",
    "claude-code",
    "cursor",
    "antigravity",
    "gemini-cli",
    "grok",
    "kiro",
    "zcode",
    "devin-desktop",
];

/// OAuth 登录与刷新能力分派。
pub(crate) struct OAuthDispatch {
    pub(crate) supported: bool,
    pub(crate) refresh_skew_seconds: i64,
    pub(crate) start: fn(&str, &StartOptions) -> OAuthResult<oauth::agents::Started>,
    pub(crate) refresh: fn(&TokenSet, Option<&Value>) -> OAuthResult<TokenSet>,
}

/// 本地客户端凭据切换能力分派。
pub(crate) struct SwitchDispatch {
    pub(crate) credential_path: fn(&PathRoots) -> PathBuf,
    pub(crate) write: fn(&PathRoots, &SwitchRequest<'_>) -> AccountResult<SwitchOutcome>,
    pub(crate) read: fn(&PathRoots) -> AccountResult<Option<LocalCredential>>,
    pub(crate) snapshot: fn(&PathRoots) -> AccountResult<ClientSnapshot>,
    pub(crate) live_identity: fn(&Identity) -> Option<String>,
}

/// 单个受管 Agent 的完整能力与子域描述符。
pub(crate) struct AgentSpec {
    pub(crate) id: &'static str,
    pub(crate) display_name: &'static str,
    pub(crate) local_source: &'static str,
    pub(crate) oauth: OAuthDispatch,
    pub(crate) usage: fn(&Credentials) -> UsageResult<UsageOutcome>,
    pub(crate) switch: SwitchDispatch,
    pub(crate) provider_account_id: fn(&str, Option<&str>) -> Option<String>,
}

/// 按 [Agent] 返回唯一的静态描述符（穷尽匹配，新增枚举项时由编译器强制补齐）。
pub(crate) fn spec(agent: Agent) -> AgentSpec {
    match agent {
        Agent::Codex => AgentSpec {
            id: "codex",
            display_name: "Codex",
            local_source: "~/.codex/auth.json",
            oauth: OAuthDispatch {
                supported: true,
                refresh_skew_seconds: DEFAULT_REFRESH_SKEW_SECONDS,
                start: oauth::agents::codex::start,
                refresh: |tokens, _| oauth::agents::codex::refresh(tokens),
            },
            usage: fetch::codex::fetch,
            switch: SwitchDispatch {
                credential_path: switch::codex::credential_path,
                write: switch::codex::write,
                read: switch::codex::read,
                snapshot: switch::codex::snapshot,
                live_identity: switch::codex::live_identity,
            },
            provider_account_id: codex_account_id,
        },
        Agent::ClaudeCode => AgentSpec {
            id: "claude-code",
            display_name: "Claude Code",
            local_source: "~/.claude/.credentials.json",
            oauth: OAuthDispatch {
                supported: false,
                refresh_skew_seconds: DEFAULT_REFRESH_SKEW_SECONDS,
                start: oauth::agents::claude_code::start,
                refresh: |tokens, _| oauth::agents::claude_code::refresh(tokens),
            },
            usage: fetch::claude_code::fetch,
            switch: SwitchDispatch {
                credential_path: switch::claude_code::credential_path,
                write: switch::claude_code::write,
                read: switch::claude_code::read,
                snapshot: switch::claude_code::snapshot,
                live_identity: switch::claude_code::live_identity,
            },
            provider_account_id: no_account_id,
        },
        Agent::Cursor => AgentSpec {
            id: "cursor",
            display_name: "Cursor",
            local_source: "Cursor state.vscdb",
            oauth: OAuthDispatch {
                supported: true,
                refresh_skew_seconds: DEFAULT_REFRESH_SKEW_SECONDS,
                start: oauth::agents::cursor::start,
                refresh: |tokens, _| oauth::agents::cursor::refresh(tokens),
            },
            usage: fetch::cursor::fetch,
            switch: SwitchDispatch {
                credential_path: switch::cursor::credential_path,
                write: switch::cursor::write,
                read: switch::cursor::read,
                snapshot: switch::cursor::snapshot,
                live_identity: switch::cursor::live_identity,
            },
            provider_account_id: |access_token, _| identity::jwt_subject_tail(access_token),
        },
        Agent::Antigravity => AgentSpec {
            id: "antigravity",
            display_name: "Antigravity",
            local_source: "Antigravity Keychain / state.vscdb",
            oauth: OAuthDispatch {
                supported: true,
                refresh_skew_seconds: oauth::agents::antigravity::REFRESH_SKEW_SECONDS,
                start: oauth::agents::antigravity::start,
                refresh: |tokens, _| oauth::agents::antigravity::refresh(tokens),
            },
            usage: fetch::antigravity::fetch,
            switch: SwitchDispatch {
                credential_path: switch::antigravity::credential_path,
                write: switch::antigravity::write,
                read: switch::antigravity::read,
                snapshot: switch::antigravity::snapshot,
                live_identity: switch::antigravity::live_identity,
            },
            provider_account_id: no_account_id,
        },
        Agent::GeminiCli => AgentSpec {
            id: "gemini-cli",
            display_name: "Gemini CLI",
            local_source: "~/.gemini/oauth_creds.json",
            oauth: OAuthDispatch {
                supported: true,
                refresh_skew_seconds: oauth::agents::gemini_cli::REFRESH_SKEW_SECONDS,
                start: oauth::agents::gemini_cli::start,
                refresh: |tokens, _| oauth::agents::gemini_cli::refresh(tokens),
            },
            usage: fetch::gemini_cli::fetch,
            switch: SwitchDispatch {
                credential_path: switch::gemini_cli::credential_path,
                write: switch::gemini_cli::write,
                read: switch::gemini_cli::read,
                snapshot: switch::gemini_cli::snapshot,
                live_identity: switch::gemini_cli::live_identity,
            },
            provider_account_id: no_account_id,
        },
        Agent::Grok => AgentSpec {
            id: "grok",
            display_name: "Grok",
            local_source: "~/.grok/auth.json",
            oauth: OAuthDispatch {
                supported: true,
                refresh_skew_seconds: DEFAULT_REFRESH_SKEW_SECONDS,
                start: oauth::agents::grok::start,
                refresh: |tokens, _| oauth::agents::grok::refresh(tokens),
            },
            usage: fetch::grok::fetch,
            switch: SwitchDispatch {
                credential_path: switch::grok::credential_path,
                write: switch::grok::write,
                read: switch::grok::read,
                snapshot: switch::grok::snapshot,
                live_identity: switch::grok::live_identity,
            },
            provider_account_id: no_account_id,
        },
        Agent::Kiro => AgentSpec {
            id: "kiro",
            display_name: "Kiro",
            local_source: "~/.aws/sso/cache/kiro-auth-token.json",
            oauth: OAuthDispatch {
                supported: true,
                refresh_skew_seconds: oauth::agents::kiro::REFRESH_SKEW_SECONDS,
                start: oauth::agents::kiro::start,
                refresh: oauth::agents::kiro::refresh,
            },
            usage: fetch::kiro::fetch,
            switch: SwitchDispatch {
                credential_path: switch::kiro::credential_path,
                write: switch::kiro::write,
                read: switch::kiro::read,
                snapshot: switch::kiro::snapshot,
                live_identity: switch::kiro::live_identity,
            },
            provider_account_id: no_account_id,
        },
        Agent::Zcode => AgentSpec {
            id: "zcode",
            display_name: "ZCode",
            local_source: "~/.zcode/v2/credentials.json",
            oauth: OAuthDispatch {
                supported: true,
                refresh_skew_seconds: DEFAULT_REFRESH_SKEW_SECONDS,
                start: oauth::agents::zcode::start,
                refresh: oauth::agents::zcode::refresh,
            },
            usage: fetch::zcode::fetch,
            switch: SwitchDispatch {
                credential_path: switch::zcode::credential_path,
                write: switch::zcode::write,
                read: switch::zcode::read,
                snapshot: switch::zcode::snapshot,
                live_identity: switch::zcode::live_identity,
            },
            provider_account_id: no_account_id,
        },
        Agent::DevinDesktop => AgentSpec {
            id: "devin-desktop",
            display_name: "Devin Desktop",
            local_source: "Devin Desktop state.vscdb",
            oauth: OAuthDispatch {
                supported: false,
                refresh_skew_seconds: DEFAULT_REFRESH_SKEW_SECONDS,
                start: oauth::agents::devin_desktop::start,
                refresh: |tokens, _| oauth::agents::devin_desktop::refresh(tokens),
            },
            usage: fetch::devin_desktop::fetch,
            switch: SwitchDispatch {
                credential_path: switch::devin_desktop::credential_path,
                write: switch::devin_desktop::write,
                read: switch::devin_desktop::read,
                snapshot: switch::devin_desktop::snapshot,
                live_identity: switch::devin_desktop::live_identity,
            },
            provider_account_id: no_account_id,
        },
    }
}

fn no_account_id(_access_token: &str, _id_token: Option<&str>) -> Option<String> {
    None
}

fn codex_account_id(access_token: &str, id_token: Option<&str>) -> Option<String> {
    id_token
        .and_then(|token| {
            identity::jwt_nested_claim(token, "https://api.openai.com/auth", "chatgpt_account_id")
        })
        .or_else(|| {
            identity::jwt_nested_claim(
                access_token,
                "https://api.openai.com/auth",
                "chatgpt_account_id",
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn specs_match_supported_agents_order_and_uniqueness() {
        let ids: Vec<&str> = Agent::supported().iter().map(|agent| agent.id()).collect();
        assert_eq!(ids, SUPPORTED_IDS);

        let unique: HashSet<&str> = ids.iter().copied().collect();
        assert_eq!(unique.len(), ids.len(), "duplicate agent ids");

        for agent in Agent::supported() {
            assert_eq!(Agent::from_id(agent.id()).expect("roundtrip"), *agent);
            assert!(!agent.display_name().is_empty(), "{}", agent.id());
            assert!(!agent.local_source().is_empty(), "{}", agent.id());
        }
    }
}
