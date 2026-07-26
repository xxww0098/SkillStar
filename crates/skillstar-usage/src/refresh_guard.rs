//! Serialize usage refreshes per provider catalog to avoid concurrent API calls
//! when one vendor has multiple accounts (rate limits / account bans).

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use tokio::sync::Mutex;

use crate::{UsageError, UsageResult};

/// Minimum gap between two refreshes for the same catalog id.
const SAME_CATALOG_GAP: Duration = Duration::from_secs(1);

static CATALOG_LOCKS: LazyLock<Mutex<HashMap<String, Arc<Mutex<()>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

static LAST_CATALOG_REFRESH: LazyLock<Mutex<HashMap<String, Instant>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

static REFRESH_ALL_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

async fn catalog_lock(catalog_id: &str) -> Arc<Mutex<()>> {
    let mut locks = CATALOG_LOCKS.lock().await;
    locks
        .entry(catalog_id.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

async fn catalog_file_lock(catalog_id: &str) -> UsageResult<File> {
    let catalog_id: String = catalog_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect();
    tokio::task::spawn_blocking(move || {
        #[cfg(test)]
        let root = std::env::temp_dir().join(format!(
            "skillstar-refresh-guard-test-{}",
            std::process::id()
        ));
        #[cfg(not(test))]
        let root = skillstar_core::infra::paths::config_dir().join("usage");
        let locks_dir: PathBuf = root.join("locks");
        fs::create_dir_all(&locks_dir)?;
        let path = locks_dir.join(format!("catalog-{catalog_id}.lock"));
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(&path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        }
        file.lock()?;
        Ok::<_, std::io::Error>(file)
    })
    .await
    .map_err(|error| UsageError::Other(format!("catalog lock task failed: {error}")))?
    .map_err(UsageError::from)
}

async fn wait_catalog_gap(catalog_id: &str) {
    let sleep_for = {
        let last = LAST_CATALOG_REFRESH.lock().await;
        last.get(catalog_id).and_then(|prev| {
            let elapsed = prev.elapsed();
            (elapsed < SAME_CATALOG_GAP).then(|| SAME_CATALOG_GAP - elapsed)
        })
    };
    if let Some(delay) = sleep_for {
        tokio::time::sleep(delay).await;
    }
}

async fn mark_catalog_refreshed(catalog_id: &str) {
    LAST_CATALOG_REFRESH
        .lock()
        .await
        .insert(catalog_id.to_string(), Instant::now());
}

/// Run a single subscription refresh with per-catalog serialization + spacing.
pub async fn with_catalog_refresh<F, Fut, T>(catalog_id: &str, f: F) -> UsageResult<T>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = T>,
{
    let lock = catalog_lock(catalog_id).await;
    let _guard = lock.lock().await;
    let _file_guard = catalog_file_lock(catalog_id).await?;
    // Check spacing while holding the same lock. Otherwise two callers can
    // both observe the old timestamp before either acquires the mutex, and the
    // second request starts immediately after the first.
    wait_catalog_gap(catalog_id).await;
    let result = f().await;
    mark_catalog_refreshed(catalog_id).await;
    Ok(result)
}

/// Serialize non-refresh credential operations with refreshes for the same
/// catalog, without applying the HTTP rate-limit spacing or updating its
/// timestamp. Grok account activation uses this so a concurrent Usage refresh
/// cannot rotate tokens underneath its capture/install transaction.
pub async fn with_catalog_lock<F, Fut, T>(catalog_id: &str, f: F) -> UsageResult<T>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = T>,
{
    let lock = catalog_lock(catalog_id).await;
    let _guard = lock.lock().await;
    let _file_guard = catalog_file_lock(catalog_id).await?;
    Ok(f().await)
}

/// Ensure only one batch refresh runs at a time.
pub async fn with_refresh_all_lock<F, Fut, T>(f: F) -> T
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = T>,
{
    let _guard = REFRESH_ALL_LOCK.lock().await;
    f().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn same_catalog_refreshes_are_serialized() {
        let started = Arc::new(Mutex::new(Vec::<Instant>::new()));
        let started_a = Arc::clone(&started);
        let started_b = Arc::clone(&started);

        let first = tokio::spawn(async move {
            with_catalog_refresh("cursor", || async {
                started_a.lock().await.push(Instant::now());
                tokio::time::sleep(Duration::from_millis(120)).await;
                1_u8
            })
            .await
        });
        tokio::time::sleep(Duration::from_millis(20)).await;
        let second = tokio::spawn(async move {
            with_catalog_refresh("cursor", || async {
                started_b.lock().await.push(Instant::now());
                2_u8
            })
            .await
        });

        assert_eq!(first.await.unwrap().unwrap(), 1);
        assert_eq!(second.await.unwrap().unwrap(), 2);

        let times = started.lock().await;
        assert_eq!(times.len(), 2);
        assert!(
            times[1].duration_since(times[0]) >= Duration::from_millis(900),
            "the second request must observe the one-second catalog gap"
        );
    }
}
