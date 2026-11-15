//! Codex (OpenAI) login: authorize URL, loopback listen, code exchange, refresh,
//! and the identity claims the id token carries.
//!
//! The flow is the Codex CLI's own: PKCE with S256 over a localhost redirect.
//! When the browser and this process cannot see each other the user can paste
//! the callback URL or a bare authorization code, which is why the flow is
//! LocalCallback rather than SchemePaste.

use std::sync::mpsc;

use url::Url;

use crate::oauth::agents::Started;
use crate::oauth::error::OAuthResult;
use crate::oauth::loopback::{CALLBACK_TIMEOUT, LoopbackServer, QueryMap};
use crate::oauth::pkce;
use crate::oauth::session::{CancelSignal, LoginFlow, LoginStart, PendingLogin, StartOptions};
use crate::oauth::token::{self, FormGrant, GrantKind};
use crate::oauth::{Identity, LoginOutcome, TokenSet, jwt};

pub(crate) const AGENT_ID: &str = "codex";

const AUTH_ENDPOINT: &str = "https://auth.openai.com/oauth/authorize";
const TOKEN_ENDPOINT: &str = "https://auth.openai.com/oauth/token";
/// The Codex CLI's public OAuth client; env override for a self-hosted build.
const DEFAULT_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const CLIENT_ID_ENV: &str = "SKILLSTAR_CODEX_CLIENT_ID";
const SCOPES: &str = "openid profile email offline_access";
const ORIGINATOR: &str = "codex_cli_rs";
/// Ports the official client registered for its localhost redirect.
const FIXED_PORTS: [u16; 2] = [1455, 1457];

const PLAN_CLAIM: &str = "chatgpt_plan_type";
const AUTH_NAMESPACE: &str = "https://api.openai.com/auth";

/// Start a Codex login: bind the callback listener and wait for the code.
pub(crate) fn start(login_id: &str, opts: &StartOptions) -> OAuthResult<Started> {
    let pkce = pkce::generate();
    let state = uuid::Uuid::new_v4().to_string();
    let server = LoopbackServer::bind(&ports(opts))?;
    let redirect_uri = server.redirect_uri();
    let auth_url = authorize_url(&client_id(), &redirect_uri, &pkce.challenge, &state);

    let (callback_tx, callback_rx) = mpsc::channel();
    let (pending, channel) = PendingLogin::pair(Some(callback_tx));

    let verifier = pkce.verifier;
    std::thread::spawn(move || {
        let outcome = receive_and_exchange(
            server,
            callback_rx,
            &state,
            &redirect_uri,
            &verifier,
            &channel.cancel,
        );
        let _ = channel.results.send(outcome);
    });

    Ok(Started {
        start: LoginStart {
            login_id: login_id.to_string(),
            flow: LoginFlow::LocalCallback,
            auth_url,
            user_code: None,
            verification_uri: None,
            interval_secs: None,
            expires_at: Some(crate::oauth::now_secs() + CALLBACK_TIMEOUT.as_secs() as i64),
        },
        pending,
    })
}

/// The authorize URL for one login. Pure so a test can check every parameter
/// without a browser or a network.
pub(crate) fn authorize_url(
    client_id: &str,
    redirect_uri: &str,
    challenge: &str,
    state: &str,
) -> String {
    let mut url = Url::parse(AUTH_ENDPOINT).expect("static authorize endpoint parses");
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("scope", SCOPES)
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("id_token_add_organizations", "true")
        .append_pair("codex_cli_simplified_flow", "true")
        .append_pair("state", state)
        .append_pair("originator", ORIGINATOR);
    url.to_string()
}

/// The refresh form for one refresh, pure for testing.
pub(crate) fn refresh_grant(refresh_token: &str, client_id: &str) -> FormGrant {
    FormGrant {
        url: TOKEN_ENDPOINT.to_string(),
        client_id: client_id.to_string(),
        refresh_token: Some(refresh_token.to_string()),
        ..FormGrant::default()
    }
}

/// Refresh a Codex credential.
pub(crate) fn refresh(tokens: &TokenSet) -> OAuthResult<TokenSet> {
    let refresh_token = crate::oauth::agents::refresh_token_of(AGENT_ID, tokens)?;
    let grant = refresh_grant(&refresh_token, &client_id());
    let refreshed = token::post_token(&grant, GrantKind::RefreshToken)?;
    Ok(token::carry_over(tokens, refreshed))
}

/// The client id this process sends: the env override when it is set and
/// non-blank, else the Codex CLI's public client.
pub(crate) fn client_id() -> String {
    resolve_client_id(std::env::var(CLIENT_ID_ENV).ok())
}

fn resolve_client_id(configured: Option<String>) -> String {
    configured
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_CLIENT_ID.to_string())
}

