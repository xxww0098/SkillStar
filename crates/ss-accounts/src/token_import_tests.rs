//! Pasted-credential parsing: the shapes people actually paste, and the
//! identity hints the tokens carry.

use crate::token_import;
use crate::test_support::jwt_with_claims;

#[test]
fn a_bare_token_is_the_whole_credential() {
    let imported = token_import::parse("codex", "  at-bare  ").expect("parse");
    assert_eq!(imported.tokens.access_token, "at-bare");
    assert_eq!(imported.tokens.refresh_token, None);
    assert_eq!(imported.tokens.token_type, "Bearer");
    assert_eq!(imported.identity.account_id, None);
}

#[test]
fn snake_case_json_is_read() {
    let payload = serde_json::json!({
        "access_token": "at-1",
        "refresh_token": "rt-1",
        "id_token": "id-1",
        "expires_at": 1_800_000_000_i64,
        "email": "dev@example.com",
        "account_id": "acc-1"
    })
    .to_string();
    let imported = token_import::parse("codex", &payload).expect("parse");
    assert_eq!(imported.tokens.access_token, "at-1");
    assert_eq!(imported.tokens.refresh_token.as_deref(), Some("rt-1"));
    assert_eq!(imported.tokens.id_token.as_deref(), Some("id-1"));
    assert_eq!(imported.tokens.expires_at, Some(1_800_000_000));
    assert_eq!(imported.identity.email.as_deref(), Some("dev@example.com"));
    assert_eq!(imported.identity.account_id.as_deref(), Some("acc-1"));
    assert_eq!(imported.identity.display_name.as_deref(), Some("dev@example.com"));
}

#[test]
fn camel_case_json_is_read_too() {
    let payload = serde_json::json!({
        "accessToken": "at-2",
        "refreshToken": "rt-2",
        "expiresAt": 1_800_000_000_000_i64,
        "cachedEmail": "dev@example.com"
    })
    .to_string();
    let imported = token_import::parse("cursor", &payload).expect("parse");
    assert_eq!(imported.tokens.access_token, "at-2");
    assert_eq!(imported.tokens.refresh_token.as_deref(), Some("rt-2"));
    // Milliseconds become the unix seconds the OAuth module carries.
    assert_eq!(imported.tokens.expires_at, Some(1_800_000_000));
    assert_eq!(imported.identity.email.as_deref(), Some("dev@example.com"));
}

#[test]
fn a_nested_session_object_is_descended_into() {
    let payload = serde_json::json!({
        "OPENAI_API_KEY": null,
        "tokens": {
            "access_token": "at-3",
            "refresh_token": "rt-3",
            "account_id": "acc-3"
        }
    })
    .to_string();
    let imported = token_import::parse("codex", &payload).expect("parse");
    assert_eq!(imported.tokens.access_token, "at-3");
    assert_eq!(imported.identity.account_id.as_deref(), Some("acc-3"));
}

#[test]
fn the_tokens_fill_in_the_identity_the_paste_left_out() {
    let id_token = jwt_with_claims(serde_json::json!({
        "email": "dev@example.com",
        "exp": 1_800_000_000_i64,
        "https://api.openai.com/auth": { "chatgpt_account_id": "acc-from-token" }
    }));
    let payload = serde_json::json!({ "access_token": "at-4", "id_token": id_token }).to_string();
    let imported = token_import::parse("codex", &payload).expect("parse");
    assert_eq!(imported.identity.account_id.as_deref(), Some("acc-from-token"));
    assert_eq!(imported.identity.email.as_deref(), Some("dev@example.com"));
    assert_eq!(imported.tokens.expires_at, None, "the access token carries no exp");

    let cursor_token = jwt_with_claims(serde_json::json!({ "sub": "user_01|auth-77" }));
    let imported = token_import::parse("cursor", &cursor_token).expect("parse");
    assert_eq!(imported.identity.account_id.as_deref(), Some("auth-77"));
}

#[test]
fn the_go_keyring_json_of_antigravity_is_read() {
    // cockpit 的载荷：会话嵌在 "token" 里，到期时间是 RFC3339。
    let payload = serde_json::json!({
        "token": {
            "access_token": "at-ag",
            "token_type": "Bearer",
            "refresh_token": "rt-ag",
            "expiry": "2027-01-15T08:00:00.000000Z"
        },
        "auth_method": "consumer"
    })
    .to_string();
    let imported = token_import::parse("antigravity", &payload).expect("parse");
    assert_eq!(imported.tokens.access_token, "at-ag");
    assert_eq!(imported.tokens.refresh_token.as_deref(), Some("rt-ag"));
    assert_eq!(imported.tokens.expires_at, Some(1_800_000_000));
    assert_eq!(imported.tokens.token_type, "Bearer");
}

#[test]
fn a_keychain_value_is_unwrapped_before_it_is_parsed() {
    use base64::Engine;
    let payload = serde_json::json!({
        "token": {
            "access_token": "at-keychain",
            "refresh_token": "rt-keychain",
            "expiry": "2027-01-15T08:00:00Z"
        },
        "auth_method": "consumer"
    })
    .to_string();
    let wrapped = format!(
        "go-keyring-base64:{}",
        base64::engine::general_purpose::STANDARD.encode(&payload)
    );
    let imported = token_import::parse("antigravity", &wrapped).expect("parse");
    assert_eq!(imported.tokens.access_token, "at-keychain");
    assert_eq!(imported.tokens.refresh_token.as_deref(), Some("rt-keychain"));
    assert_eq!(imported.tokens.expires_at, Some(1_800_000_000));

    let broken = token_import::parse("antigravity", "go-keyring-base64:not base64!!");
    assert!(matches!(broken, Err(crate::AccountError::Client(_))));
}

#[test]
fn an_antigravity_paste_keeps_its_email_and_never_echoes_the_tokens() {
    let payload = serde_json::json!({
        "access_token": "at-ag",
        "refresh_token": "rt-ag",
        "email": "dev@example.com"
    })
    .to_string();
    let imported = token_import::parse("antigravity", &payload).expect("parse");
    assert_eq!(imported.identity.email.as_deref(), Some("dev@example.com"));
    assert_eq!(imported.identity.display_name.as_deref(), Some("dev@example.com"));
    let printed = format!("{imported:?}");
    assert!(!printed.contains("at-ag"), "{printed}");
    assert!(!printed.contains("rt-ag"), "{printed}");
}

#[test]
fn a_paste_without_an_access_token_is_refused() {
    let error = token_import::parse("codex", r#"{"refresh_token":"rt-only"}"#).expect_err("no token");
    assert!(matches!(error, crate::AccountError::Client(_)));
    let error = token_import::parse("codex", "   ").expect_err("empty paste");
    assert!(matches!(error, crate::AccountError::Client(_)));
    assert!(token_import::parse("codex", "{not json").is_err());
}

#[test]
fn the_debug_form_never_prints_a_token() {
    let imported = token_import::parse("codex", "at-secret-value").expect("parse");
    let printed = format!("{imported:?}");
    assert!(!printed.contains("at-secret-value"), "{printed}");
}
