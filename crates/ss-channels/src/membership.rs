use super::{
    CHANNEL_DESCRIPTOR_VERSION, GitHubOrganization, RemoteRepository, RepositoryPermissions,
    SHARED_CHANNEL_MUTATION_GATE, SharedChannelAuthorization, SharedChannelDescriptor,
    SharedChannelError, SharedChannelErrorCode, SharedChannelGateway, SharedChannelRegistry,
    SharedChannelRole, SharedChannelStatus, project_role, validate_remote_repository,
};
use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelInviteRole {
    Subscriber,
    Publisher,
}

impl ChannelInviteRole {
    pub fn github_permission(self) -> &'static str {
        match self {
            Self::Subscriber => "pull",
            Self::Publisher => "push",
        }
    }

    fn from_effective_role(role: SharedChannelRole) -> Self {
        match role {
            SharedChannelRole::Subscriber => Self::Subscriber,
            SharedChannelRole::Owner | SharedChannelRole::Publisher => Self::Publisher,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelMembershipStatus {
    Pending,
    Accepted,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelMemberIdentity {
    pub id: u64,
    pub login: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelMember {
    pub user: ChannelMemberIdentity,
    pub role: SharedChannelRole,
    pub github_role_name: String,
    pub status: ChannelMembershipStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelInvitation {
    pub id: u64,
    pub repository_id: u64,
    pub organization_id: u64,
    pub owner: String,
    pub repository_name: String,
    pub html_url: String,
    pub invitee: Option<ChannelMemberIdentity>,
    pub inviter: Option<ChannelMemberIdentity>,
    pub role: ChannelInviteRole,
    pub effective_role: SharedChannelRole,
    pub status: ChannelMembershipStatus,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelMembershipSnapshot {
    pub repository_id: u64,
    pub members: Vec<ChannelMember>,
    pub invitations: Vec<ChannelInvitation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CreateChannelInvitationRequest {
    pub repository_id: u64,
    pub username: String,
    pub role: ChannelInviteRole,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelInvitationAction {
    pub repository_id: u64,
    pub invitation_id: Option<u64>,
    pub username: String,
    pub role: ChannelInviteRole,
    pub status: ChannelMembershipStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepositoryAccessSource {
    Direct,
    Inherited,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelMemberRevocationStatus {
    Revoked,
    AccessRemains,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChannelMemberRevocationResult {
    pub repository_id: u64,
    pub username: String,
    pub status: ChannelMemberRevocationStatus,
    pub effective_role: Option<SharedChannelRole>,
    pub access_source: Option<RepositoryAccessSource>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveRepositoryAccess {
    pub role: SharedChannelRole,
    pub source: RepositoryAccessSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteChannelInvitation {
    pub invitation: ChannelInvitation,
    pub repository: RemoteRepository,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteInvitationOutcome {
    Pending(Box<RemoteChannelInvitation>),
    AlreadyAccepted(EffectiveRepositoryAccess),
}

#[async_trait]
pub trait ChannelMembershipGateway: Send + Sync {
    async fn effective_access(
        &self,
        repository: &RemoteRepository,
        username: &str,
    ) -> Result<Option<EffectiveRepositoryAccess>, SharedChannelError>;
    async fn list_members(
        &self,
        repository: &RemoteRepository,
    ) -> Result<Vec<ChannelMember>, SharedChannelError>;
    async fn list_repository_invitations(
        &self,
        repository: &RemoteRepository,
    ) -> Result<Vec<RemoteChannelInvitation>, SharedChannelError>;
    async fn create_invitation(
        &self,
        repository: &RemoteRepository,
        username: &str,
        role: ChannelInviteRole,
    ) -> Result<RemoteInvitationOutcome, SharedChannelError>;
    async fn remove_direct_collaborator(
        &self,
        repository: &RemoteRepository,
        username: &str,
    ) -> Result<(), SharedChannelError>;
    async fn cancel_invitation(
        &self,
        repository: &RemoteRepository,
        invitation_id: u64,
    ) -> Result<(), SharedChannelError>;
    async fn list_user_invitations(
        &self,
    ) -> Result<Vec<RemoteChannelInvitation>, SharedChannelError>;
    async fn accept_invitation(&self, invitation_id: u64) -> Result<(), SharedChannelError>;
    async fn decline_invitation(&self, invitation_id: u64) -> Result<(), SharedChannelError>;
    async fn get_repository_by_id(
        &self,
        repository_id: u64,
    ) -> Result<RemoteRepository, SharedChannelError>;
}

pub struct ChannelMembershipFacade<G, R> {
    gateway: G,
    registry: R,
}

impl<G, R> ChannelMembershipFacade<G, R>
where
    G: SharedChannelGateway + ChannelMembershipGateway,
    R: SharedChannelRegistry,
{
    pub fn new(gateway: G, registry: R) -> Self {
        Self { gateway, registry }
    }

    pub async fn list_membership(
        &self,
        repository_id: u64,
    ) -> Result<ChannelMembershipSnapshot, SharedChannelError> {
        let repository = self.admin_repository(repository_id).await?;
        let members = self.gateway.list_members(&repository).await?;
        let invitations = self
            .gateway
            .list_repository_invitations(&repository)
            .await?
            .into_iter()
            .map(|remote| remote.invitation)
            .collect();
        Ok(ChannelMembershipSnapshot {
            repository_id,
            members,
            invitations,
        })
    }

    pub async fn invite(
        &self,
        request: CreateChannelInvitationRequest,
    ) -> Result<ChannelInvitationAction, SharedChannelError> {
        let _mutation_guard = SHARED_CHANNEL_MUTATION_GATE.lock().await;
        let _registry_lease = self.registry.acquire_mutation_lease().await?;
        validate_username(&request.username)?;
        let repository = self.admin_repository(request.repository_id).await?;
        if let Some(access) = self
            .gateway
            .effective_access(&repository, &request.username)
            .await?
        {
            return Ok(ChannelInvitationAction {
                repository_id: request.repository_id,
                invitation_id: None,
                username: request.username,
                role: ChannelInviteRole::from_effective_role(access.role),
                status: ChannelMembershipStatus::Accepted,
            });
        }
        if let Some(pending) =
            self.gateway
                .list_repository_invitations(&repository)
                .await?
                .into_iter()
                .map(|remote| remote.invitation)
                .find(|invitation| {
                    invitation.invitee.as_ref().is_some_and(|invitee| {
                        invitee.login.eq_ignore_ascii_case(&request.username)
                    })
                })
        {
            return Ok(action_from_invitation(
                pending,
                ChannelMembershipStatus::Pending,
            ));
        }
        self.invite_without_access_check(repository, request.username, request.role)
            .await
    }

    pub async fn cancel(
        &self,
        repository_id: u64,
        invitation_id: u64,
    ) -> Result<ChannelInvitationAction, SharedChannelError> {
        let _mutation_guard = SHARED_CHANNEL_MUTATION_GATE.lock().await;
        let _registry_lease = self.registry.acquire_mutation_lease().await?;
        let repository = self.admin_repository(repository_id).await?;
        let invitation = self.pending_invitation(&repository, invitation_id).await?;
        self.gateway
            .cancel_invitation(&repository, invitation_id)
            .await?;
        Ok(action_from_invitation(
            invitation,
            ChannelMembershipStatus::Cancelled,
        ))
    }

    pub async fn revoke_member(
        &self,
        repository_id: u64,
        username: &str,
    ) -> Result<ChannelMemberRevocationResult, SharedChannelError> {
        validate_username(username)?;
        let _mutation_guard = SHARED_CHANNEL_MUTATION_GATE.lock().await;
        let _registry_lease = self.registry.acquire_mutation_lease().await?;
        let repository = self.admin_repository(repository_id).await?;
        self.gateway
            .remove_direct_collaborator(&repository, username)
            .await?;
        let access = self
            .gateway
            .effective_access(&repository, username)
            .await
            .map_err(|error| {
                SharedChannelError::new(
                    error.code,
                    format!(
                        "The direct collaborator grant was removed, but SkillStar could not verify effective GitHub access: {}",
                        error.message
                    ),
                )
            })?;
        Ok(match access {
            Some(access) => ChannelMemberRevocationResult {
                repository_id,
                username: username.to_string(),
                status: ChannelMemberRevocationStatus::AccessRemains,
                effective_role: Some(access.role),
                access_source: Some(access.source),
            },
            None => ChannelMemberRevocationResult {
                repository_id,
                username: username.to_string(),
                status: ChannelMemberRevocationStatus::Revoked,
                effective_role: None,
                access_source: None,
            },
        })
    }

    pub async fn resend(
        &self,
        repository_id: u64,
        invitation_id: u64,
    ) -> Result<ChannelInvitationAction, SharedChannelError> {
        let _mutation_guard = SHARED_CHANNEL_MUTATION_GATE.lock().await;
        let _registry_lease = self.registry.acquire_mutation_lease().await?;
        let repository = self.admin_repository(repository_id).await?;
        let invitation = self.pending_invitation(&repository, invitation_id).await?;
        if invitation.effective_role == SharedChannelRole::Owner {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::InvitationValidation,
                "SkillStar does not re-invite repository Admin grants; manage this invitation in GitHub",
            ));
        }
        let invitee = invitation.invitee.clone().ok_or_else(|| {
            SharedChannelError::new(
                SharedChannelErrorCode::InvitationValidation,
                "GitHub returned a pending invitation without an invitee",
            )
        })?;
        self.gateway
            .cancel_invitation(&repository, invitation_id)
            .await?;
        self.invite_without_access_check(repository, invitee.login, invitation.role)
            .await
    }

    pub async fn list_inbox(&self) -> Result<Vec<ChannelInvitation>, SharedChannelError> {
        Ok(self
            .gateway
            .list_user_invitations()
            .await?
            .into_iter()
            .filter(|remote| is_supported_invitation_repository(&remote.repository))
            .map(|remote| remote.invitation)
            .collect())
    }

    pub async fn accept(
        &self,
        invitation_id: u64,
    ) -> Result<SharedChannelDescriptor, SharedChannelError> {
        let invitation = self.user_invitation(invitation_id).await?;
        validate_invitation_repository(&invitation.repository)?;
        let repository = invitation.repository;
        let role = project_role(&repository.permissions).ok_or_else(|| {
            SharedChannelError::new(
                SharedChannelErrorCode::PermissionDenied,
                "The accepted repository does not grant the current GitHub user read access",
            )
        })?;

        let _mutation_guard = SHARED_CHANNEL_MUTATION_GATE.lock().await;
        let _registry_lease = self.registry.acquire_mutation_lease().await?;
        let mut store = self.registry.load()?;
        let now = Utc::now().to_rfc3339();
        let existing = store
            .channels
            .iter()
            .find(|channel| channel.repository_id == repository.id)
            .cloned();
        let mut descriptor = SharedChannelDescriptor {
            descriptor_version: CHANNEL_DESCRIPTOR_VERSION,
            repository_id: repository.id,
            organization_id: repository.owner_id,
            owner: repository.owner_login,
            name: repository.name,
            html_url: repository.html_url,
            clone_url: repository.clone_url,
            role,
            status: SharedChannelStatus::AwaitingInvitationAcceptance,
            authorization: SharedChannelAuthorization::default(),
            created_at: existing
                .as_ref()
                .map(|channel| channel.created_at.clone())
                .unwrap_or_else(|| now.clone()),
            updated_at: now.clone(),
        };
        upsert_channel(&mut store.channels, descriptor.clone());
        self.registry.save(&store)?;

        if let Err(error) = self.gateway.accept_invitation(invitation_id).await {
            if matches!(
                error.code,
                SharedChannelErrorCode::Network | SharedChannelErrorCode::Protocol
            ) {
                return Err(SharedChannelError::new(
                    error.code,
                    format!(
                        "GitHub invitation {invitation_id} has an uncertain acceptance outcome; SkillStar kept a local recovery marker. Use Retry accepted invitation import, or retry the invitation from the inbox if GitHub still shows it"
                    ),
                ));
            }
            match existing {
                Some(previous) => upsert_channel(&mut store.channels, previous),
                None => store
                    .channels
                    .retain(|channel| channel.repository_id != descriptor.repository_id),
            }
            if self.registry.save(&store).is_err() {
                return Err(SharedChannelError::new(
                    SharedChannelErrorCode::Storage,
                    format!(
                        "GitHub did not accept invitation {invitation_id}, and SkillStar could not roll back its local recovery marker; refresh the invitation inbox before retrying"
                    ),
                ));
            }
            return Err(error);
        }

        descriptor.status = SharedChannelStatus::Active;
        descriptor.updated_at = Utc::now().to_rfc3339();
        upsert_channel(&mut store.channels, descriptor.clone());
        self.registry.save(&store).map_err(|_| {
            SharedChannelError::new(
                SharedChannelErrorCode::Storage,
                format!(
                    "GitHub accepted the invitation for repository {}, but SkillStar could not finish the local import; use Retry accepted invitation import on the pending channel",
                    descriptor.repository_id
                ),
            )
        })?;
        Ok(descriptor)
    }

    pub async fn resume_accepted_channel(
        &self,
        repository_id: u64,
    ) -> Result<SharedChannelDescriptor, SharedChannelError> {
        let _mutation_guard = SHARED_CHANNEL_MUTATION_GATE.lock().await;
        let _registry_lease = self.registry.acquire_mutation_lease().await?;
        let mut store = self.registry.load()?;
        let pending = store
            .channels
            .iter()
            .find(|channel| channel.repository_id == repository_id)
            .cloned()
            .ok_or_else(|| {
                SharedChannelError::new(
                    SharedChannelErrorCode::RepositoryNotFound,
                    "The accepted invitation recovery marker is not registered on this device",
                )
            })?;
        if pending.status == SharedChannelStatus::Active {
            return Ok(pending);
        }
        if pending.status != SharedChannelStatus::AwaitingInvitationAcceptance {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::InvitationValidation,
                "This channel is not waiting for accepted invitation recovery",
            ));
        }
        let repository = self.gateway.get_repository_by_id(repository_id).await?;
        let organization = GitHubOrganization {
            id: pending.organization_id,
            // Organization login is mutable; numeric owner identity remains
            // bound to the recovery marker and catches repository transfer.
            login: repository.owner_login.clone(),
            avatar_url: None,
            viewer_is_admin: false,
        };
        validate_remote_repository(&repository, &organization, repository_id)?;
        let role = project_role(&repository.permissions).ok_or_else(|| {
            SharedChannelError::new(
                SharedChannelErrorCode::PermissionDenied,
                "The accepted repository does not grant the current GitHub user read access",
            )
        })?;
        let mut active = pending;
        active.owner = repository.owner_login;
        active.name = repository.name;
        active.html_url = repository.html_url;
        active.clone_url = repository.clone_url;
        active.role = role;
        active.status = SharedChannelStatus::Active;
        active.updated_at = Utc::now().to_rfc3339();
        upsert_channel(&mut store.channels, active.clone());
        self.registry.save(&store)?;
        Ok(active)
    }

    pub async fn decline(
        &self,
        invitation_id: u64,
    ) -> Result<ChannelInvitationAction, SharedChannelError> {
        let _mutation_guard = SHARED_CHANNEL_MUTATION_GATE.lock().await;
        let _registry_lease = self.registry.acquire_mutation_lease().await?;
        let invitation = self.user_invitation(invitation_id).await?;
        let mut store = self.registry.load()?;
        let channel_count = store.channels.len();
        store.channels.retain(|channel| {
            channel.repository_id != invitation.invitation.repository_id
                || channel.status != SharedChannelStatus::AwaitingInvitationAcceptance
        });
        if store.channels.len() != channel_count {
            // Remove a marker from a previous uncertain accept before touching
            // GitHub. If this save fails the remote invitation stays pending.
            self.registry.save(&store)?;
        }
        self.gateway.decline_invitation(invitation_id).await?;
        Ok(action_from_invitation(
            invitation.invitation,
            ChannelMembershipStatus::Cancelled,
        ))
    }

    async fn admin_repository(
        &self,
        repository_id: u64,
    ) -> Result<RemoteRepository, SharedChannelError> {
        let channel = self
            .registry
            .load()?
            .channels
            .into_iter()
            .find(|channel| channel.repository_id == repository_id)
            .ok_or_else(|| {
                SharedChannelError::new(
                    SharedChannelErrorCode::RepositoryNotFound,
                    "The shared channel is not registered on this device",
                )
            })?;
        if channel.status != SharedChannelStatus::Active {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::AppRepositoryAccessRequired,
                "Complete GitHub App authorization before managing channel members",
            ));
        }
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
        let repository = self
            .gateway
            .get_selected_repository(channel.organization_id, repository_id)
            .await?;
        validate_remote_repository(&repository, &organization, repository_id)?;
        if !repository.permissions.admin {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::PermissionDenied,
                "Current GitHub Admin permission is required to manage channel members",
            ));
        }
        Ok(repository)
    }

    async fn pending_invitation(
        &self,
        repository: &RemoteRepository,
        invitation_id: u64,
    ) -> Result<ChannelInvitation, SharedChannelError> {
        self.gateway
            .list_repository_invitations(repository)
            .await?
            .into_iter()
            .find(|candidate| candidate.invitation.id == invitation_id)
            .map(|candidate| candidate.invitation)
            .ok_or_else(invitation_not_found)
    }

    async fn user_invitation(
        &self,
        invitation_id: u64,
    ) -> Result<RemoteChannelInvitation, SharedChannelError> {
        self.gateway
            .list_user_invitations()
            .await?
            .into_iter()
            .find(|candidate| candidate.invitation.id == invitation_id)
            .ok_or_else(invitation_not_found)
    }

    async fn invite_without_access_check(
        &self,
        repository: RemoteRepository,
        username: String,
        role: ChannelInviteRole,
    ) -> Result<ChannelInvitationAction, SharedChannelError> {
        match self
            .gateway
            .create_invitation(&repository, &username, role)
            .await?
        {
            RemoteInvitationOutcome::Pending(remote) => Ok(ChannelInvitationAction {
                repository_id: repository.id,
                invitation_id: Some(remote.invitation.id),
                username,
                role,
                status: ChannelMembershipStatus::Pending,
            }),
            RemoteInvitationOutcome::AlreadyAccepted(access) => Ok(ChannelInvitationAction {
                repository_id: repository.id,
                invitation_id: None,
                username,
                role: ChannelInviteRole::from_effective_role(access.role),
                status: ChannelMembershipStatus::Accepted,
            }),
        }
    }
}

fn action_from_invitation(
    invitation: ChannelInvitation,
    status: ChannelMembershipStatus,
) -> ChannelInvitationAction {
    ChannelInvitationAction {
        repository_id: invitation.repository_id,
        invitation_id: Some(invitation.id),
        username: invitation
            .invitee
            .map(|identity| identity.login)
            .unwrap_or_default(),
        role: invitation.role,
        status,
    }
}

fn upsert_channel(
    channels: &mut Vec<SharedChannelDescriptor>,
    descriptor: SharedChannelDescriptor,
) {
    channels.retain(|channel| channel.repository_id != descriptor.repository_id);
    channels.push(descriptor);
}

fn validate_username(username: &str) -> Result<(), SharedChannelError> {
    let valid = !username.is_empty()
        && username.len() <= 39
        && !username.starts_with('-')
        && !username.ends_with('-')
        && username
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-');
    if valid {
        Ok(())
    } else {
        Err(SharedChannelError::new(
            SharedChannelErrorCode::InvitationValidation,
            "Enter a valid GitHub username",
        ))
    }
}

fn is_supported_invitation_repository(repository: &RemoteRepository) -> bool {
    repository.private
        && repository.owner_type.eq_ignore_ascii_case("organization")
        && validate_invitation_repository(repository).is_ok()
}

fn validate_invitation_repository(repository: &RemoteRepository) -> Result<(), SharedChannelError> {
    let organization = GitHubOrganization {
        id: repository.owner_id,
        login: repository.owner_login.clone(),
        avatar_url: None,
        viewer_is_admin: false,
    };
    validate_remote_repository(repository, &organization, repository.id)
}

fn invitation_not_found() -> SharedChannelError {
    SharedChannelError::new(
        SharedChannelErrorCode::InvitationNotFound,
        "The GitHub invitation is no longer pending; refresh and try again",
    )
}

pub(super) fn permissions_for_role(role: SharedChannelRole) -> RepositoryPermissions {
    RepositoryPermissions {
        admin: role == SharedChannelRole::Owner,
        maintain: false,
        push: matches!(
            role,
            SharedChannelRole::Owner | SharedChannelRole::Publisher
        ),
        pull: true,
    }
}

#[cfg(test)]
#[path = "membership_tests.rs"]
mod tests;
