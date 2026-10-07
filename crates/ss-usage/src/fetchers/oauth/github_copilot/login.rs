//! Browser login for the VS Code Copilot OAuth client.
//!
//! GitHub redirects to `https://vscode.dev/redirect`. That page 302s to the
//! loopback URL carried in `state`, appending `code` and echoing `state`.
//! A pasted vscode.dev URL is rewritten the same way before replay.

use url::Url;

use super::quota::{self, CATALOG_ID, Endpoints};
use crate::fetchers::oauth::common::{self, SubscriptionBuilder};
use crate::oauth::local_server::{self, CallbackSession};
use crate::oauth::pkce::PkcePair;
use crate::oauth::token_endpoint::{self, TokenResponse};
use crate::storage;
use crate::subscription::Subscription;
use crate::{UsageError, UsageResult};

/// Public VS Code Copilot OAuth client. The secret ships in the VS Code client.
const CLIENT_ID: &str = "01ab8ac9400c4e429b23";
const CLIENT_SECRET: &str = "2af589bb2ffd03a29cc0df83f767e3f6693f14cd";
const AUTHORIZE_URL: &str = "https://github.com/login/oauth/authorize";
const TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
const REDIRECT_URI: &str = "https://vscode.dev/redirect";
const SCOPE: &str = "read:user repo user:email workflow";
const TITLE_PLACEHOLDERS: &[&str] = &["GitHub Copilot"];

pub(crate) struct GitHubIdentity {
    pub login: String,
    pub email: Option<String>,
}

pub(crate) async fn start_login(
    _region: Option<&str>,
    target_subscription_id: Option<&str>,
) -> UsageResult<super::super::OAuthStartInfo> {
    let pkce = PkcePair::generate();
    let port = pick_port()?;
    let nonce = crate::oauth::pkce::random_state();
    let callback_url = format!("http://127.0.0.1:{port}/callback?nonce={nonce}");
    let session = local_server::start_session(port, None)?;
    let auth_url = build_authorize_url(&pkce.challenge, &callback_url);

    let pending_id = crate::oauth::pending_state::register_with_callback_port(
        CATALOG_ID,
        None,
        auth_url.clone(),
        Some(port),
    );
    crate::oauth::pending_state::set_target_subscription_id(
        &pending_id,
        target_subscription_id.map(str::to_string),
    );

    let pid = pending_id.clone();
    let verifier = pkce.verifier;
    tokio::spawn(async move {
        let target = crate::oauth::pending_state::target_subscription_id(&pid);
        let result = drive_login(session, callback_url, verifier, target).await;
        if let Some(tx) = crate::oauth::pending_state::take_sender(&pid) {
            let _ = tx.send(result);
        }
    });

    Ok(super::super::OAuthStartInfo::browser(auth_url, pending_id))
}

/// vscode.dev paste → the loopback URL that page would redirect to.
/// Anything else is returned unchanged.
pub(crate) fn normalize_callback_input(input: &str) -> UsageResult<String> {
    let trimmed = input.trim();
    let Ok(url) = Url::parse(trimmed) else {
        return Ok(trimmed.to_string());
    };
    if url.scheme() != "https" || url.host_str() != Some("vscode.dev") || url.path() != "/redirect"
    {
        return Ok(trimmed.to_string());
    }
    let Some(state) = url
        .query_pairs()
        .find(|(key, _)| key == "state")
        .map(|(_, value)| value.into_owned())
        .filter(|state| !state.is_empty())
    else {
        return Ok(trimmed.to_string());
    };
    Ok(replay_loopback(&state, &url))
}

fn replay_loopback(state: &str, outer: &Url) -> String {
    let Ok(mut target) = Url::parse(state) else {
        return state.to_string();
    };
    let mut pairs: Vec<(String, String)> = target
        .query_pairs()
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    for (key, value) in outer.query_pairs() {
        if key != "state" {
            pairs.push((key.into_owned(), value.into_owned()));
        }
    }
    pairs.push(("state".to_string(), state.to_string()));
    target.set_query(None);
    {
        let mut query = target.query_pairs_mut();
        for (key, value) in &pairs {
            query.append_pair(key, value);
        }
    }
    target.to_string()
}

