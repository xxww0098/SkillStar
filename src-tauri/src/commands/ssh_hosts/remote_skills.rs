//! Remote skill operations: discovery, push/batch-push, migrate, delete, and
//! the phase-2 content / git commands. Each command reuses the shared
//! [`with_session`] connect + host-key gate from the parent module.

use skillstar_core::infra::error::AppError;
use skillstar_ssh::Session;
use skillstar_ssh::progress::{ProgressSink, SshProgressEvent};
use skillstar_ssh::sftp;
use tauri::{AppHandle, Emitter, Runtime};

use super::{
    CONNECT_STREAM_CHANNEL, DiscoveryResult, MigrateResult, PushResult, RemoteSkill,
    RemoteSkillContent, RemoteSkillUpdateState, TauriProgressSink, new_session_id,
    resolve_host_for_session, to_ssh_err, with_session,
};

// ── Discovery ───────────────────────────────────────────────────────

/// Run discovery once SFTP is available (emits `scan` progress; shared by command + tests).
pub(crate) async fn discover_skills_with_progress<E, F, S>(
    exec: &mut E,
    fs: &F,
    session_id: &str,
    sink: &S,
) -> Result<DiscoveryResult, AppError>
where
    E: skillstar_ssh::client::RemoteExec,
    F: skillstar_ssh::remote_fs::RemoteDiscoveryFs,
    S: ProgressSink,
{
    sink.emit(skillstar_ssh::progress::event(
        session_id,
        skillstar_ssh::progress::Phase::Scan,
        skillstar_ssh::progress::Status::Start,
        "scanning remote for skills…",
    ));
    let res = sftp::discover_remote_skills(exec, fs)
        .await
        .map_err(to_ssh_err);
    sink.emit(skillstar_ssh::progress::event(
        session_id,
        skillstar_ssh::progress::Phase::Scan,
        skillstar_ssh::progress::Status::Ok,
        format!(
            "found {} skill(s) across {} agent(s)",
            res.as_ref().map(|r| r.skills.len()).unwrap_or(0),
            res.as_ref().map(|r| r.agents.len()).unwrap_or(0),
        ),
    ));
    res
}

/// Open SFTP on a live session, then scan for skills.
pub(crate) async fn discover_skills_on_session<S>(
    handle: &mut Session,
    session_id: &str,
    sink: &S,
) -> Result<DiscoveryResult, AppError>
where
    S: ProgressSink,
{
    let sftp = sftp::open_sftp(handle, session_id, sink)
        .await
        .map_err(to_ssh_err)?;
    discover_skills_with_progress(handle, &sftp, session_id, sink).await
}

/// Discover-only connection: live SSH session or (tests) mock FS via `mock_connector_slot`.
enum DiscoverConnection {
    Live(Session),
    #[cfg(test)]
    Mock {
        exec: skillstar_ssh::remote_fs::MockRemoteExec,
        fs: skillstar_ssh::remote_fs::MockRemoteFs,
    },
}

/// Resolve → connect (or mock) → scan → terminal `done`. Mirrors [`with_session`] for discovery.
async fn with_discover_session<S: ProgressSink>(
    host_id: &str,
    session_id: &str,
    sink: &S,
) -> Result<DiscoveryResult, AppError> {
    let (host, secrets) = resolve_host_for_session(host_id)?;
    let mut conn = open_discover_connection(&host, &secrets, session_id, sink).await?;
    let result = match &mut conn {
        DiscoverConnection::Live(handle) => {
            discover_skills_on_session(handle, session_id, sink).await?
        }
        #[cfg(test)]
        DiscoverConnection::Mock { exec, fs } => {
            discover_skills_with_progress(exec, fs, session_id, sink).await?
        }
    };
    sink.emit(skillstar_ssh::progress::event(
        session_id,
        skillstar_ssh::progress::Phase::Done,
        skillstar_ssh::progress::Status::Ok,
        "discovery complete",
    ));
    Ok(result)
}

/// Open a discover connection (prod dials; tests may install `mock_connector_slot`).
async fn open_discover_connection<S: ProgressSink>(
    host: &skillstar_ssh::SshHostDef,
    secrets: &skillstar_ssh::store::KeyringSecretStore,
    session_id: &str,
    sink: &S,
) -> Result<DiscoverConnection, AppError> {
    #[cfg(test)]
    if let Some(mock) = super::mock_connector_slot::take() {
        emit_mock_connect_success(host, session_id, sink);
        return Ok(DiscoverConnection::Mock {
            exec: mock.exec,
            fs: mock.fs,
        });
    }

    let handle = skillstar_ssh::client::connect(host, secrets, session_id, sink)
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
    Ok(DiscoverConnection::Live(handle))
}

