//! Thin Tauri adapters for organization-private shared channels.

use skillstar_skills::shared_channels::{
    ChannelInvitation, ChannelInvitationAction, ChannelMembershipSnapshot, ChannelPublishPreview,
    ChannelPublishResult, CreateChannelInvitationRequest, CreateSharedChannelRequest,
    DiskSharedChannelRegistry, ExistingChannelRepositoryCandidate, ExistingChannelScanPreview,
    ExistingChannelScanRequest, GitHubOrganization, SharedChannelDescriptor, SharedChannelError,
    SharedChannelRegistry,
};
use tauri::{AppHandle, State};

use crate::core::github_auth::GitHubAuthState;

#[tauri::command]
pub async fn list_shared_channel_organizations(
    state: State<'_, GitHubAuthState>,
) -> Result<Vec<GitHubOrganization>, SharedChannelError> {
    state.shared_channel_facade()?.list_organizations().await
}

#[tauri::command]
pub fn list_shared_channels() -> Result<Vec<SharedChannelDescriptor>, SharedChannelError> {
    Ok(DiskSharedChannelRegistry.load()?.channels)
}

#[tauri::command]
pub async fn create_shared_channel(
    request: CreateSharedChannelRequest,
    state: State<'_, GitHubAuthState>,
) -> Result<SharedChannelDescriptor, SharedChannelError> {
    state.shared_channel_facade()?.create_channel(request).await
}

#[tauri::command]
pub async fn resume_shared_channel(
    repository_id: u64,
    state: State<'_, GitHubAuthState>,
) -> Result<SharedChannelDescriptor, SharedChannelError> {
    state
        .shared_channel_facade()?
        .resume_channel(repository_id)
        .await
}

#[tauri::command]
pub async fn list_existing_channel_repositories(
    organization_id: u64,
    state: State<'_, GitHubAuthState>,
) -> Result<Vec<ExistingChannelRepositoryCandidate>, SharedChannelError> {
    state
        .existing_channel_registration_facade()?
        .list_candidates(organization_id)
        .await
}

#[tauri::command]
pub async fn scan_existing_shared_channel(
    request: ExistingChannelScanRequest,
    session_id: String,
    app: AppHandle,
    state: State<'_, GitHubAuthState>,
) -> Result<ExistingChannelScanPreview, SharedChannelError> {
    let git_facade = state
        .begin_git_operation(app, Some(session_id.clone()))
        .map_err(SharedChannelError::from)?;
    let registered_session_id = git_facade.session().id().to_string();
    let result = match state.existing_channel_scan_facade(git_facade) {
        Ok(facade) => facade.scan(request, registered_session_id.clone()).await,
        Err(error) => Err(error),
    };
    state.finish_git_operation(&registered_session_id);
    result
}

#[tauri::command]
pub async fn confirm_existing_shared_channel(
    session_id: String,
    state: State<'_, GitHubAuthState>,
) -> Result<SharedChannelDescriptor, SharedChannelError> {
    state
        .existing_channel_registration_facade()?
        .confirm(&session_id)
        .await
}

#[tauri::command]
pub fn cancel_existing_shared_channel_registration(
    session_id: String,
    state: State<'_, GitHubAuthState>,
) -> bool {
    state.cancel_existing_channel_registration(&session_id)
}

#[tauri::command]
pub async fn preview_shared_channel_publish(
    repository_id: u64,
    session_id: String,
    app: AppHandle,
    state: State<'_, GitHubAuthState>,
) -> Result<ChannelPublishPreview, SharedChannelError> {
    let git_facade = state
        .begin_git_operation(app, Some(session_id))
        .map_err(SharedChannelError::from)?;
    let registered_session_id = git_facade.session().id().to_string();
    let result = match state.channel_publication_scan_facade(git_facade) {
        Ok(facade) => {
            facade
                .preview(repository_id, registered_session_id.clone())
                .await
        }
        Err(error) => Err(error),
    };
    state.finish_git_operation(&registered_session_id);
    result
}

#[tauri::command]
pub async fn publish_shared_channel(
    session_id: String,
    title: String,
    notes: String,
    state: State<'_, GitHubAuthState>,
) -> Result<ChannelPublishResult, SharedChannelError> {
    state
        .channel_publication_facade()?
        .publish(&session_id, title, notes)
        .await
}

#[tauri::command]
pub fn cancel_shared_channel_publish(
    session_id: String,
    state: State<'_, GitHubAuthState>,
) -> bool {
    state.cancel_channel_publication(&session_id)
}

#[tauri::command]
pub async fn list_shared_channel_membership(
    repository_id: u64,
    state: State<'_, GitHubAuthState>,
) -> Result<ChannelMembershipSnapshot, SharedChannelError> {
    state
        .channel_membership_facade()?
        .list_membership(repository_id)
        .await
}

#[tauri::command]
pub async fn invite_shared_channel_member(
    request: CreateChannelInvitationRequest,
    state: State<'_, GitHubAuthState>,
) -> Result<ChannelInvitationAction, SharedChannelError> {
    state.channel_membership_facade()?.invite(request).await
}

#[tauri::command]
pub async fn cancel_shared_channel_invitation(
    repository_id: u64,
    invitation_id: u64,
    state: State<'_, GitHubAuthState>,
) -> Result<ChannelInvitationAction, SharedChannelError> {
    state
        .channel_membership_facade()?
        .cancel(repository_id, invitation_id)
        .await
}

#[tauri::command]
pub async fn resend_shared_channel_invitation(
    repository_id: u64,
    invitation_id: u64,
    state: State<'_, GitHubAuthState>,
) -> Result<ChannelInvitationAction, SharedChannelError> {
    state
        .channel_membership_facade()?
        .resend(repository_id, invitation_id)
        .await
}

#[tauri::command]
pub async fn list_shared_channel_invitation_inbox(
    state: State<'_, GitHubAuthState>,
) -> Result<Vec<ChannelInvitation>, SharedChannelError> {
    state.channel_membership_facade()?.list_inbox().await
}

#[tauri::command]
pub async fn accept_shared_channel_invitation(
    invitation_id: u64,
    state: State<'_, GitHubAuthState>,
) -> Result<SharedChannelDescriptor, SharedChannelError> {
    state
        .channel_membership_facade()?
        .accept(invitation_id)
        .await
}

#[tauri::command]
pub async fn decline_shared_channel_invitation(
    invitation_id: u64,
    state: State<'_, GitHubAuthState>,
) -> Result<ChannelInvitationAction, SharedChannelError> {
    state
        .channel_membership_facade()?
        .decline(invitation_id)
        .await
}

#[tauri::command]
pub async fn resume_accepted_shared_channel(
    repository_id: u64,
    state: State<'_, GitHubAuthState>,
) -> Result<SharedChannelDescriptor, SharedChannelError> {
    state
        .channel_membership_facade()?
        .resume_accepted_channel(repository_id)
        .await
}
