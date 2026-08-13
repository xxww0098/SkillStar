use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;

use super::{
    DeviceAuthorizationGrant, GatewayDevicePoll, GitHubAuthError, GitHubAuthErrorCode,
    GitHubGateway, GitHubIdentity, TokenGrant,
};

const DEVICE_CODE_URL: &str = "https://github.com/login/device/code";
const ACCESS_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
const CURRENT_USER_URL: &str = "https://api.github.com/user";

#[derive(Clone)]
pub struct GitHubHttpResponse {
    pub status: u16,
    pub body: String,
}

#[async_trait]
pub trait GitHubHttpTransport: Send + Sync {
    async fn post_form(
        &self,
        url: &str,
        fields: &[(String, String)],
    ) -> Result<GitHubHttpResponse, GitHubAuthError>;

    async fn get_bearer(
        &self,
        url: &str,
        bearer: &str,
    ) -> Result<GitHubHttpResponse, GitHubAuthError>;
}

#[derive(Clone)]
pub struct ReqwestGitHubTransport;

impl ReqwestGitHubTransport {
    fn client() -> Result<reqwest::Client, GitHubAuthError> {
        skillstar_core::infra::http_client::probe_http_client(Duration::from_secs(30)).map_err(
            |_| {
                GitHubAuthError::new(
                    GitHubAuthErrorCode::Proxy,
                    "Unable to create the GitHub client; check proxy settings",
                )
            },
        )
    }
}

#[async_trait]
impl GitHubHttpTransport for ReqwestGitHubTransport {
    async fn post_form(
        &self,
        url: &str,
        fields: &[(String, String)],
    ) -> Result<GitHubHttpResponse, GitHubAuthError> {
        let response = Self::client()?
            .post(url)
            .header(reqwest::header::ACCEPT, "application/json")
            .header(reqwest::header::USER_AGENT, "SkillStar")
            .form(fields)
            .send()
            .await
            .map_err(network_error)?;
        response_body(response).await
    }

    async fn get_bearer(
        &self,
        url: &str,
        bearer: &str,
    ) -> Result<GitHubHttpResponse, GitHubAuthError> {
        let response = Self::client()?
            .get(url)
            .header(reqwest::header::ACCEPT, "application/vnd.github+json")
            .header(reqwest::header::USER_AGENT, "SkillStar")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .bearer_auth(bearer)
            .send()
            .await
            .map_err(network_error)?;
        response_body(response).await
    }
}

async fn response_body(response: reqwest::Response) -> Result<GitHubHttpResponse, GitHubAuthError> {
    let status = response.status().as_u16();
    let body = response.text().await.map_err(network_error)?;
    Ok(GitHubHttpResponse { status, body })
}

fn network_error(_error: reqwest::Error) -> GitHubAuthError {
    GitHubAuthError::new(
        GitHubAuthErrorCode::Network,
        "Unable to reach GitHub; check the network and proxy settings",
    )
}

pub struct GitHubHttpGateway<T = ReqwestGitHubTransport> {
    client_id: String,
    transport: T,
}

pub struct ProductionGitHubGateway {
    inner: Result<GitHubHttpGateway, GitHubAuthError>,
}

impl ProductionGitHubGateway {
    pub fn from_environment() -> Self {
        Self {
            inner: github_app_client_id().map(GitHubHttpGateway::new),
        }
    }

    fn ready(&self) -> Result<&GitHubHttpGateway, GitHubAuthError> {
        self.inner.as_ref().map_err(Clone::clone)
    }
}

impl Default for ProductionGitHubGateway {
    fn default() -> Self {
        Self::from_environment()
    }
}

#[async_trait]
impl GitHubGateway for ProductionGitHubGateway {
    async fn start_device_authorization(
        &self,
    ) -> Result<DeviceAuthorizationGrant, GitHubAuthError> {
        self.ready()?.start_device_authorization().await
    }

    async fn poll_device_authorization(
        &self,
        device_code: &str,
    ) -> Result<GatewayDevicePoll, GitHubAuthError> {
        self.ready()?.poll_device_authorization(device_code).await
    }

    async fn refresh_access_token(
        &self,
        refresh_token: &str,
    ) -> Result<TokenGrant, GitHubAuthError> {
        self.ready()?.refresh_access_token(refresh_token).await
    }

    async fn current_user(&self, access_token: &str) -> Result<GitHubIdentity, GitHubAuthError> {
        self.ready()?.current_user(access_token).await
    }
}

impl GitHubHttpGateway<ReqwestGitHubTransport> {
    pub fn new(client_id: impl Into<String>) -> Self {
        Self {
            client_id: client_id.into(),
            transport: ReqwestGitHubTransport,
        }
    }
}

impl<T> GitHubHttpGateway<T> {
    pub fn with_transport(client_id: impl Into<String>, transport: T) -> Self {
        Self {
            client_id: client_id.into(),
            transport,
        }
    }
}

#[derive(Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: u64,
}

#[derive(Deserialize)]
struct TokenEndpointResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    refresh_token_expires_in: Option<u64>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct UserResponse {
    id: u64,
    login: String,
    avatar_url: Option<String>,
}

