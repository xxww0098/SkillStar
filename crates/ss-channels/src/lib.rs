//! Organization-private GitHub repositories used as shared Skill channels.
//!
//! The facade owns the recoverable create/bind transaction. Presentation
//! layers never infer repository identity from mutable owner/name routing data.

mod channel_auto_update;
mod channel_receipt;
mod channel_removal;
mod channel_removal_installer;
mod channel_rollback;
mod channel_update;
mod channel_update_installer;
mod channel_update_types;
mod existing;
mod git_read;
mod github;
mod github_membership;
mod github_status;
mod marketplace_export;
mod membership;
mod plugin_market_format;
mod release;
mod release_content_verifier;
mod release_scanner;
mod subscription;
mod subscription_installer;
mod subscription_remote;
mod subscription_validation;

#[cfg(test)]
mod channel_access_tests;
#[cfg(test)]
mod channel_auto_update_tests;
#[cfg(test)]
mod channel_conversion_tests;
#[cfg(all(test, not(windows)))]
mod channel_install_authority_tests;
#[cfg(test)]
mod channel_removal_tests;
#[cfg(test)]
mod channel_rollback_tests;
#[cfg(test)]
mod channel_update_tests;
#[cfg(test)]
mod marketplace_export_tests;
#[cfg(test)]
mod subscription_installer_tests;
#[cfg(test)]
mod subscription_tests;

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};

pub use ss_skills::channel_registry::*;

pub use channel_auto_update::{channel_auto_update_state, set_channel_auto_update_enabled};
pub use channel_removal::{
    ChannelRemovedSkillHandler, ConvertRemovedChannelSkillRequest, HandleRemovedChannelSkillResult,
    HandleRevokedChannelSkillResult, InstallChannelSkillResult,
};
pub use channel_rollback::{
    ChannelSkillRollbackResult, ChannelSkillRollbackTarget, RollbackChannelSkillRequest,
};
pub use channel_update::{
    ApplyChannelUpdateRequest, ApplyChannelUpdateResult, ChannelSkillUpdateReceipt,
    ChannelSkillUpdateRequest, ChannelSkillUpdateResolution, ChannelSubscriptionUpdater,
    ChannelUpdateInspection, RetainedChannelContent, channel_update_state,
};
pub use existing::{
    ExistingChannelExposure, ExistingChannelRegistrationFacade,
    ExistingChannelRegistrationSessions, ExistingChannelRepositoryCandidate,
    ExistingChannelScanPreview, ExistingChannelScanRequest, ExistingChannelSkillPreview,
    ExistingRepositoryInventory, ExistingRepositoryScanner, GitExistingRepositoryScanner,
};
pub use github::ProductionSharedChannelGateway;
pub use marketplace_export::{
    ChannelMarketplaceExport, MarketplaceManifest, MarketplaceMetadata, PluginEntry,
    PluginManifest, PluginOwner, PluginSource, PreparedChannelMarketplace, PreparedChannelPlugin,
    RemoteSource, WrittenMarketplace, build_channel_marketplace, export_channel_marketplace,
    write_prepared_marketplace,
};
pub use membership::{
    ChannelInvitation, ChannelInvitationAction, ChannelInviteRole, ChannelMember,
    ChannelMemberIdentity, ChannelMemberRevocationResult, ChannelMemberRevocationStatus,
    ChannelMembershipFacade, ChannelMembershipGateway, ChannelMembershipSnapshot,
    ChannelMembershipStatus, CreateChannelInvitationRequest, EffectiveRepositoryAccess,
    RemoteChannelInvitation, RemoteInvitationOutcome, RepositoryAccessSource,
};
pub use release::{
    CHANNEL_RELEASE_MANIFEST_VERSION, ChannelDraftScanner, ChannelDraftSkill, ChannelDraftSnapshot,
    ChannelPublicationFacade, ChannelPublicationGateway, ChannelPublishPreview,
    ChannelPublishResult, ChannelPublishSessions, ChannelReleaseManifest, ChannelReleaseSkill,
    ChannelSkillReleaseStatus, RemoteChannelRelease, validate_manifest,
};
pub use release_scanner::GitChannelDraftScanner;
pub use subscription::{
    ChannelInstallReceipt, ChannelInstallRequest, ChannelRepositoryExposure,
    ChannelSubscriptionFacade, ChannelSubscriptionGateway, ChannelSubscriptionInstaller,
    ChannelSubscriptionReview, ChannelSubscriptionReviewSkill, SubscribeChannelRequest,
};
pub use subscription_installer::GitChannelSubscriptionInstaller;

pub(crate) static SHARED_CHANNEL_MUTATION_GATE: tokio::sync::Mutex<()> =
    tokio::sync::Mutex::const_new(());

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

    pub fn list_channels(&self) -> Result<Vec<SharedChannelDescriptor>, SharedChannelError> {
        list_shared_channels(&self.registry)
    }

    pub async fn list_organizations(&self) -> Result<Vec<GitHubOrganization>, SharedChannelError> {
        self.gateway.list_organizations().await
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

pub fn list_shared_channels<R: SharedChannelRegistry>(
    registry: &R,
) -> Result<Vec<SharedChannelDescriptor>, SharedChannelError> {
    registry.list_read_only()
}

pub fn list_channel_subscriptions<S: ChannelSubscriptionRegistry>(
    subscriptions: &S,
) -> Result<Vec<ChannelSubscriptionView>, SharedChannelError> {
    subscriptions.list_views()
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

pub(crate) fn validate_remote_repository(
    remote: &RemoteRepository,
    organization: &GitHubOrganization,
    expected_repository_id: u64,
) -> Result<(), SharedChannelError> {
    if remote.id != expected_repository_id {
        return Err(SharedChannelError::new(
            SharedChannelErrorCode::Integrity,
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

/// Collapse catalog + harness copies of one identity into the install unit:
/// the non-harness (catalog) copy wins; two independent catalog folders with
/// the same identity are a collision (`Err`).
pub(crate) fn collapse_identity_copies(
    skills: Vec<ss_skills::repo_scanner::DiscoveredSkill>,
) -> Result<Vec<ss_skills::repo_scanner::DiscoveredSkill>, ()> {
    use std::collections::BTreeMap;
    let mut groups: BTreeMap<String, Vec<ss_skills::repo_scanner::DiscoveredSkill>> = BTreeMap::new();
    for skill in skills {
        groups
            .entry(skill.id.to_ascii_lowercase())
            .or_default()
            .push(skill);
    }
    let mut collapsed = Vec::new();
    for (_, group) in groups {
        let independents = group
            .iter()
            .filter(|skill| !is_harness_folder(&skill.folder_path))
            .count();
        if independents > 1 {
            return Err(());
        }
        let chosen = group
            .iter()
            .find(|skill| !is_harness_folder(&skill.folder_path))
            .unwrap_or(&group[0]);
        collapsed.push(chosen.clone());
    }
    Ok(collapsed)
}

fn is_harness_folder(folder_path: &str) -> bool {
    folder_path.starts_with('.') && folder_path.contains("/skills/")
}
