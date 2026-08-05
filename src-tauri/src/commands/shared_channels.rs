//! Thin Tauri adapters for organization-private shared channels.

use skillstar_skills::shared_channels::{
    CreateSharedChannelRequest, DiskSharedChannelRegistry, GitHubOrganization,
    SharedChannelDescriptor, SharedChannelError, SharedChannelRegistry,
};
use tauri::State;

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
