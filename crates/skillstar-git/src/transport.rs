//! Operation-scoped Git transport policy for public and private GitHub repositories.
//!
//! Remote Git subprocesses must pass through this module so authentication,
//! proxy, cancellation, progress, and redaction share one boundary.

use serde::Serialize;
use skillstar_core::config::{github_mirror, proxy};
use skillstar_core::infra::path_env::command_with_path;
use std::fmt;
use std::io::Read;
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use url::Url;

const ASKPASS_MODE_ENV: &str = "SKILLSTAR_GIT_ASKPASS_MODE";
const ASKPASS_TOKEN_ENV: &str = "SKILLSTAR_GIT_ASKPASS_TOKEN";

#[derive(Clone)]
pub struct GitAuthMaterial {
    state: GitAuthState,
}

#[derive(Clone)]
enum GitAuthState {
    Missing,
    Expired,
    Unavailable(Arc<str>),
    Available(Arc<str>),
}

impl GitAuthMaterial {
    pub fn missing() -> Self {
        Self {
            state: GitAuthState::Missing,
        }
    }

    pub fn expired() -> Self {
        Self {
            state: GitAuthState::Expired,
        }
    }

    pub fn available(token: impl Into<String>) -> Self {
        Self {
            state: GitAuthState::Available(Arc::from(token.into())),
        }
    }

    pub fn unavailable(message: impl Into<String>) -> Self {
        Self {
            state: GitAuthState::Unavailable(Arc::from(message.into())),
        }
    }

    fn token(&self) -> Option<&str> {
        match &self.state {
            GitAuthState::Available(token) => Some(token),
            GitAuthState::Missing | GitAuthState::Expired | GitAuthState::Unavailable(_) => None,
        }
    }

    fn is_expired(&self) -> bool {
        matches!(self.state, GitAuthState::Expired)
    }

    fn is_missing(&self) -> bool {
        matches!(self.state, GitAuthState::Missing)
    }

    fn unavailable_message(&self) -> Option<&str> {
        match &self.state {
            GitAuthState::Unavailable(message) => Some(message),
            _ => None,
        }
    }
}

impl Default for GitAuthMaterial {
    fn default() -> Self {
        Self::missing()
    }
}