fn build_authorize_url(code_challenge: &str, callback_url: &str) -> String {
    let mut params = url::form_urlencoded::Serializer::new(String::new());
    params.append_pair("client_id", CLIENT_ID);
    params.append_pair("redirect_uri", REDIRECT_URI);
    params.append_pair("scope", SCOPE);
    params.append_pair("state", callback_url);
    params.append_pair("code_challenge", code_challenge);
    params.append_pair("code_challenge_method", "S256");
    params.append_pair("get_started_with", "copilot-vscode");
    params.append_pair("prompt", "select_account");
    format!("{AUTHORIZE_URL}?{}", params.finish())
}

fn pick_port() -> UsageResult<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .map_err(|error| UsageError::Other(format!("无法监听 GitHub Copilot 回调: {error}")))?;
    listener
        .local_addr()
        .map(|addr| addr.port())
        .map_err(|error| UsageError::Other(format!("无法读取 GitHub Copilot 回调端口: {error}")))
}

async fn drive_login(
    session: CallbackSession,
    callback_url: String,
    verifier: String,
    target_subscription_id: Option<String>,
) -> UsageResult<Subscription> {
    let params = local_server::wait(
        session,
        callback_url,
        Some(std::time::Duration::from_secs(300)),
    )
    .await?;
    let code = local_server::callback_code(&params)?;
    let client = crate::fetchers::http_client()?;
    let tokens = exchange_code(&client, TOKEN_URL, &code, &verifier).await?;
    crate::refresh_guard::with_catalog_lock(CATALOG_ID, || async {
        finalize(tokens, target_subscription_id.as_deref()).await
    })
    .await?
}

/// GitHub's token endpoint is form-urlencoded unless `Accept: application/json`.
/// [`token_endpoint::post_token`] cannot set that header, so the POST lives
/// here and the status table stays `parse_token_body`.
pub(super) async fn exchange_code(
    client: &reqwest::Client,
    token_url: &str,
    code: &str,
    verifier: &str,
) -> UsageResult<TokenResponse> {
    let response = client
        .post(token_url)
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::USER_AGENT, "SkillStar")
        .form(&[
            ("client_id", CLIENT_ID),
            ("client_secret", CLIENT_SECRET),
            ("code", code),
            ("redirect_uri", REDIRECT_URI),
            ("code_verifier", verifier),
        ])
        .send()
        .await
        .map_err(|error| UsageError::transport("GitHub Copilot token", error))?;
    let status = response.status().as_u16();
    let body = response.text().await.unwrap_or_default();
    token_endpoint::parse_token_body(status, &body, "GitHub Copilot token")
}

async fn finalize(
    tokens: TokenResponse,
    target_subscription_id: Option<&str>,
) -> UsageResult<Subscription> {
    let access_token = tokens
        .access_token()
        .ok_or(UsageError::AuthRequired)?
        .to_string();
    let client = crate::fetchers::http_client()?;
    let endpoints = Endpoints::production();
    let identity = load_github_identity(&client, &access_token, &endpoints).await?;
    let sub = build_subscription(&access_token, &identity, target_subscription_id)?;
    // Quota is best-effort at login, same as Codex: a Copilot hiccup must not
    // drop the GitHub account we just authorized.
    if let Ok(usage) =
        quota::fetch_with_github_token(&client, &sub.id, &access_token, &endpoints).await
    {
        storage::save_usage_snapshot(usage).ok();
    }
    storage::upsert_subscription(sub)
        .map_err(|error| UsageError::Other(format!("GitHub Copilot 订阅保存失败：{error}")))
}

pub(super) fn build_subscription(
    github_token: &str,
    identity: &GitHubIdentity,
    target_subscription_id: Option<&str>,
) -> UsageResult<Subscription> {
    let display_name = identity
        .email
        .clone()
        .filter(|email| common::looks_like_email(email))
        .unwrap_or_else(|| identity.login.clone());
    let mut sub = SubscriptionBuilder::new(CATALOG_ID, display_name, "USD", github_token, None)
        .oauth_account_id(Some(identity.login.clone()))
        .build();
    if let Some(existing) = common::reauth_target(CATALOG_ID, target_subscription_id) {
        common::carry_over_user_metadata(&mut sub, &existing, TITLE_PLACEHOLDERS);
    }
    Ok(sub)
}

