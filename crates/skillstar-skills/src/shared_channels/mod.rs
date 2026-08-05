//! Organization-private GitHub repositories used as shared Skill channels.
//!
//! The facade owns the recoverable create/bind transaction. Presentation
//! layers never infer repository identity from mutable owner/name routing data.

mod existing;
mod github;
mod github_membership;
mod membership;
mod release;
mod release_scanner;
mod store;
mod subscription;
mod subscription_installer;
mod subscription_store;

#[cfg(test)]
mod subscription_tests;

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fmt;

pub use existing::{
    ExistingChannelExposure, ExistingChannelRegistrationFacade,
    ExistingChannelRegistrationSessions, ExistingChannelRepositoryCandidate,
    ExistingChannelScanPreview, ExistingChannelScanRequest, ExistingChannelSkillPreview,
    ExistingRepositoryInventory, ExistingRepositoryScanner, GitExistingRepositoryScanner,
};
pub use github::ProductionSharedChannelGateway;
pub use membership::{
    ChannelInvitation, ChannelInvitationAction, ChannelInviteRole, ChannelMember,
    ChannelMemberIdentity, ChannelMembershipFacade, ChannelMembershipGateway,
    ChannelMembershipSnapshot, ChannelMembershipStatus, CreateChannelInvitationRequest,
    EffectiveRepositoryAccess, RemoteChannelInvitation, RemoteInvitationOutcome,
    RepositoryAccessSource,
};
pub use release::{
    CHANNEL_CONTENT_HASH_VERSION, CHANNEL_RELEASE_MANIFEST_VERSION, ChannelDraftScanner,
    ChannelDraftSkill, ChannelDraftSnapshot, ChannelPublicationFacade, ChannelPublicationGateway,
    ChannelPublishPreview, ChannelPublishResult, ChannelPublishSessions, ChannelPublisherIdentity,
    ChannelReleaseManifest, ChannelReleaseSkill, ChannelSkillReleaseStatus, RemoteChannelRelease,
    revision_tag, validate_manifest,
};
pub use release_scanner::GitChannelDraftScanner;
pub use store::DiskSharedChannelRegistry;
pub use subscription::{
    CHANNEL_SUBSCRIPTION_DESCRIPTOR_VERSION, CHANNEL_SUBSCRIPTION_STORE_VERSION,
    ChannelInstallReceipt, ChannelInstallRequest, ChannelReleaseTarget, ChannelRepositoryExposure,
    ChannelSkillProvenance, ChannelSubscribedSkill, ChannelSubscription, ChannelSubscriptionFacade,
    ChannelSubscriptionGateway, ChannelSubscriptionInstaller, ChannelSubscriptionRegistry,
    ChannelSubscriptionReview, ChannelSubscriptionReviewSkill, ChannelSubscriptionStore,
    ChannelSubscriptionView, SubscribeChannelRequest,
};
pub use subscription_installer::GitChannelSubscriptionInstaller;
pub use subscription_store::DiskChannelSubscriptionRegistry;

pub const CHANNEL_DESCRIPTOR_VERSION: u32 = 1;
pub const SHARED_CHANNEL_STORE_VERSION: u32 = 1;

