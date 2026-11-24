//! Devin Desktop（Windsurf）额度：座位管理的 GetUserStatus。
//!
//! POST {apiServer}/exa.seat_management_pb.SeatManagementService/GetUserStatus
//! 带 Connect-Protocol-Version: 1，body 的 metadata 里放 apiKey 与 IDE 身份。
//! 响应可能是 JSON（本 crate 只读 JSON；planStatus protobuf 解不开就报 Unreadable，
//! 不猜字段）。
//!
//! 凭据是 **apiKey**：它没有 refresh 概念，导入即用；apiServer 默认
//! https://server.codeium.com，导入时从 state.vscdb 里读到的值会覆盖它。

use serde_json::{Value, json};

use crate::usage::credentials::{Credentials, UsageOutcome};
use crate::usage::error::{UsageError, UsageResult};

use super::devin_desktop_parse;
use super::parse;

const SEAT_SERVICE: &str = "exa.seat_management_pb.SeatManagementService";
const DEFAULT_API_SERVER: &str = "https://server.codeium.com";
const LABEL: &str = "devin-desktop GetUserStatus";
/// provider_state 里的键。
const API_SERVER_KEY: &str = "api_server_url";

/// Read the Devin Desktop quota for one account.
pub fn fetch(creds: &Credentials) -> UsageResult<UsageOutcome> {
    let now = super::unix_now();
    // apiKey 没有过期时间，也就没有"先刷新"这一步；有 refresh token 的旧记录
    // 也不兑换（Devin 的刷新会换掉客户端正在用的会话）。
    let api_key = creds.tokens.access_token.trim();
    if api_key.is_empty() {
        return Err(UsageError::AuthRequired(
            "Devin Desktop 没有可用的 apiKey，请从本机重新导入".to_string(),
        ));
    }
    let base = api_server(creds.provider_state.as_ref());
    let body = json!({
        "metadata": {
            "apiKey": api_key,
            "ideName": "Windsurf",
            "ideVersion": "1.0.0",
            "extensionName": "codeium.windsurf",
            "extensionVersion": "1.0.0",
            "locale": "en",
            "os": ide_os(),
            "disableTelemetry": true
        }
    });
    let answer = super::http_post_json(
        LABEL,
        &format!("{}/{}", base.trim_end_matches('/'), format!("{SEAT_SERVICE}/GetUserStatus")),
        &[
            ("Accept", "application/json".to_string()),
            (
                "Connect-Protocol-Version",
                "1".to_string(),
            ),
        ],
        &body,
    )?;
    if !(200..300).contains(&answer.status) {
        return Err(super::classify_status(LABEL, answer.status, &answer.body));
    }
    outcome_from_body(&answer.body, now, creds)
}

fn outcome_from_body(body: &str, now: i64, creds: &Credentials) -> UsageResult<UsageOutcome> {
    let payload: Value = serde_json::from_str(body).map_err(|error| {
        UsageError::Unreadable(format!(
            "{LABEL}: cannot parse the answer (planStatus protobuf is not read here): {error}: {}",
            parse::snippet(body)
        ))
    })?;
    let snapshot = devin_desktop_parse::snapshot_from(&payload, Some(&payload), now);
    if snapshot.windows.is_empty() && snapshot.plan.is_none() {
        return Err(UsageError::Unreadable(format!(
            "{LABEL}: the reply carries no quota field: {}",
            parse::snippet(body)
        )));
    }
    Ok(UsageOutcome {
        snapshot,
        tokens: creds.tokens.clone(),
        provider_state: creds.provider_state.clone(),
        identity: Some(identity_from_status(&payload)),
    })
}

fn identity_from_status(payload: &Value) -> crate::oauth::Identity {
    crate::oauth::Identity {
        display_name: parse::text_in(payload, &["name", "displayName", "username"]),
        email: devin_desktop_parse::email_of(payload),
        ..crate::oauth::Identity::default()
    }
}

/// 这次读该打哪个 apiServer：provider_state 里记的优先。
pub(crate) fn api_server(provider_state: Option<&Value>) -> String {
    provider_state
        .and_then(|state| state.get(API_SERVER_KEY))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_API_SERVER)
        .to_string()
}

fn ide_os() -> &'static str {
    match std::env::consts::OS {
        "macos" => "darwin",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oauth::TokenSet;

    #[test]
    fn the_api_server_defaults_to_codeium_and_follows_the_record() {
        assert_eq!(api_server(None), DEFAULT_API_SERVER);
        assert_eq!(
            api_server(Some(&json!({ "api_server_url": "https://server.self-serve.windsurf.com" }))),
            "https://server.self-serve.windsurf.com"
        );
        assert_eq!(api_server(Some(&json!({ "api_server_url": "  " }))), DEFAULT_API_SERVER);
    }

    #[test]
    fn status_identity_carries_name_and_email_when_available() {
        let identity = identity_from_status(&json!({
            "name": "Example User", "email": "user@example.com"
        }));
        assert_eq!(identity.display_name.as_deref(), Some("Example User"));
        assert_eq!(identity.email.as_deref(), Some("user@example.com"));
        assert_eq!(identity_from_status(&json!({})).email, None);
    }

    #[test]
    fn a_read_without_an_api_key_asks_for_a_new_import() {
        let creds = Credentials {
            tokens: TokenSet::default(),
            account_id: None,
            provider_state: None,
            needs_identity: false,
        };
        let error = fetch(&creds).expect_err("no api key");
        assert!(matches!(error, UsageError::AuthRequired(_)), "got {error:?}");
        assert!(error.to_string().contains("重新导入"), "{error}");
    }

    #[test]
    fn empty_200_response_is_rejected_as_unreadable() {
        let creds = Credentials {
            tokens: TokenSet {
                access_token: "sk-devin".to_string(),
                ..TokenSet::default()
            },
            ..Credentials::default()
        };
        let err = outcome_from_body("{}", 1_800_000_000, &creds).expect_err("empty 200 rejected");
        assert!(matches!(err, UsageError::Unreadable(_)), "got {err:?}");
    }
}
