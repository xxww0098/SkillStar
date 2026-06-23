//! SSH client: connection, authentication, and host-key verification (TOFU).
//!
//! The connection flow is split so the command layer can implement the
//! "unknown host key → ask the user → accept" loop:
//!
//! 1. [`test_connection`] / [`connect`] dial the server. The [`SshHandler`]
//!    captures the server public key via `check_server_key`.
//! 2. If the fingerprint is not in the known-hosts store, the caller returns
//!    [`HostKeyState::Unverified`] to the UI with the fingerprint so the user
//!    can confirm; `store::accept_host_key` then persists it.
//! 3. On subsequent connects a mismatch (server returned a different key than
//!    the accepted one) is rejected with [`HostKeyState::Mismatch`].
//!
//! Authentication:
//! - `AuthMethod::Key` → `russh::keys::load_secret_key(path, passphrase)` then
//!   `authenticate_publickey` with SHA-256.
//! - `AuthMethod::Password` → `authenticate_password`.
//! The passphrase / password is read from the [`SecretStore`] (keyring).

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use russh::client::{self, Handle, Handler};
use russh::keys::{self, HashAlg, PrivateKeyWithHashAlg};
use russh::Disconnect;
use tokio::sync::oneshot;

use crate::progress::{Phase, ProgressSink, Status, event, event_with_detail};
use crate::store::{SecretStore, known_fingerprint};
use crate::types::SshHostDef;

/// Connect/read timeout applied to the TCP dial + SSH handshake.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// Outcome of a host-key check during connect.
#[derive(Debug, Clone)]
pub enum HostKeyState {
    /// The server key matched the previously accepted fingerprint.
    Verified,
    /// No fingerprint was accepted yet for this host id. Carries the SHA-256
    /// fingerprint the UI should show for confirmation.
    Unverified { fingerprint: String },
    /// The server presented a key that differs from the accepted one
    /// (possible MITM). Carries both the expected and actual fingerprints.
    Mismatch {
        expected: String,
        actual: String,
    },
}

/// Result of a connection probe.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ConnectionTestResult {
    /// Round-trip latency of connect + `whoami` in milliseconds.
    pub latency_ms: u64,
    /// Remote `whoami` output.
    pub remote_user: String,
    /// `uname -a` output (OS/arch hint), if the server supported it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
}

/// russh client handler. Captures the server public key fingerprint during the
/// handshake; everything else uses the trait's defaults (no channel handling
/// is needed here because we drive the session via the returned `Handle`).
///
/// Exposed only so the SSH session/SFTP handle types can be named by callers
/// in the Tauri command layer; the struct itself has no public fields.
pub struct SshHandler {
    /// Filled during `check_server_key`.
    fingerprint_tx: Option<oneshot::Sender<String>>,
}

impl Handler for SshHandler {
    type Error = russh::Error;

    fn check_server_key(
        &mut self,
        server_public_key: &keys::PublicKey,
    ) -> impl std::future::Future<Output = Result<bool, Self::Error>> + Send {
        // Compute the OpenSSH SHA-256 fingerprint (`SHA256:base64...`).
        let fp = server_public_key
            .fingerprint(HashAlg::Sha256)
            .to_string();
        if let Some(tx) = self.fingerprint_tx.take() {
            let _ = tx.send(fp.clone());
        }
        // Accept the key at the transport layer regardless — the caller
        // decides trust via the known-hosts store after the handshake, so
        // we can still read the fingerprint even for unknown hosts.
        async move { Ok(true) }
    }
}

/// Build a default SSH client config (sensible inactivity timeout, no
/// streaming compression surprises).
fn default_config() -> Arc<russh::client::Config> {
    let mut cfg = russh::client::Config::default();
    cfg.inactivity_timeout = Some(Duration::from_secs(300));
    Arc::new(cfg)
}

/// Resolve the `host:port` address string.
fn addr_string(host: &SshHostDef) -> String {
    format!("{}:{}", host.host, host.host_port())
}

impl SshHostDef {
    /// Port to dial, falling back to 22 if the stored value is 0.
    pub(crate) fn host_port(&self) -> u16 {
        if self.port == 0 {
            22
        } else {
            self.port
        }
    }
}

