//! Grok 额度切片的测试：请求头、三态分类、退化路径与两张卡的分工。
//!
//! 不打网络：只驱动纯函数与分类表。

use super::*;
use crate::oauth::TokenSet;
use crate::usage::credentials::Credentials;
use serde_json::json;

#[test]
fn every_request_names_the_cli_authenticator() {
    let headers = headers("at-1");
    let map: std::collections::BTreeMap<&str, &str> = headers
        .iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect();
    assert_eq!(map.get("Authorization"), Some(&"Bearer at-1"));
    assert_eq!(map.get(TOKEN_AUTH_HEADER), Some(&TOKEN_AUTH_VALUE));
}

#[test]
fn a_read_without_a_token_asks_for_a_new_login() {
    let creds = Credentials {
        tokens: TokenSet::default(),
        provider_state: None,
    };
    let error = fetch(&creds).expect_err("no token");
    assert!(matches!(error, UsageError::AuthRequired(_)), "got {error:?}");
}

#[test]
fn a_401_on_billing_is_auth_required_and_a_500_is_transient() {
    assert!(matches!(
        super::super::classify_status(BILLING_LABEL, 401, "{}"),
        UsageError::AuthRequired(_)
    ));
    assert!(matches!(
        super::super::classify_status(BILLING_LABEL, 503, "{}"),
        UsageError::Transient(_)
    ));
}

#[test]
fn the_settings_plan_is_read_through_the_same_candidate_roots() {
    assert_eq!(
        parse_grok_plan(&json!({ "subscription_tier_display": "SuperGrok" })).as_deref(),
        Some("SuperGrok")
    );
    assert_eq!(
        parse_grok_plan(&json!({ "config": { "subscription_tier_display": "X Premium" } })).as_deref(),
        Some("X Premium")
    );
    assert_eq!(parse_grok_plan(&json!({ "subscription_tier_display": "Grok" })), None);
}

#[test]
fn a_read_with_a_stale_token_is_refreshed_before_the_proxy_call() {
    // 过期在窗口内 → tokens_for_read 会先刷新；没有 refresh token 时它必须报
    // AuthRequired，而不是把过期令牌发出去。
    let creds = Credentials {
        tokens: TokenSet {
            access_token: "at".to_string(),
            refresh_token: None,
            expires_at: Some(super::super::unix_now() - 10),
            ..TokenSet::default()
        },
        provider_state: None,
    };
    let error = fetch(&creds).expect_err("nothing to refresh with");
    assert!(matches!(error, UsageError::AuthRequired(_)), "got {error:?}");
}
