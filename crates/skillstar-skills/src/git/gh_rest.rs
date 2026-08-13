//! GitHub REST access for the Skill publishing flow.
//!
//! Publishing used to shell out to the `gh` CLI, which authenticated as the
//! machine's global GitHub CLI login and inherited the process proxy
//! environment. Both are single-sourced here instead: every request carries the
//! SkillStar GitHub App credential from the system keyring (D-013) and is built
//! on the shared proxy-aware client, so the publish path cannot drift away from
//! the identity and proxy policy the rest of the app obeys.
//!
//! The transport is a trait so tests can prove pagination, affiliation and
//! secret handling without contacting GitHub.

use std::fmt;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use skillstar_github_auth::{
    GitHubAuthError, GitHubAuthErrorCode, GitHubAuthFacade, KeyringCredentialStore,
    ProductionGitHubGateway, SystemClock,
};

const API_ROOT: &str = "https://api.github.com";
const PER_PAGE: u32 = 100;
const MAX_PAGES: u32 = 20;
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Repositories the publish picker may offer. `gh repo list <login>` only ever
/// returned personally owned repositories, which is why an organization
/// repository could never be picked as a publish target.
pub(super) const REPO_AFFILIATION: &str = "owner,collaborator,organization_member";

// ── Errors ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GhRestErrorCode {
    /// No SkillStar GitHub credential, or GitHub rejected the one we have.
    NotAuthenticated,
    /// Signed in, but this account (or the App installation) lacks access.
    Unauthorized,
    /// The system credential store could not be read.
    CredentialUnavailable,
    /// GitHub throttled the request.
    RateLimited,
    /// GitHub refused the request itself (e.g. the repository name is taken).
    Rejected,
    /// GitHub could not be reached.
    Network,
    /// GitHub answered with something we cannot interpret.
    Protocol,
}

impl GhRestErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotAuthenticated => "not_authenticated",
            Self::Unauthorized => "unauthorized",
            Self::CredentialUnavailable => "credential_unavailable",
            Self::RateLimited => "rate_limited",
            Self::Rejected => "rejected",
            Self::Network => "network",
            Self::Protocol => "protocol",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhRestError {
    pub code: GhRestErrorCode,
    pub message: String,
}

impl GhRestError {
    pub fn new(code: GhRestErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// Map an authentication-facade failure onto the publish vocabulary.
    fn from_auth(error: GitHubAuthError) -> Self {
        let code = match error.code {
            GitHubAuthErrorCode::Network | GitHubAuthErrorCode::Proxy => GhRestErrorCode::Network,
            GitHubAuthErrorCode::CredentialStore => GhRestErrorCode::CredentialUnavailable,
            GitHubAuthErrorCode::Protocol => GhRestErrorCode::Protocol,
            _ => GhRestErrorCode::NotAuthenticated,
        };
        Self::new(code, error.message)
    }
}

impl fmt::Display for GhRestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for GhRestError {}

fn network_error() -> GhRestError {
    GhRestError::new(
        GhRestErrorCode::Network,
        "GitHub could not be reached; check the SkillStar proxy and network settings",
    )
}

// ── Transport ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhRestResponse {
    pub status: u16,
    pub body: String,
}

/// One request boundary for the publish flow.
///
/// The token is handed in per call rather than stored by the transport so a
/// test double can assert that it only ever reaches the `Authorization` header.
pub trait GhRestTransport: Send + Sync {
    fn get(&self, url: &str, token: &str) -> Result<GhRestResponse, GhRestError>;

