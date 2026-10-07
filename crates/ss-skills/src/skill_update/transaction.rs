use anyhow::{Context, Result};
use std::cell::{Cell, RefCell};
use std::fs::File;
use std::marker::PhantomData;
use std::path::Path;
use std::sync::{Mutex, MutexGuard, TryLockError};
use std::thread::LocalKey;
use std::time::{Duration, Instant};

/// What one thread holds while it owns a re-entrant lock: the process mutex
/// and the OS file lock. Lives in the thread's [`LockSlot`], not in any guard,
/// so guards may drop in any order and the lock is released exactly when the
/// last of them goes.
struct Held {
    file: File,
    _process: MutexGuard<'static, ()>,
}

/// Per-thread state of one re-entrant lock.
pub(crate) struct LockSlot {
    depth: Cell<u32>,
    held: RefCell<Option<Held>>,
}

impl LockSlot {
    pub(crate) const fn new() -> Self {
        Self {
            depth: Cell::new(0),
            held: RefCell::new(None),
        }
    }
}

/// Process mutex + OS file lock that the owning thread may re-enter.
///
/// A second open of the lock file is never used for re-entry: `flock` blocks
/// the same process on a new file description. Other threads queue on the
/// process mutex first, other processes on the file lock.
pub(crate) struct ReentrantFileGuard {
    slot: &'static LocalKey<LockSlot>,
    _not_send: PhantomData<*const ()>,
}

impl Drop for ReentrantFileGuard {
    fn drop(&mut self) {
        let released = self.slot.with(|slot| {
            let next = slot.depth.get().saturating_sub(1);
            slot.depth.set(next);
            if next == 0 {
                slot.held.borrow_mut().take()
            } else {
                None
            }
        });
        if let Some(held) = released {
            let _ = held.file.unlock();
        }
    }
}

/// Another SkillStar thread or process holds the lock and did not release it
/// within the caller's wait budget.
#[derive(Debug)]
pub struct OperationInProgress {
    label: &'static str,
}

impl std::fmt::Display for OperationInProgress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Another SkillStar operation is changing Skills right now ({}); try again when it finishes",
            self.label
        )
    }
}

impl std::error::Error for OperationInProgress {}

const POLL_INTERVAL: Duration = Duration::from_millis(25);

pub(crate) fn acquire_reentrant_file_lock(
    mutex: &'static Mutex<()>,
    slot: &'static LocalKey<LockSlot>,
    lock_path: &Path,
    label: &'static str,
    wait: Option<Duration>,
) -> Result<ReentrantFileGuard> {
    let guard = ReentrantFileGuard {
        slot,
        _not_send: PhantomData,
    };
    if slot.with(|slot| slot.depth.get()) > 0 {
        slot.with(|slot| slot.depth.set(slot.depth.get() + 1));
        return Ok(guard);
    }
    let deadline = wait.map(|wait| Instant::now() + wait);
    let busy = || anyhow::Error::new(OperationInProgress { label });
    // The guarded state lives on disk; a panic elsewhere leaves nothing torn
    // in this unit mutex.
    let process = match deadline {
        None => mutex
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()),
        Some(deadline) => loop {
            match mutex.try_lock() {
                Ok(process) => break process,
                Err(TryLockError::Poisoned(poisoned)) => break poisoned.into_inner(),
                Err(TryLockError::WouldBlock) if Instant::now() >= deadline => return Err(busy()),
                Err(TryLockError::WouldBlock) => std::thread::sleep(POLL_INTERVAL),
            }
        },
    };
    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(lock_path)
        .with_context(|| format!("Failed to open {label} '{}'", lock_path.display()))?;
    match deadline {
        None => file
            .lock()
            .with_context(|| format!("Failed to lock {label} '{}'", lock_path.display()))?,
        Some(deadline) => loop {
            match file.try_lock() {
                Ok(()) => break,
                Err(std::fs::TryLockError::WouldBlock) if Instant::now() >= deadline => {
                    return Err(busy());
                }
                Err(std::fs::TryLockError::WouldBlock) => std::thread::sleep(POLL_INTERVAL),
                Err(std::fs::TryLockError::Error(error)) => {
                    return Err(error).with_context(|| {
                        format!("Failed to lock {label} '{}'", lock_path.display())
                    });
                }
            }
        },
    }
    slot.with(|slot| {
        *slot.held.borrow_mut() = Some(Held {
            file,
            _process: process,
        });
        slot.depth.set(1);
    });
    Ok(guard)
}

static UPDATE_TRANSACTION_MUTEX: Mutex<()> = Mutex::new(());

thread_local! {
    static UPDATE_TRANSACTION: LockSlot = const { LockSlot::new() };
}

