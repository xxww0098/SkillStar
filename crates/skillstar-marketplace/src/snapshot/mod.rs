use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Duration, Utc};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, LazyLock, Mutex, RwLock};
use tracing::{error, warn};

use crate::db;
use crate::models::{
    CuratedRegistryEntry, CuratedRegistryKind, CuratedRegistryUpsert, MarketplaceCategory,
    MarketplaceCategoryUpsert, MarketplaceRatingSummary, MarketplaceRatingSummaryUpsert,
    MarketplaceReview, MarketplaceReviewUpsert, MarketplaceSkillCategoryAssignment,
    MarketplaceSkillCategoryAssignmentInput, MarketplaceSkillTagAssignment,
    MarketplaceSkillTagAssignmentInput, MarketplaceSourceObservation,
    MarketplaceSourceObservationUpsert, MarketplaceSourceSummary, MarketplaceTag,
    MarketplaceTagUpsert, MarketplaceUpdateNotification, MarketplaceUpdateNotificationUpsert,
};
use crate::remote::{
    self, AiKeywordSearchResult, MarketplaceResult, MarketplaceSkillDetails, PublisherRepo,
    SecurityAudit,
};
use crate::{OfficialPublisher, Skill, SkillType, extract_github_source_from_url};

const SNAPSHOT_SCHEMA_VERSION: i64 = 10;
const LEADERBOARD_TTL_HOURS: i64 = 6;
const PUBLISHER_TTL_HOURS: i64 = 24;
const DETAIL_TTL_HOURS: i64 = 48;
const SEARCH_SEED_LIMIT: u32 = 50;
const STALE_SKILL_RETENTION_DAYS: i64 = 30;
const AI_SEARCH_REMOTE_SEED_MIN_HITS: usize = 3;
const AI_SEARCH_LOW_COVERAGE_ROWS: i64 = 500;
const RESOLVE_SOURCE_REMOTE_LIMIT: u32 = 20;
const DEFAULT_CURATED_REGISTRY_ID: &str = "skills_sh";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalFirstResult<T> {
    pub data: T,
    pub snapshot_status: SnapshotStatus,
    pub snapshot_updated_at: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotStatus {
    Fresh,
    Stale,
    Seeding,
    Miss,
    ErrorFallback,
    RemoteError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStateEntry {
    pub scope: String,
    pub last_success_at: Option<String>,
    pub last_attempt_at: Option<String>,
    pub last_error: Option<String>,
    pub next_refresh_at: Option<String>,
    pub schema_version: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct InstalledSkillState {
    installed: bool,
    update_available: bool,
    skill_type: SkillType,
    tree_hash: Option<String>,
    agent_links: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolveSkillRequest {
    original_name: String,
    normalized_name: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolveSourceCandidate {
    source: String,
    git_url: String,
    installs: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScopeSeedState {
    Synced,
    NeverSynced,
}

pub(crate) enum ScopeSpec {
    Leaderboard { category: String },
    OfficialPublishers,
    PublisherRepos { publisher_name: String },
    RepoSkills { source: String },
    SkillDetail { source: String, name: String },
    SearchSeed { query: String },
}

pub type InstalledSkillsFuture = Pin<Box<dyn Future<Output = Result<Vec<Skill>>> + Send>>;
type InstalledMarkersLoader = Arc<dyn Fn() -> HashSet<String> + Send + Sync>;
type InstalledSkillsLoader = Arc<dyn Fn() -> InstalledSkillsFuture + Send + Sync>;

#[derive(Clone)]
pub struct SnapshotRuntimeConfig {
    pub db_path: PathBuf,
    pub data_root: PathBuf,
    installed_markers: InstalledMarkersLoader,
    installed_skills: InstalledSkillsLoader,
}

impl std::fmt::Debug for SnapshotRuntimeConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SnapshotRuntimeConfig")
            .field("db_path", &self.db_path)
            .field("data_root", &self.data_root)
            .finish_non_exhaustive()
    }
}

impl SnapshotRuntimeConfig {
    pub fn new<FM, FS>(
        db_path: PathBuf,
        data_root: PathBuf,
        installed_markers: FM,
        installed_skills: FS,
    ) -> Self
    where
        FM: Fn() -> HashSet<String> + Send + Sync + 'static,
        FS: Fn() -> InstalledSkillsFuture + Send + Sync + 'static,
    {
        Self {
            db_path,
            data_root,
            installed_markers: Arc::new(installed_markers),
            installed_skills: Arc::new(installed_skills),
        }
    }
}

fn default_runtime() -> SnapshotRuntimeConfig {
    let data_root = std::env::temp_dir().join("skillstar-marketplace");
    SnapshotRuntimeConfig::new(
        data_root.join("marketplace.db"),
        data_root,
        HashSet::new,
        || -> InstalledSkillsFuture { Box::pin(async { Ok(Vec::new()) }) },
    )
}

static SNAPSHOT_RUNTIME: LazyLock<RwLock<SnapshotRuntimeConfig>> =
    LazyLock::new(|| RwLock::new(default_runtime()));
static SNAPSHOT_POOL: LazyLock<Mutex<Option<(PathBuf, db::DbPool)>>> =
    LazyLock::new(|| Mutex::new(None));
static SCHEMA_READY_PATH: LazyLock<Mutex<Option<PathBuf>>> = LazyLock::new(|| Mutex::new(None));

pub fn configure_runtime(config: SnapshotRuntimeConfig) {
    let new_db_path = config.db_path.clone();
    let old_db_path = SNAPSHOT_RUNTIME
        .read()
        .ok()
        .map(|runtime| runtime.db_path.clone());

    if let Ok(mut runtime) = SNAPSHOT_RUNTIME.write() {
        *runtime = config;
    } else {
        return;
    }

    if old_db_path.as_ref() != Some(&new_db_path) {
        if let Ok(mut ready) = SCHEMA_READY_PATH.lock() {
            *ready = None;
        }
        if let Ok(mut guard) = SNAPSHOT_POOL.lock() {
            let should_clear = guard
                .as_ref()
                .is_some_and(|(current_path, _)| current_path != &new_db_path);
            if should_clear {
                *guard = None;
            }
        }
    }
}

fn snapshot_runtime() -> SnapshotRuntimeConfig {
    SNAPSHOT_RUNTIME
        .read()
        .map(|runtime| runtime.clone())
        .unwrap_or_else(|_| default_runtime())
}

fn legacy_cache_path() -> PathBuf {
    snapshot_runtime()
        .data_root
        .join("marketplace_description_cache.json")
}

fn installed_markers() -> HashSet<String> {
    let runtime = snapshot_runtime();
    (runtime.installed_markers)()
}

async fn load_installed_skills() -> Result<Vec<Skill>> {
    let runtime = snapshot_runtime();
    (runtime.installed_skills)().await
}

// Only reached from `with_conn`'s non-test branch; the test build uses
// `create_connection` instead, so this is dead code under `cfg(test)`.
#[cfg_attr(test, allow(dead_code))]
fn ensure_schema_ready(db_path: &PathBuf) -> Result<()> {
    if SCHEMA_READY_PATH
        .lock()
        .map(|ready| ready.as_ref() == Some(db_path))
        .unwrap_or(false)
    {
        return Ok(());
    }

    let pool = db::create_pool(db_path, 4)?;
    let conn = pool
        .get()
        .map_err(|e| anyhow!("Failed to get marketplace pool connection: {e}"))?;
    migrate_schema(&conn)?;
    drop(conn);

    if let Ok(mut ready) = SCHEMA_READY_PATH.lock() {
        *ready = Some(db_path.clone());
    }

    Ok(())
}

#[cfg_attr(test, allow(dead_code))]
fn snapshot_pool(db_path: &PathBuf) -> Result<db::DbPool> {
    let mut guard = SNAPSHOT_POOL
        .lock()
        .map_err(|_| anyhow!("Failed to lock marketplace snapshot pool state"))?;

    if let Some(pool) = guard.as_ref().and_then(|(current_path, pool)| {
        if current_path == db_path {
            Some(pool.clone())
        } else {
            None
        }
    }) {
        return Ok(pool);
    }

    let pool = db::create_pool(db_path, 4)?;
    *guard = Some((db_path.clone(), pool.clone()));
    Ok(pool)
}

/// Test-only: open a standalone connection with full schema migration.
/// Pool connections are not used in tests because tests swap
/// `SKILLSTAR_DATA_DIR`, but the pool path resolves only once at init.
#[cfg(test)]
fn create_connection() -> Result<Connection> {
    let path = snapshot_runtime().db_path;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("Failed to create marketplace db directory")?;
    }
    let conn = Connection::open(&path).context("Failed to open marketplace.db")?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA busy_timeout=5000;
         PRAGMA foreign_keys=ON;",
    )
    .context("Failed to configure marketplace db pragmas")?;
    migrate_schema(&conn)?;
    Ok(conn)
}

pub(crate) fn with_conn<F, T>(f: F) -> Result<T>
where
    F: FnOnce(&Connection) -> Result<T>,
{
    #[cfg(not(test))]
    {
        let runtime = snapshot_runtime();
        ensure_schema_ready(&runtime.db_path)?;
        let pool = snapshot_pool(&runtime.db_path)?;
        let conn = pool
            .get()
            .map_err(|e| anyhow!("Failed to get marketplace pool connection: {e}"))?;
        f(&conn)
    }

    #[cfg(test)]
    {
        let conn = create_connection()?;
        f(&conn)
    }
}

pub fn initialize() -> Result<()> {
    with_conn(|_| Ok(()))
}

pub async fn schedule_startup_refreshes() {
    let mut scopes = Vec::new();
    if let Ok(true) = is_scope_stale("leaderboard_all") {
        scopes.push("leaderboard_all".to_string());
    }
    if let Ok(true) = is_scope_stale("official_publishers") {
        scopes.push("official_publishers".to_string());
    }

    for scope in scopes {
        if let Err(err) = sync_marketplace_scope(&scope).await {
            error!(target: "marketplace_snapshot", scope = %scope, error = %err, "startup refresh failed");
        }
    }
}

/// Purge marketplace skill entries that haven't been seen by any remote
/// sync within `STALE_SKILL_RETENTION_DAYS`. Skills in active leaderboard
/// listings are kept regardless of age.
pub fn purge_stale_skills() -> Result<usize> {
    with_conn(|conn| {
        let cutoff = (Utc::now() - Duration::days(STALE_SKILL_RETENTION_DAYS)).to_rfc3339();

        let deleted = conn
            .execute(
                "DELETE FROM marketplace_skill
                 WHERE last_seen_remote_at < ?1
                   AND skill_key NOT IN (SELECT skill_key FROM marketplace_listing)
                   AND skill_key NOT IN (SELECT skill_key FROM marketplace_repo_skill)",
                params![cutoff],
            )
            .context("Failed to purge stale marketplace skills")?;

        if deleted > 0 {
            tracing::info!(
                target: "marketplace_snapshot",
                purged = deleted,
                cutoff_days = STALE_SKILL_RETENTION_DAYS,
                "purged stale marketplace skills"
            );
        }

        Ok(deleted)
    })
}

pub async fn refresh_startup_scopes_if_needed() -> Result<()> {
    // Purge stale entries before refreshing — keeps the DB lean.
    if let Err(err) = purge_stale_skills() {
        warn!(target: "marketplace_snapshot", error = %err, "stale skill purge failed");
    }

    schedule_startup_refreshes().await;
    Ok(())
}

mod helpers;
mod loaders;
mod local_first;
mod migrations;
mod registries;
mod resolve;
mod skills;
mod sync;
mod sync_state;
mod taxonomy;

#[cfg(test)]
mod tests;

// Modules that only contain crate-internal helpers: re-export at `pub(crate)`
// so siblings reach them via the flat `snapshot::` namespace (`super::*`).
pub(crate) use helpers::*;
pub(crate) use loaders::*;
pub(crate) use migrations::*;

// Modules that expose part of the public API: `pub use` re-exports the `pub`
// items externally and also brings every `pub(crate)` item into this module's
// namespace for sibling access via `super::*`.
pub use local_first::*;
pub use registries::*;
pub use resolve::*;
pub use skills::*;
pub use sync::*;
pub use sync_state::*;
pub use taxonomy::*;
