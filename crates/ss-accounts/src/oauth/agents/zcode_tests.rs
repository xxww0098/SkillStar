//! ZCode 登录切片的测试：上游选择、授权页、深链白名单、业务令牌与刷新分类。
//!
//! 不打网络：全部驱动纯函数。

use super::*;
use crate::test_support::jwt_with_claims;
use serde_json::json;

#[test]
fn the_upstream_defaults_to_zai_and_bigmodel_is_explicit() {
    assert_eq!(normalize_provider(None).expect("default"), "zai");
    assert_eq!(normalize_provider(Some("  ")).expect("blank"), "zai");
    assert_eq!(normalize_provider(Some("ZAI")).expect("upper"), "zai");
    assert_eq!(normalize_provider(Some("bigmodel")).expect("bigmodel"), "bigmodel");
    let refused = normalize_provider(Some("openai")).expect_err("unknown upstream");
    assert!(refused.to_string().contains("zai 或 bigmodel"), "{refused}");
}

#[test]
fn the_provider_state_remembers_only_the_upstream() {
    let state = provider_state("bigmodel");
    assert_eq!(state, json!({ "provider": "bigmodel" }));
    assert_eq!(provider_of(Some(&state)).expect("provider"), "bigmodel");
    assert_eq!(provider_of(None).expect("default"), "zai");
}

#[test]
fn each_upstream_gets_its_own_authorization_page_and_callback() {
    let zai = url::Url::parse(&authorize_url("zai", "state-1").expect("zai")).expect("url");
    assert_eq!(zai.host_str(), Some("chat.z.ai"));
    assert_eq!(zai.path(), "/api/oauth/authorize");
    let query: std::collections::BTreeMap<String, String> = zai
        .query_pairs()
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    assert_eq!(
        query.get("redirect_uri").map(String::as_str),
        Some(ZAI_REDIRECT)
    );
    assert_eq!(query.get("client_id").map(String::as_str), Some(ZAI_CLIENT_ID));
    assert_eq!(query.get("state").map(String::as_str), Some("state-1"));

    let bigmodel = url::Url::parse(&authorize_url("bigmodel", "state-2").expect("bigmodel"))
        .expect("url");
    assert_eq!(bigmodel.host_str(), Some("bigmodel.cn"));
    let query: std::collections::BTreeMap<String, String> = bigmodel
        .query_pairs()
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    assert_eq!(
        query.get("redirect").map(String::as_str),
        Some(BIGMODEL_REDIRECT)
    );
    assert_eq!(query.get("appId").map(String::as_str), Some("zcode"));
}

#[test]
fn the_deep_link_whitelist_checks_scheme_host_upstream_and_state() {
    let good = "zcode://zai-auth/callback?code=abc&state=s1";
    assert_eq!(
        parse_callback(good, "zai", "s1").expect("code"),
        "abc".to_string()
    );
    // authCode 也认（bigmodel 的写法）。
    assert_eq!(
        parse_callback("zcode://oauth/callback?authCode=abc&state=s1", "bigmodel", "s1")
            .expect("authCode"),
        "abc".to_string()
    );

    for (callback, needle) in [
        ("https://zai-auth/callback?code=1&state=s1", "协议"),
        ("zcode://evil/callback?code=1&state=s1", "白名单"),
        ("zcode://oauth/callback?code=1&state=s1", "不匹配"),
        ("zcode://zai-auth/callback?code=1&state=s2", "state"),
        ("zcode://zai-auth/callback?code=1", "state"),
        ("zcode://zai-auth/callback?state=s1", "code"),
        ("zcode://zai-auth/callback?error=denied&state=s1", "授权失败"),
    ] {
        let error = parse_callback(callback, "zai", "s1").expect_err(callback);
        assert!(error.to_string().contains(needle), "{callback}: got {error}");
    }
}

#[test]
fn the_business_token_exchange_accepts_zero_and_two_hundred_but_not_a_failure() {
    assert_eq!(
        zai_business_token(&json!({ "code": 0, "data": { "token": "business" } })).expect("code 0"),
        "business"
    );
    assert_eq!(
        zai_business_token(&json!({ "code": 200, "token": "business-2" })).expect("code 200"),
        "business-2"
    );
    // 没有 code 也接受（有的镜像不回它）。
    assert_eq!(
        zai_business_token(&json!({ "data": { "accessToken": "business-3" } })).expect("no code"),
        "business-3"
    );
    assert!(zai_business_token(&json!({ "code": 1001, "msg": "nope" })).is_err());
    assert!(zai_business_token(&json!({ "code": 0 })).is_err());
}