pub(super) static SHARED_CHANNEL_MUTATION_GATE: tokio::sync::Mutex<()> =
    tokio::sync::Mutex::const_new(());

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitHubOrganization {
    pub id: u64,
    pub login: String,
    pub avatar_url: Option<String>,
    pub viewer_is_admin: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryPermissions {
    pub admin: bool,
    pub maintain: bool,
    pub push: bool,
    pub pull: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteRepository {
    pub id: u64,
    pub owner_id: u64,
    pub owner_login: String,
    pub owner_type: String,
    pub name: String,
    pub default_branch: String,
    pub html_url: String,
    pub clone_url: String,
    pub private: bool,
    pub permissions: RepositoryPermissions,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CreateSharedChannelRequest {
    pub organization: String,
    pub repository_name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
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
    SubscriptionSchemaUnsupported,
    SubscriptionSelectionInvalid,
    SubscriptionInstallFailed,
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

#[async_trait]
pub trait SharedChannelGateway: Send + Sync {
    async fn list_organizations(&self) -> Result<Vec<GitHubOrganization>, SharedChannelError>;
    async fn list_selected_repositories(
        &self,
        organization_id: u64,
    ) -> Result<Vec<RemoteRepository>, SharedChannelError>;
    async fn create_private_repository(
        &self,
        organization: &str,
        name: &str,
        description: &str,
    ) -> Result<RemoteRepository, SharedChannelError>;
    async fn validate_selected_installation(
        &self,
        organization_id: u64,
    ) -> Result<(), SharedChannelError>;
    async fn get_selected_repository(
        &self,
        organization_id: u64,
        repository_id: u64,
    ) -> Result<RemoteRepository, SharedChannelError>;
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
}

pub struct SharedChannelFacade<G, R> {
    gateway: G,
    registry: R,
}

impl<G, R> SharedChannelFacade<G, R>
where
    G: SharedChannelGateway,
    R: SharedChannelRegistry,
{
    pub fn new(gateway: G, registry: R) -> Self {
        Self { gateway, registry }
    }

    pub async fn list_organizations(&self) -> Result<Vec<GitHubOrganization>, SharedChannelError> {
        self.gateway.list_organizations().await
    }

    pub fn list_channels(&self) -> Result<Vec<SharedChannelDescriptor>, SharedChannelError> {
        Ok(self.registry.load()?.channels)
    }

    pub async fn create_channel(
        &self,
        request: CreateSharedChannelRequest,
    ) -> Result<SharedChannelDescriptor, SharedChannelError> {
        let _mutation_guard = SHARED_CHANNEL_MUTATION_GATE.lock().await;
        let _registry_lease = self.registry.acquire_mutation_lease().await?;
        validate_repository_name(&request.repository_name)?;
        let organizations = self.gateway.list_organizations().await?;
        if organizations.is_empty() {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::NoOrganizations,
                "The signed-in GitHub user does not belong to an organization",
            ));
        }
        let organization = organizations
            .into_iter()
            .find(|candidate| candidate.login.eq_ignore_ascii_case(&request.organization))
            .ok_or_else(|| {
                SharedChannelError::new(
                    SharedChannelErrorCode::OrganizationUnavailable,
                    "The selected GitHub organization is unavailable to this identity",
                )
            })?;
        if !organization.viewer_is_admin {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::PermissionDenied,
                "Organization owner access is required to create and authorize a shared channel",
            ));
        }
        self.gateway
            .validate_selected_installation(organization.id)
            .await?;

        let mut store = self.registry.load()?;
        let remote = self
            .gateway
            .create_private_repository(
                &organization.login,
                &request.repository_name,
                &request.description,
            )
            .await?;
        validate_remote_repository(&remote, &organization, remote.id)?;
        let role = project_role(&remote.permissions).ok_or_else(|| {
            SharedChannelError::new(
                SharedChannelErrorCode::PermissionDenied,
                "The repository does not grant the current GitHub user read access",
            )
        })?;
        if role != SharedChannelRole::Owner {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::PermissionDenied,
                "Administration write access is required to own a shared channel",
            ));
        }

        let now = Utc::now().to_rfc3339();
        let pending = SharedChannelDescriptor {
            descriptor_version: CHANNEL_DESCRIPTOR_VERSION,
            repository_id: remote.id,
            organization_id: organization.id,
            owner: remote.owner_login,
            name: remote.name,
            html_url: remote.html_url,
            clone_url: remote.clone_url,
            role,
            status: SharedChannelStatus::AwaitingAppInstallation,
            authorization: SharedChannelAuthorization::default(),
            created_at: now.clone(),
            updated_at: now,
        };
        store.channels.push(pending.clone());
        self.registry.save(&store).map_err(|_| {
            SharedChannelError::new(
                SharedChannelErrorCode::Storage,
                format!(
                    "GitHub created {}/{} (repository ID {}), but SkillStar could not register it locally; manage the repository in GitHub before retrying",
                    pending.owner, pending.name, pending.repository_id
                ),
            )
        })?;

        self.activate_pending(pending, store).await
    }

    pub async fn resume_channel(
        &self,
        repository_id: u64,
    ) -> Result<SharedChannelDescriptor, SharedChannelError> {
        let _mutation_guard = SHARED_CHANNEL_MUTATION_GATE.lock().await;
        let _registry_lease = self.registry.acquire_mutation_lease().await?;
        let store = self.registry.load()?;
        let channel = store
            .channels
            .iter()
            .find(|channel| channel.repository_id == repository_id)
            .cloned()
            .ok_or_else(|| {
                SharedChannelError::new(
                    SharedChannelErrorCode::RepositoryNotFound,
                    "No pending shared channel exists for this repository ID",
                )
            })?;
        if channel.status == SharedChannelStatus::Active {
            return Ok(channel);
        }
        self.activate_pending(channel, store).await
    }

    async fn activate_pending(
        &self,
        mut channel: SharedChannelDescriptor,
        mut store: SharedChannelStore,
    ) -> Result<SharedChannelDescriptor, SharedChannelError> {
        let organization = self
            .gateway
            .list_organizations()
            .await?
            .into_iter()
            .find(|organization| organization.id == channel.organization_id)
            .ok_or_else(|| {
                SharedChannelError::new(
                    SharedChannelErrorCode::OrganizationUnavailable,
                    "The channel organization is no longer available to this GitHub identity",
                )
            })?;
        let remote = self
            .gateway
            .get_selected_repository(channel.organization_id, channel.repository_id)
            .await?;
        validate_remote_repository(&remote, &organization, channel.repository_id)?;
        let role = project_role(&remote.permissions).ok_or_else(|| {
            SharedChannelError::new(
                SharedChannelErrorCode::PermissionDenied,
                "The repository no longer grants the current GitHub user read access",
            )
        })?;
        if role != SharedChannelRole::Owner {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::PermissionDenied,
                "Administration write access is required to authorize this shared channel",
            ));
        }
        // The ID is stable; refresh mutable routing metadata before validating
        // App access so a renamed repository stays accurate.
        channel.owner = remote.owner_login;
        channel.name = remote.name;
        channel.html_url = remote.html_url;
        channel.clone_url = remote.clone_url;
        channel.role = role;
        channel.updated_at = Utc::now().to_rfc3339();
        let pending = store
            .channels
            .iter_mut()
            .find(|candidate| candidate.repository_id == channel.repository_id)
            .ok_or_else(|| {
                SharedChannelError::new(
                    SharedChannelErrorCode::RepositoryNotFound,
                    "The pending shared channel disappeared before App access validation",
                )
            })?;
        *pending = channel.clone();
        self.registry.save(&store)?;

        let active = store
            .channels
            .iter_mut()
            .find(|candidate| candidate.repository_id == channel.repository_id)
            .ok_or_else(|| {
                SharedChannelError::new(
                    SharedChannelErrorCode::RepositoryNotFound,
                    "The pending shared channel disappeared during App access validation",
                )
            })?;
        active.status = SharedChannelStatus::Active;
        active.updated_at = Utc::now().to_rfc3339();
        let active = active.clone();
        self.registry.save(&store)?;
        Ok(active)
    }
}

