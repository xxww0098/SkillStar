//! Tauri commands for SSH remote host management.
//!
//! Thin forwarder layer over the `skillstar-ssh` crate, mirroring the
//! `commands/agents.rs` two-tier structure: all logic lives in the crate, the
//! commands only translate errors to [`AppError`] and pass types across IPC.
//!
//! Credential handling: passphrases / passwords are never returned to the
//! frontend. `add_ssh_host` / `update_ssh_host` accept an optional
//! `credential` string that is written to the system keyring and then dropped.

use std::collections::HashSet;

use skillstar_core::infra::error::AppError;
use skillstar_ssh::client::HostKeyState;
use skillstar_ssh::progress::{ProgressSink, SshProgressEvent};
use skillstar_ssh::store::{KeyringSecretStore, accept_host_key, load_hosts};
use skillstar_ssh::sftp;
use skillstar_ssh::{HostsStore, Session, SshHostDef, SystemHost, parse_system_hosts};
use tauri::{AppHandle, Emitter};

/// Re-exported DTOs so the command signatures stay terse.
pub use skillstar_ssh::{ConnectionTestResult, DiscoveryResult, MigrateResult, PushResult, RemoteSkill};

/// The Tauri event channel the frontend listens on for connection progress.
const CONNECT_STREAM_CHANNEL: &str = "ssh://connect-stream";

/// A [`ProgressSink`] that forwards each event to the Tauri frontend via
/// `window.emit("ssh://connect-stream", event)`.
#[derive(Clone)]
struct TauriProgressSink {
    app: AppHandle,
}

impl ProgressSink for TauriProgressSink {
    fn emit(&self, event: SshProgressEvent) {
        let _ = self.app.emit(CONNECT_STREAM_CHANNEL, event);
    }
}

/// A fresh, unique-ish session id per command invocation so the frontend can
/// filter events to the in-flight operation.
fn new_session_id() -> String {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("sess_{now_ms}")
}

/// A host entry surfaced in the SSH page list.
///
/// `Managed` hosts live in `ssh_hosts.toml` (editable/deletable). `System`
/// hosts are discovered from `~/.ssh/config` (read-only, importable).
#[derive(Debug, serde::Serialize)]
#[serde(tag = "source", rename_all = "lowercase")]
pub enum SshHostListItem {
    Managed(SshHostDef),
    System(SystemHost),
}

/// Map an `anyhow::Error` into an [`AppError::Ssh`], preserving the
/// special-cased host-key markers used by the frontend TOFU flow.
fn to_ssh_err(err: anyhow::Error) -> AppError {
    let msg = err.to_string();
    if msg.starts_with("UNVERIFIED_HOST_KEY:") || msg.starts_with("HOST_KEY_MISMATCH:") {
        // Keep these machine-readable prefixes so the UI can branch.
        AppError::Ssh(msg)
    } else {
        AppError::Ssh(msg)
    }
}

// ── Host CRUD ───────────────────────────────────────────────────────

#[tauri::command]
pub async fn list_ssh_hosts() -> Result<Vec<SshHostListItem>, AppError> {
    let managed = load_hosts();
    // De-dup: a system host whose HostName matches a managed host's `host`
    // is already in the user's library, so don't show it twice.
    let managed_hosts: HashSet<String> = managed.iter().map(|h| h.host.clone()).collect();
    let system = parse_system_hosts()
        .into_iter()
        .filter(|s| !managed_hosts.contains(&s.host))
        .map(SshHostListItem::System);

    Ok(managed.into_iter().map(SshHostListItem::Managed).chain(system).collect())
}

/// Import a system-discovered host (from `~/.ssh/config`) into the managed
/// store so it becomes editable and gains a `default_remote_dir`. Reuses the
/// system IdentityFile path as the auth method.
#[tauri::command]
pub async fn import_system_host(alias: String) -> Result<SshHostDef, AppError> {
    let sys = skillstar_ssh::find_host_by_alias(&alias)
        .ok_or_else(|| AppError::Ssh(format!("system host '{alias}' not found")))?;
    let def = SshHostDef {
        id: String::new(),
        display_name: sys.alias.clone(),
        host: sys.host,
        port: sys.port,
        username: if sys.username.is_empty() {
            whoami_username()
        } else {
            sys.username
        },
        auth_method: match sys.identity_file {
            Some(path) => skillstar_ssh::AuthMethod::Key { key_path: path },
            None => skillstar_ssh::AuthMethod::Password,
        },
        default_remote_dir: String::new(),
    };
    let store = HostsStore::new(KeyringSecretStore);
    store
        .add(def, None)
        .map_err(|e| AppError::Ssh(e.to_string()))
}