#[test]
fn a_zai_account_cannot_be_refreshed_and_says_so() {
    let tokens = TokenSet {
        access_token: "at".to_string(),
        refresh_token: Some("rt".to_string()),
        ..TokenSet::default()
    };
    let error = refresh(&tokens, Some(&provider_state("zai"))).expect_err("zai has no refresh");
    assert!(matches!(error, OAuthError::AuthRequired(_)), "got {error:?}");
    assert!(error.to_string().contains("重新登录"), "{error}");

    let no_refresh = refresh(&TokenSet::default(), None).expect_err("nothing to refresh with");
    assert!(matches!(no_refresh, OAuthError::AuthRequired(_)), "got {no_refresh:?}");
}

#[test]
fn identity_prefers_the_jwt_claims_and_falls_back_to_user_info() {
    let jwt = jwt_with_claims(json!({ "email": "dev@example.com", "sub": "user-1" }));
    let tokens = TokenSet {
        access_token: "opaque".to_string(),
        id_token: Some(jwt),
        ..TokenSet::default()
    };
    let identity = identity_of(&tokens, None);
    assert_eq!(identity.email.as_deref(), Some("dev@example.com"));
    assert_eq!(identity.account_id.as_deref(), Some("user-1"));

    let opaque = TokenSet {
        access_token: "opaque".to_string(),
        ..TokenSet::default()
    };
    let identity = identity_of(
        &opaque,
        Some(&json!({ "email": "info@example.com", "userId": "u2" })),
    );
    assert_eq!(identity.email.as_deref(), Some("info@example.com"));
    assert_eq!(identity.account_id.as_deref(), Some("u2"));
    assert_eq!(identity.display_name.as_deref(), Some("info@example.com"));
}

#[test]
fn zai_business_token_accepts_nested_access_token_and_rejects_success_false() {
    assert_eq!(
        zai_business_token(&json!({
            "code": 200,
            "data": { "access_token": "biz-at", "expires_in": 3600 }
        }))
        .expect("data.access_token"),
        "biz-at"
    );
    assert!(
        zai_business_token(&json!({
            "code": 200,
            "success": false,
            "data": { "access_token": "biz-at" }
        }))
        .is_err()
    );
}

#[test]
fn identity_extracts_bigmodel_user_info_and_customer_info_without_email() {
    let tokens = TokenSet {
        access_token: jwt_with_claims(json!({
            "customer_id": "27741789126905948",
            "username": "jwt-name"
        })),
        id_token: Some(jwt_with_claims(json!({ "sub": "zcode-jwt-sub" }))),
        ..TokenSet::default()
    };

    // 1. 本地 user_info 结构（无邮箱）。
    let local_info = json!({
        "id": "27741789126905948",
        "username": "xxww0098",
        "displayName": "xxww0098",
        "rawProfile": { "user_id": "27741789126905948", "email": "", "name": "xxww0098" }
    });
    let from_local = identity_of(&tokens, Some(&local_info));
    assert_eq!(from_local.display_name.as_deref(), Some("xxww0098"));
    assert_eq!(from_local.account_id.as_deref(), Some("27741789126905948"));
    assert_eq!(from_local.email, None);

    // 2. getCustomerInfo 响应结构。
    let customer = json!({
        "code": 200,
        "data": {
            "id": 12130266,
            "customerNumber": "27741789126905948",
            "customerName": "xxww0098",
            "nickName": null,
            "email": null,
            "phoneNumber": "188****1685"
        }
    });
    let from_customer = identity_of(&tokens, Some(&customer));
    assert_eq!(from_customer.display_name.as_deref(), Some("xxww0098"));
    assert_eq!(from_customer.account_id.as_deref(), Some("27741789126905948"));
    assert_eq!(from_customer.email, None);

    // 3. BigModel 默认占位（id=unknown, username=user, displayName=User）退回 access_token JWT。
    let placeholder = json!({
        "id": "unknown",
        "username": "user",
        "displayName": "User"
    });
    let fallback_tokens = TokenSet {
        access_token: tokens.access_token.clone(),
        id_token: None,
        ..TokenSet::default()
    };
    let from_placeholder = identity_of(&fallback_tokens, Some(&placeholder));
    assert_eq!(from_placeholder.display_name.as_deref(), Some("jwt-name"));
    assert_eq!(from_placeholder.account_id.as_deref(), Some("27741789126905948"));
}
