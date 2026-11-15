//! Gemini CLI 额度切片的测试：请求体、端点选择、分类与 provider_state。
//!
//! 一律不碰真网络：fetch 的两条早退分支（没有 token、token 已过期）在发请求之前
//! 就返回，其余断言都在纯函数上。

use super::*;
use crate::oauth::TokenSet;
use crate::test_support::Sandbox;
use crate::usage::credentials::Credentials;
use crate::usage::fetch::antigravity_parse::OnboardStep;
use serde_json::json;

fn credentials(tokens: TokenSet) -> Credentials {
    Credentials {
        tokens,
        provider_state: None,
    }
}

#[test]
fn the_load_body_asks_for_full_eligibility_and_carries_the_project() {
    let without = load_code_assist_body(None);
    assert_eq!(without["mode"], json!("FULL_ELIGIBILITY_CHECK"));
    assert!(without.get("cloudaicompanionProject").is_none());
    assert_eq!(without["metadata"]["pluginType"], json!("GEMINI"));
    assert_eq!(without["metadata"]["ideType"], json!("IDE_UNSPECIFIED"));
    assert_eq!(without["metadata"]["platform"], json!("PLATFORM_UNSPECIFIED"));

    let with = load_code_assist_body(Some("proj-1"));
    assert_eq!(with["cloudaicompanionProject"], json!("proj-1"));
    assert_eq!(with["metadata"]["duetProject"], json!("proj-1"));
}

#[test]
fn the_metadata_never_claims_the_antigravity_product() {
    let metadata = metadata(Some("proj-1")).to_string();
    assert!(!metadata.contains("ANTIGRAVITY"), "{metadata}");
    assert!(!metadata.contains("antigravity"), "{metadata}");
}

#[test]
fn the_provider_state_carries_only_a_usable_project() {
    assert_eq!(provider_state(Some("proj-1"))["project_id"], json!("proj-1"));
    assert_eq!(provider_state(Some("   ")), json!({}));
    assert_eq!(provider_state(None), json!({}));
}

#[test]
fn a_read_without_an_access_token_asks_for_a_new_login() {
    let error = fetch(&credentials(TokenSet::default())).expect_err("no token");
    assert!(matches!(error, UsageError::AuthRequired(_)), "got {error:?}");
}

#[test]
fn an_expired_token_is_unreadable_not_invalid() {
    // 这个切片拒绝兑换 refresh token，所以过期只能报 Unreadable：导入路径要原样
    // 保存账号，而不是把它标成失效。
    let tokens = TokenSet {
        access_token: "at".to_string(),
        refresh_token: Some("rt".to_string()),
        expires_at: Some(super::super::unix_now() - 60),
        ..TokenSet::default()
    };
    let error = fetch(&credentials(tokens)).expect_err("expired");
    assert!(matches!(error, UsageError::Unreadable(_)), "got {error:?}");
    assert!(error.to_string().contains("已过期"), "{error}");
}

#[test]
fn the_endpoint_defaults_to_the_production_cloud_code_host() {
    let sandbox = Sandbox::new();
    assert_eq!(base_url(), BASE_URL);
    assert!(base_url().starts_with("https://cloudcode-pa.googleapis.com"));

    // 覆盖变量只为调试/代理：空值等于没设。
    unsafe { std::env::set_var(BASE_URL_ENV, "  ") };
    assert_eq!(base_url(), BASE_URL);
    unsafe { std::env::set_var(BASE_URL_ENV, "https://proxy.example/gemini/") };
    assert_eq!(base_url(), "https://proxy.example/gemini");
    drop(sandbox);
}

#[test]
fn an_unfinished_onboard_operation_reports_its_name() {
    let pending = onboard_step(&json!({ "name": "operations/abc" })).expect("pending");
    assert_eq!(pending, OnboardStep::Pending("operations/abc".to_string()));

    let done = onboard_step(&json!({
        "done": true,
        "response": { "cloudaicompanionProject": { "id": "proj-2" } }
    }))
    .expect("done");
    assert_eq!(done, OnboardStep::Done(Some("proj-2".to_string())));

    let nameless = onboard_step(&json!({ "done": false })).expect_err("no name");
    assert!(matches!(nameless, UsageError::Unreadable(_)), "got {nameless:?}");
}

#[test]
fn a_cached_project_that_was_refused_is_recognised() {
    let refused = UsageError::Unreadable("gemini-cli loadCodeAssist: HTTP 400 {}".to_string());
    assert!(is_bad_request(&refused));
    assert!(!is_bad_request(&UsageError::Transient("boom".to_string())));
    assert!(!is_endpoint_missing(&refused));
    assert!(is_endpoint_missing(&UsageError::Unreadable(
        "gemini-cli retrieveUserQuota: HTTP 404 {}".to_string()
    )));
}