    fn post_json(
        &self,
        url: &str,
        token: &str,
        body: &serde_json::Value,
    ) -> Result<GhRestResponse, GhRestError>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ReqwestGhRestTransport;

impl ReqwestGhRestTransport {
    fn request(
        method: reqwest::Method,
        url: &str,
        token: &str,
    ) -> Result<reqwest::RequestBuilder, GhRestError> {
        let client = skillstar_core::infra::http_client::probe_http_client(REQUEST_TIMEOUT)
            .map_err(|_| {
                GhRestError::new(
                    GhRestErrorCode::Network,
                    "Unable to create the GitHub client; check SkillStar proxy settings",
                )
            })?;
        Ok(client
            .request(method, url)
            .header(reqwest::header::ACCEPT, "application/vnd.github+json")
            .header(reqwest::header::USER_AGENT, "SkillStar")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .bearer_auth(token))
    }
}

impl GhRestTransport for ReqwestGhRestTransport {
    fn get(&self, url: &str, token: &str) -> Result<GhRestResponse, GhRestError> {
        let url = url.to_string();
        let token = token.to_string();
        block_on_blocking_context(async move {
            let request = Self::request(reqwest::Method::GET, &url, &token)?;
            send(request).await
        })?
    }

    fn post_json(
        &self,
        url: &str,
        token: &str,
        body: &serde_json::Value,
    ) -> Result<GhRestResponse, GhRestError> {
        let url = url.to_string();
        let token = token.to_string();
        let payload = serde_json::to_string(body).map_err(|_| {
            GhRestError::new(
                GhRestErrorCode::Protocol,
                "SkillStar could not encode the GitHub request body",
            )
        })?;
        block_on_blocking_context(async move {
            let request = Self::request(reqwest::Method::POST, &url, &token)?
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(payload);
            send(request).await
        })?
    }
}

async fn send(request: reqwest::RequestBuilder) -> Result<GhRestResponse, GhRestError> {
    let mut response = request.send().await.map_err(|_| network_error())?;
    let status = response.status().as_u16();
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
    {
        return Err(oversized_response());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| network_error())? {
        if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            return Err(oversized_response());
        }
        bytes.extend_from_slice(&chunk);
    }
    let body = String::from_utf8(bytes).map_err(|_| {
        GhRestError::new(
            GhRestErrorCode::Protocol,
            "GitHub returned a non-UTF-8 response",
        )
    })?;
    Ok(GhRestResponse { status, body })
}

fn oversized_response() -> GhRestError {
    GhRestError::new(
        GhRestErrorCode::Protocol,
        "GitHub returned a response above the supported size limit",
    )
}

/// Drive one async request from a blocking caller.
///
/// The publish entry points are synchronous — Tauri wraps them in
/// `spawn_blocking` and the CLI calls them straight from `main` — while
/// `probe_http_client` (the only HTTP client allowed to leave this app) is
/// async. `reqwest::blocking` would bypass that client and its proxy config,
/// so a short-lived runtime bridges the two instead.
///
/// Only call this from a blocking context; starting a runtime from inside an
/// async task panics.
pub(super) fn block_on_blocking_context<F: Future>(future: F) -> Result<F::Output, GhRestError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| {
            GhRestError::new(
                GhRestErrorCode::Network,
                format!("Unable to start the GitHub request runtime: {error}"),
            )
        })?;
    Ok(runtime.block_on(future))
}

// ── Client ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhRepository {
    pub full_name: String,
    pub html_url: String,
    pub description: String,
    pub private: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhCreatedRepository {
    pub full_name: String,
    pub html_url: String,
    pub clone_url: String,
}

/// GitHub REST calls the publish flow needs, bound to one App credential.
pub struct GhRestClient<T: GhRestTransport = ReqwestGhRestTransport> {
    transport: T,
    token: Arc<str>,
}