pub fn project_role(permissions: &RepositoryPermissions) -> Option<SharedChannelRole> {
    if permissions.admin {
        Some(SharedChannelRole::Owner)
    } else if permissions.maintain || permissions.push {
        Some(SharedChannelRole::Publisher)
    } else if permissions.pull {
        Some(SharedChannelRole::Subscriber)
    } else {
        None
    }
}

fn validate_repository_name(name: &str) -> Result<(), SharedChannelError> {
    let valid = !name.is_empty()
        && name.len() <= 100
        && !name.starts_with('.')
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || ".-_".contains(character));
    if valid {
        Ok(())
    } else {
        Err(SharedChannelError::new(
            SharedChannelErrorCode::InvalidRepositoryName,
            "Repository names may contain letters, numbers, '.', '-' and '_'",
        ))
    }
}

pub(super) fn validate_remote_repository(
    remote: &RemoteRepository,
    organization: &GitHubOrganization,
    expected_repository_id: u64,
) -> Result<(), SharedChannelError> {
    if remote.id != expected_repository_id {
        return Err(SharedChannelError::new(
            SharedChannelErrorCode::RepositoryNotFound,
            "GitHub returned a different repository than the requested repository ID",
        ));
    }
    if !remote.owner_type.eq_ignore_ascii_case("organization") {
        return Err(SharedChannelError::new(
            SharedChannelErrorCode::PersonalOwnerRejected,
            "Shared channels require a GitHub organization repository",
        ));
    }
    if remote.owner_id != organization.id
        || !remote.owner_login.eq_ignore_ascii_case(&organization.login)
    {
        return Err(SharedChannelError::new(
            SharedChannelErrorCode::OrganizationUnavailable,
            "GitHub created the repository under an unexpected owner",
        ));
    }
    if !remote.private {
        return Err(SharedChannelError::new(
            SharedChannelErrorCode::PublicRepositoryRejected,
            "Shared channel repositories must be private",
        ));
    }
    for raw in [&remote.html_url, &remote.clone_url] {
        let valid = url::Url::parse(raw).is_ok_and(|url| {
            url.scheme() == "https"
                && url
                    .host_str()
                    .is_some_and(|host| host.eq_ignore_ascii_case("github.com"))
                && url.port().is_none()
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none()
        });
        if !valid {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::UnsupportedHost,
                "Shared channels only support credential-free github.com HTTPS URLs",
            ));
        }
    }
    let expected_html_url = format!("https://github.com/{}/{}", remote.owner_login, remote.name);
    let expected_clone_url = format!("{expected_html_url}.git");
    if !remote.html_url.eq_ignore_ascii_case(&expected_html_url)
        || !remote.clone_url.eq_ignore_ascii_case(&expected_clone_url)
    {
        return Err(SharedChannelError::new(
            SharedChannelErrorCode::UnsupportedHost,
            "GitHub repository URLs do not match the validated owner and repository name",
        ));
    }
    Ok(())
}
