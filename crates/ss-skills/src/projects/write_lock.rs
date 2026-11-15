//! One process-wide lock for `projects.json`, `skills-list.json`, and project
//! skill directories.
//!
//! The owner thread may re-enter. The process mutex and file lock live in its
//! thread slot until the last guard drops, even when guards drop out of order.
//! Other callers wait at most five seconds; a poisoned unit mutex is recovered.

use anyhow::Result;
#[cfg(test)]
use std::cell::Cell;
use std::sync::Mutex;
use std::time::Duration;

use ss_core::infra::paths as fs_paths;

use crate::skill_update::transaction::{LockSlot, ReentrantFileGuard, acquire_reentrant_file_lock};

thread_local! {
    static PROJECT_WRITE: LockSlot = const { LockSlot::new() };
}

static PROCESS_MUTEX: Mutex<()> = Mutex::new(());
const PROJECT_WRITE_LOCK_WAIT: Duration = Duration::from_secs(5);

#[cfg(test)]
thread_local! {
    static PROBE: Cell<Option<fn()>> = const { Cell::new(None) };
}

pub struct ProjectWriteGuard {
    _inner: ReentrantFileGuard,
}

pub fn with_project_write_lock<T>(body: impl FnOnce() -> Result<T>) -> Result<T> {
    let _guard = lock_project_write()?;
    body()
}

pub(super) fn lock_project_write() -> Result<ProjectWriteGuard> {
    let guard = acquire_project_write(PROJECT_WRITE_LOCK_WAIT)?;
    #[cfg(test)]
    fire_probe();
    Ok(guard)
}

fn acquire_project_write(wait: Duration) -> Result<ProjectWriteGuard> {
    let inner = acquire_reentrant_file_lock(
        &PROCESS_MUTEX,
        &PROJECT_WRITE,
        &fs_paths::state_dir().join("project-write.lock"),
        "project write lock",
        Some(wait),
    )?;
    debug_assert!(current_project_write_held());
    Ok(ProjectWriteGuard { _inner: inner })
}

#[cfg(test)]
pub(super) fn try_lock_project_write() -> Result<ProjectWriteGuard> {
    acquire_project_write(Duration::ZERO)
}

pub(super) fn current_project_write_held() -> bool {
    PROJECT_WRITE.with(|slot| slot.depth() > 0)
}

#[cfg(test)]
fn current_depth() -> u32 {
    PROJECT_WRITE.with(LockSlot::depth)
}

#[cfg(test)]
fn fire_probe() {
    let probe = PROBE.with(|cell| cell.get());
    if let Some(probe) = probe {
        probe();
    }
}

#[cfg(test)]
fn set_probe(probe: Option<fn()>) {
    PROBE.with(|cell| cell.set(probe));
}