/// Best-effort local username fallback when ssh config omits `User`.
fn whoami_username() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| String::from("root"))
}

/// Add a new SSH host. `credential` is the passphrase (for key auth) or
/// password (for password auth); it is stored in the keyring and discarded.
#[tauri::command]
pub async fn add_ssh_host(
    def: SshHostDef,
    credential: Option<String>,
) -> Result<SshHostDef, AppError> {
    let store = HostsStore::new(KeyringSecretStore);
    store
        .add(def, credential.as_deref())
        .map_err(|e| AppError::Ssh(e.to_string()))
}

#[tauri::command]
pub async fn update_ssh_host(
    id: String,
    def: SshHostDef,
    credential: Option<String>,
) -> Result<(), AppError> {
    let store = HostsStore::new(KeyringSecretStore);
    store
        .update(&id, def, credential.as_deref())
        .map_err(|e| AppError::Ssh(e.to_string()))
}

#[tauri::command]
pub async fn delete_ssh_host(id: String) -> Result<(), AppError> {
    let store = HostsStore::new(KeyringSecretStore);
    store
        .remove(&id)
        .map_err(|e| AppError::Ssh(e.to_string()))
}

// ── Connection test + host-key TOFU ─────────────────────────────────

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

// ── Remote skill operations ─────────────────────────────────────────

/// Discover all agent skills on the remote host in one scan: lists `$HOME/.*`
/// and reports every `<dir>/skills/<name>` that contains a `SKILL.md`, grouped
/// by agent. Replaces the old fixed-path probe so unknown agents (grok,
/// .agents, …) are found without a hardcoded table.
#[tauri::command]
pub async fn discover_remote_skills(
    host_id: String,
    app: AppHandle,
) -> Result<DiscoveryResult, AppError> {
    let session_id = new_session_id();
    let sink = TauriProgressSink { app };
    with_session(&host_id, &session_id, &sink, {
        let session_id = session_id.clone();
        let sink = sink.clone();
        move |mut handle| async move {
            let sftp = sftp::open_sftp(&mut handle, &session_id, &sink)
                .await
                .map_err(to_ssh_err)?;
            sink.emit(skillstar_ssh::progress::event(
                &session_id,
                skillstar_ssh::progress::Phase::Done,
                skillstar_ssh::progress::Status::Start,
                "scanning remote for skills…",
            ));
            let res = sftp::discover_remote_skills(&mut handle, &sftp)
                .await
                .map_err(to_ssh_err);
            sink.emit(skillstar_ssh::progress::event(
                &session_id,
                skillstar_ssh::progress::Phase::Done,
                skillstar_ssh::progress::Status::Ok,
                format!(
                    "found {} skill(s) across {} agent(s)",
                    res.as_ref().map(|r| r.skills.len()).unwrap_or(0),
                    res.as_ref().map(|r| r.agents.len()).unwrap_or(0),
                ),
            ));
            res
        }
    })
    .await
}

#[tauri::command]
pub async fn list_remote_skills(
    host_id: String,
    remote_dir: String,
    app: AppHandle,
) -> Result<Vec<RemoteSkill>, AppError> {
    let session_id = new_session_id();
    let sink = TauriProgressSink { app };
    with_session(&host_id, &session_id, &sink, {
        let session_id = session_id.clone();
        let sink = sink.clone();
        move |mut handle| async move {
            let sftp = sftp::open_sftp(&mut handle, &session_id, &sink)
                .await
                .map_err(to_ssh_err)?;
            sftp::list_remote_skills(&sftp, &remote_dir)
                .await
                .map_err(to_ssh_err)
        }
    })
    .await
}

#[tauri::command]
pub async fn push_skill_to_remote(
    host_id: String,
    skill_name: String,
    remote_dir: String,
    app: AppHandle,
) -> Result<PushResult, AppError> {
    let session_id = new_session_id();
    let sink = TauriProgressSink { app };
    with_session(&host_id, &session_id, &sink, {
        let session_id = session_id.clone();
        let sink = sink.clone();
        move |mut handle| async move {
            sink.emit(skillstar_ssh::progress::event(
                &session_id,
                skillstar_ssh::progress::Phase::Done,
                skillstar_ssh::progress::Status::Start,
                format!("pushing {skill_name} via ~/.skillstar/hub…"),
            ));
            let res = skillstar_ssh::hub::push_skill_via_hub(
                &mut handle,
                &session_id,
                &sink,
                &skill_name,
                &remote_dir,
            )
            .await
            .map_err(to_ssh_err);
            sink.emit(skillstar_ssh::progress::event(
                &session_id,
                skillstar_ssh::progress::Phase::Done,
                skillstar_ssh::progress::Status::Ok,
                format!("pushed {skill_name} (done)"),
            ));
            res
        }
    })
    .await
}