/// Candidate callback ports in priority order: an explicit request first, then
/// the two ports the official client registered.
fn ports(opts: &StartOptions) -> Vec<u16> {
    let mut ports = Vec::with_capacity(FIXED_PORTS.len() + 1);
    if let Some(port) = opts.prefer_port {
        ports.push(port);
    }
    for port in FIXED_PORTS {
        if !ports.contains(&port) {
            ports.push(port);
        }
    }
    ports
}

fn receive_and_exchange(
    server: LoopbackServer,
    callbacks: mpsc::Receiver<QueryMap>,
    state: &str,
    redirect_uri: &str,
    verifier: &str,
    cancel: &CancelSignal,
) -> OAuthResult<LoginOutcome> {
    let query = server.wait_for_query(&callbacks, CALLBACK_TIMEOUT, || cancel.is_cancelled())?;
    let code = code_from_query(&query, state)?;
    let tokens = exchange_code(&code, redirect_uri, verifier)?;
    let identity = identity_from_tokens(&tokens);
    Ok(LoginOutcome {
        agent: AGENT_ID.to_string(),
        tokens,
        identity,
        provider_state: None,
    })
}

/// The callback-code extraction every loopback flow shares (see
/// `agents::code_from_query`); re-exported so this module's tests can keep
/// exercising it next to the flow that uses it.
pub(crate) use super::code_from_query;

fn exchange_code(code: &str, redirect_uri: &str, verifier: &str) -> OAuthResult<TokenSet> {
    let grant = FormGrant {
        url: TOKEN_ENDPOINT.to_string(),
        client_id: client_id(),
        code: Some(code.to_string()),
        redirect_uri: Some(redirect_uri.to_string()),
        code_verifier: Some(verifier.to_string()),
        ..FormGrant::default()
    };
    token::post_token(&grant, GrantKind::AuthorizationCode)
}

