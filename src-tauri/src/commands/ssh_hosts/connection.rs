//! Connection probe + host-key trust-on-first-use (TOFU) acceptance.

use skillstar_core::infra::error::AppError;
use skillstar_ssh::client::HostKeyState;
use skillstar_ssh::progress::ProgressSink;
use skillstar_ssh::store::{KeyringSecretStore, accept_host_key};
use skillstar_ssh::SshHostDef;
use tauri::AppHandle;

use super::{ConnectionTestResult, TauriProgressSink, new_session_id, to_ssh_err};

/// Result of a connection probe, including the host-key trust state so the UI
/// can prompt the user to accept a previously-unseen key.
///
/// Serialized snake_case to match the project DTO convention (see `AgentProfile`).
#[derive(Debug, serde::Serialize)]
pub struct TestConnectionOutput {
    pub result: ConnectionTestResult,
    /// `verified` | `unverified` | `mismatch`
    pub host_key_state: String,
    /// Present only when `host_key_state` is `unverified` or `mismatch`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
}

#[tauri::command]
pub async fn test_ssh_connection(
    def: SshHostDef,
    app: AppHandle,
) -> Result<TestConnectionOutput, AppError> {
    let secrets = KeyringSecretStore;
    let session_id = new_session_id();
    let sink = TauriProgressSink { app };
    let (result, state) =
        skillstar_ssh::client::test_connection(&def, &secrets, &session_id, &sink)
            .await
            .map_err(|e| {
                sink.emit(skillstar_ssh::progress::event(
                    &session_id,
                    skillstar_ssh::progress::Phase::Error,
                    skillstar_ssh::progress::Status::Fail,
                    e.to_string(),
                ));
                to_ssh_err(e)
            })?;
    Ok(encode_host_key_state(result, state))
}

/// Persist an accepted server fingerprint for a host id (TOFU confirmation).
#[tauri::command]
pub async fn accept_ssh_host_key(
    id: String,
    host: String,
    fingerprint: String,
) -> Result<(), AppError> {
    accept_host_key(&id, &host, &fingerprint).map_err(|e| AppError::Ssh(e.to_string()))
}

fn encode_host_key_state(
    result: ConnectionTestResult,
    state: HostKeyState,
) -> TestConnectionOutput {
    match state {
        HostKeyState::Verified => TestConnectionOutput {
            result,
            host_key_state: "verified".into(),
            fingerprint: None,
        },
        HostKeyState::Unverified { fingerprint } => TestConnectionOutput {
            result,
            host_key_state: "unverified".into(),
            fingerprint: Some(fingerprint),
        },
        HostKeyState::Mismatch { actual, .. } => TestConnectionOutput {
            result,
            host_key_state: "mismatch".into(),
            fingerprint: Some(actual),
        },
    }
}
