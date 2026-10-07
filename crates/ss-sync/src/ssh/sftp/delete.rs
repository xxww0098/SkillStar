//! Delete a remote skill directory over SFTP.
//!
//! Recursive and idempotent, but guarded: a typo'd `remote_path` must never
//! wipe a user's home, the filesystem root, a whole `skills/` dir, or the
//! skillstar hub content root — those targets are rejected before any SFTP
//! call (see [`validate_delete_target`]).

use anyhow::Result;
use russh_sftp::client::SftpSession;

/// Paths too dangerous to recursively delete — a typo'd `remote_path` must
/// never wipe a user's home, the filesystem root, or the whole skillstar hub.
/// Bare `~` and `/`/`//` are rejected; the hub content *root* is rejected
/// (individual skills under it are fine).
fn is_destructive_path(p: &str) -> bool {
    let trimmed = p.trim_matches('/');
    trimmed.is_empty() // "/" or "//" or "" → root
        || p.trim() == "~"
        || p.trim_end_matches('/').ends_with(".skillstar/hub/content")
        || p.trim_end_matches('/').ends_with(".skillstar/hub/content/")
        || p.trim_end_matches('/').ends_with("/skills")
}

/// Reject obviously-dangerous delete targets before touching SFTP.
///
/// Deleting `~`, `/`, `.../skills` (a whole agent's skill dir, by accident),
/// or the hub content root would be catastrophic and is never intentional from
/// the UI. Individual skill paths (`.../skills/<name>`) are allowed.
fn validate_delete_target(remote_path: &str) -> Result<()> {
    if is_destructive_path(remote_path) {
        anyhow::bail!(
            "refusing to delete destructive path '{}': refusing root, home, a skills dir, or the hub content root",
            remote_path
        );
    }
    Ok(())
}

/// Delete a remote skill directory (recursive). Removes files first, then
/// the (now empty) directory.
pub async fn delete_remote_skill(sftp: &SftpSession, remote_path: &str) -> Result<()> {
    validate_delete_target(remote_path)?;
    // Recursively remove children.
    remove_remote_tree(sftp, remote_path).await?;
    match sftp.remove_dir(remote_path).await {
        Ok(()) => Ok(()),
        Err(_) => Ok(()), // already gone — idempotent
    }
}

async fn remove_remote_tree(sftp: &SftpSession, remote_path: &str) -> Result<()> {
    let entries = match sftp.read_dir(remote_path).await {
        Ok(rd) => rd,
        Err(_) => return Ok(()),
    };
    for entry in entries {
        let name = entry.file_name();
        let attrs = entry.metadata();
        let child = format!("{}/{name}", remote_path.trim_end_matches('/'));
        if attrs.is_dir() {
            Box::pin(remove_remote_tree(sftp, &child)).await?;
            let _ = sftp.remove_dir(&child).await;
        } else {
            let _ = sftp.remove_file(&child).await;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_destructive_path_rejects_roots_and_dirs() {
        assert!(is_destructive_path("/"));
        assert!(is_destructive_path("//"));
        assert!(is_destructive_path(""));
        assert!(is_destructive_path("~"));
        assert!(is_destructive_path("~/.claude/skills"));
        assert!(is_destructive_path("~/.skillstar/hub/content"));
        assert!(is_destructive_path("/root/.skillstar/hub/content/"));
        assert!(!is_destructive_path("~/.claude/skills/my-skill"));
        assert!(!is_destructive_path("/root/.codex/skills/foo"));
    }

    #[tokio::test]
    async fn delete_remote_skill_refuses_destructive_paths() {
        // We can't easily exercise the SFTP path without a live server, but the
        // guard runs before any SFTP call, so a refused path errors immediately
        // without needing an SftpSession.
        for bad in ["/", "~", "~/.codex/skills", "~/.skillstar/hub/content"] {
            let err = validate_delete_target(bad).unwrap_err();
            assert!(
                err.to_string().contains("destructive"),
                "expected destructive-path refusal for {bad}, got: {err}"
            );
        }
        assert!(validate_delete_target("~/.codex/skills/my-skill").is_ok());
    }
}
