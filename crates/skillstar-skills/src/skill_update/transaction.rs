use anyhow::{Context, Result};
use std::sync::Mutex;

static UPDATE_TRANSACTION_MUTEX: Mutex<()> = Mutex::new(());

pub struct UpdateTransactionGuard {
    _process_guard: std::sync::MutexGuard<'static, ()>,
    file: std::fs::File,
}

impl Drop for UpdateTransactionGuard {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

pub fn acquire_update_transaction_lock() -> Result<UpdateTransactionGuard> {
    let process_guard = UPDATE_TRANSACTION_MUTEX
        .lock()
        .map_err(|_| anyhow::anyhow!("Skill update transaction mutex poisoned"))?;
    let lock_path = skillstar_core::infra::paths::state_dir().join("skill-update.lock");
    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .with_context(|| format!("Failed to open update lock '{}'", lock_path.display()))?;
    file.lock().with_context(|| {
        format!(
            "Failed to lock update transaction '{}'",
            lock_path.display()
        )
    })?;
    Ok(UpdateTransactionGuard {
        _process_guard: process_guard,
        file,
    })
}

/// Held while one repository cache entry is being fetched, reset or scanned.
///
/// Install and scan phases that hit the network hold only this lock, so a
/// slow repository never serializes installs from other repositories. Every
/// open of the lock file is a distinct open-file description, so threads in
/// this process contend on it exactly like other processes do.
///
/// Lock order: always release this guard before acquiring
/// [`acquire_update_transaction_lock`] — the global transaction lock may
/// internally wait on repository work, never the reverse.
pub struct RepoCacheGuard {
    _file: std::fs::File,
}

impl Drop for RepoCacheGuard {
    fn drop(&mut self) {
        let _ = self._file.unlock();
    }
}

pub fn acquire_repo_cache_lock(cache_key: &str) -> Result<RepoCacheGuard> {
    let dir = skillstar_core::infra::paths::state_dir().join("repo-locks");
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create the repo lock dir '{}'", dir.display()))?;
    // `cache_key` already names a directory under `hub/repos`, so it is a
    // filesystem-safe path segment by construction.
    let lock_path = dir.join(format!("{cache_key}.lock"));
    let file = std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .with_context(|| format!("Failed to open repo cache lock '{}'", lock_path.display()))?;
    file.lock().with_context(|| {
        format!(
            "Failed to lock repository cache '{}'",
            lock_path.display()
        )
    })?;
    Ok(RepoCacheGuard { _file: file })
}

/// Cross-process lock around a lockfile read-modify-write that runs while
/// only the repo cache lock is held (checkout resets refresh content
/// baselines mid-scan, outside the global transaction lock).
pub struct LockfileWriteGuard {
    _file: std::fs::File,
}

impl Drop for LockfileWriteGuard {
    fn drop(&mut self) {
        let _ = self._file.unlock();
    }
}

pub fn acquire_lockfile_write_lock() -> Result<LockfileWriteGuard> {
    let lock_path = skillstar_core::infra::paths::state_dir().join("lockfile.lock");
    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .with_context(|| format!("Failed to open lockfile write lock '{}'", lock_path.display()))?;
    file.lock()
        .with_context(|| format!("Failed to lock lockfile writes '{}'", lock_path.display()))?;
    Ok(LockfileWriteGuard { _file: file })
}
