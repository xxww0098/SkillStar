//! Thin IPC adapters for GitHub App device authentication.

use skillstar_skills::github_auth::{
    DeviceAuthorization, DeviceFlowPoll, GitHubAuthError, GitHubConnectionStatus,
};
use tauri::State;

use crate::core::github_auth::GitHubAuthState;

#[tauri::command]
pub async fn github_auth_status(
    state: State<'_, GitHubAuthState>,
) -> Result<GitHubConnectionStatus, GitHubAuthError> {
    state.facade().status().await
}

#[tauri::command]
pub async fn github_auth_start(
    state: State<'_, GitHubAuthState>,
) -> Result<DeviceAuthorization, GitHubAuthError> {
    state.facade().start_device_flow().await
}

#[tauri::command]
pub async fn github_auth_poll(
    state: State<'_, GitHubAuthState>,
) -> Result<DeviceFlowPoll, GitHubAuthError> {
    state.facade().poll_device_flow().await
}

#[tauri::command]
pub async fn github_auth_cancel(
    state: State<'_, GitHubAuthState>,
) -> Result<bool, GitHubAuthError> {
    Ok(state.facade().cancel_device_flow())
}

#[tauri::command]
pub async fn github_auth_refresh(
    state: State<'_, GitHubAuthState>,
) -> Result<GitHubConnectionStatus, GitHubAuthError> {
    state.facade().refresh().await
}

#[tauri::command]
pub async fn github_auth_logout(state: State<'_, GitHubAuthState>) -> Result<(), GitHubAuthError> {
    state.facade().logout()
}