#[tauri::command]
pub async fn migrate_remote_skill_to_hub(
    host_id: String,
    skill_name: String,
    agent_skills_dir: String,
    standalone_path: String,
    app: AppHandle,
) -> Result<MigrateResult, AppError> {
    let session_id = new_session_id();
    let sink = TauriProgressSink { app };
    with_session(&host_id, &session_id, &sink, {
        let session_id = session_id.clone();
        let sink = sink.clone();
        move |mut handle| async move {
            sink.emit(skillstar_ssh::progress::event(
                &session_id,
                skillstar_ssh::progress::Phase::Done,
                skillstar_ssh::progress::Status::Start,
                format!("migrating {skill_name} to ~/.skillstar/hub…"),
            ));
            let res = skillstar_ssh::hub::migrate_remote_skill_to_hub(
                &mut handle,
                &skill_name,
                &agent_skills_dir,
                &standalone_path,
            )
            .await
            .map_err(to_ssh_err);
            sink.emit(skillstar_ssh::progress::event(
                &session_id,
                skillstar_ssh::progress::Phase::Done,
                skillstar_ssh::progress::Status::Ok,
                format!("migrated {skill_name} (done)"),
            ));
            res
        }
    })
    .await
}

#[tauri::command]
pub async fn delete_remote_skill(
    host_id: String,
    remote_path: String,
    app: AppHandle,
) -> Result<(), AppError> {
    let session_id = new_session_id();
    let sink = TauriProgressSink { app };
    with_session(&host_id, &session_id, &sink, {
        let session_id = session_id.clone();
        let sink = sink.clone();
        move |mut handle| async move {
            let sftp = sftp::open_sftp(&mut handle, &session_id, &sink)
                .await
                .map_err(to_ssh_err)?;
            sftp::delete_remote_skill(&sftp, &remote_path)
                .await
                .map_err(to_ssh_err)
        }
    })
    .await
}

/// Resolve a host by id (managed store) or `system:<alias>` (live ssh config),
/// connect (streaming progress to `sink`), and run `f` on the live session.
///
/// System hosts are resolved fresh from `~/.ssh/config` each call (read-only)
/// and authenticate with the system key file directly — no keyring involved.
async fn with_session<T, F, Fut>(
    host_id: &str,
    session_id: &str,
    sink: &TauriProgressSink,
    f: F,
) -> Result<T, AppError>
where
    F: FnOnce(Session) -> Fut,
    Fut: std::future::Future<Output = Result<T, AppError>>,
{
    let (host, secrets) = resolve_host(host_id)?;

    let handle = skillstar_ssh::client::connect(&host, &secrets, session_id, sink)
        .await
        .map_err(|e| {
            sink.emit(skillstar_ssh::progress::event(
                session_id,
                skillstar_ssh::progress::Phase::Error,
                skillstar_ssh::progress::Status::Fail,
                e.to_string(),
            ));
            to_ssh_err(e)
        })?;
    sink.emit(skillstar_ssh::progress::event(
        session_id,
        skillstar_ssh::progress::Phase::Done,
        skillstar_ssh::progress::Status::Ok,
        "session ready",
    ));
    f(handle).await
}

/// Resolve a `SshHostDef` from either the managed store or `~/.ssh/config`.
fn resolve_host(host_id: &str) -> Result<(SshHostDef, KeyringSecretStore), AppError> {
    if let Some(alias) = host_id.strip_prefix("system:") {
        let sys = skillstar_ssh::find_host_by_alias(alias)
            .ok_or_else(|| AppError::Ssh(format!("system host '{alias}' not found")))?;
        let def = SshHostDef {
            id: host_id.to_string(),
            display_name: sys.alias.clone(),
            host: sys.host,
            port: sys.port,
            username: if sys.username.is_empty() {
                whoami_username()
            } else {
                sys.username
            },
            auth_method: match sys.identity_file {
                Some(path) => skillstar_ssh::AuthMethod::Key { key_path: path },
                None => skillstar_ssh::AuthMethod::Password,
            },
            default_remote_dir: String::new(),
        };
        Ok((def, KeyringSecretStore))
    } else {
        let def = load_hosts()
            .into_iter()
            .find(|h| h.id == host_id)
            .ok_or_else(|| AppError::Ssh(format!("SSH host '{host_id}' not found")))?;
        Ok((def, KeyringSecretStore))
    }
}
