//! Devin Desktop（原 Windsurf）账号：没有可再驱动的登录腿。
//!
//! 与 Claude Code 同一形状（Immediate + AuthRequired）：客户端把凭据放在
//! state.vscdb 的行里，登录走的是它自己的浏览器隐式授令牌 → Firebase →
//! register.windsurf.com 换 apiKey 的链路。那条链路本 crate 没有移植，所以
//! start 明确报"需要导入"，而不是假装开一个浏览器窗口再把用户丢在死路上。
//!
//! 刷新同样不存在：Devin Desktop 的凭据是 apiKey（或 auth1 令牌），没有
//! refresh token 可换，重新导入是唯一的续期方式。额度读取只用导入时拿到的
//! apiKey，也不需要刷新。

use crate::oauth::agents::{Started, immediate_auth_required};
use crate::oauth::error::{OAuthError, OAuthResult};
use crate::oauth::session::StartOptions;
use crate::oauth::TokenSet;

#[cfg(test)]
pub(crate) const AGENT_ID: &str = "devin-desktop";

/// 注册表里的产品名（消息里用它，UI 的 display_name 在桥里）。
pub(crate) const PRODUCT: &str = "Devin Desktop";

/// 记一次"没有这条腿"的标志：AccountError 会把 AuthRequired 翻成
/// requires_reauth，用 Immediate 流程配合它，UI 的路径与 claude-code 一致。
pub(crate) fn start(login_id: &str, _opts: &StartOptions) -> OAuthResult<Started> {
    immediate_auth_required(login_id, auth_required())
}

/// 这条腿的固定答复：说清楚该走哪条路。
pub(crate) fn auth_required() -> OAuthError {
    OAuthError::AuthRequired(format!(
        "{PRODUCT} 没有可再驱动的登录端点：请从本机 state.vscdb 导入它已登录的账号，或粘贴 apiKey"
    ))
}

/// 刷新：apiKey 凭据没有 refresh token，直接要求重新导入。
pub(crate) fn refresh(tokens: &TokenSet) -> OAuthResult<TokenSet> {
    let _ = tokens;
    Err(auth_required())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oauth::session::LoginFlow;

    #[test]
    fn the_login_leg_says_import_instead_of_hanging() {
        // 走公共入口：这条腿要经注册表才能被 complete 取走。
        let started = crate::oauth::start(AGENT_ID, &StartOptions::default()).expect("start");
        assert_eq!(started.flow, LoginFlow::Immediate);
        assert!(started.auth_url.is_empty());
        match crate::oauth::complete(&started.login_id) {
            Err(OAuthError::AuthRequired(message)) => {
                assert!(message.contains(PRODUCT), "{message}");
                assert!(message.contains("state.vscdb"), "{message}");
                assert!(message.contains("apiKey"), "{message}");
            }
            other => panic!("expected AuthRequired, got {other:?}"),
        }
    }

    #[test]
    fn a_refresh_asks_for_a_new_import_and_never_trades_anything() {
        let error = refresh(&TokenSet::default()).expect_err("no refresh");
        assert!(matches!(error, OAuthError::AuthRequired(_)), "got {error:?}");
    }
}
