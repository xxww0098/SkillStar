#[cfg(not(test))]
use anyhow::{Context, Result};
#[cfg(not(test))]
use r2d2::Pool;
#[cfg(not(test))]
use r2d2_sqlite::SqliteConnectionManager;
#[cfg(not(test))]
use rusqlite::Connection;
#[cfg(not(test))]
use std::path::Path;
#[cfg(not(test))]
use std::sync::LazyLock;

/// Pool type alias used throughout the codebase.
#[cfg(not(test))]
pub type DbPool = Pool<SqliteConnectionManager>;

/// Create a connection pool for a SQLite database file.
///
/// Every connection is configured with WAL journal mode, a 5-second busy
/// timeout, and foreign key enforcement.
#[cfg(not(test))]
pub fn create_pool(db_path: &Path, max_size: u32) -> Result<DbPool> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).context("Failed to create DB directory")?;
    }

    let manager = SqliteConnectionManager::file(db_path).with_init(|conn| {
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA busy_timeout=5000;
             PRAGMA foreign_keys=ON;",
        )
    });

    Pool::builder()
        .max_size(max_size)
        .build(manager)
        .context("Failed to build r2d2 connection pool")
}

/// Run a blocking closure on a pooled connection via `spawn_blocking`.
///
/// Use this from async contexts to avoid blocking the tokio runtime.
/// For sync contexts (within other `spawn_blocking` calls), use
/// `pool.get()` directly.
#[cfg(not(test))]
#[allow(dead_code)]
pub async fn run_blocking<F, T>(pool: &'static DbPool, f: F) -> Result<T>
where
    F: FnOnce(&Connection) -> Result<T> + Send + 'static,
    T: Send + 'static,
{
    let pool = pool.clone();
    tokio::task::spawn_blocking(move || {
        let conn = pool
            .get()
            .context("Failed to get DB connection from pool")?;
        f(&conn)
    })
    .await
    .context("spawn_blocking join error")?
}

// ── Translation Cache Pool ──────────────────────────────────────────

#[cfg(not(test))]
static TRANSLATION_POOL: LazyLock<DbPool> = LazyLock::new(|| {
    create_pool(&crate::core::infra::paths::translation_db_path(), 4)
        .expect("translation cache DB pool init failed: ~/.skillstar/db/ must be writable")
});

#[cfg(not(test))]
pub fn translation_pool() -> &'static DbPool {
    &TRANSLATION_POOL
}

// ── Security Scan Pool ──────────────────────────────────────────────

#[cfg(not(test))]
static SECURITY_SCAN_POOL: LazyLock<DbPool> = LazyLock::new(|| {
    create_pool(&crate::core::infra::paths::security_scan_db_path(), 3)
        .expect("security scan DB pool init failed: ~/.skillstar/db/ must be writable")
});

#[cfg(not(test))]
pub fn security_scan_pool() -> &'static DbPool {
    &SECURITY_SCAN_POOL
}
