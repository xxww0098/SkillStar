//! Kiro 额度切片的测试：地址形状、provider_state 的补齐与早退分支。
//!
//! 不打网络：地址与补状态是纯函数，早退分支在发请求之前返回。

use super::*;
use crate::oauth::TokenSet;
use crate::usage::credentials::Credentials;
use serde_json::json;

#[test]
fn the_usage_url_names_the_agentic_request_and_the_profile() {
    let url = usage_url(
        "https://q.us-east-1.amazonaws.com",
        Some("arn:aws:codewhisperer:us-east-1:1:profile/ABC"),
    )
    .expect("url");
    let parsed = url::Url::parse(&url).expect("parse");
    assert_eq!(parsed.host_str(), Some("q.us-east-1.amazonaws.com"));
    assert_eq!(parsed.path(), "/getUsageLimits");
    let query: std::collections::BTreeMap<String, String> = parsed
        .query_pairs()
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    assert_eq!(query.get("origin").map(String::as_str), Some("AI_EDITOR"));
    assert_eq!(
        query.get("resourceType").map(String::as_str),
        Some("AGENTIC_REQUEST")
    );
    assert_eq!(query.get("isEmailRequired").map(String::as_str), Some("true"));
    assert_eq!(
        query.get("profileArn").map(String::as_str),
        Some("arn:aws:codewhisperer:us-east-1:1:profile/ABC")
    );

    // 不知道 profileArn 时不带这个参数（带空值会被判无效）。
    let without = usage_url("https://q.eu-central-1.amazonaws.com", None).expect("url");
    assert!(!without.contains("profileArn"), "{without}");
}

#[test]
fn a_read_without_a_token_asks_for_a_new_login() {
    let creds = Credentials {
        tokens: TokenSet::default(),
        account_id: None,
        provider_state: None,
        needs_identity: false,
    };
    let error = fetch(&creds).expect_err("no token");
    assert!(matches!(error, UsageError::AuthRequired(_)), "got {error:?}");
}

#[test]
fn the_learned_state_keeps_the_registration_and_adds_the_profile() {
    let state = crate::oauth::agents::kiro::KiroState {
        client_id: Some("cid".to_string()),
        client_secret: Some("csec".to_string()),
        region: None,
        start_url: None,
        profile_arn: None,
    };
    let body = json!({
        "profileArn": "arn:aws:codewhisperer:eu-west-1:1:profile/ABC",
        "email": "dev@example.com",
        "userId": "user-1"
    });
    let learned = learned_state(&state, &body, "eu-west-1");
    assert_eq!(learned.client_id.as_deref(), Some("cid"));
    assert_eq!(learned.region.as_deref(), Some("eu-west-1"));
    assert_eq!(
        learned.profile_arn.as_deref(),
        Some("arn:aws:codewhisperer:eu-west-1:1:profile/ABC")
    );

    // 已经知道的 profileArn 不被响应覆盖。
    let known = crate::oauth::agents::kiro::KiroState {
        profile_arn: Some("arn:aws:codewhisperer:us-east-1:1:profile/OLD".to_string()),
        ..state
    };
    let learned = learned_state(&known, &body, "us-east-1");
    assert_eq!(
        learned.profile_arn.as_deref(),
        Some("arn:aws:codewhisperer:us-east-1:1:profile/OLD")
    );
}
