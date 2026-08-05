//! Thin Tauri adapters for organization-private shared channels.

use skillstar_skills::shared_channels::{
    ApplyChannelUpdateRequest, ApplyChannelUpdateResult, ChannelAutoUpdateExecution,
    ChannelAutoUpdateState, ChannelInvitation, ChannelInvitationAction,
    ChannelMemberRevocationResult, ChannelMembershipSnapshot, ChannelPublishPreview,
    ChannelPublishResult, ChannelSkillRollbackResult, ChannelSkillRollbackTarget,
    ChannelSubscription, ChannelSubscriptionRegistry, ChannelSubscriptionReview,
    ChannelSubscriptionView, ChannelUpdateSnapshot, ConvertRemovedChannelSkillRequest,
    CreateChannelInvitationRequest, CreateSharedChannelRequest, DiskChannelSubscriptionRegistry,
    DiskSharedChannelRegistry, ExistingChannelRepositoryCandidate, ExistingChannelScanPreview,
    ExistingChannelScanRequest, GitHubOrganization, HandleRemovedChannelSkillResult,
    HandleRevokedChannelSkillResult, InstallChannelSkillResult, RollbackChannelSkillRequest,
    SharedChannelDescriptor, SharedChannelError, SharedChannelRegistry, SubscribeChannelRequest,
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
    DiskSharedChannelRegistry.list_read_only()
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
pub async fn revoke_shared_channel_member(
    repository_id: u64,
    username: String,
    state: State<'_, GitHubAuthState>,
) -> Result<ChannelMemberRevocationResult, SharedChannelError> {
    state
        .channel_membership_facade()?
        .revoke_member(repository_id, &username)
        .await
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

#[tauri::command]
pub fn list_shared_channel_subscriptions()
-> Result<Vec<ChannelSubscriptionView>, SharedChannelError> {
    DiskChannelSubscriptionRegistry.list_views()
}

#[tauri::command]
pub async fn review_shared_channel_subscription(
    repository_id: u64,
    state: State<'_, GitHubAuthState>,
) -> Result<ChannelSubscriptionReview, SharedChannelError> {
    state
        .channel_subscription_facade(skillstar_skills::git_skill::GitSkillFacade::from_keyring())?
        .review(repository_id)
        .await
}

#[tauri::command]
pub async fn subscribe_shared_channel(
    request: SubscribeChannelRequest,
    session_id: String,
    app: AppHandle,
    state: State<'_, GitHubAuthState>,
) -> Result<ChannelSubscription, SharedChannelError> {
    let git_facade = state
        .begin_git_operation(app, Some(session_id))
        .map_err(SharedChannelError::from)?;
    let registered_session_id = git_facade.session().id().to_string();
    let result = match state.channel_subscription_facade(git_facade) {
        Ok(facade) => facade.subscribe(request).await,
        Err(error) => Err(error),
    };
    state.finish_git_operation(&registered_session_id);
    result
}

#[tauri::command]
pub fn get_shared_channel_update_state(
    repository_id: u64,
) -> Result<Option<ChannelUpdateSnapshot>, SharedChannelError> {
    Ok(DiskChannelSubscriptionRegistry
        .load_mutable()?
        .subscriptions
        .into_iter()
        .find(|subscription| subscription.repository_id == repository_id)
        .and_then(|subscription| subscription.last_update))
}

#[tauri::command]
pub async fn check_shared_channel_update(
    repository_id: u64,
    state: State<'_, GitHubAuthState>,
) -> Result<ChannelUpdateSnapshot, SharedChannelError> {
    state
        .channel_subscription_facade(skillstar_skills::git_skill::GitSkillFacade::from_keyring())?
        .check_update(repository_id)
        .await
}

#[tauri::command]
pub async fn apply_shared_channel_update(
    request: ApplyChannelUpdateRequest,
    session_id: String,
    app: AppHandle,
    state: State<'_, GitHubAuthState>,
) -> Result<ApplyChannelUpdateResult, SharedChannelError> {
    let git_facade = state
        .begin_git_operation(app, Some(session_id))
        .map_err(SharedChannelError::from)?;
    let registered_session_id = git_facade.session().id().to_string();
    let result = match state.channel_subscription_facade(git_facade) {
        Ok(facade) => facade.apply_update(request).await,
        Err(error) => Err(error),
    };
    state.finish_git_operation(&registered_session_id);
    result
}

#[tauri::command]
pub async fn list_shared_channel_skill_rollback_targets(
    repository_id: u64,
    skill_id: String,
    state: State<'_, GitHubAuthState>,
) -> Result<Vec<ChannelSkillRollbackTarget>, SharedChannelError> {
    state
        .channel_subscription_facade(skillstar_skills::git_skill::GitSkillFacade::from_keyring())?
        .list_skill_rollback_targets(repository_id, &skill_id)
        .await
}

#[tauri::command]
pub async fn rollback_shared_channel_skill(
    request: RollbackChannelSkillRequest,
    session_id: String,
    app: AppHandle,
    state: State<'_, GitHubAuthState>,
) -> Result<ChannelSkillRollbackResult, SharedChannelError> {
    let git_facade = state
        .begin_git_operation(app, Some(session_id))
        .map_err(SharedChannelError::from)?;
    let registered_session_id = git_facade.session().id().to_string();
    let result = match state.channel_subscription_facade(git_facade) {
        Ok(facade) => facade.rollback_skill(request).await,
        Err(error) => Err(error),
    };
    state.finish_git_operation(&registered_session_id);
    result
}

#[tauri::command]
pub async fn resume_shared_channel_skill_following(
    repository_id: u64,
    skill_id: String,
    state: State<'_, GitHubAuthState>,
) -> Result<ChannelUpdateSnapshot, SharedChannelError> {
    state
        .channel_subscription_facade(skillstar_skills::git_skill::GitSkillFacade::from_keyring())?
        .resume_following_skill(repository_id, &skill_id)
        .await
}

#[tauri::command]
pub async fn uninstall_removed_shared_channel_skill(
    repository_id: u64,
    skill_id: String,
    state: State<'_, GitHubAuthState>,
) -> Result<HandleRemovedChannelSkillResult, SharedChannelError> {
    state
        .channel_subscription_facade(skillstar_skills::git_skill::GitSkillFacade::from_keyring())?
        .uninstall_removed_skill(repository_id, &skill_id)
        .await
}

#[tauri::command]
pub async fn convert_removed_shared_channel_skill_to_local(
    request: ConvertRemovedChannelSkillRequest,
    state: State<'_, GitHubAuthState>,
) -> Result<HandleRemovedChannelSkillResult, SharedChannelError> {
    state
        .channel_subscription_facade(skillstar_skills::git_skill::GitSkillFacade::from_keyring())?
        .convert_removed_skill_to_local(request)
        .await
}

#[tauri::command]
pub async fn uninstall_revoked_shared_channel_skill(
    repository_id: u64,
    skill_id: String,
    state: State<'_, GitHubAuthState>,
) -> Result<HandleRevokedChannelSkillResult, SharedChannelError> {
    state
        .channel_subscription_facade(skillstar_skills::git_skill::GitSkillFacade::from_keyring())?
        .uninstall_revoked_skill(repository_id, &skill_id)
        .await
}

#[tauri::command]
pub async fn convert_revoked_shared_channel_skill_to_local(
    request: ConvertRemovedChannelSkillRequest,
    state: State<'_, GitHubAuthState>,
) -> Result<HandleRevokedChannelSkillResult, SharedChannelError> {
    state
        .channel_subscription_facade(skillstar_skills::git_skill::GitSkillFacade::from_keyring())?
        .convert_revoked_skill_to_local(request)
        .await
}

#[tauri::command]
pub async fn install_shared_channel_skill(
    repository_id: u64,
    skill_id: String,
    session_id: String,
    app: AppHandle,
    state: State<'_, GitHubAuthState>,
) -> Result<InstallChannelSkillResult, SharedChannelError> {
    let git_facade = state
        .begin_git_operation(app, Some(session_id))
        .map_err(SharedChannelError::from)?;
    let registered_session_id = git_facade.session().id().to_string();
    let result = match state.channel_subscription_facade(git_facade) {
        Ok(facade) => facade.install_channel_skill(repository_id, &skill_id).await,
        Err(error) => Err(error),
    };
    state.finish_git_operation(&registered_session_id);
    result
}

#[tauri::command]
pub fn get_shared_channel_auto_update_state(
    repository_id: u64,
) -> Result<ChannelAutoUpdateState, SharedChannelError> {
    let store = DiskChannelSubscriptionRegistry.load_mutable()?;
    store
        .subscriptions
        .into_iter()
        .find(|subscription| subscription.repository_id == repository_id)
        .map(|subscription| subscription.auto_update)
        .ok_or_else(|| {
            SharedChannelError::new(
                skillstar_skills::shared_channels::SharedChannelErrorCode::SubscriptionNotFound,
                "This channel has not been subscribed on this device",
            )
        })
}

#[tauri::command]
pub async fn set_shared_channel_auto_update_enabled(
    repository_id: u64,
    enabled: bool,
) -> Result<ChannelAutoUpdateState, SharedChannelError> {
    skillstar_skills::shared_channels::set_channel_auto_update_enabled(
        &DiskChannelSubscriptionRegistry,
        repository_id,
        enabled,
    )
    .await
}

#[tauri::command]
pub async fn run_shared_channel_auto_updates(
    app: AppHandle,
    session_id: String,
    state: State<'_, GitHubAuthState>,
) -> Result<Vec<ChannelAutoUpdateExecution>, SharedChannelError> {
    let git_facade = state
        .begin_git_operation(app, Some(session_id))
        .map_err(SharedChannelError::from)?;
    let registered_session_id = git_facade.session().id().to_string();
    let result = match state.channel_subscription_facade(git_facade) {
        Ok(facade) => facade.run_due_auto_updates().await,
        Err(error) => Err(error),
    };
    state.finish_git_operation(&registered_session_id);
    result
}
