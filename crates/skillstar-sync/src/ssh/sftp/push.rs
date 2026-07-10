//! Push a locally-installed skill tree onto a remote agent directory.
//!
//! Pushing mirrors the local skill directory onto a remote agent folder:
//!
//! 1. Resolve the local skill's **real** content dir (following the hub
//!    symlink, same idea as `commands/skill_content.rs::resolve_skill_dir`).
//! 2. Walk it recursively (skipping `.git`), collecting relative paths — the
//!    same rule `list_skill_files` uses.
//! 3. For each file: `create_dir -p` its remote parent, write bytes to a
//!    `.skillstar.tmp` sibling, then `rename` over the target — atomic, so a
//!    crash mid-skill never leaves a half-written file.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use russh_sftp::client::SftpSession;
use russh_sftp::protocol::OpenFlags;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;

use super::common::ensure_remote_dir;

/// Aggregate result of a successful push.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushResult {
    /// Number of files written.
    pub files_uploaded: u32,
    /// Total bytes transferred.
    pub bytes: u64,
    /// Absolute remote path the skill now lives at.
    pub remote_path: String,
}

// ── local skill resolution (mirrors commands/skill_content resolve logic) ──

/// Resolve a hub skill name to its real on-disk content directory.
///
/// Hub skills are symlinks (`skills/<name>` → `local/<name>` or a repo
/// checkout). We follow the link, then accept the first of: the dir itself if
/// it has `SKILL.md`, a nested `<name>/SKILL.md`, otherwise the resolved dir.
fn resolve_local_skill_dir(skill_name: &str) -> Result<PathBuf> {
    let skills_dir = skillstar_core::infra::paths::hub_skills_dir();
    let skill_dir = skills_dir.join(skill_name);
    if !skill_dir.exists() {
        anyhow::bail!("skill '{}' is not installed in the hub", skill_name);
    }
    let effective = if skillstar_core::infra::fs_ops::is_link(&skill_dir) {
        skillstar_core::infra::fs_ops::read_link_resolved(&skill_dir)
            .unwrap_or_else(|_| skill_dir.clone())
    } else {
        skill_dir.clone()
    };

    if effective.join("SKILL.md").exists() {
        return Ok(effective);
    }
    // Nested layout (repo with subfolders): look one level deep.
    let nested = effective.join(skill_name);
    if nested.join("SKILL.md").exists() {
        return Ok(nested);
    }
    Ok(effective)
}

/// Recursively collect `(relative_path, absolute_path)` file pairs, skipping
/// `.git` (same rule as `list_skill_files`).
fn collect_local_files(root: &Path) -> Result<Vec<(String, PathBuf)>> {
    let mut out = Vec::new();
    walk(root, root, &mut out)?;
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) -> Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("read dir {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        if name == ".git" {
            continue;
        }
        if path.is_dir() {
            walk(root, &path, out)?;
        } else if let Ok(rel) = path.strip_prefix(root) {
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            out.push((rel_str, path));
        }
    }
    Ok(())
}

/// Join a remote base dir (possibly containing `~`) with a skill name.
/// SFTP servers expand `~` themselves, so we keep it as-is and just join.
fn remote_skill_dir(remote_base: &str, skill_name: &str) -> String {
    let trimmed = remote_base.trim_end_matches('/');
    if trimmed.is_empty() {
        skill_name.to_string()
    } else {
        format!("{trimmed}/{skill_name}")
    }
}

// ── public operations ───────────────────────────────────────────────

/// Upload a local hub skill tree to `remote_content_dir` (no agent link).
pub async fn upload_local_skill_tree(
    sftp: &SftpSession,
    skill_name: &str,
    remote_content_dir: &str,
) -> Result<(u32, u64)> {
    let local_dir = resolve_local_skill_dir(skill_name)?;
    let files = collect_local_files(&local_dir)?;
    ensure_remote_dir(sftp, remote_content_dir).await?;

    let mut files_uploaded = 0u32;
    let mut bytes = 0u64;

    for (rel, abs) in &files {
        let remote_file = format!("{}/{rel}", remote_content_dir.trim_end_matches('/'));
        if let Some((file_parent, _)) = remote_file.rsplit_once('/') {
            ensure_remote_dir(sftp, file_parent).await?;
        }

        let local_bytes = std::fs::read(abs).with_context(|| format!("read {}", abs.display()))?;
        bytes += local_bytes.len() as u64;

        let tmp_path = format!("{remote_file}.skillstar.tmp");
        let mut file = sftp
            .open_with_flags(&tmp_path, OpenFlags::CREATE | OpenFlags::TRUNCATE | OpenFlags::WRITE)
            .await
            .with_context(|| format!("open remote {tmp_path}"))?;
        file.write_all(&local_bytes)
            .await
            .with_context(|| format!("write remote {tmp_path}"))?;
        file.flush().await.ok();
        drop(file);

        let _ = sftp.remove_file(&remote_file).await;
        sftp.rename(&tmp_path, &remote_file)
            .await
            .with_context(|| format!("rename {tmp_path} -> {remote_file}"))?;

        files_uploaded += 1;
    }

    Ok((files_uploaded, bytes))
}

/// Push one locally-installed skill to `remote_base` on the connected host.
///
/// Files are written to a `.skillstar.tmp` sibling first and then renamed, so
/// an interrupted push never leaves partially-written files at the final path.
pub async fn push_skill(
    sftp: &SftpSession,
    skill_name: &str,
    remote_base: &str,
) -> Result<PushResult> {
    let remote_target = remote_skill_dir(remote_base, skill_name);
    let parent = remote_target
        .rsplit_once('/')
        .map(|(p, _)| p)
        .unwrap_or(".");
    ensure_remote_dir(sftp, parent).await?;
    ensure_remote_dir(sftp, &remote_target).await?;

    let (files_uploaded, bytes) = upload_local_skill_tree(sftp, skill_name, &remote_target).await?;

    tracing::info!(
        target: "ssh",
        skill = skill_name,
        remote = %remote_target,
        files = files_uploaded,
        bytes,
        "skill pushed to remote"
    );

    Ok(PushResult {
        files_uploaded,
        bytes,
        remote_path: remote_target,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_skill_dir_joins_without_double_slash() {
        assert_eq!(remote_skill_dir("~/.claude/skills", "foo"), "~/.claude/skills/foo");
        assert_eq!(remote_skill_dir("~/.claude/skills/", "foo"), "~/.claude/skills/foo");
        assert_eq!(remote_skill_dir("", "foo"), "foo");
    }

    #[test]
    fn collect_local_files_skips_git() {
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::write(root.join("SKILL.md"), "x").unwrap();
        std::fs::create_dir_all(root.join(".git/refs")).unwrap();
        std::fs::write(root.join(".git/refs/head"), "x").unwrap();
        std::fs::create_dir_all(root.join("scripts")).unwrap();
        std::fs::write(root.join("scripts/run.sh"), "x").unwrap();

        let files = collect_local_files(root).unwrap();
        let rels: Vec<_> = files.into_iter().map(|(r, _)| r).collect();
        assert!(rels.contains(&"SKILL.md".to_string()));
        assert!(rels.contains(&"scripts/run.sh".to_string()));
        assert!(!rels.iter().any(|r| r.contains(".git")));
    }

    #[test]
    fn resolve_local_skill_dir_errors_when_missing() {
        let _guard = test_data_dir();
        let err = resolve_local_skill_dir("definitely_not_installed").unwrap_err();
        assert!(err.to_string().contains("not installed"));
    }

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
}