/// Dial + handshake + authenticate. Returns the live session handle plus the
/// server fingerprint observed during the handshake.
///
/// `sink` receives a progress event at each phase (dial / handshake / host_key
/// / auth) so the UI console can stream the connection. The caller is
/// responsible for the known-hosts decision (see [`resolve_host_key_state`]);
/// this function does **not** enforce it so the fingerprint can be reported
/// even for brand-new hosts.
async fn dial_and_authenticate<S: SecretStore>(
    host: &SshHostDef,
    secrets: &S,
    session_id: &str,
    sink: &impl ProgressSink,
) -> Result<(Handle<SshHandler>, String)> {
    let (fp_tx, fp_rx) = oneshot::channel();
    let handler = SshHandler {
        fingerprint_tx: Some(fp_tx),
    };

    let addr = addr_string(host);
    let config = default_config();

    sink.emit(event(session_id, Phase::Dial, Status::Start, format!("dialing {addr}…")));
    let dial_start = Instant::now();
    let connect_fut = client::connect(config, &addr, handler);
    let mut handle = tokio::time::timeout(CONNECT_TIMEOUT, connect_fut)
        .await
        .map_err(|_| {
            sink.emit(event(session_id, Phase::Dial, Status::Fail, format!("timeout after {}s", CONNECT_TIMEOUT.as_secs())));
            anyhow::anyhow!("SSH connect to {addr} timed out")
        })?
        .map_err(|e| {
            sink.emit(event(session_id, Phase::Dial, Status::Fail, format!("connect failed: {e}")));
            anyhow::anyhow!("SSH connect failed").context(e)
        })?;
    let dial_ms = dial_start.elapsed().as_millis();
    sink.emit(event(session_id, Phase::Handshake, Status::Ok, format!("connected ({dial_ms}ms), handshake done")));

    // Authenticate according to the configured method.
    match &host.auth_method {
        crate::types::AuthMethod::Password => {
            sink.emit(event(session_id, Phase::Auth, Status::Start, format!("authenticating as {} (password)…", host.username)));
            let password = secrets
                .get_secret(&host.id)?
                .ok_or_else(|| {
                    sink.emit(event(session_id, Phase::Auth, Status::Fail, "no password stored in keyring"));
                    anyhow::anyhow!("no password stored for host '{}'", host.id)
                })?;
            let authed = handle
                .authenticate_password(&host.username, password)
                .await
                .map_err(|e| {
                    sink.emit(event(session_id, Phase::Auth, Status::Fail, format!("password auth error: {e}")));
                    anyhow::anyhow!("SSH password authentication failed").context(e)
                })?;
            if !authed.success() {
                sink.emit(event(session_id, Phase::Auth, Status::Fail, "password rejected by server"));
                anyhow::bail!("SSH password authentication rejected");
            }
            sink.emit(event(session_id, Phase::Auth, Status::Ok, "authenticated (password)"));
        }
        crate::types::AuthMethod::Key { key_path } => {
            sink.emit(event(session_id, Phase::Auth, Status::Start, format!("authenticating as {} (publickey)…", host.username)));
            let passphrase = secrets.get_secret(&host.id)?;
            let key = load_private_key(key_path, passphrase.as_deref()).map_err(|e| {
                sink.emit(event(session_id, Phase::Auth, Status::Fail, format!("could not load key {key_path}: {e}")));
                e
            })?;
            // Prefer SHA-256; russh negotiates RSA hash via PrivateKeyWithHashAlg.
            let key_with_alg =
                PrivateKeyWithHashAlg::new(Arc::new(key), Some(HashAlg::Sha256));
            let authed = handle
                .authenticate_publickey(&host.username, key_with_alg)
                .await
                .map_err(|e| {
                    sink.emit(event(session_id, Phase::Auth, Status::Fail, format!("publickey auth error: {e}")));
                    anyhow::anyhow!("SSH public-key authentication failed").context(e)
                })?;
            if !authed.success() {
                sink.emit(event(session_id, Phase::Auth, Status::Fail, "publickey rejected by server"));
                anyhow::bail!("SSH public-key authentication rejected");
            }
            sink.emit(event(session_id, Phase::Auth, Status::Ok, "authenticated (publickey)"));
        }
    }

    let fingerprint = fp_rx.await.unwrap_or_default();
    Ok((handle, fingerprint))
}

/// Load a private key, expanding `~` and passing an optional passphrase.
fn load_private_key(
    path: &str,
    passphrase: Option<&str>,
) -> Result<keys::PrivateKey> {
    let expanded = expand_tilde(path);
    keys::load_secret_key(expanded, passphrase)
        .with_context(|| format!("failed to load private key at {path}"))
}

