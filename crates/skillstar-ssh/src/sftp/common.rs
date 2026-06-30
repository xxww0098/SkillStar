//! Shared SFTP plumbing used by the push / list / delete operations.
//!
//! Holds the low-level pieces every operation needs: opening an SFTP session,
//! `mkdir -p` over SFTP, and atomic file read/write. The operation-specific
//! logic lives in the sibling [`super::push`], [`super::list`] and
//! [`super::delete`] modules.

use anyhow::{Context, Result};
use russh::client::Handle;
use russh_sftp::client::SftpSession;
use russh_sftp::protocol::OpenFlags;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::client::SshHandler;

/// Open an SFTP subsystem session on an authenticated SSH handle.
pub async fn open_sftp(
    handle: &mut Handle<SshHandler>,
    session_id: &str,
    sink: &impl crate::progress::ProgressSink,
) -> Result<SftpSession> {
    sink.emit(crate::progress::event(
        session_id,
        crate::progress::Phase::Sftp,
        crate::progress::Status::Start,
        "opening SFTP subsystem…",
    ));
    let channel = handle
        .channel_open_session()
        .await
        .context("open SFTP session channel")?;
    channel
        .request_subsystem(true, "sftp")
        .await
        .context("request sftp subsystem")?;
    let session = SftpSession::new(channel.into_stream())
        .await
        .context("initialise SFTP session")?;
    sink.emit(crate::progress::event(
        session_id,
        crate::progress::Phase::Sftp,
        crate::progress::Status::Ok,
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
            acc = if absolute { format!("/{part}") } else { part.to_string() };
        } else {
            acc.push('/');
            acc.push_str(part);
        }
        dirs.push(acc.clone());
    }
    dirs
}

/// `mkdir -p` over SFTP — ignore "already exists" failures.
pub async fn ensure_remote_dir_pub(sftp: &SftpSession, remote_path: &str) -> Result<()> {
    ensure_remote_dir(sftp, remote_path).await
}

pub(crate) async fn ensure_remote_dir(sftp: &SftpSession, remote_path: &str) -> Result<()> {
    for dir in remote_parent_dirs(remote_path) {
        match sftp.create_dir(&dir).await {
            Ok(()) => {}
            // SFTP returns Failure for existing dirs — treat as success.
            Err(_) => {}
        }
    }
    Ok(())
}

// ── file IO ──────────────────────────────────────────────────────────

/// Read an entire remote file into memory using SFTP.
///
/// Opens the file for reading and drains it via `AsyncReadExt::read_to_end`.
/// Returns the raw bytes. The caller is responsible for interpreting the
/// content (e.g. UTF-8 text for SKILL.md).
pub async fn read_remote_file(sftp: &SftpSession, remote_path: &str) -> Result<Vec<u8>> {
    let mut file = sftp
        .open(remote_path)
        .await
        .with_context(|| format!("open remote {} for read", remote_path))?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)
        .await
        .with_context(|| format!("read remote {}", remote_path))?;
    Ok(buf)
}

/// Write bytes to a remote file using an atomic temp→rename pattern.
///
/// The file is first written to `{remote_path}.skillstar.tmp`, then renamed
/// over the target. This mirrors the pattern used by `upload_local_skill_tree`
/// so that interrupted writes never leave a partially-written final file.
pub async fn write_remote_file(sftp: &SftpSession, remote_path: &str, bytes: &[u8]) -> Result<()> {
    let parent = remote_path
        .rsplit_once('/')
        .map(|(p, _)| p)
        .unwrap_or(".");
    ensure_remote_dir(sftp, parent).await?;

    let tmp_path = format!("{}.skillstar.tmp", remote_path);
    let mut file = sftp
        .open_with_flags(&tmp_path, OpenFlags::CREATE | OpenFlags::TRUNCATE | OpenFlags::WRITE)
        .await
        .with_context(|| format!("open remote {} for write", tmp_path))?;
    file.write_all(bytes)
        .await
        .with_context(|| format!("write remote {}", tmp_path))?;
    file.flush().await.ok();
    drop(file);

    let _ = sftp.remove_file(remote_path).await;
    sftp.rename(&tmp_path, remote_path)
        .await
        .with_context(|| format!("rename {} -> {}", tmp_path, remote_path))?;

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