pub(super) fn apply_identity(subscription: &mut Subscription, identity: &GitHubIdentity) {
    if !identity.login.is_empty() {
        subscription.oauth_account_id = Some(identity.login.clone());
    }
    if let Some(email) = identity
        .email
        .as_deref()
        .filter(|email| common::looks_like_email(email))
    {
        common::apply_email_title(subscription, Some(email), TITLE_PLACEHOLDERS);
        return;
    }
    let current = subscription.display_name.trim();
    let placeholder = current.is_empty()
        || TITLE_PLACEHOLDERS
            .iter()
            .any(|name| current.eq_ignore_ascii_case(name));
    if placeholder && !identity.login.is_empty() {
        subscription.display_name = identity.login.clone();
    }
}

pub(super) async fn load_github_identity(
    client: &reqwest::Client,
    github_token: &str,
    endpoints: &Endpoints,
) -> UsageResult<GitHubIdentity> {
    let user = github_get(
        client,
        &endpoints.github_user,
        github_token,
        "GitHub Copilot user",
    )
    .await?;
    let login = user
        .get("login")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|login| !login.is_empty())
        .ok_or_else(|| UsageError::Fetcher("GitHub 用户缺少 login".into()))?
        .to_string();
    let email = match user
        .get("email")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
    {
        Some(email) if !email.is_empty() => Some(email.to_string()),
        _ => fetch_primary_email(client, &endpoints.github_emails, github_token).await?,
    };
    Ok(GitHubIdentity { login, email })
}

async fn fetch_primary_email(
    client: &reqwest::Client,
    url: &str,
    github_token: &str,
) -> UsageResult<Option<String>> {
    let response = client
        .get(url)
        .bearer_auth(github_token)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .header(reqwest::header::USER_AGENT, "SkillStar")
        .send()
        .await
        .map_err(|error| UsageError::transport("GitHub Copilot emails", error))?;
    let status = response.status().as_u16();
    if matches!(status, 401 | 403 | 404) {
        return Ok(None);
    }
    let body = response.text().await.unwrap_or_default();
    if !(200..300).contains(&status) {
        return Err(UsageError::http_status(
            "GitHub Copilot emails",
            status,
            &body,
        ));
    }
    let emails: Vec<serde_json::Value> = serde_json::from_str(&body).map_err(|error| {
        UsageError::Fetcher(format!("GitHub Copilot emails 响应解析失败: {error}"))
    })?;
    let verified = |item: &serde_json::Value| {
        item.get("verified").and_then(serde_json::Value::as_bool) == Some(true)
    };
    let address = |item: &serde_json::Value| {
        item.get("email")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|email| !email.is_empty())
            .map(str::to_string)
    };
    let primary = emails.iter().find(|item| {
        verified(item) && item.get("primary").and_then(serde_json::Value::as_bool) == Some(true)
    });
    Ok(primary
        .and_then(address)
        .or_else(|| emails.iter().find(|item| verified(item)).and_then(address)))
}