/// The credential is the whole point of this type, so its only rendering is a
/// redacted one — a debug log of the publish flow cannot leak the token.
impl<T: GhRestTransport> fmt::Debug for GhRestClient<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GhRestClient")
            .field("token", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

impl GhRestClient<ReqwestGhRestTransport> {
    /// Bind to the GitHub App credential stored in the system keyring.
    ///
    /// Fails when the user has not signed in to SkillStar or the access token
    /// has expired, which is what makes publishing follow the same identity as
    /// every other remote operation.
    pub fn from_keyring() -> Result<Self, GhRestError> {
        let auth = GitHubAuthFacade::new(
            ProductionGitHubGateway::from_environment(),
            KeyringCredentialStore,
            SystemClock,
        );
        let credential = auth.api_credential().map_err(GhRestError::from_auth)?;
        Ok(Self::with_transport(
            credential.expose_secret(),
            ReqwestGhRestTransport,
        ))
    }
}

impl<T: GhRestTransport> GhRestClient<T> {
    pub fn with_transport(token: impl Into<String>, transport: T) -> Self {
        Self {
            transport,
            token: Arc::from(token.into()),
        }
    }

    /// The login of the signed-in GitHub App user.
    pub fn current_login(&self) -> Result<String, GhRestError> {
        let response = self
            .transport
            .get(&format!("{API_ROOT}/user"), &self.token)?;
        ensure_status(&response, &[200])?;
        let user: UserResponse = parse_json(&response.body)?;
        if user.login.trim().is_empty() {
            return Err(GhRestError::new(
                GhRestErrorCode::Protocol,
                "GitHub returned an account without a login",
            ));
        }
        Ok(user.login)
    }

    /// Repositories that can serve as publish targets, newest activity first.
    ///
    /// `affiliation` is what makes organization repositories reachable; the
    /// previous `gh repo list <login>` call could only see personal ones.
    pub fn list_repositories(&self, limit: u32) -> Result<Vec<GhRepository>, GhRestError> {
        let limit = limit.clamp(1, PER_PAGE.saturating_mul(MAX_PAGES)) as usize;
        let mut repositories: Vec<GhRepository> = Vec::new();

        for page in 1..=MAX_PAGES {
            let url = format!(
                "{API_ROOT}/user/repos?affiliation={REPO_AFFILIATION}&per_page={PER_PAGE}&page={page}&sort=updated"
            );
            let response = self.transport.get(&url, &self.token)?;
            ensure_status(&response, &[200])?;
            let items: Vec<RepositoryResponse> = parse_json(&response.body)?;
            let received = items.len();
            repositories.extend(items.into_iter().map(map_repository));
            if repositories.len() >= limit || received < PER_PAGE as usize {
                break;
            }
        }

        repositories.truncate(limit);
        Ok(repositories)
    }

    /// Existing skill folders under the repository's top-level `skills/`.
    ///
    /// A repository without that directory — including a brand new empty one —
    /// is an ordinary publish target, so it answers with an empty list rather
    /// than an error.
    pub fn list_skill_folders(&self, repo_full_name: &str) -> Result<Vec<String>, GhRestError> {
        let url = format!(
            "{API_ROOT}/repos/{}/contents/skills",
            repository_path(repo_full_name)?
        );
        let response = self.transport.get(&url, &self.token)?;
        if matches!(response.status, 404 | 409) {
            return Ok(Vec::new());
        }
        ensure_status(&response, &[200])?;
        let entries: Vec<ContentEntry> = parse_json(&response.body)?;
        let mut folders: Vec<String> = entries
            .into_iter()
            .filter(|entry| entry.kind == "dir")
            .map(|entry| entry.name)
            .filter(|name| !name.is_empty() && !name.starts_with('.'))
            .collect();
        folders.sort();
        Ok(folders)
    }

    /// Create a repository owned by the signed-in user.
    pub fn create_repository(
        &self,
        name: &str,
        description: &str,
        private: bool,
    ) -> Result<GhCreatedRepository, GhRestError> {
        let body = serde_json::json!({
            "name": name,
            "description": description,
            "private": private,
            // The local cache already holds the first commit; letting GitHub
            // seed a README would create a diverged history the push cannot
            // fast-forward.
            "auto_init": false,
        });
        let response = self
            .transport
            .post_json(&format!("{API_ROOT}/user/repos"), &self.token, &body)?;
        ensure_status(&response, &[201])?;
        let created: RepositoryResponse = parse_json(&response.body)?;
        if created.clone_url.trim().is_empty() {
            return Err(GhRestError::new(
                GhRestErrorCode::Protocol,
                "GitHub created the repository without returning a clone URL",
            ));
        }
        Ok(GhCreatedRepository {
            full_name: created.full_name,
            html_url: created.html_url,
            clone_url: created.clone_url,
        })
    }
}

// ── Wire shapes ─────────────────────────────────────────────────────

#[derive(Deserialize)]
struct UserResponse {
    login: String,
}

#[derive(Deserialize)]
struct RepositoryResponse {
    full_name: String,
    html_url: String,
    #[serde(default)]
    clone_url: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    private: bool,
}

#[derive(Deserialize)]
struct ContentEntry {
    name: String,
    #[serde(rename = "type")]
    kind: String,
}

fn map_repository(response: RepositoryResponse) -> GhRepository {
    GhRepository {
        full_name: response.full_name,
        html_url: response.html_url,
        description: response.description.unwrap_or_default(),
        private: response.private,
    }
}

// ── Response handling ───────────────────────────────────────────────

pub(super) fn parse_json<T: serde::de::DeserializeOwned>(body: &str) -> Result<T, GhRestError> {
    serde_json::from_str(body).map_err(|_| {
        GhRestError::new(
            GhRestErrorCode::Protocol,
            "GitHub returned an unexpected response for the publish request",
        )
    })
}

pub(super) fn ensure_status(
    response: &GhRestResponse,
    expected: &[u16],
) -> Result<(), GhRestError> {
    if expected.contains(&response.status) {
        return Ok(());
    }
    Err(classify_status(response))
}

/// Turn a GitHub status into something the user can act on.
fn classify_status(response: &GhRestResponse) -> GhRestError {
    let status = response.status;
    if status == 429 || (status == 403 && is_rate_limited(&response.body)) {
        return GhRestError::new(
            GhRestErrorCode::RateLimited,
            "GitHub temporarily rate-limited SkillStar; wait a moment and retry publishing",
        );
    }
    match status {
        401 => GhRestError::new(
            GhRestErrorCode::NotAuthenticated,
            "GitHub rejected the SkillStar sign-in; sign in again, then retry publishing",
        ),
        403 => GhRestError::new(
            GhRestErrorCode::Unauthorized,
            "The signed-in GitHub user does not have access; check the SkillStar GitHub App installation for this account or organization",
        ),
        404 => GhRestError::new(
            GhRestErrorCode::Unauthorized,
            "GitHub could not find that repository for the signed-in user; install or authorize the SkillStar GitHub App for it, then retry",
        ),
        422 => GhRestError::new(
            GhRestErrorCode::Rejected,
            rejection_message(&response.body),
        ),
        _ => GhRestError::new(
            GhRestErrorCode::Protocol,
            format!("GitHub answered the publish request with status {status}"),
        ),
    }
}

fn is_rate_limited(body: &str) -> bool {
    let lower = body.to_ascii_lowercase();
    lower.contains("rate limit") || lower.contains("abuse detection")
}

/// GitHub's 422 payload carries the actionable part (e.g. "name already
/// exists on this account"); a bare status code would not.
fn rejection_message(body: &str) -> String {
    #[derive(Deserialize)]
    struct ValidationError {
        message: Option<String>,
        #[serde(default)]
        errors: Vec<ValidationDetail>,
    }
    #[derive(Deserialize)]
    struct ValidationDetail {
        message: Option<String>,
    }

    let detail = serde_json::from_str::<ValidationError>(body)
        .ok()
        .and_then(|parsed| {
            parsed
                .errors
                .into_iter()
                .find_map(|error| error.message)
                .or(parsed.message)
        })
        .unwrap_or_else(|| "GitHub rejected the request".to_string());
    format!("GitHub rejected the publish request: {detail}")
}

/// Validate `owner/name` before it becomes part of a URL path.
fn repository_path(repo_full_name: &str) -> Result<String, GhRestError> {
    let invalid = || {
        GhRestError::new(
            GhRestErrorCode::Protocol,
            "Repository names must look like 'owner/name'",
        )
    };
    let (owner, name) = repo_full_name.split_once('/').ok_or_else(invalid)?;
    let owner_ok = !owner.is_empty()
        && owner
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-');
    let name_ok = !name.is_empty()
        && name.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_')
        })
        && name != "."
        && name != "..";
    if owner_ok && name_ok {
        Ok(format!("{owner}/{name}"))
    } else {
        Err(invalid())
    }
}
