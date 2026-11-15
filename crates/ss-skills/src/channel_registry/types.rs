use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt;

pub const CHANNEL_DESCRIPTOR_VERSION: u32 = 1;
pub const SHARED_CHANNEL_STORE_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SharedChannelRole {
    Owner,
    Publisher,
    Subscriber,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SharedChannelStatus {
    AwaitingAppInstallation,
    AwaitingInvitationAcceptance,
    Active,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedChannelAuthorization {
    pub repository_selection: String,
    pub administration: String,
    pub contents: String,
}

impl Default for SharedChannelAuthorization {
    fn default() -> Self {
        Self {
            repository_selection: "selected".into(),
            administration: "write".into(),
            contents: "write".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedChannelDescriptor {
    pub descriptor_version: u32,
    pub repository_id: u64,
    pub organization_id: u64,
    pub owner: String,
    pub name: String,
    pub html_url: String,
    pub clone_url: String,
    pub role: SharedChannelRole,
    pub status: SharedChannelStatus,
    pub authorization: SharedChannelAuthorization,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedChannelStore {
    pub schema_version: u32,
    pub channels: Vec<SharedChannelDescriptor>,
}

impl Default for SharedChannelStore {
    fn default() -> Self {
        Self {
            schema_version: SHARED_CHANNEL_STORE_VERSION,
            channels: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SharedChannelErrorCode {
    NotAuthenticated,
    NoOrganizations,
    OrganizationUnavailable,
    PermissionDenied,
    AppNotInstalled,
    AppRepositorySelectionRequired,
    AppRepositoryAccessRequired,
    RepositoryAlreadyBound,
    RegistrationSessionNotFound,
    RegistrationSessionConflict,
    Cancelled,
    DraftChanged,
    ReleaseConflict,
    ReleaseNotFound,
    WorkflowPermissionRequired,
    Integrity,
    PersonalOwnerRejected,
    PublicRepositoryRejected,
    UnsupportedHost,
    InvalidRepositoryName,
    RepositoryConflict,
    RepositoryNotFound,
    InvitationNotFound,
    InvitationOrganizationPolicy,
    InvitationSsoRequired,
    InvitationTwoFactorRequired,
    InvitationSeatUnavailable,
    InvitationValidation,
    InvitationRateLimited,
    InvitationLimit,
    SubscriptionAlreadyExists,
    SubscriptionAccessRevoked,
    SubscriptionNotFound,
    SubscriptionSchemaUnsupported,
    SubscriptionSelectionInvalid,
    SubscriptionInstallFailed,
    SubscriptionUpdateFailed,
    Network,
    Protocol,
    Storage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SharedChannelError {
    pub code: SharedChannelErrorCode,
    pub message: String,
}

impl SharedChannelError {
    pub fn new(code: SharedChannelErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for SharedChannelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for SharedChannelError {}

impl From<crate::github_auth::GitHubAuthError> for SharedChannelError {
    fn from(error: crate::github_auth::GitHubAuthError) -> Self {
        use crate::github_auth::GitHubAuthErrorCode as AuthCode;
        let code = match error.code {
            AuthCode::NotAuthenticated | AuthCode::RefreshUnavailable => {
                SharedChannelErrorCode::NotAuthenticated
            }
            AuthCode::Network | AuthCode::Proxy => SharedChannelErrorCode::Network,
            _ => SharedChannelErrorCode::Protocol,
        };
        Self::new(code, error.message)
    }
}

pub trait SharedChannelMutationLease: Send {}

impl<T: Send> SharedChannelMutationLease for T {}

#[async_trait]
pub trait SharedChannelRegistry: Send + Sync {
    async fn acquire_mutation_lease(
        &self,
    ) -> Result<Box<dyn SharedChannelMutationLease>, SharedChannelError> {
        Ok(Box::new(()))
    }

    fn load(&self) -> Result<SharedChannelStore, SharedChannelError>;
    fn save(&self, store: &SharedChannelStore) -> Result<(), SharedChannelError>;

    fn list_read_only(&self) -> Result<Vec<SharedChannelDescriptor>, SharedChannelError> {
        Ok(self.load()?.channels)
    }
}
