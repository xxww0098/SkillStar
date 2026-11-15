//! Disposable Git object cache. The file lock covers checkout and its consumer.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ss_core::infra::{fs_ops, paths};

use super::{Checkout, git_ops};
use crate::git::transport::GitOperationSession;
use crate::source_resolver::Source;

const METADATA: &str = ".git/skillstar-import.json";

#[derive(Serialize, Deserialize)]
struct Metadata {
    revision: String,
    fetched_at: String,
}

/// Cache directory name for one `(repo_url, git_ref)` import. Callers use it
/// to tell a live checkout from a directory nothing installed still references.
pub fn import_cache_key(repo_url: &str, git_ref: Option<&str>) -> String {
    let pinned = git_ref.map(str::to_string);
    let source = serde_json::to_vec(&(repo_url, &pinned)).expect("url and ref are strings");
    hex_bytes(&Sha256::digest(source))
}

fn key(spec: &Source) -> String {
    import_cache_key(&spec.repo_url, spec.git_ref.as_deref())
}

fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from(HEX[usize::from(byte >> 4)]));
        out.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    out
}

fn open_lock(key: &str) -> Result<File> {
    let root = paths::skill_import_locks_dir();
    fs::create_dir_all(&root)?;
    Ok(OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join(key))?)
}

fn acquire(key: &str, session: &GitOperationSession) -> Result<File> {
    let file = open_lock(key)?;
    loop {
        anyhow::ensure!(!session.is_cancelled(), "The Git operation was cancelled");
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(25)),
            Err(TryLockError::Error(error)) => return Err(error.into()),
        }
    }
}

pub(super) fn fetch(
    spec: &Source,
    patterns: &[String],
    refresh: bool,
    expected_revision: Option<&str>,
    session: &GitOperationSession,
) -> Result<Checkout> {
    let key = key(spec);
    let guard = acquire(&key, session)?;
    let root = paths::skill_import_cache_dir();
    let dir = root.join(key);
    let cache_hit = dir.join(METADATA).is_file();
    anyhow::ensure!(
        cache_hit || expected_revision.is_none(),
        "Preview cache was cleared; scan the repository again"
    );
    let mut metadata: Metadata;
    if cache_hit {
        metadata = serde_json::from_slice(&fs::read(dir.join(METADATA))?)
            .context("Invalid import cache; clear the cache and scan again")?;
    } else {
        fs::create_dir_all(&root)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
        }
        let staging = tempfile::Builder::new()
            .prefix(".fetch-")
            .tempdir_in(&root)?;
        let repo = staging.path().join("repo");
        super::note_remote_git_lock_depth();
        git_ops::clone_repo_partial_in_session(
            &spec.repo_url,
            &repo,
            spec.git_ref.as_deref(),
            session,
        )?;
        metadata = Metadata {
            revision: git_ops::head_revision(&repo)?,
            fetched_at: chrono::Utc::now().to_rfc3339(),
        };
        // Publish the object store even if subsequent lazy materialization fails:
        // retries can reuse the successful clone. TempDir removes failed clones.
        save(&repo, &metadata)?;
        fs::rename(repo, &dir)
            .context("Failed to publish import cache; clear the cache and retry")?;
    }
    if refresh && cache_hit {
        super::note_remote_git_lock_depth();
        metadata.revision =
            git_ops::fetch_partial_revision_in_session(&dir, spec.git_ref.as_deref(), session)?;
        metadata.fetched_at = chrono::Utc::now().to_rfc3339();
    }
    let revision = expected_revision.unwrap_or(&metadata.revision).to_string();
    git_ops::apply_cached_sparse_revision_in_session(&dir, &revision, patterns, session)?;
    if refresh && cache_hit {
        save(&dir, &metadata)?;
    }
    Ok(Checkout {
        dir,
        _temp: None,
        _cache_lock: Some(guard),
        revision: Some(revision),
        cache_hit: cache_hit && !refresh,
        cached_at: Some(metadata.fetched_at),
    })
}

fn save(dir: &Path, metadata: &Metadata) -> Result<()> {
    fs_ops::atomic_write(&dir.join(METADATA), &serde_json::to_vec(metadata)?)?;
    Ok(())
}

/// Canonical skills never point here. Busy entries are left for the next cleanup.
pub fn clear_import_cache() -> Result<usize> {
    let root = paths::skill_import_cache_dir();
    if !root.exists() {
        return Ok(0);
    }
    let mut removed = 0;
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let key = entry.file_name().to_string_lossy().into_owned();
        if key.len() != 64
            || !key.bytes().all(|ch| ch.is_ascii_hexdigit())
            || !entry.file_type()?.is_dir()
        {
            continue;
        }
        let guard = open_lock(&key)?;
        match guard.try_lock() {
            Ok(()) => {
                if entry.path().exists() {
                    fs_ops::remove_dir_all_retry(&entry.path())?;
                    removed += 1;
                }
            }
            Err(TryLockError::WouldBlock) => {}
            Err(TryLockError::Error(error)) => return Err(error.into()),
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests;
