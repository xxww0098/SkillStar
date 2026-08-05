use anyhow::{Context, Result};
use std::sync::Mutex;

static UPDATE_TRANSACTION_MUTEX: Mutex<()> = Mutex::new(());

pub(crate) struct UpdateTransactionGuard {
    _process_guard: std::sync::MutexGuard<'static, ()>,
    file: std::fs::File,
}

impl Drop for UpdateTransactionGuard {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

pub(crate) fn acquire_update_transaction_lock() -> Result<UpdateTransactionGuard> {
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