impl fmt::Debug for GitAuthMaterial {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = match self.state {
            GitAuthState::Missing => "missing",
            GitAuthState::Expired => "expired",
            GitAuthState::Unavailable(_) => "unavailable",
            GitAuthState::Available(_) => "available",
        };
        formatter
            .debug_struct("GitAuthMaterial")
            .field("state", &state)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GitOperationPhase {
    Preparing,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GitOperationProgress {
    pub session_id: String,
    pub phase: GitOperationPhase,
    pub repository: String,
}

pub trait GitProgressSink: Send + Sync {
    fn emit(&self, progress: GitOperationProgress);
}

#[derive(Default)]
pub struct NoopGitProgressSink;

impl GitProgressSink for NoopGitProgressSink {
    fn emit(&self, _progress: GitOperationProgress) {}
}

#[derive(Clone)]
pub struct GitOperationSession {
    id: Arc<str>,
    auth: GitAuthMaterial,
    cancelled: Arc<AtomicBool>,
    progress: Arc<dyn GitProgressSink>,
}

impl GitOperationSession {
    pub fn new(
        id: impl Into<String>,
        auth: GitAuthMaterial,
        progress: Arc<dyn GitProgressSink>,
    ) -> Self {
        Self {
            id: Arc::from(id.into()),
            auth,
            cancelled: Arc::new(AtomicBool::new(false)),
            progress,
        }
    }

    pub fn public() -> Self {
        Self::new(
            uuid::Uuid::new_v4().to_string(),
            GitAuthMaterial::missing(),
            Arc::new(NoopGitProgressSink),
        )
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub fn emit(&self, phase: GitOperationPhase, remote: &str) {
        self.progress.emit(GitOperationProgress {
            session_id: self.id().to_string(),
            phase,
            repository: safe_repository_label(remote),
        });
    }

    pub fn has_credential(&self) -> bool {
        self.auth.token().is_some()
    }

    pub(super) fn unauthenticated_view(&self) -> Self {
        Self {
            id: self.id.clone(),
            auth: GitAuthMaterial::missing(),
            cancelled: self.cancelled.clone(),
            progress: self.progress.clone(),
        }
    }

    /// Recovery runs after an operation has already failed or been cancelled.
    /// It retains the operation credential but must not inherit the sticky
    /// cancellation bit, otherwise rollback cannot restore a partial checkout.
    pub fn recovery_view(&self) -> Self {
        Self {
            id: self.id.clone(),
            auth: self.auth.clone(),
            cancelled: Arc::new(AtomicBool::new(false)),
            progress: Arc::new(NoopGitProgressSink),
        }
    }
}

impl fmt::Debug for GitOperationSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitOperationSession")
            .field("id", &self.id)
            .field("auth", &self.auth)
            .field("cancelled", &self.is_cancelled())
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GitTransportErrorCode {
    NotAuthenticated,
    TokenExpired,
    Unauthorized,
    AppNotInstalled,
    Network,
    Cancelled,
    CredentialUnavailable,
    UnsafeRemote,
    Other,
}

impl GitTransportErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotAuthenticated => "not_authenticated",
            Self::TokenExpired => "token_expired",
            Self::Unauthorized => "unauthorized",
            Self::AppNotInstalled => "app_not_installed",
            Self::Network => "network",
            Self::Cancelled => "cancelled",
            Self::CredentialUnavailable => "credential_unavailable",
            Self::UnsafeRemote => "unsafe_remote",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GitTransportError {
    pub code: GitTransportErrorCode,
    pub message: String,
    pub session_id: String,
}

impl fmt::Display for GitTransportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for GitTransportError {}

#[derive(Debug)]
pub struct GitCommandOutput {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

/// Apply non-interactive, proxy-aware, operation-scoped authentication policy.
///
/// The token is deliberately supplied only through the child environment used
/// by the internal askpass bridge. It never enters argv or a Git config key.
pub fn configure_remote_command(
    command: &mut Command,
    remote: &str,
    session: &GitOperationSession,
) -> Result<(), GitTransportError> {
    if remote_has_userinfo(remote) {
        return Err(GitTransportError {
            code: GitTransportErrorCode::UnsafeRemote,
            message: "Repository URLs containing credentials are not allowed".to_string(),
            session_id: session.id().to_string(),
        });
    }
    command
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "Never")
        .env("GIT_ASKPASS_REQUIRE", "force")
        .env("LC_ALL", "C")
        .env("LANG", "C")
        // An empty operation-local helper prevents a global helper from opening
        // UI or silently selecting a different account.
        .arg("-c")
        .arg("credential.helper=")
        .arg("-c")
        .arg("core.askPass=");
    apply_proxy_environment(command);

    if is_github_https_remote(remote)
        && let Some(token) = session.auth.token()
    {
        let executable = std::env::current_exe().map_err(|error| GitTransportError {
            code: GitTransportErrorCode::Other,
            message: format!("Cannot locate SkillStar askpass executable: {error}"),
            session_id: session.id().to_string(),
        })?;
        command
            .env("GIT_ASKPASS", &executable)
            .env("SSH_ASKPASS", &executable)
            .env(ASKPASS_MODE_ENV, "1")
            .env(ASKPASS_TOKEN_ENV, token);
    } else {
        command
            .env_remove("GIT_ASKPASS")
            .env_remove("SSH_ASKPASS")
            .env_remove(ASKPASS_MODE_ENV)
            .env_remove(ASKPASS_TOKEN_ENV);
    }
    Ok(())
}

pub fn execute_remote_git(
    repo_path: Option<&Path>,
    args: &[&str],
    remote: &str,
    session: &GitOperationSession,
    allow_mirror: bool,
) -> Result<GitCommandOutput, GitTransportError> {
    // Keep public GitHub traffic credential-free even while the user is signed
    // in. Only retry the exact operation with askpass after GitHub challenges
    // the anonymous attempt; the authenticated retry never uses a mirror.
    if is_github_https_remote(remote) && session.has_credential() {
        let anonymous = session.unauthenticated_view();
        let anonymous_result = execute_anonymous_with_mirror_fallback(
            repo_path,
            args,
            remote,
            &anonymous,
            allow_mirror,
        );
        match anonymous_result {
            Ok(output) => return Ok(output),
            Err(error)
                if matches!(
                    error.code,
                    GitTransportErrorCode::NotAuthenticated
                        | GitTransportErrorCode::Unauthorized
                        | GitTransportErrorCode::AppNotInstalled
                ) => {}
            Err(error) => return Err(error),
        }
        return execute_remote_git_once(repo_path, args, remote, session, None);
    }
    if is_github_https_remote(remote) && !session.has_credential() {
        return execute_anonymous_with_mirror_fallback(
            repo_path,
            args,
            remote,
            session,
            allow_mirror,
        );
    }
    // Non-GitHub or non-HTTPS remotes never use the GitHub mirror rewrite.
    execute_remote_git_once(repo_path, args, remote, session, None)
}

fn execute_anonymous_with_mirror_fallback(
    repo_path: Option<&Path>,
    args: &[&str],
    remote: &str,
    session: &GitOperationSession,
    allow_mirror: bool,
) -> Result<GitCommandOutput, GitTransportError> {
    if !allow_mirror || session.has_credential() {
        return execute_remote_git_once(repo_path, args, remote, session, None);
    }

    let candidates = github_mirror::candidate_mirror_urls();
    if candidates.is_empty() {
        return execute_remote_git_once(repo_path, args, remote, session, None);
    }

    // Anti-censorship: try every mirror candidate in preference order before
    // falling back to a direct GitHub connection. A mirror can turn a valid
    // public repository into a 404 or another auth-looking response, so the
    // direct attempt is the final authority, not just the last resort.
    let mut last_error: Option<GitTransportError> = None;
    for mirror in &candidates {
        match execute_remote_git_once(repo_path, args, remote, session, Some(mirror)) {
            Ok(output) => return Ok(output),
            Err(error)
                if matches!(
                    error.code,
                    GitTransportErrorCode::Cancelled | GitTransportErrorCode::UnsafeRemote
                ) =>
            {
                return Err(error);
            }
            Err(error) => last_error = Some(error),
        }
    }

    match execute_remote_git_once(repo_path, args, remote, session, None) {
        Ok(output) => Ok(output),
        Err(error) => Err(last_error.unwrap_or(error)),
    }
}

fn execute_remote_git_once(
    repo_path: Option<&Path>,
    args: &[&str],
    remote: &str,
    session: &GitOperationSession,
    mirror: Option<&str>,
) -> Result<GitCommandOutput, GitTransportError> {
    let mut command = command_with_path("git");
    if let Some(mirror_url) = mirror
        && !session.has_credential()
    {
        github_mirror::apply_mirror_args_for(&mut command, mirror_url);
    }
    execute_remote_command(&mut command, repo_path, args, remote, session)
}

/// Execute an injected command through the same remote-operation boundary.
///
/// Production passes a Git command; tests can pass a fake executable and prove
/// secret visibility and cancellation without contacting a live repository.
pub fn execute_remote_command(
    command: &mut Command,
    repo_path: Option<&Path>,
    args: &[&str],
    remote: &str,
    session: &GitOperationSession,
) -> Result<GitCommandOutput, GitTransportError> {
    if session.is_cancelled() {
        session.emit(GitOperationPhase::Cancelled, remote);
        return Err(cancelled_error(session));
    }

    session.emit(GitOperationPhase::Preparing, remote);
    configure_remote_command(command, remote, session)?;
    if let Some(repo_path) = repo_path {
        command.current_dir(repo_path);
    }
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure_process_group(command);

    session.emit(GitOperationPhase::Running, remote);
    let mut child = command.spawn().map_err(|error| GitTransportError {
        code: GitTransportErrorCode::Other,
        message: format!("Failed to start Git: {error}"),
        session_id: session.id().to_string(),
    })?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_reader = std::thread::spawn(move || read_pipe(stdout));
    let stderr_reader = std::thread::spawn(move || read_pipe(stderr));

    let status = loop {
        if session.is_cancelled() {
            terminate_child_tree(&mut child);
            let _ = child.wait();
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            session.emit(GitOperationPhase::Cancelled, remote);
            return Err(cancelled_error(session));
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(error) => {
                terminate_child_tree(&mut child);
                let _ = child.wait();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                session.emit(GitOperationPhase::Failed, remote);
                return Err(GitTransportError {
                    code: GitTransportErrorCode::Other,
                    message: format!("Failed while waiting for Git: {error}"),
                    session_id: session.id().to_string(),
                });
            }
        }
    };

    let stdout = String::from_utf8_lossy(&stdout_reader.join().unwrap_or_default()).into_owned();
    let stderr = String::from_utf8_lossy(&stderr_reader.join().unwrap_or_default()).into_owned();
    let stdout = redact_git_output(&stdout, session);
    let stderr = redact_git_output(&stderr, session);
    if status.success() {
        session.emit(GitOperationPhase::Completed, remote);
        Ok(GitCommandOutput {
            status,
            stdout,
            stderr,
        })
    } else {
        session.emit(GitOperationPhase::Failed, remote);
        Err(classify_git_failure(&stderr, session))
    }
}

pub fn classify_git_failure(stderr: &str, session: &GitOperationSession) -> GitTransportError {
    let message = redact_git_output(stderr, session);
    let lower = message.to_ascii_lowercase();
    let code = if session.is_cancelled() {
        GitTransportErrorCode::Cancelled
    } else if is_network_failure(&lower) {
        GitTransportErrorCode::Network
    } else if session.auth.unavailable_message().is_some() && is_auth_failure(&lower) {
        GitTransportErrorCode::CredentialUnavailable
    } else if session.auth.is_expired() && is_auth_failure(&lower) {
        GitTransportErrorCode::TokenExpired
    } else if session.auth.is_missing() && is_auth_failure(&lower) {
        GitTransportErrorCode::NotAuthenticated
    } else if lower.contains("repository not found") || lower.contains("not found") {
        GitTransportErrorCode::AppNotInstalled
    } else if lower.contains("403")
        || lower.contains("permission denied")
        || lower.contains("write access to repository not granted")
        || is_auth_failure(&lower)
    {
        GitTransportErrorCode::Unauthorized
    } else {
        GitTransportErrorCode::Other
    };
    let guidance = match code {
        GitTransportErrorCode::NotAuthenticated => {
            "Sign in to GitHub in SkillStar, then retry this repository operation".to_string()
        }
        GitTransportErrorCode::TokenExpired => {
            "The GitHub session expired; refresh it or sign in again, then retry".to_string()
        }
        GitTransportErrorCode::Unauthorized => {
            "The signed-in GitHub user does not have access to this repository".to_string()
        }
        GitTransportErrorCode::AppNotInstalled => {
            "Install or authorize the SkillStar GitHub App for this repository, then retry"
                .to_string()
        }
        GitTransportErrorCode::Network => {
            format!("GitHub could not be reached; check the SkillStar proxy and network: {message}")
        }
        GitTransportErrorCode::Cancelled => "The Git operation was cancelled".to_string(),
        GitTransportErrorCode::CredentialUnavailable => format!(
            "The system credential store is unavailable; unlock it and retry: {}",
            session
                .auth
                .unavailable_message()
                .unwrap_or("credential unavailable")
        ),
        GitTransportErrorCode::UnsafeRemote => {
            "Repository URLs containing credentials are not allowed".to_string()
        }
        GitTransportErrorCode::Other => {
            let trimmed = message.trim();
            if trimmed.is_empty() {
                "Git operation failed".to_string()
            } else {
                trimmed.to_string()
            }
        }
    };
    GitTransportError {
        code,
        message: guidance,
        session_id: session.id().to_string(),
    }
}

pub fn redact_git_output(output: &str, session: &GitOperationSession) -> String {
    let mut redacted = output.to_string();
    if let Some(token) = session.auth.token() {
        redacted = redacted.replace(token, "[REDACTED]");
    }
    if let Ok(config) = proxy::load_config()
        && let Some(proxy_url) = proxy_url_from_config(&config)
    {
        redacted = redacted.replace(&proxy_url, "[REDACTED_PROXY]");
        if let Some(password) = config.password.filter(|value| !value.is_empty()) {
            redacted = redacted.replace(&password, "[REDACTED]");
        }
    }
    // Git sometimes echoes a credential-bearing URL even when the token itself
    // has already been replaced above. Remove userinfo as a second guard.
    redacted = redact_url_userinfo(&redacted);
    redacted
}

pub fn internal_askpass_response(
    enabled: bool,
    token: Option<&str>,
    prompt: &str,
) -> Option<String> {
    if !enabled {
        return None;
    }
    let prompt = prompt.to_ascii_lowercase();
    if !prompt.contains("github.com") {
        return None;
    }
    if prompt.contains("username") {
        Some("x-access-token".to_string())
    } else if prompt.contains("password") {
        Some(token.unwrap_or_default().to_string())
    } else {
        Some(String::new())
    }
}

/// Handle the private askpass child mode before CLI/GUI initialization.
pub fn handle_internal_askpass(args: &[String]) -> bool {
    let enabled = std::env::var(ASKPASS_MODE_ENV).as_deref() == Ok("1");
    if !enabled {
        return false;
    }
    let prompt = args.get(1).map(String::as_str).unwrap_or_default();
    let token = std::env::var(ASKPASS_TOKEN_ENV).ok();
    if let Some(response) = internal_askpass_response(enabled, token.as_deref(), prompt) {
        println!("{response}");
    }
    true
}

fn apply_proxy_environment(command: &mut Command) {
    // The desktop process can inherit proxy variables from its launcher. Keep
    // Git aligned with SkillStar's single proxy setting instead of silently
    // merging two sources of truth (including lowercase curl aliases).
    for key in [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "NO_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
        "no_proxy",
    ] {
        command.env_remove(key);
    }
    let Ok(config) = proxy::load_config() else {
        return;
    };
    let Some(proxy_url) = proxy_url_from_config(&config) else {
        return;
    };
    command
        .env("HTTP_PROXY", &proxy_url)
        .env("HTTPS_PROXY", &proxy_url)
        .env("ALL_PROXY", &proxy_url);
    if let Some(bypass) = config.bypass.filter(|value| !value.trim().is_empty()) {
        command.env("NO_PROXY", bypass);
    }
}

fn proxy_url_from_config(config: &proxy::ProxyConfig) -> Option<String> {
    if !config.enabled || config.host.trim().is_empty() {
        return None;
    }
    let raw = format!(
        "{}://{}:{}",
        config.proxy_type.as_scheme(),
        config.host.trim(),
        config.port
    );
    let mut proxy_url = Url::parse(&raw).ok()?;
    if let Some(username) = config.username.as_deref() {
        let _ = proxy_url.set_username(username);
    }
    if let Some(password) = config.password.as_deref() {
        let _ = proxy_url.set_password(Some(password));
    }
    Some(proxy_url.to_string())
}

fn is_github_https_remote(remote: &str) -> bool {
    Url::parse(remote).is_ok_and(|url| {
        url.scheme() == "https"
            && url
                .host_str()
                .is_some_and(|host| host.eq_ignore_ascii_case("github.com"))
            && url.username().is_empty()
            && url.password().is_none()
    })
}

fn remote_has_userinfo(remote: &str) -> bool {
    Url::parse(remote).is_ok_and(|url| {
        matches!(url.scheme(), "http" | "https")
            && (!url.username().is_empty() || url.password().is_some())
    })
}

fn safe_repository_label(remote: &str) -> String {
    if let Ok(url) = Url::parse(remote) {
        let host = url.host_str().unwrap_or("remote");
        return format!("{host}{}", url.path());
    }
    if Path::new(remote).is_absolute() || remote.starts_with("file:") {
        "local-repository".to_string()
    } else {
        remote
            .rsplit_once('@')
            .map(|(_, suffix)| suffix.to_string())
            .unwrap_or_else(|| remote.to_string())
    }
}

fn is_network_failure(lower: &str) -> bool {
    [
        "could not resolve host",
        "could not resolve proxy",
        "failed to connect",
        "failed to connect to proxy",
        "connection timed out",
        "operation timed out",
        "timeout was reached",
        "connection reset",
        "network is unreachable",
        "proxy connect aborted",
        "ssl certificate problem",
        "http2 framing",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn is_auth_failure(lower: &str) -> bool {
    lower.contains("authentication failed")
        || lower.contains("could not read username")
        || lower.contains("terminal prompts disabled")
        || lower.contains("401")
}

fn cancelled_error(session: &GitOperationSession) -> GitTransportError {
    GitTransportError {
        code: GitTransportErrorCode::Cancelled,
        message: "The Git operation was cancelled".to_string(),
        session_id: session.id().to_string(),
    }
}

fn read_pipe<R: Read>(pipe: Option<R>) -> Vec<u8> {
    let mut bytes = Vec::new();
    if let Some(mut pipe) = pipe {
        let _ = pipe.read_to_end(&mut bytes);
    }
    bytes
}

#[cfg(unix)]
pub fn configure_process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(not(unix))]
pub fn configure_process_group(_command: &mut Command) {}

#[cfg(unix)]
pub fn terminate_child_tree(child: &mut std::process::Child) {
    let process_group = format!("-{}", child.id());
    let _ = command_with_path("kill")
        .args(["-TERM", process_group.as_str()])
        .status();
    let _ = child.kill();
}

#[cfg(windows)]
pub fn terminate_child_tree(child: &mut std::process::Child) {
    let pid = child.id().to_string();
    let _ = command_with_path("taskkill")
        .args(["/PID", pid.as_str(), "/T", "/F"])
        .status();
    let _ = child.kill();
}

#[cfg(not(any(unix, windows)))]
pub fn terminate_child_tree(child: &mut std::process::Child) {
    let _ = child.kill();
}

fn redact_url_userinfo(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut remainder = input;
    while let Some(start) = find_url_start(remainder) {
        output.push_str(&remainder[..start]);
        let candidate = &remainder[start..];
        let end = candidate
            .find(char::is_whitespace)
            .unwrap_or(candidate.len());
        let url_text = &candidate[..end];
        if let Ok(mut url) = Url::parse(url_text.trim_end_matches(['\'', '"', ')'])) {
            if !url.username().is_empty() || url.password().is_some() {
                let _ = url.set_username("");
                let _ = url.set_password(None);
                output.push_str(url.as_str());
                let parsed_len = url_text.trim_end_matches(['\'', '"', ')']).len();
                output.push_str(&url_text[parsed_len..]);
            } else {
                output.push_str(url_text);
            }
        } else {
            output.push_str(url_text);
        }
        remainder = &candidate[end..];
    }
    output.push_str(remainder);
    output
}

fn find_url_start(input: &str) -> Option<usize> {
    match (input.find("https://"), input.find("http://")) {
        (Some(https), Some(http)) => Some(https.min(http)),
        (Some(index), None) | (None, Some(index)) => Some(index),
        (None, None) => None,
    }
}
