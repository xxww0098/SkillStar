//! Shared SFTP plumbing used by the push / list / delete operations.
//!
//! Holds the low-level pieces every operation needs: opening an SFTP session
//! and `mkdir -p` over SFTP. The operation-specific logic lives in the sibling
//! [`super::push`], [`super::list`] and [`super::delete`] modules.

use anyhow::{Context, Result};
use russh::client::Handle;
use russh_sftp::client::SftpSession;

use crate::ssh::client::SshHandler;

/// Bound on opening the SFTP subsystem (channel + subsystem + init handshake).
/// A server with a broken/missing sftp subsystem must fail fast, not hang the
/// whole operation until the connection-level inactivity timeout.
const SFTP_OPEN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

/// Open an SFTP subsystem session on an authenticated SSH handle.
pub async fn open_sftp(
    handle: &mut Handle<SshHandler>,
    session_id: &str,
    sink: &impl crate::ssh::progress::ProgressSink,
) -> Result<SftpSession> {
    sink.emit(crate::ssh::progress::event(
        session_id,
        crate::ssh::progress::Phase::Sftp,
        crate::ssh::progress::Status::Start,
        "opening SFTP subsystem…",
    ));
    let open = async {
        let channel = handle
            .channel_open_session()
            .await
            .context("open SFTP session channel")?;
        channel
            .request_subsystem(true, "sftp")
            .await
            .context("request sftp subsystem")?;
        SftpSession::new(channel.into_stream())
            .await
            .context("initialise SFTP session")
    };
    let session = tokio::time::timeout(SFTP_OPEN_TIMEOUT, open)
        .await
        .map_err(|_| {
            let msg = format!(
                "SFTP subsystem did not open within {}s",
                SFTP_OPEN_TIMEOUT.as_secs()
            );
            sink.emit(crate::ssh::progress::event(
                session_id,
                crate::ssh::progress::Phase::Sftp,
                crate::ssh::progress::Status::Fail,
                msg.clone(),
            ));
            anyhow::anyhow!(msg)
        })??;
    sink.emit(crate::ssh::progress::event(
        session_id,
        crate::ssh::progress::Phase::Sftp,
        crate::ssh::progress::Status::Ok,
        "SFTP ready",
    ));
    Ok(session)
}

// ── remote path helpers ─────────────────────────────────────────────

/// Split a posix remote path into its parent components, so we can mkdir -p.
///
/// Handles three SFTP path shapes:
/// - `~/.claude/skills`  → `["~", "~/.claude", "~/.claude/skills"]`
/// - `/home/u/skills`    → `["/home", "/home/u", "/home/u/skills"]`
/// - `relative/skills`   → `["relative", "relative/skills"]`
fn remote_parent_dirs(remote_path: &str) -> Vec<String> {
    let absolute = remote_path.starts_with('/');
    let mut dirs = Vec::new();
    let mut acc = String::new();
    for part in remote_path.split('/') {
        if part.is_empty() {
            continue;
        }
        if acc.is_empty() {
            acc = if absolute {
                format!("/{part}")
            } else {
                part.to_string()
            };
        } else {
            acc.push('/');
            acc.push_str(part);
        }
        dirs.push(acc.clone());
    }
    dirs
}

/// `mkdir -p` over SFTP — ignore "already exists" failures.
pub(crate) async fn ensure_remote_dir(sftp: &SftpSession, remote_path: &str) -> Result<()> {
    for dir in remote_parent_dirs(remote_path) {
        // SFTP returns Failure for existing dirs — treat as success either way.
        if let Ok(()) = sftp.create_dir(&dir).await {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_parent_dirs_splits_components() {
        // Tilde-prefixed: `~` is preserved so the SFTP server expands it.
        let dirs = remote_parent_dirs("~/.claude/skills");
        assert_eq!(dirs, vec!["~", "~/.claude", "~/.claude/skills"]);
    }

    #[test]
    fn remote_parent_dirs_absolute_path() {
        let dirs = remote_parent_dirs("/home/u/skills");
        assert_eq!(dirs, vec!["/home", "/home/u", "/home/u/skills"]);
    }
}
