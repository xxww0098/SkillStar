//! Shared helpers for GPUI shell tests.
//!
//! `IsolatedDataDir` keeps config, cache, and account storage off the real
//! data root while a test runs. The static lock serializes suites that flip
//! `SKILLSTAR_DATA_DIR`, because cargo runs test binaries multi-threaded and
//! parallel suites would race another suite's reads and its restore.

pub(crate) fn data_dir_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(crate) struct IsolatedDataDir {
    _lock: std::sync::MutexGuard<'static, ()>,
    previous: Option<std::ffi::OsString>,
    root: std::path::PathBuf,
}

impl IsolatedDataDir {
    pub(crate) fn new() -> Self {
        let lock = data_dir_lock();
        let root = std::env::temp_dir().join(format!(
            "skillstar-gpui-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let previous = std::env::var_os("SKILLSTAR_DATA_DIR");
        unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", &root) };
        Self {
            _lock: lock,
            previous,
            root,
        }
    }
}

impl Drop for IsolatedDataDir {
    fn drop(&mut self) {
        unsafe {
            match self.previous.take() {
                Some(value) => std::env::set_var("SKILLSTAR_DATA_DIR", value),
                None => std::env::remove_var("SKILLSTAR_DATA_DIR"),
            }
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
