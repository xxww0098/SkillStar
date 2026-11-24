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

#[test]
fn safe_storage_ciphertext_is_refused_as_access_token_and_stripped_from_refresh() {
    let err = token_import::parse("cursor", "cursor-safe-storage:v1:encrypted").expect_err("ciphertext");
    assert!(matches!(err, crate::AccountError::Client(_)));

    let payload = serde_json::json!({
        "accessToken": "plain-cursor-jwt",
        "refreshToken": "cursor-safe-storage:v1:encrypted-rt",
        "email": "dev@example.com"
    })
    .to_string();
    let imported = token_import::parse("cursor", &payload).expect("plain access with stripped refresh");
    assert_eq!(imported.tokens.access_token, "plain-cursor-jwt");
    assert_eq!(imported.tokens.refresh_token, None);
}

#[test]
fn zcode_paste_unwraps_enc_v1_and_extracts_jwt_and_provider_state() {
    let _sandbox = crate::test_support::Sandbox::new();
    let roots = crate::switch::paths::PathRoots::from_env();
    let key = crate::switch::zcode_store::credential_key(
        &roots.home,
        &crate::switch::zcode_store::current_username(),
    );
    let seal = |plain: &str| crate::switch::zcode_store::seal(&key, plain).expect("seal");
    let jwt = crate::test_support::jwt_with_claims(serde_json::json!({
        "sub": "u-bigmodel",
        "email": "big@example.com",
        "exp": 1_800_000_000_i64
    }));

    let credentials_payload = serde_json::json!({
        "oauth:active_provider": seal("bigmodel"),
        "oauth:bigmodel:access_token": seal("at-big"),
        "oauth:bigmodel:refresh_token": seal("rt-big"),
        "oauth:bigmodel:user_info": seal(r#"{"id":"u-bigmodel","username":"Big User","email":"big@example.com"}"#),
        "zcodejwttoken": seal(&jwt)
    })
    .to_string();

    let imported = token_import::parse("zcode", &credentials_payload).expect("zcode credentials");
    assert_eq!(imported.tokens.access_token, "at-big");
    assert_eq!(imported.tokens.refresh_token.as_deref(), Some("rt-big"));
    assert_eq!(imported.tokens.id_token.as_deref(), Some(jwt.as_str()));
    assert_eq!(imported.tokens.expires_at, Some(1_800_000_000));
    assert_eq!(imported.identity.account_id.as_deref(), Some("u-bigmodel"));
    assert_eq!(imported.identity.email.as_deref(), Some("big@example.com"));
    assert_eq!(
        imported.provider_state,
        Some(serde_json::json!({ "provider": "bigmodel" }))
    );

    // Direct enc:v1 sealed JSON string + flat JWT-only JSON also work.
    let sealed_json = seal(&serde_json::json!({
        "provider": "zai",
        "zcodejwttoken": jwt
    })
    .to_string());
    let imported_sealed = token_import::parse("zcode", &sealed_json).expect("sealed json");
    assert_eq!(imported_sealed.tokens.id_token.as_deref(), Some(jwt.as_str()));
    assert_eq!(
        imported_sealed.provider_state,
        Some(serde_json::json!({ "provider": "zai" }))
    );
}

#[test]
fn devin_desktop_paste_accepts_api_key_variants_and_preserves_api_server_url() {
    let camel = serde_json::json!({
        "apiKey": "sk-devin-1",
        "apiServerUrl": "https://server.devin.local",
        "email": "devin@example.com"
    })
    .to_string();
    let parsed_camel = token_import::parse("devin-desktop", &camel).expect("camel");
    assert_eq!(parsed_camel.tokens.access_token, "sk-devin-1");
    assert_eq!(parsed_camel.identity.email.as_deref(), Some("devin@example.com"));
    assert_eq!(
        parsed_camel.provider_state,
        Some(serde_json::json!({ "api_server_url": "https://server.devin.local" }))
    );

    let snake = serde_json::json!({
        "api_key": "sk-devin-2",
        "api_server_url": "https://api.codeium.com"
    })
    .to_string();
    let parsed_snake = token_import::parse("devin-desktop", &snake).expect("snake");
    assert_eq!(parsed_snake.tokens.access_token, "sk-devin-2");
    assert_eq!(
        parsed_snake.provider_state,
        Some(serde_json::json!({ "api_server_url": "https://api.codeium.com" }))
    );
}

#[test]
fn grok_paste_keeps_json_metadata_and_rejects_bare_opaque_tokens() {
    let err = token_import::parse("grok", "xai-opaque-bare-token").expect_err("reject opaque grok");
    assert!(matches!(err, crate::AccountError::Client(_)));

    let scoped = serde_json::json!({
        "https://auth.x.ai::custom-client": {
            "key": "grok-at",
            "refresh_token": "grok-rt",
            "email": "grok@example.com",
            "user_id": "grok-user-1",
            "first_name": "Ada",
            "last_name": "Lovelace",
            "expires_at": "2027-01-15T08:00:00Z"
        }
    })
    .to_string();
    let parsed = token_import::parse("grok", &scoped).expect("grok auth.json");
    assert_eq!(parsed.tokens.access_token, "grok-at");
    assert_eq!(parsed.tokens.refresh_token.as_deref(), Some("grok-rt"));
    assert_eq!(parsed.tokens.expires_at, Some(1_800_000_000));
    assert_eq!(parsed.identity.email.as_deref(), Some("grok@example.com"));
    assert_eq!(parsed.identity.account_id.as_deref(), Some("grok-user-1"));
    assert_eq!(parsed.identity.display_name.as_deref(), Some("Ada Lovelace"));

    // A bare JWT with claims carries metadata and is accepted.
    let jwt = crate::test_support::jwt_with_claims(serde_json::json!({
        "email": "jwt@x.ai",
        "sub": "u-jwt",
        "exp": 1_800_000_000_i64
    }));
    let parsed_jwt = token_import::parse("grok", &jwt).expect("grok jwt");
    assert_eq!(parsed_jwt.identity.email.as_deref(), Some("jwt@x.ai"));
    assert_eq!(parsed_jwt.identity.account_id.as_deref(), Some("u-jwt"));
}

#[test]
fn kiro_paste_preserves_idc_provider_state_fields() {
    let payload = serde_json::json!({
        "accessToken": "at-kiro",
        "refreshToken": "rt-kiro",
        "email": "kiro@example.com",
        "userId": "user-kiro-1",
        "region": "eu-west-1",
        "startUrl": "https://d-123.awsapps.com/start",
        "profileArn": "arn:aws:codewhisperer:eu-west-1:1:profile/K1",
        "clientId": "cid-1",
        "clientSecret": "csec-1"
    })
    .to_string();
    let parsed = token_import::parse("kiro", &payload).expect("kiro paste");
    assert_eq!(parsed.tokens.access_token, "at-kiro");
    assert_eq!(parsed.identity.email.as_deref(), Some("kiro@example.com"));
    assert_eq!(parsed.identity.account_id.as_deref(), Some("user-kiro-1"));
    let state = crate::oauth::agents::kiro::KiroState::parse(parsed.provider_state.as_ref());
    assert_eq!(state.region.as_deref(), Some("eu-west-1"));
    assert_eq!(
        state.start_url.as_deref(),
        Some("https://d-123.awsapps.com/start")
    );
    assert_eq!(
        state.profile_arn.as_deref(),
        Some("arn:aws:codewhisperer:eu-west-1:1:profile/K1")
    );
    assert_eq!(state.client_id.as_deref(), Some("cid-1"));
    assert_eq!(state.client_secret.as_deref(), Some("csec-1"));
}
