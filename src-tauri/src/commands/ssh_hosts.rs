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
use tauri::{AppHandle, Emitter, Runtime};

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

/// Resolve step shared by [`with_session`] and the discover command (testable).
fn resolve_host_for_session(host_id: &str) -> Result<(SshHostDef, KeyringSecretStore), AppError> {
    resolve_host(host_id)
}

/// Discover-only connection: live SSH session or (tests) mock FS via [`mock_connector_slot`].
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
        "session ready",
    ));
    Ok(result)
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

/// Resolve a host by id (managed store) or `system:<alias>` (live ssh config),
/// connect (streaming progress to `sink`), and run `f` on the live session.
///
/// System hosts are resolved fresh from `~/.ssh/config` each call (read-only)
/// and authenticate with the system key file directly — no keyring involved.
async fn with_session<T, F, Fut, S>(
    host_id: &str,
    session_id: String,
    sink: S,
    f: F,
) -> Result<T, AppError>
where
    F: FnOnce(String, S, Session) -> Fut,
    Fut: std::future::Future<Output = Result<T, AppError>>,
    S: ProgressSink,
{
    let (host, secrets) = resolve_host_for_session(host_id)?;

    let handle = skillstar_ssh::client::connect(&host, &secrets, &session_id, &sink)
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
    f(session_id, sink, handle).await
}