#[cfg(test)]
mod project_write_lock_tests {
    use super::{
        PROCESS_MUTEX, acquire_project_write, current_depth, lock_project_write, set_probe,
        try_lock_project_write, with_project_write_lock,
    };
    use crate::projects::save_and_sync;
    use crate::skill_update::acquire_update_transaction_lock;
    use std::collections::HashMap;
    use std::fs::{self, OpenOptions};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    struct EnvGuard {
        root: PathBuf,
        home: Option<std::ffi::OsString>,
        data: Option<std::ffi::OsString>,
        #[cfg(windows)]
        userprofile: Option<std::ffi::OsString>,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl EnvGuard {
        fn new(label: &str) -> Self {
            let _lock = crate::lock_test_env();
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0);
            let root = std::env::temp_dir().join(format!("skillstar-lock-{label}-{nanos}"));
            fs::create_dir_all(root.join("home")).unwrap();
            fs::create_dir_all(root.join("data")).unwrap();
            let guard = Self {
                home: std::env::var_os("HOME"),
                data: std::env::var_os("SKILLSTAR_DATA_DIR"),
                #[cfg(windows)]
                userprofile: std::env::var_os("USERPROFILE"),
                root,
                _lock,
            };
            unsafe {
                std::env::set_var("HOME", guard.root.join("home"));
                std::env::set_var("SKILLSTAR_DATA_DIR", guard.root.join("data"));
                #[cfg(windows)]
                std::env::set_var("USERPROFILE", guard.root.join("home"));
            }
            guard
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            set_probe(None);
            unsafe {
                restore("HOME", self.home.take());
                restore("SKILLSTAR_DATA_DIR", self.data.take());
                #[cfg(windows)]
                restore("USERPROFILE", self.userprofile.take());
            }
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    unsafe fn restore(key: &str, previous: Option<std::ffi::OsString>) {
        match previous {
            Some(value) => unsafe { std::env::set_var(key, value) },
            None => unsafe { std::env::remove_var(key) },
        }
    }

    #[test]
    fn project_write_lock_excludes_a_second_thread() {
        let _env = EnvGuard::new("exclude");
        let _held = lock_project_write().unwrap();
        let blocked = std::thread::spawn(|| try_lock_project_write().is_err())
            .join()
            .unwrap();
        assert!(blocked);

        let path = ss_core::infra::paths::state_dir().join("project-write.lock");
        let second = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        assert!(
            second.try_lock().is_err(),
            "a second file description must not share the held lock"
        );
    }

    #[test]
    fn project_write_lock_reenters_on_the_owner_thread() {
        let _env = EnvGuard::new("reenter");
        let _outer = lock_project_write().unwrap();
        with_project_write_lock(|| Ok(())).unwrap();
        assert!(current_depth() >= 1);
        let _again = try_lock_project_write().unwrap();
        assert!(current_depth() >= 2);
    }

    #[test]
    fn project_write_lock_times_out_waiting_for_another_thread() {
        let _env = EnvGuard::new("thread-timeout");
        let _held = lock_project_write().unwrap();
        let wait = Duration::from_millis(80);
        let (error, elapsed) = std::thread::spawn(move || {
            let start = Instant::now();
            let error = acquire_project_write(wait)
                .err()
                .expect("another thread holds the project lock");
            (error, start.elapsed())
        })
        .join()
        .unwrap();
        assert!(
            error
                .downcast_ref::<crate::skill_update::OperationInProgress>()
                .is_some()
        );
        assert!(elapsed >= wait && elapsed < Duration::from_secs(2));
        assert_eq!(current_depth(), 1);
    }

    #[test]
    fn project_write_lock_times_out_waiting_for_another_file_description() {
        let _env = EnvGuard::new("file-timeout");
        let path = ss_core::infra::paths::state_dir().join("project-write.lock");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let held = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(path)
            .unwrap();
        held.lock().unwrap();
        let wait = Duration::from_millis(80);
        let start = Instant::now();
        let error = acquire_project_write(wait)
            .err()
            .expect("a separate file description holds the lock");
        let elapsed = start.elapsed();
        held.unlock().unwrap();
        assert!(
            error
                .downcast_ref::<crate::skill_update::OperationInProgress>()
                .is_some()
        );
        assert!(elapsed >= wait && elapsed < Duration::from_secs(2));
        assert_eq!(current_depth(), 0);
        let _recovered = try_lock_project_write().unwrap();
        assert_eq!(current_depth(), 1);
    }

    #[test]
    fn project_write_lock_keeps_resources_until_the_last_out_of_order_drop() {
        let _env = EnvGuard::new("out-of-order");
        let outer = lock_project_write().unwrap();
        let inner = lock_project_write().unwrap();
        let last = try_lock_project_write().unwrap();
        assert_eq!(current_depth(), 3);
        drop(outer);
        assert_eq!(current_depth(), 2);
        assert!(
            std::thread::spawn(|| try_lock_project_write().is_err())
                .join()
                .unwrap()
        );
        let path = ss_core::infra::paths::state_dir().join("project-write.lock");
        let second = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .unwrap();
        assert!(second.try_lock().is_err(), "the file lock must remain held");
        drop(last);
        assert_eq!(current_depth(), 1);
        assert!(
            std::thread::spawn(|| try_lock_project_write().is_err())
                .join()
                .unwrap()
        );
        assert!(second.try_lock().is_err());
        drop(inner);
        assert_eq!(current_depth(), 0);
        assert!(
            std::thread::spawn(|| try_lock_project_write().is_ok())
                .join()
                .unwrap()
        );
        second.try_lock().unwrap();
        second.unlock().unwrap();
    }

    #[test]
    fn project_write_lock_recovers_after_a_panic() {
        let _env = EnvGuard::new("panic");
        let panic = std::panic::catch_unwind(|| {
            let _: anyhow::Result<()> = with_project_write_lock(|| {
                let _inner = lock_project_write().unwrap();
                assert_eq!(current_depth(), 2);
                panic!("panic while holding reentrant project guards");
            });
        });
        assert!(panic.is_err());
        assert!(PROCESS_MUTEX.is_poisoned());
        assert_eq!(current_depth(), 0);
        let outer = lock_project_write().unwrap();
        let inner = try_lock_project_write().unwrap();
        assert_eq!(current_depth(), 2);
        drop(outer);
        drop(inner);
        assert_eq!(current_depth(), 0);
        assert!(
            std::thread::spawn(|| try_lock_project_write().is_ok())
                .join()
                .unwrap()
        );
    }

    #[test]
    fn save_and_sync_holds_the_lock_across_full_sync() {
        let env = EnvGuard::new("sync");
        let project = env.root.join("project");
        fs::create_dir_all(&project).unwrap();
        static MAX_DEPTH: AtomicU32 = AtomicU32::new(0);
        static BLOCKED: AtomicBool = AtomicBool::new(false);
        MAX_DEPTH.store(0, Ordering::Relaxed);
        BLOCKED.store(false, Ordering::Relaxed);
        set_probe(Some(|| {
            let depth = current_depth();
            MAX_DEPTH.fetch_max(depth, Ordering::Relaxed);
            if depth >= 2 {
                let blocked = std::thread::spawn(|| try_lock_project_write().is_err())
                    .join()
                    .unwrap();
                BLOCKED.store(blocked, Ordering::Relaxed);
            }
        }));

        save_and_sync(project.to_str().unwrap(), HashMap::new(), HashMap::new()).unwrap();
        set_probe(None);
        assert!(
            MAX_DEPTH.load(Ordering::Relaxed) >= 2,
            "full_sync must run while save_and_sync still holds the lock"
        );
        assert!(BLOCKED.load(Ordering::Relaxed));
    }

    #[test]
    fn import_can_take_project_lock_while_holding_update_lock() {
        let env = EnvGuard::new("import");
        let project = env.root.join("project");
        fs::create_dir_all(&project).unwrap();
        let _update = acquire_update_transaction_lock().unwrap();
        with_project_write_lock(|| Ok(())).unwrap();
        drop(_update);

        crate::projects::import_scanned_skills(project.to_str().unwrap(), "demo", &[]).unwrap();
    }
}
