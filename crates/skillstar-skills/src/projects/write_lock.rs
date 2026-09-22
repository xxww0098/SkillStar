//! One process-wide lock for `projects.json`, `skills-list.json`, and project
//! skill directories.
//!
//! The same thread may re-enter. A second open of the lock file is not used
//! for that, because `flock` blocks the same process on a new file description.
//! Other threads take the process mutex first. A poisoned mutex is an error.

use anyhow::{Context, Result};
use std::cell::Cell;
use std::fs::{File, OpenOptions};
use std::sync::{Mutex, MutexGuard, TryLockError};

use skillstar_core::infra::paths as fs_paths;

thread_local! {
    static DEPTH: Cell<u32> = const { Cell::new(0) };
}

static PROCESS_MUTEX: Mutex<()> = Mutex::new(());

#[cfg(test)]
thread_local! {
    static PROBE: Cell<Option<fn()>> = const { Cell::new(None) };
}

pub struct ProjectWriteGuard {
    _mutex: Option<MutexGuard<'static, ()>>,
    file: Option<File>,
}

impl Drop for ProjectWriteGuard {
    fn drop(&mut self) {
        let release_file = DEPTH.with(|depth| {
            let next = depth.get().saturating_sub(1);
            depth.set(next);
            next == 0
        });
        if release_file {
            if let Some(file) = self.file.as_ref() {
                let _ = file.unlock();
            }
        }
    }
}

pub fn with_project_write_lock<T>(body: impl FnOnce() -> Result<T>) -> Result<T> {
    let _guard = lock_project_write()?;
    body()
}

pub(super) fn lock_project_write() -> Result<ProjectWriteGuard> {
    let guard = if current_depth() > 0 {
        DEPTH.with(|depth| depth.set(depth.get() + 1));
        ProjectWriteGuard {
            _mutex: None,
            file: None,
        }
    } else {
        let mutex = PROCESS_MUTEX
            .lock()
            .map_err(|_| anyhow::anyhow!("project write lock poisoned"))?;
        let file = open_lock_file()?;
        file.lock()
            .with_context(|| "failed to lock state/project-write.lock")?;
        DEPTH.with(|depth| depth.set(1));
        ProjectWriteGuard {
            _mutex: Some(mutex),
            file: Some(file),
        }
    };
    #[cfg(test)]
    fire_probe();
    Ok(guard)
}

pub(super) fn try_lock_project_write() -> Result<ProjectWriteGuard> {
    if current_depth() > 0 {
        DEPTH.with(|depth| depth.set(depth.get() + 1));
        return Ok(ProjectWriteGuard {
            _mutex: None,
            file: None,
        });
    }
    let mutex = match PROCESS_MUTEX.try_lock() {
        Ok(guard) => guard,
        Err(TryLockError::WouldBlock) => anyhow::bail!("project write lock busy"),
        Err(TryLockError::Poisoned(_)) => anyhow::bail!("project write lock poisoned"),
    };
    let file = open_lock_file()?;
    if let Err(err) = file.try_lock() {
        anyhow::bail!("project write lock busy: {err}");
    }
    DEPTH.with(|depth| depth.set(1));
    Ok(ProjectWriteGuard {
        _mutex: Some(mutex),
        file: Some(file),
    })
}

fn current_depth() -> u32 {
    DEPTH.with(|depth| depth.get())
}

fn open_lock_file() -> Result<File> {
    let path = fs_paths::state_dir().join("project-write.lock");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&path)
        .with_context(|| format!("failed to open {}", path.display()))
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
        current_depth, lock_project_write, set_probe, try_lock_project_write,
        with_project_write_lock,
    };
    use crate::projects::save_and_sync;
    use crate::skill_update::acquire_update_transaction_lock;
    use std::collections::HashMap;
    use std::fs::{self, OpenOptions};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

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

        let path = skillstar_core::infra::paths::state_dir().join("project-write.lock");
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
        let _inner = with_project_write_lock(|| Ok(())).unwrap();
        assert!(current_depth() >= 1);
        let _again = try_lock_project_write().unwrap();
        assert!(current_depth() >= 2);
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