/// Synthetic connect progress for mock connector tests (no network dial).
#[cfg(test)]
fn emit_mock_connect_success(
    host: &skillstar_ssh::SshHostDef,
    session_id: &str,
    sink: &impl ProgressSink,
) {
    use skillstar_ssh::progress::{Phase, Status, event};
    let port = if host.port == 0 { 22 } else { host.port };
    let addr = format!("{}:{port}", host.host);
    sink.emit(event(
        session_id,
        Phase::Dial,
        Status::Start,
        format!("dialing {addr} (system:vps-yy mock)…"),
    ));
    sink.emit(event(
        session_id,
        Phase::Handshake,
        Status::Ok,
        "connected (0ms), handshake done",
    ));
    sink.emit(event(
        session_id,
        Phase::Auth,
        Status::Start,
        format!("authenticating as {} (publickey)…", host.username),
    ));
    sink.emit(event(
        session_id,
        Phase::Auth,
        Status::Ok,
        "authenticated (publickey)",
    ));
    sink.emit(event(
        session_id,
        Phase::HostKey,
        Status::Ok,
        "server key matches saved fingerprint",
    ));
    sink.emit(event(
        session_id,
        Phase::Sftp,
        Status::Start,
        "opening SFTP subsystem…",
    ));
    sink.emit(event(session_id, Phase::Sftp, Status::Ok, "SFTP ready"));
}

/// Progress sink for [`discover_remote_skills`] (generic over runtime for mock-app tests).
struct DiscoverProgressSink<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> ProgressSink for DiscoverProgressSink<R> {
    fn emit(&self, event: SshProgressEvent) {
        let _ = self.app.emit(CONNECT_STREAM_CHANNEL, event);
    }
}

#[tauri::command]
pub async fn discover_remote_skills<R: Runtime>(
    host_id: String,
    app: AppHandle<R>,
) -> Result<DiscoveryResult, AppError> {
    let session_id = new_session_id();
    let sink = DiscoverProgressSink { app };
    with_discover_session(&host_id, &session_id, &sink).await
}

