//! GitHub App user authentication for private shared channels.
//!
//! The public facade deliberately separates provider I/O, credential storage,
//! and time. Tokens and device codes stay in non-serializable internal values;
//! only guidance, identity, expiry, and structured outcomes can cross IPC.

mod gateway;
mod store;

use std::fmt;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

pub use gateway::{
    GitHubHttpGateway, GitHubHttpResponse, GitHubHttpTransport, ProductionGitHubGateway,
    github_app_client_id,
};
pub use store::KeyringCredentialStore;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GitHubAuthErrorCode {
    Unavailable,
    Network,
    Proxy,
    CredentialStore,
    Protocol,
    NoPendingAuthorization,
    NotAuthenticated,
    RefreshUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GitHubAuthError {
    pub code: GitHubAuthErrorCode,
    pub message: String,
}

impl GitHubAuthError {
    pub fn new(code: GitHubAuthErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn credential_store() -> Self {
        Self::new(
            GitHubAuthErrorCode::CredentialStore,
            "Unable to access the system credential store",
        )
    }
}

impl fmt::Display for GitHubAuthError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for GitHubAuthError {}

pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

#[derive(Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

#[derive(Clone)]
pub struct DeviceAuthorizationGrant {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in_seconds: u64,
    pub interval_seconds: u64,
}

#[derive(Clone)]
pub struct TokenGrant {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub access_expires_in_seconds: Option<u64>,
    pub refresh_expires_in_seconds: Option<u64>,
}

#[derive(Clone)]
pub enum GatewayDevicePoll {
    Pending,
    SlowDown,
    Authorized(TokenGrant),
    Denied,
    Expired,
}

#[async_trait]
pub trait GitHubGateway: Send + Sync {
    async fn start_device_authorization(&self)
    -> Result<DeviceAuthorizationGrant, GitHubAuthError>;

    async fn poll_device_authorization(
        &self,
        device_code: &str,
    ) -> Result<GatewayDevicePoll, GitHubAuthError>;

    async fn refresh_access_token(
        &self,
        refresh_token: &str,
    ) -> Result<TokenGrant, GitHubAuthError>;

    async fn current_user(&self, access_token: &str) -> Result<GitHubIdentity, GitHubAuthError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitHubIdentity {
    pub id: u64,
    pub login: String,
    pub avatar_url: Option<String>,
}

#[derive(Clone)]
pub struct StoredCredential {
    access_token: String,
    refresh_token: Option<String>,
    access_expires_at: Option<DateTime<Utc>>,
    refresh_expires_at: Option<DateTime<Utc>>,
}

impl StoredCredential {
    pub fn access_token(&self) -> &str {
        &self.access_token
    }

    pub fn refresh_token(&self) -> Option<&str> {
        self.refresh_token.as_deref()
    }

    pub fn access_expires_at(&self) -> Option<DateTime<Utc>> {
        self.access_expires_at
    }

    pub fn refresh_expires_at(&self) -> Option<DateTime<Utc>> {
        self.refresh_expires_at
    }
}

pub trait CredentialStore: Send + Sync {
    fn load(&self) -> Result<Option<StoredCredential>, GitHubAuthError>;
    fn save(&self, credential: &StoredCredential) -> Result<(), GitHubAuthError>;
    fn delete(&self) -> Result<(), GitHubAuthError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeviceAuthorization {
    pub user_code: String,
    pub verification_uri: String,
    pub expires_at: DateTime<Utc>,
    pub interval_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum GitHubConnectionStatus {
    SignedOut,
    Connected {
        identity: GitHubIdentity,
        access_expires_at: Option<DateTime<Utc>>,
    },
    Expired {
        identity: Option<GitHubIdentity>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum DeviceFlowPoll {
    Pending { retry_after_seconds: u64 },
    SlowDown { retry_after_seconds: u64 },
    Connected { connection: GitHubConnectionStatus },
    Denied,
    Expired,
}

struct PendingAuthorization {
    generation: u64,
    device_code: String,
    expires_at: DateTime<Utc>,
    interval_seconds: u64,
}

#[derive(Default)]
struct RuntimeState {
    generation: u64,
    pending: Option<PendingAuthorization>,
    identity: Option<GitHubIdentity>,
}

pub struct GitHubAuthFacade<G, S, C> {
    gateway: G,
    credentials: S,
    clock: C,
    runtime: Mutex<RuntimeState>,
}

impl<G, S, C> GitHubAuthFacade<G, S, C>
where
    G: GitHubGateway,
    S: CredentialStore,
    C: Clock,
{
    pub fn new(gateway: G, credentials: S, clock: C) -> Self {
        Self {
            gateway,
            credentials,
            clock,
            runtime: Mutex::new(RuntimeState::default()),
        }
    }

    pub async fn start_device_flow(&self) -> Result<DeviceAuthorization, GitHubAuthError> {
        let generation = {
            let mut runtime = self.runtime.lock().expect("github auth runtime lock");
            runtime.generation = runtime.generation.wrapping_add(1);
            runtime.pending = None;
            runtime.generation
        };
        let grant = self.gateway.start_device_authorization().await?;
        let expires_at = add_seconds(self.clock.now(), grant.expires_in_seconds);
        let guidance = DeviceAuthorization {
            user_code: grant.user_code,
            verification_uri: grant.verification_uri,
            expires_at,
            interval_seconds: grant.interval_seconds,
        };
        let mut runtime = self.runtime.lock().expect("github auth runtime lock");
        if runtime.generation != generation {
            return Err(no_pending_authorization());
        }
        runtime.pending = Some(PendingAuthorization {
            generation,
            device_code: grant.device_code,
            expires_at,
            interval_seconds: grant.interval_seconds,
        });
        Ok(guidance)
    }

    pub async fn poll_device_flow(&self) -> Result<DeviceFlowPoll, GitHubAuthError> {
        let (generation, device_code, expires_at, interval_seconds) = {
            let runtime = self.runtime.lock().expect("github auth runtime lock");
            let pending = runtime.pending.as_ref().ok_or_else(|| {
                GitHubAuthError::new(
                    GitHubAuthErrorCode::NoPendingAuthorization,
                    "No GitHub device authorization is pending",
                )
            })?;
            (
                pending.generation,
                pending.device_code.clone(),
                pending.expires_at,
                pending.interval_seconds,
            )
        };

        if self.clock.now() >= expires_at {
            self.clear_pending(generation);
            return Ok(DeviceFlowPoll::Expired);
        }

        match self.gateway.poll_device_authorization(&device_code).await? {
            GatewayDevicePoll::Pending => {
                self.ensure_pending(generation)?;
                Ok(DeviceFlowPoll::Pending {
                    retry_after_seconds: interval_seconds,
                })
            }
            GatewayDevicePoll::SlowDown => {
                let retry_after_seconds = interval_seconds.saturating_add(5);
                let mut runtime = self.runtime.lock().expect("github auth runtime lock");
                let pending = pending_for_generation(&mut runtime, generation)?;
                pending.interval_seconds = retry_after_seconds;
                Ok(DeviceFlowPoll::SlowDown {
                    retry_after_seconds,
                })
            }
            GatewayDevicePoll::Denied => {
                self.clear_pending(generation);
                Ok(DeviceFlowPoll::Denied)
            }
            GatewayDevicePoll::Expired => {
                self.clear_pending(generation);
                Ok(DeviceFlowPoll::Expired)
            }
            GatewayDevicePoll::Authorized(grant) => {
                self.ensure_pending(generation)?;
                let identity = self.gateway.current_user(&grant.access_token).await?;
                let credential = credential_from_grant(grant, self.clock.now());
                let connection =
                    connection_from_credential(&credential, identity.clone(), self.clock.now());
                let mut runtime = self.runtime.lock().expect("github auth runtime lock");
                pending_for_generation(&mut runtime, generation)?;
                self.credentials.save(&credential)?;
                runtime.pending = None;
                runtime.identity = Some(identity);
                Ok(DeviceFlowPoll::Connected { connection })
            }
        }
    }

    pub fn cancel_device_flow(&self) -> bool {
        let mut runtime = self.runtime.lock().expect("github auth runtime lock");
        runtime.generation = runtime.generation.wrapping_add(1);
        runtime.pending.take().is_some()
    }

    pub async fn status(&self) -> Result<GitHubConnectionStatus, GitHubAuthError> {
        let generation = self
            .runtime
            .lock()
            .expect("github auth runtime lock")
            .generation;
        let Some(credential) = self.credentials.load()? else {
            return Ok(GitHubConnectionStatus::SignedOut);
        };
        let now = self.clock.now();
        let cached_identity = self
            .runtime
            .lock()
            .expect("github auth runtime lock")
            .identity
            .clone();
        if credential
            .access_expires_at
            .is_some_and(|expires_at| now >= expires_at)
        {
            if !self.is_generation_current(generation) {
                return Ok(GitHubConnectionStatus::SignedOut);
            }
            return Ok(GitHubConnectionStatus::Expired {
                identity: cached_identity,
            });
        }
        let identity = match cached_identity {
            Some(identity) => identity,
            None => self.gateway.current_user(&credential.access_token).await?,
        };
        let mut runtime = self.runtime.lock().expect("github auth runtime lock");
        if runtime.generation != generation {
            return Ok(GitHubConnectionStatus::SignedOut);
        }
        runtime.identity = Some(identity.clone());
        Ok(connection_from_credential(&credential, identity, now))
    }

    pub async fn refresh(&self) -> Result<GitHubConnectionStatus, GitHubAuthError> {
        let generation = self
            .runtime
            .lock()
            .expect("github auth runtime lock")
            .generation;
        let existing = self.credentials.load()?.ok_or_else(|| {
            GitHubAuthError::new(
                GitHubAuthErrorCode::NotAuthenticated,
                "Sign in to GitHub before refreshing the connection",
            )
        })?;
        let refresh_token = existing.refresh_token.as_deref().ok_or_else(|| {
            GitHubAuthError::new(
                GitHubAuthErrorCode::RefreshUnavailable,
                "The GitHub session cannot be refreshed; sign in again",
            )
        })?;
        let now = self.clock.now();
        if existing
            .refresh_expires_at
            .is_some_and(|expires_at| now >= expires_at)
        {
            return Err(GitHubAuthError::new(
                GitHubAuthErrorCode::RefreshUnavailable,
                "The GitHub refresh credential has expired; sign in again",
            ));
        }

        let grant = self.gateway.refresh_access_token(refresh_token).await?;
        let identity = self.gateway.current_user(&grant.access_token).await?;
        let refresh_was_rotated = grant.refresh_token.is_some();
        let credential = StoredCredential {
            access_token: grant.access_token,
            refresh_token: grant
                .refresh_token
                .or_else(|| existing.refresh_token.clone()),
            access_expires_at: grant
                .access_expires_in_seconds
                .map(|seconds| add_seconds(now, seconds)),
            refresh_expires_at: if refresh_was_rotated {
                grant
                    .refresh_expires_in_seconds
                    .map(|seconds| add_seconds(now, seconds))
            } else {
                existing.refresh_expires_at
            },
        };
        let mut runtime = self.runtime.lock().expect("github auth runtime lock");
        if runtime.generation != generation {
            return Err(GitHubAuthError::new(
                GitHubAuthErrorCode::NotAuthenticated,
                "The GitHub session changed while it was being refreshed",
            ));
        }
        self.credentials.save(&credential)?;
        runtime.identity = Some(identity.clone());
        Ok(connection_from_credential(&credential, identity, now))
    }

    pub fn logout(&self) -> Result<(), GitHubAuthError> {
        let mut runtime = self.runtime.lock().expect("github auth runtime lock");
        runtime.generation = runtime.generation.wrapping_add(1);
        runtime.pending = None;
        runtime.identity = None;
        self.credentials.delete()
    }

    fn clear_pending(&self, generation: u64) {
        let mut runtime = self.runtime.lock().expect("github auth runtime lock");
        if runtime.generation == generation {
            runtime.pending = None;
        }
    }

    fn ensure_pending(&self, generation: u64) -> Result<(), GitHubAuthError> {
        let mut runtime = self.runtime.lock().expect("github auth runtime lock");
        pending_for_generation(&mut runtime, generation).map(|_| ())
    }

    fn is_generation_current(&self, generation: u64) -> bool {
        self.runtime
            .lock()
            .expect("github auth runtime lock")
            .generation
            == generation
    }
}

fn pending_for_generation(
    runtime: &mut RuntimeState,
    generation: u64,
) -> Result<&mut PendingAuthorization, GitHubAuthError> {
    if runtime.generation != generation {
        return Err(no_pending_authorization());
    }
    runtime
        .pending
        .as_mut()
        .filter(|pending| pending.generation == generation)
        .ok_or_else(no_pending_authorization)
}

fn no_pending_authorization() -> GitHubAuthError {
    GitHubAuthError::new(
        GitHubAuthErrorCode::NoPendingAuthorization,
        "No GitHub device authorization is pending",
    )
}

fn add_seconds(now: DateTime<Utc>, seconds: u64) -> DateTime<Utc> {
    now.checked_add_signed(Duration::seconds(
        i64::try_from(seconds).unwrap_or(i64::MAX),
    ))
    .unwrap_or(DateTime::<Utc>::MAX_UTC)
}

fn credential_from_grant(grant: TokenGrant, now: DateTime<Utc>) -> StoredCredential {
    StoredCredential {
        access_token: grant.access_token,
        refresh_token: grant.refresh_token,
        access_expires_at: grant
            .access_expires_in_seconds
            .map(|seconds| add_seconds(now, seconds)),
        refresh_expires_at: grant
            .refresh_expires_in_seconds
            .map(|seconds| add_seconds(now, seconds)),
    }
}

fn connection_from_credential(
    credential: &StoredCredential,
    identity: GitHubIdentity,
    now: DateTime<Utc>,
) -> GitHubConnectionStatus {
    if credential
        .access_expires_at
        .is_some_and(|expires_at| now >= expires_at)
    {
        GitHubConnectionStatus::Expired {
            identity: Some(identity),
        }
    } else {
        GitHubConnectionStatus::Connected {
            identity,
            access_expires_at: credential.access_expires_at,
        }
    }
}

#[cfg(test)]
mod tests;