#[async_trait]
impl<T> GitHubGateway for GitHubHttpGateway<T>
where
    T: GitHubHttpTransport,
{
    async fn start_device_authorization(
        &self,
    ) -> Result<DeviceAuthorizationGrant, GitHubAuthError> {
        let response = self
            .transport
            .post_form(
                DEVICE_CODE_URL,
                &[("client_id".into(), self.client_id.clone())],
            )
            .await?;
        ensure_success(&response)?;
        let parsed: DeviceCodeResponse = serde_json::from_str(&response.body).map_err(|_| {
            GitHubAuthError::new(
                GitHubAuthErrorCode::Protocol,
                "GitHub returned an invalid device authorization response",
            )
        })?;
        Ok(DeviceAuthorizationGrant {
            device_code: parsed.device_code,
            user_code: parsed.user_code,
            verification_uri: parsed.verification_uri,
            expires_in_seconds: parsed.expires_in,
            interval_seconds: parsed.interval,
        })
    }

    async fn poll_device_authorization(
        &self,
        device_code: &str,
    ) -> Result<GatewayDevicePoll, GitHubAuthError> {
        let response = self
            .transport
            .post_form(
                ACCESS_TOKEN_URL,
                &[
                    ("client_id".into(), self.client_id.clone()),
                    ("device_code".into(), device_code.into()),
                    (
                        "grant_type".into(),
                        "urn:ietf:params:oauth:grant-type:device_code".into(),
                    ),
                ],
            )
            .await?;
        ensure_success(&response)?;
        let parsed = parse_token_response(&response.body)?;
        match parsed.error.as_deref() {
            Some("authorization_pending") => Ok(GatewayDevicePoll::Pending),
            Some("slow_down") => Ok(GatewayDevicePoll::SlowDown),
            Some("expired_token") => Ok(GatewayDevicePoll::Expired),
            Some("access_denied") => Ok(GatewayDevicePoll::Denied),
            Some("incorrect_client_credentials" | "device_flow_disabled") => {
                Err(GitHubAuthError::new(
                    GitHubAuthErrorCode::Unavailable,
                    "GitHub device login is unavailable for this app",
                ))
            }
            Some(_) => Err(GitHubAuthError::new(
                GitHubAuthErrorCode::Protocol,
                "GitHub rejected the device authorization request",
            )),
            None => Ok(GatewayDevicePoll::Authorized(token_grant(parsed)?)),
        }
    }

    async fn refresh_access_token(
        &self,
        refresh_token: &str,
    ) -> Result<TokenGrant, GitHubAuthError> {
        let response = self
            .transport
            .post_form(
                ACCESS_TOKEN_URL,
                &[
                    ("client_id".into(), self.client_id.clone()),
                    ("refresh_token".into(), refresh_token.into()),
                    ("grant_type".into(), "refresh_token".into()),
                ],
            )
            .await?;
        ensure_success(&response)?;
        let parsed = parse_token_response(&response.body)?;
        if parsed.error.is_some() {
            return Err(GitHubAuthError::new(
                GitHubAuthErrorCode::RefreshUnavailable,
                "GitHub could not refresh this session; sign in again",
            ));
        }
        token_grant(parsed)
    }

    async fn current_user(&self, access_token: &str) -> Result<GitHubIdentity, GitHubAuthError> {
        let response = self
            .transport
            .get_bearer(CURRENT_USER_URL, access_token)
            .await?;
        if response.status == 401 || response.status == 403 {
            return Err(GitHubAuthError::new(
                GitHubAuthErrorCode::NotAuthenticated,
                "GitHub rejected the current session; sign in again",
            ));
        }
        ensure_success(&response)?;
        let parsed: UserResponse = serde_json::from_str(&response.body).map_err(|_| {
            GitHubAuthError::new(
                GitHubAuthErrorCode::Protocol,
                "GitHub returned an invalid user response",
            )
        })?;
        Ok(GitHubIdentity {
            id: parsed.id,
            login: parsed.login,
            avatar_url: parsed.avatar_url,
        })
    }
}

fn parse_token_response(body: &str) -> Result<TokenEndpointResponse, GitHubAuthError> {
    serde_json::from_str(body).map_err(|_| {
        GitHubAuthError::new(
            GitHubAuthErrorCode::Protocol,
            "GitHub returned an invalid token response",
        )
    })
}

fn token_grant(response: TokenEndpointResponse) -> Result<TokenGrant, GitHubAuthError> {
    let access_token = response.access_token.ok_or_else(|| {
        GitHubAuthError::new(
            GitHubAuthErrorCode::Protocol,
            "GitHub token response did not include an access token",
        )
    })?;
    Ok(TokenGrant {
        access_token,
        refresh_token: response.refresh_token,
        access_expires_in_seconds: response.expires_in,
        refresh_expires_in_seconds: response.refresh_token_expires_in,
    })
}

fn ensure_success(response: &GitHubHttpResponse) -> Result<(), GitHubAuthError> {
    if (200..300).contains(&response.status) {
        return Ok(());
    }
    Err(GitHubAuthError::new(
        GitHubAuthErrorCode::Protocol,
        format!("GitHub request failed with status {}", response.status),
    ))
}

pub fn github_app_client_id() -> Result<String, GitHubAuthError> {
    std::env::var("SKILLSTAR_GITHUB_APP_CLIENT_ID")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            option_env!("SKILLSTAR_GITHUB_APP_CLIENT_ID")
                .filter(|value| !value.trim().is_empty())
                .map(ToOwned::to_owned)
        })
        .ok_or_else(|| {
            GitHubAuthError::new(
                GitHubAuthErrorCode::Unavailable,
                "This build does not include a GitHub App client ID",
            )
        })
}