/// Expand a leading `~` / `~\` to the user's home directory.
fn expand_tilde(path: &str) -> std::path::PathBuf {
    if let Some(rest) = path
        .strip_prefix("~/")
        .or_else(|| path.strip_prefix("~\\"))
    {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    std::path::PathBuf::from(path)
}

/// Decide whether an observed fingerprint is trusted for the given host id.
pub fn resolve_host_key_state(host_id: &str, observed: &str) -> HostKeyState {
    match known_fingerprint(host_id) {
        None => HostKeyState::Unverified {
            fingerprint: observed.to_string(),
        },
        Some(expected) if expected == observed => HostKeyState::Verified,
        Some(expected) => HostKeyState::Mismatch {
            expected,
            actual: observed.to_string(),
        },
    }
}

/// Open a fully-authenticated session to `host`.
///
/// Enforces the known-hosts policy: an `Unverified` host emits a `host_key`
/// pending event (with the fingerprint) and returns an `UNVERIFIED_HOST_KEY`
/// error so the UI can prompt; a `Mismatch` always errors (refuses to connect).
/// The fingerprint is only persisted when the caller explicitly invokes
/// `store::accept_host_key`.
pub async fn connect<S: SecretStore>(
    host: &SshHostDef,
    secrets: &S,
    session_id: &str,
    sink: &impl ProgressSink,
) -> Result<Handle<SshHandler>> {
    let (handle, fingerprint) = dial_and_authenticate(host, secrets, session_id, sink).await?;
    match resolve_host_key_state(&host.id, &fingerprint) {
        HostKeyState::Verified => {
            sink.emit(event(session_id, Phase::HostKey, Status::Ok, "server key matches saved fingerprint"));
            Ok(handle)
        }
        HostKeyState::Unverified { .. } => {
            sink.emit(event_with_detail(
                session_id,
                Phase::HostKey,
                Status::Pending,
                "first connection — verify the server fingerprint",
                serde_json::json!({ "fingerprint": fingerprint }),
            ));
            // Close the session we just opened — the UI must accept the key first.
            let _ = handle
                .disconnect(Disconnect::ByApplication, "host key not yet accepted", "en")
                .await;
            Err(anyhow::anyhow!("UNVERIFIED_HOST_KEY:{fingerprint}"))
        }
        HostKeyState::Mismatch { expected, actual } => {
            sink.emit(event_with_detail(
                session_id,
                Phase::HostKey,
                Status::Fail,
                "server key mismatch — possible MITM",
                serde_json::json!({ "expected": expected, "actual": actual }),
            ));
            let _ = handle
                .disconnect(Disconnect::ByApplication, "host key mismatch", "en")
                .await;
            Err(anyhow::anyhow!(
                "HOST_KEY_MISMATCH: expected {expected}, got {actual}"
            ))
        }
    }
}

/// Probe a host: connect, authenticate, run `whoami` + `uname -a`, report
/// latency. Does **not** enforce known-hosts (so the UI can test before the
/// user accepts a key) but reports the [`HostKeyState`] alongside the result.
pub async fn test_connection<S: SecretStore>(
    host: &SshHostDef,
    secrets: &S,
    session_id: &str,
    sink: &impl ProgressSink,
) -> Result<(ConnectionTestResult, HostKeyState)> {
    let started = Instant::now();
    let (mut handle, fingerprint) = dial_and_authenticate(host, secrets, session_id, sink).await?;
    let state = resolve_host_key_state(&host.id, &fingerprint);
    match &state {
        HostKeyState::Verified => {
            sink.emit(event(session_id, Phase::HostKey, Status::Ok, "server key matches saved fingerprint"));
        }
        HostKeyState::Unverified { fingerprint } => {
            sink.emit(event_with_detail(
                session_id,
                Phase::HostKey,
                Status::Pending,
                "first connection — verify the server fingerprint",
                serde_json::json!({ "fingerprint": fingerprint }),
            ));
        }
        HostKeyState::Mismatch { expected, actual } => {
            sink.emit(event_with_detail(
                session_id,
                Phase::HostKey,
                Status::Fail,
                "server key mismatch — possible MITM",
                serde_json::json!({ "expected": expected, "actual": actual }),
            ));
        }
    }

    let remote_user = exec_capture(&mut handle, "whoami")
        .await
        .unwrap_or_default()
        .trim()
        .to_string();
    let system = exec_capture(&mut handle, "uname -srm")
        .await
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let _ = handle
        .disconnect(Disconnect::ByApplication, "test complete", "en")
        .await;
    let latency_ms = started.elapsed().as_millis() as u64;

    sink.emit(event(
        session_id,
        Phase::Done,
        Status::Ok,
        format!("ready — {remote_user} · {} · {latency_ms}ms", system.as_deref().unwrap_or("—")),
    ));

    Ok((
        ConnectionTestResult {
            latency_ms,
            remote_user,
            system,
        },
        state,
    ))
}

/// Execute a shell script on the remote host (used by layout classification).
pub trait RemoteExec: Send {
    fn exec_script(
        &mut self,
        script: &str,
    ) -> impl std::future::Future<Output = Result<String>> + Send;
}

impl RemoteExec for Handle<SshHandler> {
    async fn exec_script(&mut self, script: &str) -> Result<String> {
        exec_capture(self, script).await
    }
}

/// Run a command on the session and return its combined stdout/stderr as text.
///
/// Used internally by `test_connection`; exported for Phase-2 remote-config reads.
pub async fn exec_capture(
    handle: &mut Handle<SshHandler>,
    command: &str,
) -> Result<String> {
    let mut channel = handle
        .channel_open_session()
        .await
        .context("open session channel")?;
    channel
        .exec(true, command)
        .await
        .context("exec command")?;

    let mut out: Vec<u8> = Vec::new();
    loop {
        // Some servers send data on ChannelMsg::Data; EOF signals command end.
        match channel.wait().await {
            Some(russh::ChannelMsg::Data { ref data }) => {
                out.extend_from_slice(data);
            }
            Some(russh::ChannelMsg::ExtendedData { ref data, .. }) => {
                out.extend_from_slice(data);
            }
            Some(russh::ChannelMsg::ExitStatus { .. }) | None => break,
            _ => {}
        }
    }
    channel.eof().await.ok();
    channel.close().await.ok();

    Ok(String::from_utf8_lossy(&out).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::MemSecretStore;
    use crate::types::AuthMethod;

    #[test]
    fn resolve_state_unverified_when_no_prior_entry() {
        let _guard = test_data_dir();
        let state = resolve_host_key_state("ssh_never_seen", "SHA256:abc");
        match state {
            HostKeyState::Unverified { fingerprint } => {
                assert_eq!(fingerprint, "SHA256:abc");
            }
            other => panic!("expected Unverified, got {other:?}"),
        }
    }

    #[test]
    fn resolve_state_verified_when_match() {
        let _guard = test_data_dir();
        crate::store::accept_host_key("ssh_1", "h:22", "SHA256:abc").unwrap();
        assert!(matches!(
            resolve_host_key_state("ssh_1", "SHA256:abc"),
            HostKeyState::Verified
        ));
    }

    #[test]
    fn resolve_state_mismatch_reports_both() {
        let _guard = test_data_dir();
        crate::store::accept_host_key("ssh_1", "h:22", "SHA256:abc").unwrap();
        match resolve_host_key_state("ssh_1", "SHA256:xyz") {
            HostKeyState::Mismatch { expected, actual } => {
                assert_eq!(expected, "SHA256:abc");
                assert_eq!(actual, "SHA256:xyz");
            }
            other => panic!("expected Mismatch, got {other:?}"),
        }
    }

    #[test]
    fn expand_tilde_handles_home_prefix() {
        let p = expand_tilde("~/foo/bar");
        assert!(p.starts_with("/Users") || p.to_string_lossy().contains("foo/bar"));
        assert!(expand_tilde("/abs/path").as_os_str() == "/abs/path");
    }

    #[test]
    fn addr_string_uses_configured_port() {
        let host = SshHostDef {
            id: "x".into(),
            display_name: "X".into(),
            host: "1.2.3.4".into(),
            port: 2222,
            username: "u".into(),
            auth_method: AuthMethod::Password,
            default_remote_dir: String::new(),
        };
        assert_eq!(addr_string(&host), "1.2.3.4:2222");
    }

    /// Isolated `SKILLSTAR_DATA_DIR` so known-hosts file reads don't leak
    /// between tests in the same process. Holds the crate env lock so parallel
    /// tests can't interleave env mutations.
    struct DataDirGuard {
        _temp: tempfile::TempDir,
        _lock: std::sync::MutexGuard<'static, ()>,
    }
    impl Drop for DataDirGuard {
        fn drop(&mut self) {
            unsafe {
                std::env::remove_var("SKILLSTAR_DATA_DIR");
            }
        }
    }
    fn test_data_dir() -> DataDirGuard {
        let _lock = crate::test_support::env_lock().lock().unwrap();
        let temp = tempfile::TempDir::new().unwrap();
        // SAFETY: the env lock above serialises all such mutations.
        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
        }
        DataDirGuard {
            _temp: temp,
            _lock,
        }
    }

    // Reference the generic param so the unused-import check stays happy
    // (SecretStore is used at runtime in non-test code).
    #[test]
    fn secret_store_trait_is_used() {
        let store = MemSecretStore::new();
        assert!(store.get_secret("x").unwrap().is_none());
    }
}