/// Open a discover connection (prod dials; tests may install [`mock_connector_slot`]).
async fn open_discover_connection<S: ProgressSink>(
    host: &SshHostDef,
    secrets: &KeyringSecretStore,
    session_id: &str,
    sink: &S,
) -> Result<DiscoverConnection, AppError> {
    #[cfg(test)]
    if let Some(mock) = mock_connector_slot::take() {
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
fn emit_mock_connect_success(host: &SshHostDef, session_id: &str, sink: &impl ProgressSink) {
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
    sink.emit(event(
        session_id,
        Phase::Sftp,
        Status::Ok,
        "SFTP ready",
    ));
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

#[cfg(test)]
pub(crate) struct MockConnector {
    pub exec: skillstar_ssh::remote_fs::MockRemoteExec,
    pub fs: skillstar_ssh::remote_fs::MockRemoteFs,
}

#[cfg(test)]
pub(crate) mod mock_connector_slot {
    use super::MockConnector;
    use std::sync::{Mutex, OnceLock};

    static SLOT: OnceLock<Mutex<Option<MockConnector>>> = OnceLock::new();

    pub fn install(mock: MockConnector) {
        *SLOT.get_or_init(|| Mutex::new(None)).lock().unwrap() = Some(mock);
    }

    pub fn take() -> Option<MockConnector> {
        SLOT.get_or_init(|| Mutex::new(None)).lock().unwrap().take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use skillstar_ssh::AuthMethod;
    use skillstar_ssh::progress::{Phase, Status};
    use std::sync::{Arc, Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    struct SshHomeGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
        _temp: tempfile::TempDir,
    }

    impl Drop for SshHomeGuard {
        fn drop(&mut self) {
            // SAFETY: env_lock serialises HOME mutations across crate tests.
            unsafe {
                std::env::remove_var("HOME");
            }
        }
    }

    fn with_ssh_home(content: &str) -> SshHomeGuard {
        let lock = env_lock().lock().unwrap();
        let tmp = tempfile::TempDir::new().unwrap();
        let cfg_dir = tmp.path().join(".ssh");
        std::fs::create_dir_all(&cfg_dir).unwrap();
        std::fs::write(cfg_dir.join("config"), content).unwrap();
        // SAFETY: env_lock serialises HOME mutations across crate tests.
        unsafe {
            std::env::set_var("HOME", tmp.path());
        }
        SshHomeGuard {
            _lock: lock,
            _temp: tmp,
        }
    }

    #[test]
    fn resolve_host_system_vps_yy_uses_ssh_config_key_auth() {
        let _home = with_ssh_home(
            r#"
Host vps-yy
    HostName 64.83.38.21
    User root
    Port 2222
    IdentityFile ~/.ssh/id_ed25519_dstools
"#,
        );
        let (def, _secrets) = resolve_host("system:vps-yy").expect("system:vps-yy must resolve");
        assert_eq!(def.id, "system:vps-yy");
        assert_eq!(def.display_name, "vps-yy");
        assert_eq!(def.host, "64.83.38.21");
        assert_eq!(def.username, "root");
        assert_eq!(def.port, 2222);
        match def.auth_method {
            AuthMethod::Key { key_path } => {
                assert_eq!(key_path, "~/.ssh/id_ed25519_dstools");
            }
            other => panic!("expected key auth, got {other:?}"),
        }
    }

    #[test]
    fn resolve_host_system_missing_alias_errors() {
        let _home = with_ssh_home("Host other\n    HostName 1.2.3.4\n");
        match resolve_host("system:vps-yy") {
            Err(e) => assert!(e.to_string().contains("vps-yy")),
            Ok(_) => panic!("expected system:vps-yy to fail when alias is absent"),
        }
    }

    const VPS_YY_CONFIG: &str = r#"
Host vps-yy
    HostName 64.83.38.21
    User root
    Port 2222
    IdentityFile ~/.ssh/id_ed25519_dstools
"#;

    fn format_event_transcript(events: &[SshProgressEvent]) -> String {
        events
            .iter()
            .map(|e| format!("{:?}/{:?}: {}", e.phase, e.status, e.message))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn assert_phases_in_order(events: &[SshProgressEvent], expected: &[Phase]) {
        let mut last = 0usize;
        for phase in expected {
            let pos = events
                .iter()
                .skip(last)
                .position(|e| e.phase == *phase)
                .unwrap_or_else(|| panic!("missing phase {phase:?} in {events:?}"));
            last += pos + 1;
        }
    }

    /// Full chain: `discover_remote_skills` for system:vps-yy via mock connector.
    #[tokio::test]
    async fn discover_remote_skills_system_vps_yy() {
        use tauri::Listener;

        let _home = with_ssh_home(VPS_YY_CONFIG);
        mock_connector_slot::install(MockConnector {
            exec: skillstar_ssh::remote_fs::MockRemoteExec::default(),
            fs: skillstar_ssh::remote_fs::MockRemoteFs::vps_yy_layout(),
        });

        let app = tauri::test::mock_app();
        let events = Arc::new(Mutex::new(Vec::<SshProgressEvent>::new()));
        let events_c = events.clone();
        let _listener = app.handle().listen(CONNECT_STREAM_CHANNEL, move |event| {
            if let Ok(ev) = serde_json::from_str::<SshProgressEvent>(event.payload()) {
                events_c.lock().unwrap().push(ev);
            }
        });

        let result = discover_remote_skills("system:vps-yy".into(), app.handle().clone())
            .await
            .expect("discover_remote_skills for system:vps-yy");

        assert_eq!(result.skills.len(), 2, "vps-yy mock must report 2 skills");
        assert_eq!(result.needs_migration_count, 1);
        assert!(
            result.skills.iter().any(|s| {
                s.name == "hub-skill" && s.layout == skillstar_ssh::RemoteSkillLayout::HubManaged
            })
        );

        let events = events.lock().unwrap();
        let transcript = format_event_transcript(&events);
        eprintln!("vps-yy discover event transcript:\n{transcript}");

        assert!(
            transcript.contains("vps-yy"),
            "progress must mention vps-yy alias"
        );
        assert!(
            transcript.contains("found 2 skill"),
            "scan Ok must report found 2 skills, got:\n{transcript}"
        );
        assert!(events.iter().any(|e| {
            e.phase == Phase::Scan
                && e.status == Status::Start
                && e.message.contains("scanning remote")
        }));
        assert!(events.iter().any(|e| e.phase == Phase::Done && e.status == Status::Ok));
        assert_phases_in_order(
            &events,
            &[
                Phase::Dial,
                Phase::Handshake,
                Phase::Auth,
                Phase::HostKey,
                Phase::Sftp,
                Phase::Scan,
                Phase::Done,
            ],
        );
    }
}