/// The account identity the id token describes. Unreadable or absent claims are
/// simply unknown: the id token is never a condition for a finished login.
pub(crate) fn identity_from_tokens(tokens: &TokenSet) -> Identity {
    let Some(id_token) = tokens.id_token.as_deref() else {
        return Identity::default();
    };
    let Some(payload) = jwt::payload(id_token) else {
        return Identity::default();
    };
    Identity {
        email: jwt::claim(&payload, &["email"]),
        display_name: jwt::claim(&payload, &["name"]),
        plan: jwt::claim(&payload, &[PLAN_CLAIM]).or_else(|| {
            jwt::claim(&payload, &[AUTH_NAMESPACE, PLAN_CLAIM])
        }),
        account_id: jwt::claim(&payload, &[AUTH_NAMESPACE, "chatgpt_account_id"]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oauth::error::OAuthError;
    use crate::oauth::token::{GrantKind, classify_status, form_pairs};
    use serde_json::json;
    use std::collections::BTreeMap;

    fn query_of(url: &str) -> BTreeMap<String, String> {
        Url::parse(url)
            .expect("authorize url parses")
            .query_pairs()
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect()
    }

    fn callback(code: &str, state: &str) -> QueryMap {
        QueryMap::from([
            ("code".to_string(), code.to_string()),
            ("state".to_string(), state.to_string()),
        ])
    }

    #[test]
    fn authorize_url_carries_every_required_parameter() {
        let url = authorize_url(
            DEFAULT_CLIENT_ID,
            "http://localhost:1455/auth/callback",
            "challenge-1",
            "state-1",
        );
        let parsed = Url::parse(&url).expect("url");
        assert_eq!(parsed.scheme(), "https");
        assert_eq!(parsed.host_str(), Some("auth.openai.com"));
        assert_eq!(parsed.path(), "/oauth/authorize");

        let query = query_of(&url);
        assert_eq!(query.get("response_type").map(String::as_str), Some("code"));
        assert_eq!(
            query.get("client_id").map(String::as_str),
            Some(DEFAULT_CLIENT_ID)
        );
        assert_eq!(
            query.get("redirect_uri").map(String::as_str),
            Some("http://localhost:1455/auth/callback")
        );
        assert_eq!(
            query.get("scope").map(String::as_str),
            Some("openid profile email offline_access")
        );
        assert_eq!(
            query.get("code_challenge").map(String::as_str),
            Some("challenge-1")
        );
        assert_eq!(
            query.get("code_challenge_method").map(String::as_str),
            Some("S256")
        );
        assert_eq!(query.get("state").map(String::as_str), Some("state-1"));
        assert_eq!(
            query.get("codex_cli_simplified_flow").map(String::as_str),
            Some("true")
        );
        assert_eq!(
            query.get("id_token_add_organizations").map(String::as_str),
            Some("true")
        );
        assert_eq!(
            query.get("originator").map(String::as_str),
            Some("codex_cli_rs")
        );
    }

    #[test]
    fn refresh_request_body_shape() {
        let grant = refresh_grant("refresh-1", "client-1");
        let pairs = form_pairs(&grant, GrantKind::RefreshToken);
        let map: BTreeMap<&str, &str> = pairs
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect();
        assert_eq!(map.get("grant_type"), Some(&"refresh_token"));
        assert_eq!(map.get("refresh_token"), Some(&"refresh-1"));
        assert_eq!(map.get("client_id"), Some(&"client-1"));
        assert_eq!(map.len(), 3, "no code, secret or verifier on a refresh");
        assert_eq!(grant.url, TOKEN_ENDPOINT);
    }

    #[test]
    fn refresh_without_a_refresh_token_requires_a_login() {
        let error = refresh(&TokenSet::default()).expect_err("no refresh token");
        assert!(matches!(error, OAuthError::AuthRequired(_)), "got {error:?}");
    }

    #[test]
    fn failure_classification_keeps_5xx_retryable() {
        assert!(matches!(
            classify_status(401, "{}"),
            OAuthError::AuthRequired(_)
        ));
        assert!(matches!(
            classify_status(400, "{\"error\":\"invalid_grant\"}"),
            OAuthError::AuthRequired(_)
        ));
        for status in [429, 500, 502, 503] {
            assert!(
                matches!(classify_status(status, ""), OAuthError::Transient(_)),
                "{status} must not fail the login"
            );
        }
        assert!(matches!(
            classify_status(400, "{\"error\":\"invalid_request\"}"),
            OAuthError::Rejected(_)
        ));
    }

    #[test]
    fn callback_code_is_extracted_and_checked() {
        assert_eq!(
            code_from_query(&callback("code-1", "state-1"), "state-1").expect("code"),
            "code-1"
        );

        let denied = QueryMap::from([
            ("error".to_string(), "access_denied".to_string()),
            ("error_description".to_string(), "user said no".to_string()),
        ]);
        let error = code_from_query(&denied, "state-1").expect_err("denied");
        assert!(matches!(error, OAuthError::Rejected(_)), "got {error:?}");
        assert!(error.to_string().contains("access_denied"));

        let mismatched = code_from_query(&callback("code-1", "other"), "state-1");
        assert!(matches!(mismatched, Err(OAuthError::Rejected(_))));

        let bare_code = QueryMap::from([("code".to_string(), "pasted".to_string())]);
        assert_eq!(
            code_from_query(&bare_code, "state-1").expect("pasted code"),
            "pasted"
        );

        assert!(matches!(
            code_from_query(&QueryMap::new(), "state-1"),
            Err(OAuthError::Rejected(_))
        ));
    }

    #[test]
    fn identity_comes_from_the_id_token_without_gating_the_login() {
        let claims = json!({
            "email": "dev@example.com",
            "name": "Dev",
            "chatgpt_plan_type": "plus"
        });
        let tokens = TokenSet {
            access_token: "access".to_string(),
            id_token: Some(crate::oauth::jwt::test_token(&claims)),
            ..TokenSet::default()
        };
        let identity = identity_from_tokens(&tokens);
        assert_eq!(identity.email.as_deref(), Some("dev@example.com"));
        assert_eq!(identity.display_name.as_deref(), Some("Dev"));
        assert_eq!(identity.plan.as_deref(), Some("plus"));

        let planned = json!({
            "https://api.openai.com/auth": {
                "chatgpt_plan_type": "pro",
                "chatgpt_account_id": "acct-1"
            }
        });
        let tokens = TokenSet {
            access_token: "access".to_string(),
            id_token: Some(crate::oauth::jwt::test_token(&planned)),
            ..TokenSet::default()
        };
        let identity = identity_from_tokens(&tokens);
        assert_eq!(identity.plan.as_deref(), Some("pro"));
        assert_eq!(identity.account_id.as_deref(), Some("acct-1"));
        assert_eq!(identity.email, None);

        let opaque = TokenSet {
            access_token: "access".to_string(),
            id_token: Some("not-a-jwt".to_string()),
            ..TokenSet::default()
        };
        assert_eq!(identity_from_tokens(&opaque), Identity::default());
        assert_eq!(identity_from_tokens(&TokenSet::default()), Identity::default());
    }

    #[test]
    fn client_id_prefers_a_configured_override() {
        assert_eq!(resolve_client_id(None), DEFAULT_CLIENT_ID);
        assert_eq!(resolve_client_id(Some("  ".to_string())), DEFAULT_CLIENT_ID);
        assert_eq!(resolve_client_id(Some(" custom ".to_string())), "custom");
    }

    #[test]
    fn callback_port_order_starts_with_the_requested_one() {
        let preferred = StartOptions {
            prefer_port: Some(43001),
            ..StartOptions::default()
        };
        assert_eq!(ports(&preferred), vec![43001, 1455, 1457]);
        let fixed = StartOptions::default();
        assert_eq!(ports(&fixed), vec![1455, 1457]);
        let duplicate = StartOptions {
            prefer_port: Some(1457),
            ..StartOptions::default()
        };
        assert_eq!(ports(&duplicate), vec![1457, 1455]);
    }
}

