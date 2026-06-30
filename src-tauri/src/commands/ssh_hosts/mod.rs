//! Tauri commands for SSH remote host management.
//!
//! Thin forwarder layer over the `skillstar-ssh` crate, mirroring the
//! `commands/agents.rs` two-tier structure: all logic lives in the crate, the
//! commands only translate errors to [`AppError`] and pass types across IPC.
//!
//! Split by concern to keep each file navigable:
//! - [`host_crud`] — managed/system host list + CRUD + import.
//! - [`connection`] — connection probe + host-key TOFU acceptance.
//! - [`remote_skills`] — remote skill discovery / push / migrate / git ops.
//!
//! This `mod.rs` keeps only the shared connection plumbing (`with_session`,
//! host resolution, the Tauri progress sink, error mapping) that all three
//! submodules build on, plus the cross-module test harness.
//!
//! Credential handling: passphrases / passwords are never returned to the
//! frontend. `add_ssh_host` / `update_ssh_host` accept an optional
//! `credential` string that is written to the system keyring and then dropped.

mod connection;
mod host_crud;
mod remote_skills;

pub use connection::*;
pub use host_crud::*;
pub use remote_skills::*;

use skillstar_core::infra::error::AppError;
use skillstar_ssh::progress::{ProgressSink, SshProgressEvent};
use skillstar_ssh::store::KeyringSecretStore;
use skillstar_ssh::{Session, SshHostDef};
use tauri::{AppHandle, Emitter};

/// Re-exported DTOs so the command signatures stay terse.
pub use skillstar_ssh::{
    ConnectionTestResult, DiscoveryResult, MigrateResult, PushResult, RemoteSkill,
    RemoteSkillContent, RemoteSkillUpdateState,
};

/// The Tauri event channel the frontend listens on for connection progress.
pub(crate) const CONNECT_STREAM_CHANNEL: &str = "ssh://connect-stream";

/// A [`ProgressSink`] that forwards each event to the Tauri frontend via
/// `window.emit("ssh://connect-stream", event)`.
#[derive(Clone)]
pub(crate) struct TauriProgressSink {
    pub app: AppHandle,
}

impl ProgressSink for TauriProgressSink {
    fn emit(&self, event: SshProgressEvent) {
        let _ = self.app.emit(CONNECT_STREAM_CHANNEL, event);
    }
}

/// A fresh, unique-ish session id per command invocation so the frontend can
/// filter events to the in-flight operation.
pub(crate) fn new_session_id() -> String {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("sess_{now_ms}")
}

/// Map an `anyhow::Error` into an [`AppError::Ssh`], preserving the
/// special-cased host-key markers used by the frontend TOFU flow.
pub(crate) fn to_ssh_err(err: anyhow::Error) -> AppError {
    // Machine-readable prefixes (`UNVERIFIED_HOST_KEY:` / `HOST_KEY_MISMATCH:`)
    // are preserved verbatim so the UI can branch on the TOFU state.
    AppError::Ssh(err.to_string())
}

/// Best-effort local username fallback when ssh config omits `User`.
pub(crate) fn whoami_username() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| String::from("root"))
}

/// Resolve step shared by [`with_session`] and the discover command (testable).
pub(crate) fn resolve_host_for_session(
    host_id: &str,
) -> Result<(SshHostDef, KeyringSecretStore), AppError> {
    resolve_host(host_id)
}

/// Resolve a host by id (managed store) or `system:<alias>` (live ssh config),
/// connect (streaming progress to `sink`), and run `f` on the live session.
///
/// System hosts are resolved fresh from `~/.ssh/config` each call (read-only)
/// and authenticate with the system key file directly — no keyring involved.
pub(crate) async fn with_session<T, F, Fut, S>(
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
        let def = load_hosts_internal()
            .into_iter()
            .find(|h| h.id == host_id)
            .ok_or_else(|| AppError::Ssh(format!("SSH host '{host_id}' not found")))?;
        Ok((def, KeyringSecretStore))
    }
}

/// Local alias for the crate's host loader (kept private to this module so the
/// submodules go through `resolve_host`/`with_session` instead).
fn load_hosts_internal() -> Vec<SshHostDef> {
    skillstar_ssh::store::load_hosts()
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
        assert!(events.iter().any(|e| {
            e.phase == Phase::Done
                && e.status == Status::Ok
                && e.message.contains("discovery complete")
        }));
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