async fn github_get(
    client: &reqwest::Client,
    url: &str,
    github_token: &str,
    label: &str,
) -> UsageResult<serde_json::Value> {
    let response = client
        .get(url)
        .bearer_auth(github_token)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .header(reqwest::header::USER_AGENT, "SkillStar")
        .send()
        .await
        .map_err(|error| UsageError::transport(label, error))?;
    let status = response.status().as_u16();
    let body = response.text().await.unwrap_or_default();
    if status == 401 {
        return Err(UsageError::AuthRequired);
    }
    if !(200..300).contains(&status) {
        return Err(UsageError::http_status(label, status, &body));
    }
    serde_json::from_str(&body)
        .map_err(|error| UsageError::Fetcher(format!("{label} 响应解析失败: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto;

    #[test]
    fn authorize_url_matches_vscode_copilot_login() {
        let url = build_authorize_url("challenge-value", "http://127.0.0.1:9/callback?nonce=n");
        let parsed = Url::parse(&url).unwrap();
        assert_eq!(parsed.host_str(), Some("github.com"));
        assert_eq!(parsed.path(), "/login/oauth/authorize");
        let query: std::collections::HashMap<_, _> = parsed.query_pairs().into_owned().collect();
        assert_eq!(query.get("client_id").map(String::as_str), Some(CLIENT_ID));
        assert_eq!(
            query.get("redirect_uri").map(String::as_str),
            Some(REDIRECT_URI)
        );
        assert_eq!(
            query.get("state").map(String::as_str),
            Some("http://127.0.0.1:9/callback?nonce=n")
        );
        assert_eq!(
            query.get("code_challenge").map(String::as_str),
            Some("challenge-value")
        );
        assert_eq!(
            query.get("code_challenge_method").map(String::as_str),
            Some("S256")
        );
        assert_eq!(query.get("scope").map(String::as_str), Some(SCOPE));
        assert_eq!(
            query.get("get_started_with").map(String::as_str),
            Some("copilot-vscode")
        );
        assert_eq!(
            query.get("prompt").map(String::as_str),
            Some("select_account")
        );
    }

    #[test]
    fn vscode_dev_redirect_unwraps_to_the_loopback_callback() {
        let pasted = "https://vscode.dev/redirect?code=abc&state=http%3A%2F%2F127.0.0.1%3A9%2Fcallback%3Fnonce%3Dn";
        let normalized = normalize_callback_input(pasted).unwrap();
        let url = Url::parse(&normalized).unwrap();
        assert_eq!(url.host_str(), Some("127.0.0.1"));
        assert_eq!(url.path(), "/callback");
        let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(query.get("nonce").map(String::as_str), Some("n"));
        assert_eq!(query.get("code").map(String::as_str), Some("abc"));
        assert_eq!(
            query.get("state").map(String::as_str),
            Some("http://127.0.0.1:9/callback?nonce=n")
        );
    }

    #[test]
    fn normalize_callback_input_leaves_other_pastes_unchanged() {
        let localhost = "http://127.0.0.1:9/callback?code=abc";
        assert_eq!(normalize_callback_input(localhost).unwrap(), localhost);
        assert_eq!(normalize_callback_input("abc").unwrap(), "abc");
        let other = "https://github.com/login?code=abc";
        assert_eq!(normalize_callback_input(other).unwrap(), other);
    }

    #[test]
    fn built_row_stores_the_github_token_and_login_only() {
        let identity = GitHubIdentity {
            login: "octocat".into(),
            email: Some("octo@example.com".into()),
        };
        let sub = build_subscription("gho_longlived", &identity, None).unwrap();
        assert_eq!(sub.catalog_id, CATALOG_ID);
        assert_eq!(
            crypto::decrypt(sub.access_token_encrypted.as_deref().unwrap()),
            "gho_longlived"
        );
        assert!(sub.refresh_token_encrypted.is_none());
        assert!(sub.provider_state_encrypted.is_none());
        assert!(sub.id_token_encrypted.is_none());
        assert_eq!(sub.oauth_account_id.as_deref(), Some("octocat"));
        assert_eq!(sub.display_name, "octo@example.com");
        assert!(sub.access_token_expires_at.is_none());
    }

    #[tokio::test]
    async fn token_exchange_classifies_status_like_post_token() {
        let client = super::super::quota::test_support::no_proxy_client();
        let (url, _hits) = super::super::quota::test_support::spawn(vec![(401, "unauthorized")]);
        let err = exchange_code(&client, &url, "code", "verifier")
            .await
            .unwrap_err();
        assert!(matches!(err, UsageError::AuthRequired), "{err:?}");

        let (url, _hits) = super::super::quota::test_support::spawn(vec![(429, "slow down")]);
        let err = exchange_code(&client, &url, "code", "verifier")
            .await
            .unwrap_err();
        assert!(matches!(err, UsageError::Transient(_)), "{err:?}");

        let (url, _hits) = super::super::quota::test_support::spawn(vec![(403, "forbidden")]);
        let err = exchange_code(&client, &url, "code", "verifier")
            .await
            .unwrap_err();
        assert!(matches!(err, UsageError::Fetcher(_)), "{err:?}");
    }
}