/// Cross-process Skill mutation transaction. Every write to the canonical
/// skills root, the install lock or Agent deployments runs under it; the
/// owning thread may re-enter, so composed use cases (channel installs that
/// call the generic installer) do not deadlock.
pub struct UpdateTransactionGuard {
    _inner: ReentrantFileGuard,
}

fn acquire(wait: Option<Duration>) -> Result<UpdateTransactionGuard> {
    let outermost = UPDATE_TRANSACTION.with(|slot| slot.depth.get()) == 0;
    let inner = acquire_reentrant_file_lock(
        &UPDATE_TRANSACTION_MUTEX,
        &UPDATE_TRANSACTION,
        &ss_core::infra::paths::skill_update_lock_path(),
        "skill update transaction lock",
        wait,
    )?;
    if outermost {
        crate::materialize::sweep_stale_transients_now_and_then();
    }
    Ok(UpdateTransactionGuard { _inner: inner })
}

/// Block until the transaction is free. Background and CLI paths use this.
pub fn acquire_update_transaction_lock() -> Result<UpdateTransactionGuard> {
    acquire(None)
}

/// Wait at most `wait` for the transaction, then fail with
/// [`OperationInProgress`] (downcast the error to tell it apart). For
/// interactive callers that must not freeze behind a long install.
pub fn try_acquire_update_transaction_lock(wait: Duration) -> Result<UpdateTransactionGuard> {
    acquire(Some(wait))
}

#[cfg(test)]
pub(crate) fn transaction_depth_for_test() -> u32 {
    UPDATE_TRANSACTION.with(|slot| slot.depth.get())
}

/// Restore or delete what crashed Skill transactions left behind (hidden
/// `.skillstar-stage-*` / `-backup-*` / `-remove-*` / `-retain-*` entries)
/// in the canonical root and every Agent's Global skills directory.
///
/// Holds the transaction, so nothing it touches can belong to a live write.
/// Transaction entry points already run it periodically; repair tools call it
/// directly.
pub fn sweep_stale_transients() -> Result<crate::materialize::TransientSweep> {
    let _transaction = acquire_update_transaction_lock()?;
    Ok(crate::materialize::sweep_stale_transients(
        crate::materialize::STALE_TRANSIENT_AGE,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_thread_reenters_and_other_threads_wait() {
        let _env = crate::test_sandbox::Sandbox::new();
        let outer = acquire_update_transaction_lock().unwrap();
        let inner = acquire_update_transaction_lock().unwrap();
        drop(inner);
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            let _guard = acquire_update_transaction_lock().unwrap();
            tx.send(()).unwrap();
        });
        assert!(
            rx.recv_timeout(Duration::from_millis(150)).is_err(),
            "another thread must wait while the owner still holds the lock"
        );
        drop(outer);
        rx.recv_timeout(Duration::from_secs(5)).unwrap();
        handle.join().unwrap();
    }

    #[test]
    fn dropping_the_outer_guard_first_keeps_the_lock_until_the_last_guard() {
        let _env = crate::test_sandbox::Sandbox::new();
        let outer = acquire_update_transaction_lock().unwrap();
        let inner = acquire_update_transaction_lock().unwrap();
        drop(outer);
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            let _guard = acquire_update_transaction_lock().unwrap();
            tx.send(()).unwrap();
        });
        assert!(
            rx.recv_timeout(Duration::from_millis(150)).is_err(),
            "the inner guard still holds the transaction"
        );
        drop(inner);
        rx.recv_timeout(Duration::from_secs(5)).unwrap();
        handle.join().unwrap();
    }

    #[test]
    fn try_acquire_reports_a_busy_transaction_and_reenters_for_the_owner() {
        let _env = crate::test_sandbox::Sandbox::new();
        let (locked_tx, locked_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let holder = std::thread::spawn(move || {
            let _guard = acquire_update_transaction_lock().unwrap();
            locked_tx.send(()).unwrap();
            let _ = release_rx.recv();
        });
        locked_rx.recv().unwrap();
        let Err(error) = try_acquire_update_transaction_lock(Duration::from_millis(80)) else {
            panic!("the transaction is held by another thread");
        };
        assert!(error.downcast_ref::<OperationInProgress>().is_some());
        assert!(error.to_string().contains("Another SkillStar operation"));
        release_tx.send(()).unwrap();
        holder.join().unwrap();

        let outer = try_acquire_update_transaction_lock(Duration::from_secs(5)).unwrap();
        let inner = try_acquire_update_transaction_lock(Duration::ZERO).unwrap();
        drop(inner);
        drop(outer);
    }
}