#[tauri::command]
pub async fn list_remote_skills(
    host_id: String,
    remote_dir: String,
    app: AppHandle,
) -> Result<Vec<RemoteSkill>, AppError> {
    let session_id = new_session_id();
    let sink = TauriProgressSink { app };
    with_session(&host_id, session_id, sink, |session_id, sink, mut handle| async move {
        let sftp = sftp::open_sftp(&mut handle, &session_id, &sink)
            .await
            .map_err(to_ssh_err)?;
        sftp::list_remote_skills(&sftp, &remote_dir)
            .await
            .map_err(to_ssh_err)
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
    with_session(&host_id, session_id, sink, |session_id, sink, mut handle| async move {
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
    with_session(&host_id, session_id, sink, |session_id, sink, mut handle| async move {
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
    with_session(&host_id, session_id, sink, |session_id, sink, mut handle| async move {
        let sftp = sftp::open_sftp(&mut handle, &session_id, &sink)
            .await
            .map_err(to_ssh_err)?;
        sftp::delete_remote_skill(&sftp, &remote_path)
            .await
            .map_err(to_ssh_err)
    })
    .await
}

/// Result of a batch push: per-skill outcome plus a success/total tally.
#[derive(Debug, serde::Serialize)]
pub struct BatchPushResult {
    pub pushed: Vec<PushResult>,
    /// Skills that failed to push, with the error message.
    pub failed: Vec<BatchPushFailure>,
    pub total: u32,
    pub succeeded: u32,
}

#[derive(Debug, serde::Serialize)]
pub struct BatchPushFailure {
    pub skill_name: String,
    pub error: String,
}

/// Push many local skills to the same remote host in **one** SSH session.
///
/// Each skill is pushed independently; a single failure does not abort the
/// batch — failures are collected and surfaced to the UI (mirrors the global
/// `batch_link_skills_to_agent` semantics for local agents). Uses the hub
/// layout (content + symlink) per skill.
#[tauri::command]
pub async fn push_skills_to_remote(
    host_id: String,
    skill_names: Vec<String>,
    remote_dir: String,
    app: AppHandle,
) -> Result<BatchPushResult, AppError> {
    let session_id = new_session_id();
    let sink = TauriProgressSink { app };
    let total = skill_names.len() as u32;
    with_session(&host_id, session_id, sink, |session_id, sink, mut handle| async move {
        sink.emit(skillstar_ssh::progress::event(
            &session_id,
            skillstar_ssh::progress::Phase::Done,
            skillstar_ssh::progress::Status::Start,
            format!("batch pushing {total} skill(s) via ~/.skillstar/hub…"),
        ));
        let mut pushed = Vec::new();
        let mut failed = Vec::new();
        for name in &skill_names {
            match skillstar_ssh::hub::push_skill_via_hub(
                &mut handle,
                &session_id,
                &sink,
                name,
                &remote_dir,
            )
            .await
            {
                Ok(r) => pushed.push(r),
                Err(e) => failed.push(BatchPushFailure {
                    skill_name: name.clone(),
                    error: e.to_string(),
                }),
            }
        }
        let succeeded = pushed.len() as u32;
        sink.emit(skillstar_ssh::progress::event(
            &session_id,
            skillstar_ssh::progress::Phase::Done,
            skillstar_ssh::progress::Status::Ok,
            format!("batch push done: {succeeded}/{total} ok"),
        ));
        Ok(BatchPushResult {
            pushed,
            failed,
            total,
            succeeded,
        })
    })
    .await
}

// ── Phase-2 remote skill content / git operations ──────────────────
//
// These wrap the crate helpers in `skillstar_ssh::hub` that were implemented
// but not exposed over IPC. Each reuses the single `with_session` connect +
// host-key gate, so they inherit the same TOFU + keepalive hardening.

/// Read the SKILL.md content of a hub-managed remote skill.
#[tauri::command]
pub async fn read_remote_skill_content(
    host_id: String,
    skill_name: String,
    app: AppHandle,
) -> Result<RemoteSkillContent, AppError> {
    let session_id = new_session_id();
    let sink = TauriProgressSink { app };
    with_session(&host_id, session_id, sink, |session_id, sink, mut handle| async move {
        skillstar_ssh::hub::read_remote_skill_content(&mut handle, &session_id, &sink, &skill_name)
            .await
            .map_err(to_ssh_err)
    })
    .await
}

/// Write raw text to a hub-managed remote skill's SKILL.md (atomic write).
#[tauri::command]
pub async fn write_remote_skill_content(
    host_id: String,
    skill_name: String,
    content: String,
    app: AppHandle,
) -> Result<(), AppError> {
    let session_id = new_session_id();
    let sink = TauriProgressSink { app };
    with_session(&host_id, session_id, sink, |session_id, sink, mut handle| async move {
        skillstar_ssh::hub::write_remote_skill_content(
            &mut handle,
            &session_id,
            &sink,
            &skill_name,
            &content,
        )
        .await
        .map_err(to_ssh_err)
    })
    .await
}

/// `git pull --ff-only` a hub-managed remote skill (git-backed clones only).
#[tauri::command]
pub async fn pull_remote_skill(
    host_id: String,
    skill_name: String,
    app: AppHandle,
) -> Result<(), AppError> {
    let session_id = new_session_id();
    let sink = TauriProgressSink { app };
    with_session(&host_id, session_id, sink, |session_id, sink, mut handle| async move {
        sink.emit(skillstar_ssh::progress::event(
            &session_id,
            skillstar_ssh::progress::Phase::Done,
            skillstar_ssh::progress::Status::Start,
            format!("pulling {skill_name}…"),
        ));
        let res = skillstar_ssh::hub::pull_remote_skill(&mut handle, &session_id, &sink, &skill_name)
            .await
            .map_err(to_ssh_err);
        sink.emit(skillstar_ssh::progress::event(
            &session_id,
            skillstar_ssh::progress::Phase::Done,
            skillstar_ssh::progress::Status::Ok,
            format!("pulled {skill_name} (done)"),
        ));
        res
    })
    .await
}

/// Toggle (create/remove) the agent symlink for a hub-managed skill.
#[tauri::command]
pub async fn toggle_remote_agent_link(
    host_id: String,
    skill_name: String,
    agent_skills_dir: String,
    enable: bool,
    app: AppHandle,
) -> Result<(), AppError> {
    let session_id = new_session_id();
    let sink = TauriProgressSink { app };
    with_session(&host_id, session_id, sink, |_session_id, _sink, mut handle| async move {
        skillstar_ssh::hub::toggle_remote_agent_link(
            &mut handle,
            &skill_name,
            &agent_skills_dir,
            enable,
        )
        .await
        .map_err(to_ssh_err)
    })
    .await
}

/// Install a skill from a git URL directly onto the remote host (clone + link).
#[tauri::command]
pub async fn install_remote_skill(
    host_id: String,
    url: String,
    skill_name: String,
    agent_skills_dir: String,
    app: AppHandle,
) -> Result<(), AppError> {
    let session_id = new_session_id();
    let sink = TauriProgressSink { app };
    with_session(&host_id, session_id, sink, |session_id, sink, mut handle| async move {
        skillstar_ssh::hub::install_remote_skill(
            &mut handle,
            &session_id,
            &sink,
            &url,
            &skill_name,
            &agent_skills_dir,
        )
        .await
        .map_err(to_ssh_err)
    })
    .await
}

/// Check update availability for all hub-managed skills on a host (git repos).
#[tauri::command]
pub async fn check_remote_skill_updates(
    host_id: String,
    app: AppHandle,
) -> Result<Vec<RemoteSkillUpdateState>, AppError> {
    let session_id = new_session_id();
    let sink = TauriProgressSink { app };
    with_session(&host_id, session_id, sink, |session_id, sink, mut handle| async move {
        skillstar_ssh::hub::check_remote_skill_updates(&mut handle, &session_id, &sink)
            .await
            .map_err(to_ssh_err)
    })
    .await
}
