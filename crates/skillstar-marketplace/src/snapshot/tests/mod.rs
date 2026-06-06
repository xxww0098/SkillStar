use crate::snapshot::{InstalledSkillsFuture, SnapshotRuntimeConfig, configure_runtime};
use rusqlite::Connection;
use std::collections::HashSet;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

mod part1;
mod part2;
mod part3;

fn test_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn with_temp_data_root<F: FnOnce(&Path)>(f: F) {
    let _guard = test_env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let temp = tempfile::tempdir().expect("create temp dir");
    let temp_root = temp.path().to_path_buf();
    configure_runtime(SnapshotRuntimeConfig::new(
        temp_root.join("marketplace.db"),
        temp_root.clone(),
        HashSet::new,
        || -> InstalledSkillsFuture { Box::pin(async { Ok(Vec::new()) }) },
    ));
    f(&temp_root);
}

fn open_raw_conn(path: &Path) -> Connection {
    Connection::open(path).expect("open raw sqlite")
}
