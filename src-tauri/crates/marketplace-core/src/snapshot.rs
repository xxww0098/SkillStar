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
use crate::remote::{
    self, AiKeywordSearchResult, MarketplaceResult, MarketplaceSkillDetails, PublisherRepo,
    SecurityAudit,
};
use crate::{OfficialPublisher, Skill, SkillType, extract_github_source_from_url};

const SNAPSHOT_SCHEMA_VERSION: i64 = 2;
const LEADERBOARD_TTL_HOURS: i64 = 6;
const PUBLISHER_TTL_HOURS: i64 = 24;
const DETAIL_TTL_HOURS: i64 = 48;
const SEARCH_SEED_LIMIT: u32 = 50;
const STALE_SKILL_RETENTION_DAYS: i64 = 30;
const AI_SEARCH_REMOTE_SEED_MIN_HITS: usize = 3;
const AI_SEARCH_LOW_COVERAGE_ROWS: i64 = 500;
const RESOLVE_SOURCE_REMOTE_LIMIT: u32 = 20;

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
struct InstalledSkillState {
    installed: bool,
    update_available: bool,
    skill_type: SkillType,
    tree_hash: Option<String>,
    agent_links: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
struct ResolveSkillRequest {
    original_name: String,
    normalized_name: String,
}

#[derive(Debug, Clone)]
struct ResolveSourceCandidate {
    source: String,
    git_url: String,
    installs: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScopeSeedState {
    Synced,
    NeverSynced,
}

enum ScopeSpec {
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
    let data_root = std::env::temp_dir().join("skillstar-marketplace-core");
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
            if guard
                .as_ref()
                .is_some_and(|(current_path, _)| current_path != &new_db_path)
            {
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

fn snapshot_pool(db_path: &PathBuf) -> Result<db::DbPool> {
    let mut guard = SNAPSHOT_POOL
        .lock()
        .map_err(|_| anyhow!("Failed to lock marketplace snapshot pool state"))?;

    if let Some((current_path, pool)) = guard.as_ref() {
        if current_path == db_path {
            return Ok(pool.clone());
        }
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

fn with_conn<F, T>(f: F) -> Result<T>
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

fn migrate_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS marketplace_skill (
            skill_key TEXT PRIMARY KEY,
            source TEXT NOT NULL,
            name TEXT NOT NULL,
            git_url TEXT NOT NULL DEFAULT '',
            author TEXT,
            publisher_name TEXT,
            repo_name TEXT,
            description TEXT NOT NULL DEFAULT '',
            installs INTEGER NOT NULL DEFAULT 0,
            last_seen_remote_at TEXT,
            last_list_sync_at TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_skill_source ON marketplace_skill(source);
        CREATE INDEX IF NOT EXISTS idx_skill_publisher ON marketplace_skill(publisher_name);
        CREATE INDEX IF NOT EXISTS idx_skill_installs ON marketplace_skill(installs DESC);
        CREATE INDEX IF NOT EXISTS idx_skill_name ON marketplace_skill(name);

        CREATE TABLE IF NOT EXISTS marketplace_skill_detail (
            skill_key TEXT PRIMARY KEY REFERENCES marketplace_skill(skill_key) ON DELETE CASCADE,
            summary TEXT,
            readme TEXT,
            weekly_installs TEXT,
            github_stars INTEGER,
            first_seen TEXT,
            security_audits_json TEXT,
            last_detail_sync_at TEXT
        );

        CREATE TABLE IF NOT EXISTS marketplace_publisher (
            publisher_name TEXT PRIMARY KEY,
            repo_count INTEGER NOT NULL DEFAULT 0,
            skill_count INTEGER NOT NULL DEFAULT 0,
            url TEXT NOT NULL DEFAULT '',
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS marketplace_repo (
            source TEXT PRIMARY KEY,
            publisher_name TEXT NOT NULL,
            repo_name TEXT NOT NULL,
            skill_count INTEGER NOT NULL DEFAULT 0,
            installs INTEGER NOT NULL DEFAULT 0,
            installs_label TEXT NOT NULL DEFAULT '',
            url TEXT NOT NULL DEFAULT '',
            updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_repo_publisher ON marketplace_repo(publisher_name);

        CREATE TABLE IF NOT EXISTS marketplace_repo_skill (
            source TEXT NOT NULL,
            skill_key TEXT NOT NULL,
            installs INTEGER NOT NULL DEFAULT 0,
            rank INTEGER,
            updated_at TEXT NOT NULL,
            PRIMARY KEY (source, skill_key)
        );

        CREATE TABLE IF NOT EXISTS marketplace_listing (
            listing_type TEXT NOT NULL,
            skill_key TEXT NOT NULL,
            rank INTEGER NOT NULL,
            updated_at TEXT NOT NULL,
            PRIMARY KEY (listing_type, skill_key)
        );
        CREATE INDEX IF NOT EXISTS idx_listing_type_rank ON marketplace_listing(listing_type, rank);

        CREATE TABLE IF NOT EXISTS marketplace_sync_state (
            scope TEXT PRIMARY KEY,
            last_success_at TEXT,
            last_attempt_at TEXT,
            last_error TEXT,
            next_refresh_at TEXT,
            schema_version INTEGER NOT NULL DEFAULT 1
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS marketplace_skill_fts USING fts5(
            skill_key,
            name,
            description,
            summary,
            publisher_name,
            repo_name,
            tokenize='unicode61'
        );",
    )
    .context("Failed to initialize marketplace snapshot schema")?;

    let version: i64 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .context("Failed to read marketplace user_version")?;

    if version < SNAPSHOT_SCHEMA_VERSION {
        if version < 1 {
            migrate_v0_to_v1(conn)?;
        }
        if version < 2 {
            migrate_v1_to_v2(conn)?;
        }
        conn.pragma_update(None, "user_version", SNAPSHOT_SCHEMA_VERSION)
            .context("Failed to update marketplace user_version")?;
    }

    let legacy_path = legacy_cache_path();
    if legacy_path.exists() {
        let _ = std::fs::remove_file(legacy_path);
    }

    Ok(())
}

fn migrate_v0_to_v1(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "marketplace_cache")? {
        return Ok(());
    }

    let mut stmt = conn
        .prepare("SELECT key, description, updated_at FROM marketplace_cache")
        .context("Failed to prepare legacy marketplace_cache query")?;

    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .context("Failed to read legacy marketplace_cache rows")?;

    let tx = conn
        .unchecked_transaction()
        .context("Failed to start legacy marketplace migration transaction")?;

    for row in rows {
        let (key, description, updated_at) = row.context("Failed to decode legacy cache row")?;
        let Some((source, name)) = parse_skill_key(&key) else {
            continue;
        };

        let (publisher_name, repo_name) = split_source(&source);
        let git_url = format!("https://github.com/{source}");

        tx.execute(
            "INSERT INTO marketplace_skill (
                skill_key,
                source,
                name,
                git_url,
                author,
                publisher_name,
                repo_name,
                description,
                installs,
                last_seen_remote_at,
                last_list_sync_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, ?9, ?9)
            ON CONFLICT(skill_key) DO UPDATE SET
                source = excluded.source,
                name = excluded.name,
                git_url = excluded.git_url,
                author = excluded.author,
                publisher_name = excluded.publisher_name,
                repo_name = excluded.repo_name,
                description = CASE
                    WHEN excluded.description <> '' THEN excluded.description
                    ELSE marketplace_skill.description
                END,
                last_seen_remote_at = COALESCE(excluded.last_seen_remote_at, marketplace_skill.last_seen_remote_at),
                last_list_sync_at = COALESCE(excluded.last_list_sync_at, marketplace_skill.last_list_sync_at)",
            params![
                key,
                source,
                name,
                git_url,
                source,
                publisher_name,
                repo_name,
                description,
                updated_at
            ],
        )
        .context("Failed to migrate legacy marketplace description")?;

        refresh_fts_entry_in_tx(&tx, &key)?;
    }

    tx.execute("DROP TABLE IF EXISTS marketplace_cache", [])
        .context("Failed to drop legacy marketplace_cache table")?;
    tx.commit()
        .context("Failed to commit legacy marketplace migration")?;

    Ok(())
}

fn migrate_v1_to_v2(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS marketplace_pack (
            pack_key TEXT PRIMARY KEY,
            source TEXT NOT NULL,
            name TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            skill_count INTEGER NOT NULL DEFAULT 0,
            author TEXT,
            git_url TEXT NOT NULL DEFAULT '',
            installs INTEGER NOT NULL DEFAULT 0,
            last_seen_at TEXT
        );

        CREATE TABLE IF NOT EXISTS marketplace_pack_skill (
            pack_key TEXT NOT NULL REFERENCES marketplace_pack(pack_key) ON DELETE CASCADE,
            skill_key TEXT NOT NULL REFERENCES marketplace_skill(skill_key) ON DELETE CASCADE,
            skill_name TEXT NOT NULL,
            PRIMARY KEY (pack_key, skill_key)
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS marketplace_pack_fts USING fts5(
            pack_key,
            name,
            description,
            author,
            content=marketplace_pack,
            content_rowid=rowid
        );",
    )
    .context("Failed to create marketplace pack tables (v2)")?;
    Ok(())
}

fn table_exists(conn: &Connection, table_name: &str) -> Result<bool> {
    let exists = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1 LIMIT 1",
            [table_name],
            |_| Ok(()),
        )
        .optional()
        .context("Failed to inspect sqlite schema")?
        .is_some();
    Ok(exists)
}

fn normalize_source(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let lowered = trimmed
        .trim_start_matches("https://github.com/")
        .trim_start_matches("http://github.com/")
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .to_ascii_lowercase();

    let mut parts = lowered.split('/').filter(|part| !part.is_empty());
    let publisher = parts.next()?;
    let repo = parts.next()?;
    Some(format!("{publisher}/{repo}"))
}

fn normalize_skill_name(raw: &str) -> Option<String> {
    let trimmed = raw.trim().to_ascii_lowercase();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn split_source(source: &str) -> (String, String) {
    let mut parts = source.split('/');
    let publisher = parts.next().unwrap_or_default().to_string();
    let repo = parts.next().unwrap_or_default().to_string();
    (publisher, repo)
}

fn build_skill_key(source: &str, name: &str) -> Option<String> {
    let source = normalize_source(source)?;
    let name = normalize_skill_name(name)?;
    Some(format!("{source}/{name}"))
}

fn parse_skill_key(skill_key: &str) -> Option<(String, String)> {
    let normalized = skill_key.trim().to_ascii_lowercase();
    let mut parts = normalized.split('/').filter(|part| !part.is_empty());
    let publisher = parts.next()?;
    let repo = parts.next()?;
    let name = parts.collect::<Vec<_>>().join("/");
    if name.is_empty() {
        return None;
    }
    Some((format!("{publisher}/{repo}"), name))
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

fn scope_ttl(scope: &str) -> Option<Duration> {
    if scope.starts_with("leaderboard_") {
        Some(Duration::hours(LEADERBOARD_TTL_HOURS))
    } else if scope == "official_publishers"
        || scope.starts_with("publisher_repos:")
        || scope.starts_with("repo_skills:")
    {
        Some(Duration::hours(PUBLISHER_TTL_HOURS))
    } else if scope.starts_with("skill_detail:") {
        Some(Duration::hours(DETAIL_TTL_HOURS))
    } else {
        None
    }
}

fn leaderboard_scope(category: &str) -> String {
    match category {
        "trending" => "leaderboard_trending".to_string(),
        "hot" => "leaderboard_hot".to_string(),
        _ => "leaderboard_all".to_string(),
    }
}

fn skill_detail_scope(source: &str, name: &str) -> Option<String> {
    Some(format!("skill_detail:{}", build_skill_key(source, name)?))
}

fn parse_scope(scope: &str) -> Result<ScopeSpec> {
    if let Some(category) = scope.strip_prefix("leaderboard_") {
        let normalized = match category {
            "hot" => "hot",
            "trending" => "trending",
            "all" | "popular" => "all",
            other => other,
        };
        return Ok(ScopeSpec::Leaderboard {
            category: normalized.to_string(),
        });
    }

    if scope == "official_publishers" {
        return Ok(ScopeSpec::OfficialPublishers);
    }

    if let Some(value) = scope.strip_prefix("publisher_repos:") {
        return Ok(ScopeSpec::PublisherRepos {
            publisher_name: value.trim().to_ascii_lowercase(),
        });
    }

    if let Some(value) = scope.strip_prefix("repo_skills:") {
        let source = normalize_source(value)
            .ok_or_else(|| anyhow!("Invalid repo_skills scope source: {value}"))?;
        return Ok(ScopeSpec::RepoSkills { source });
    }

    if let Some(value) = scope.strip_prefix("skill_detail:") {
        let (source, name) =
            parse_skill_key(value).ok_or_else(|| anyhow!("Invalid skill_detail scope key"))?;
        return Ok(ScopeSpec::SkillDetail { source, name });
    }

    if let Some(query) = scope.strip_prefix("search_seed:") {
        return Ok(ScopeSpec::SearchSeed {
            query: query.trim().to_string(),
        });
    }

    Err(anyhow!("Unsupported marketplace scope: {scope}"))
}

fn next_refresh_at_for_scope(scope: &str, now: DateTime<Utc>) -> Option<String> {
    scope_ttl(scope).map(|ttl| (now + ttl).to_rfc3339())
}

fn mark_scope_attempt_in_tx(tx: &Transaction<'_>, scope: &str) -> Result<()> {
    let now = now_rfc3339();
    tx.execute(
        "INSERT INTO marketplace_sync_state (
            scope,
            last_success_at,
            last_attempt_at,
            last_error,
            next_refresh_at,
            schema_version
        ) VALUES (?1, NULL, ?2, NULL, NULL, ?3)
        ON CONFLICT(scope) DO UPDATE SET
            last_attempt_at = excluded.last_attempt_at,
            last_error = NULL,
            schema_version = excluded.schema_version",
        params![scope, now, SNAPSHOT_SCHEMA_VERSION],
    )
    .with_context(|| format!("Failed to mark marketplace scope attempt: {scope}"))?;
    Ok(())
}

fn mark_scope_success_in_tx(tx: &Transaction<'_>, scope: &str) -> Result<()> {
    let now = Utc::now();
    tx.execute(
        "INSERT INTO marketplace_sync_state (
            scope,
            last_success_at,
            last_attempt_at,
            last_error,
            next_refresh_at,
            schema_version
        ) VALUES (?1, ?2, ?2, NULL, ?3, ?4)
        ON CONFLICT(scope) DO UPDATE SET
            last_success_at = excluded.last_success_at,
            last_attempt_at = excluded.last_attempt_at,
            last_error = NULL,
            next_refresh_at = excluded.next_refresh_at,
            schema_version = excluded.schema_version",
        params![
            scope,
            now.to_rfc3339(),
            next_refresh_at_for_scope(scope, now),
            SNAPSHOT_SCHEMA_VERSION
        ],
    )
    .with_context(|| format!("Failed to mark marketplace scope success: {scope}"))?;
    Ok(())
}

fn mark_scope_error(scope: &str, error: &str) -> Result<()> {
    with_conn(|conn| {
        let now = now_rfc3339();
        conn.execute(
            "INSERT INTO marketplace_sync_state (
                scope,
                last_success_at,
                last_attempt_at,
                last_error,
                next_refresh_at,
                schema_version
            ) VALUES (?1, NULL, ?2, ?3, NULL, ?4)
            ON CONFLICT(scope) DO UPDATE SET
                last_attempt_at = excluded.last_attempt_at,
                last_error = excluded.last_error,
                schema_version = excluded.schema_version",
            params![scope, now, truncate_error(error), SNAPSHOT_SCHEMA_VERSION],
        )
        .with_context(|| format!("Failed to mark marketplace scope error: {scope}"))?;
        Ok(())
    })
}

fn truncate_error(error: &str) -> String {
    error.chars().take(500).collect()
}

fn scope_sync_state(conn: &Connection, scope: &str) -> Result<Option<SyncStateEntry>> {
    conn.query_row(
        "SELECT scope, last_success_at, last_attempt_at, last_error, next_refresh_at, schema_version
         FROM marketplace_sync_state
         WHERE scope = ?1",
        [scope],
        |row| {
            Ok(SyncStateEntry {
                scope: row.get(0)?,
                last_success_at: row.get(1)?,
                last_attempt_at: row.get(2)?,
                last_error: row.get(3)?,
                next_refresh_at: row.get(4)?,
                schema_version: row.get(5)?,
            })
        },
    )
    .optional()
    .context("Failed to load marketplace sync state")
}

fn scope_updated_at(conn: &Connection, scope: &str) -> Result<Option<String>> {
    Ok(scope_sync_state(conn, scope)?.and_then(|entry| entry.last_success_at))
}

fn is_scope_fresh_conn(conn: &Connection, scope: &str) -> Result<bool> {
    let Some(state) = scope_sync_state(conn, scope)? else {
        return Ok(false);
    };

    let Some(next_refresh_at) = state.next_refresh_at else {
        return Ok(false);
    };

    let next_refresh = DateTime::parse_from_rfc3339(&next_refresh_at)
        .map(|value| value.with_timezone(&Utc))
        .ok();
    Ok(next_refresh.is_some_and(|value| value > Utc::now()))
}

pub fn is_scope_stale(scope: &str) -> Result<bool> {
    with_conn(|conn| {
        let Some(state) = scope_sync_state(conn, scope)? else {
            return Ok(false);
        };
        Ok(state.last_success_at.is_some() && !is_scope_fresh_conn(conn, scope)?)
    })
}

fn sync_seed_state(conn: &Connection, scope: &str) -> Result<ScopeSeedState> {
    Ok(if scope_sync_state(conn, scope)?.is_some() {
        ScopeSeedState::Synced
    } else {
        ScopeSeedState::NeverSynced
    })
}

pub fn get_marketplace_sync_states() -> Result<Vec<SyncStateEntry>> {
    with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT scope, last_success_at, last_attempt_at, last_error, next_refresh_at, schema_version
                 FROM marketplace_sync_state
                 ORDER BY scope ASC",
            )
            .context("Failed to prepare marketplace sync-state query")?;

        let rows = stmt
            .query_map([], |row| {
                Ok(SyncStateEntry {
                    scope: row.get(0)?,
                    last_success_at: row.get(1)?,
                    last_attempt_at: row.get(2)?,
                    last_error: row.get(3)?,
                    next_refresh_at: row.get(4)?,
                    schema_version: row.get(5)?,
                })
            })
            .context("Failed to read marketplace sync-state rows")?;

        let mut entries = Vec::new();
        for row in rows {
            entries.push(row.context("Failed to decode marketplace sync-state row")?);
        }
        Ok(entries)
    })
}

fn skill_from_snapshot_row(
    source: String,
    name: String,
    git_url: String,
    author: Option<String>,
    description: String,
    installs: u32,
    last_updated: Option<String>,
    rank: Option<u32>,
) -> Skill {
    let skill_author = author.unwrap_or_else(|| source.clone());
    let mut skill =
        Skill::from_skills_sh(name, description, installs, skill_author.clone(), git_url);
    skill.skill_type = SkillType::Hub;
    skill.author = Some(skill_author);
    skill.source = Some(source);
    skill.last_updated = last_updated.unwrap_or_else(now_rfc3339);
    skill.rank = rank;
    skill.classify();
    skill
}

fn decode_security_audits(raw: Option<String>) -> Vec<SecurityAudit> {
    raw.and_then(|value| serde_json::from_str::<Vec<SecurityAudit>>(&value).ok())
        .unwrap_or_default()
}

fn load_leaderboard_snapshot(conn: &Connection, scope: &str) -> Result<Vec<Skill>> {
    let mut stmt = conn
        .prepare(
            "SELECT
                s.source,
                s.name,
                s.git_url,
                s.author,
                s.description,
                s.installs,
                s.last_list_sync_at,
                l.rank
             FROM marketplace_listing l
             JOIN marketplace_skill s ON s.skill_key = l.skill_key
             WHERE l.listing_type = ?1
             ORDER BY l.rank ASC, s.installs DESC, s.name ASC",
        )
        .context("Failed to prepare leaderboard snapshot query")?;

    let rows = stmt
        .query_map([scope], |row| {
            Ok(skill_from_snapshot_row(
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get::<_, i64>(5)?.max(0) as u32,
                row.get(6)?,
                row.get::<_, Option<i64>>(7)?
                    .map(|value| value.max(0) as u32),
            ))
        })
        .context("Failed to read leaderboard snapshot rows")?;

    let mut skills = Vec::new();
    for row in rows {
        skills.push(row.context("Failed to decode leaderboard skill row")?);
    }
    Ok(skills)
}

fn any_skill_rows(conn: &Connection) -> Result<bool> {
    let count: i64 = conn
        .query_row("SELECT COUNT(1) FROM marketplace_skill", [], |row| {
            row.get(0)
        })
        .context("Failed to count marketplace skills")?;
    Ok(count > 0)
}

fn skill_row_count(conn: &Connection) -> Result<i64> {
    conn.query_row("SELECT COUNT(1) FROM marketplace_skill", [], |row| {
        row.get(0)
    })
    .context("Failed to count marketplace skills")
}

fn load_search_snapshot(
    conn: &Connection,
    query: &str,
    limit: u32,
) -> Result<(Vec<Skill>, Option<String>)> {
    let limit = (limit.max(1)).min(200) as i64;
    let normalized_query = query.trim().to_ascii_lowercase();

    if normalized_query.is_empty() {
        let mut stmt = conn
            .prepare(
                "SELECT
                    source,
                    name,
                    git_url,
                    author,
                    description,
                    installs,
                    last_list_sync_at
                 FROM marketplace_skill
                 ORDER BY installs DESC, name ASC
                 LIMIT ?1",
            )
            .context("Failed to prepare blank search snapshot query")?;
        let rows = stmt
            .query_map([limit], |row| {
                Ok(skill_from_snapshot_row(
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get::<_, i64>(5)?.max(0) as u32,
                    row.get(6)?,
                    None,
                ))
            })
            .context("Failed to read blank search snapshot rows")?;

        let mut skills = Vec::new();
        for row in rows {
            skills.push(row.context("Failed to decode blank search row")?);
        }
        let updated_at: Option<String> = conn
            .query_row(
                "SELECT MAX(last_list_sync_at) FROM marketplace_skill",
                [],
                |row| row.get(0),
            )
            .optional()
            .context("Failed to read marketplace search snapshot timestamp")?
            .flatten();
        return Ok((skills, updated_at));
    }

    let Some(fts_query) = build_fts_query(&normalized_query) else {
        return Ok((Vec::new(), None));
    };
    let prefix_query = format!("{normalized_query}%");

    let mut stmt = conn
        .prepare(
            "SELECT
                s.source,
                s.name,
                s.git_url,
                s.author,
                s.description,
                s.installs,
                s.last_list_sync_at
             FROM marketplace_skill_fts fts
             JOIN marketplace_skill s ON s.skill_key = fts.skill_key
             WHERE marketplace_skill_fts MATCH ?1
             ORDER BY
                CASE
                    WHEN lower(s.name) = ?2 THEN 0
                    WHEN lower(s.name) LIKE ?3 THEN 1
                    ELSE 2
                END ASC,
                bm25(marketplace_skill_fts, 10.0, 4.0, 2.0, 1.0, 1.0) ASC,
                s.installs DESC,
                s.name ASC
             LIMIT ?4",
        )
        .context("Failed to prepare marketplace FTS query")?;

    let rows = stmt
        .query_map(
            params![fts_query, normalized_query, prefix_query, limit],
            |row| {
                Ok(skill_from_snapshot_row(
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get::<_, i64>(5)?.max(0) as u32,
                    row.get(6)?,
                    None,
                ))
            },
        )
        .context("Failed to execute marketplace FTS query")?;

    let mut skills = Vec::new();
    let mut latest_updated_at: Option<String> = None;
    for row in rows {
        let skill = row.context("Failed to decode marketplace FTS row")?;
        if latest_updated_at
            .as_ref()
            .is_none_or(|current| skill.last_updated > *current)
        {
            latest_updated_at = Some(skill.last_updated.clone());
        }
        skills.push(skill);
    }

    Ok((skills, latest_updated_at))
}

fn load_publishers_snapshot(conn: &Connection) -> Result<Vec<OfficialPublisher>> {
    let mut stmt = conn
        .prepare(
            "SELECT publisher_name, repo_count, skill_count, url
             FROM marketplace_publisher
             ORDER BY skill_count DESC, repo_count DESC, publisher_name ASC",
        )
        .context("Failed to prepare publisher snapshot query")?;

    let rows = stmt
        .query_map([], |row| {
            Ok(OfficialPublisher {
                name: row.get(0)?,
                repo: "skills".to_string(),
                repo_count: row.get::<_, i64>(1)?.max(0) as u32,
                skill_count: row.get::<_, i64>(2)?.max(0) as u32,
                url: row.get(3)?,
            })
        })
        .context("Failed to read publisher snapshot rows")?;

    let mut publishers = Vec::new();
    for row in rows {
        publishers.push(row.context("Failed to decode publisher row")?);
    }
    Ok(publishers)
}

fn load_publisher_repos_snapshot(
    conn: &Connection,
    publisher_name: &str,
) -> Result<Vec<PublisherRepo>> {
    let mut stmt = conn
        .prepare(
            "SELECT source, repo_name, skill_count, installs, installs_label, url
             FROM marketplace_repo
             WHERE publisher_name = ?1
             ORDER BY installs DESC, repo_name ASC",
        )
        .context("Failed to prepare publisher repo snapshot query")?;

    let rows = stmt
        .query_map([publisher_name], |row| {
            Ok(PublisherRepo {
                source: row.get(0)?,
                repo: row.get(1)?,
                skill_count: row.get::<_, i64>(2)?.max(0) as u32,
                installs: row.get::<_, i64>(3)?.max(0) as u32,
                installs_label: row.get(4)?,
                url: row.get(5)?,
                skills: Vec::new(),
            })
        })
        .context("Failed to read publisher repo snapshot rows")?;

    let mut repos = Vec::new();
    for row in rows {
        repos.push(row.context("Failed to decode publisher repo row")?);
    }
    Ok(repos)
}

fn load_repo_skills_snapshot(conn: &Connection, source: &str) -> Result<Vec<Skill>> {
    let mut stmt = conn
        .prepare(
            "SELECT
                s.source,
                s.name,
                s.git_url,
                s.author,
                s.description,
                COALESCE(rs.installs, s.installs),
                COALESCE(s.last_list_sync_at, rs.updated_at),
                rs.rank
             FROM marketplace_repo_skill rs
             JOIN marketplace_skill s ON s.skill_key = rs.skill_key
             WHERE rs.source = ?1
             ORDER BY
                CASE WHEN rs.rank IS NULL THEN 1 ELSE 0 END ASC,
                rs.rank ASC,
                rs.installs DESC,
                s.name ASC",
        )
        .context("Failed to prepare repo-skill snapshot query")?;

    let rows = stmt
        .query_map([source], |row| {
            Ok(skill_from_snapshot_row(
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get::<_, i64>(5)?.max(0) as u32,
                row.get(6)?,
                row.get::<_, Option<i64>>(7)?
                    .map(|value| value.max(0) as u32),
            ))
        })
        .context("Failed to read repo-skill snapshot rows")?;

    let mut skills = Vec::new();
    for row in rows {
        skills.push(row.context("Failed to decode repo-skill row")?);
    }
    Ok(skills)
}

fn load_skill_detail_snapshot(
    conn: &Connection,
    skill_key: &str,
) -> Result<Option<MarketplaceSkillDetails>> {
    conn.query_row(
        "SELECT summary, readme, weekly_installs, github_stars, first_seen, security_audits_json
         FROM marketplace_skill_detail
         WHERE skill_key = ?1",
        [skill_key],
        |row| {
            Ok(MarketplaceSkillDetails {
                summary: row.get(0)?,
                readme: row.get(1)?,
                weekly_installs: row.get(2)?,
                github_stars: row
                    .get::<_, Option<i64>>(3)?
                    .map(|value| value.max(0) as u32),
                first_seen: row.get(4)?,
                security_audits: decode_security_audits(row.get(5)?),
            })
        },
    )
    .optional()
    .context("Failed to load marketplace detail snapshot")
}

fn normalize_resolve_requests(names: &[String]) -> Vec<ResolveSkillRequest> {
    names
        .iter()
        .filter_map(|name| {
            let original_name = name.trim().to_string();
            let normalized_name = normalize_skill_name(&original_name)?;
            Some(ResolveSkillRequest {
                original_name,
                normalized_name,
            })
        })
        .collect()
}

fn existing_named_sources(existing_sources: &HashMap<String, String>) -> HashMap<String, String> {
    existing_sources
        .iter()
        .filter_map(|(name, url)| {
            let normalized_name = normalize_skill_name(name)?;
            let trimmed_url = url.trim();
            if trimmed_url.is_empty() {
                None
            } else {
                Some((normalized_name, trimmed_url.to_string()))
            }
        })
        .collect()
}

fn preferred_source_repos(existing_sources: &HashMap<String, String>) -> HashSet<String> {
    existing_sources
        .values()
        .filter_map(|value| {
            normalize_source(value).or_else(|| {
                extract_github_source_from_url(value).and_then(|source| normalize_source(&source))
            })
        })
        .collect()
}

fn load_exact_source_candidates(
    conn: &Connection,
    normalized_names: &[String],
) -> Result<HashMap<String, Vec<ResolveSourceCandidate>>> {
    let mut candidates: HashMap<String, Vec<ResolveSourceCandidate>> = HashMap::new();
    if normalized_names.is_empty() {
        return Ok(candidates);
    }

    let placeholders = (1..=normalized_names.len())
        .map(|index| format!("?{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT name, source, git_url, installs
         FROM marketplace_skill
         WHERE name IN ({placeholders})
           AND git_url <> ''
         ORDER BY name ASC, installs DESC, source ASC"
    );
    let mut stmt = conn
        .prepare(&sql)
        .context("Failed to prepare marketplace source-resolution query")?;

    let params: Vec<&dyn rusqlite::types::ToSql> = normalized_names
        .iter()
        .map(|name| name as &dyn rusqlite::types::ToSql)
        .collect();
    let rows = stmt
        .query_map(params.as_slice(), |row| {
            Ok((
                row.get::<_, String>(0)?,
                ResolveSourceCandidate {
                    source: row.get(1)?,
                    git_url: row.get(2)?,
                    installs: row.get::<_, i64>(3)?.max(0) as u32,
                },
            ))
        })
        .context("Failed to read marketplace source-resolution rows")?;

    for row in rows {
        let (name, candidate) = row.context("Failed to decode marketplace source candidate")?;
        candidates.entry(name).or_default().push(candidate);
    }

    Ok(candidates)
}

fn unique_top_install_candidate<'a>(
    candidates: &[&'a ResolveSourceCandidate],
) -> Option<&'a ResolveSourceCandidate> {
    let mut sorted = candidates.to_vec();
    sorted.sort_by(|left, right| {
        right
            .installs
            .cmp(&left.installs)
            .then_with(|| left.source.cmp(&right.source))
    });

    let top = sorted.first().copied()?;
    let next = sorted.get(1).copied();
    if next.is_none_or(|candidate| candidate.installs < top.installs) {
        Some(top)
    } else {
        None
    }
}

fn choose_source_candidate(
    candidates: Option<&[ResolveSourceCandidate]>,
    preferred_repos: &HashSet<String>,
) -> Option<String> {
    let candidates = candidates?;
    if candidates.is_empty() {
        return None;
    }
    if candidates.len() == 1 {
        return Some(candidates[0].git_url.clone());
    }

    let preferred: Vec<&ResolveSourceCandidate> = candidates
        .iter()
        .filter(|candidate| preferred_repos.contains(&candidate.source))
        .collect();

    if preferred.len() == 1 {
        return Some(preferred[0].git_url.clone());
    }

    if !preferred.is_empty() {
        return unique_top_install_candidate(&preferred).map(|candidate| candidate.git_url.clone());
    }

    let all: Vec<&ResolveSourceCandidate> = candidates.iter().collect();
    unique_top_install_candidate(&all).map(|candidate| candidate.git_url.clone())
}

fn resolve_skill_sources_from_snapshot(
    conn: &Connection,
    requests: &[ResolveSkillRequest],
    named_sources: &HashMap<String, String>,
    preferred_repos: &HashSet<String>,
) -> Result<HashMap<String, String>> {
    let mut normalized_names = Vec::new();
    let mut seen_names = HashSet::new();
    for request in requests {
        if named_sources.contains_key(&request.normalized_name) {
            continue;
        }
        if seen_names.insert(request.normalized_name.clone()) {
            normalized_names.push(request.normalized_name.clone());
        }
    }

    let candidates = load_exact_source_candidates(conn, &normalized_names)?;
    let mut resolved = HashMap::new();

    for request in requests {
        if let Some(url) = named_sources.get(&request.normalized_name) {
            resolved.insert(request.original_name.clone(), url.clone());
            continue;
        }

        if let Some(url) = choose_source_candidate(
            candidates.get(&request.normalized_name).map(Vec::as_slice),
            preferred_repos,
        ) {
            resolved.insert(request.original_name.clone(), url);
        }
    }

    Ok(resolved)
}

fn unresolved_normalized_names(
    requests: &[ResolveSkillRequest],
    resolved: &HashMap<String, String>,
    named_sources: &HashMap<String, String>,
) -> Vec<String> {
    let mut unresolved = Vec::new();
    let mut seen = HashSet::new();

    for request in requests {
        if named_sources.contains_key(&request.normalized_name)
            || resolved.contains_key(&request.original_name)
        {
            continue;
        }

        if seen.insert(request.normalized_name.clone()) {
            unresolved.push(request.normalized_name.clone());
        }
    }

    unresolved
}

async fn seed_resolution_names(names: &[String]) {
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(3));
    let mut tasks = tokio::task::JoinSet::new();

    for name in names {
        let permit = match semaphore.clone().acquire_owned().await {
            Ok(permit) => permit,
            Err(err) => {
                warn!(
                    target: "marketplace_snapshot",
                    error = %err,
                    "failed to acquire source-resolution permit"
                );
                break;
            }
        };
        let name = name.clone();
        tasks.spawn(async move {
            let _permit = permit;
            let result = seed_search_results(&name, RESOLVE_SOURCE_REMOTE_LIMIT).await;
            (name, result)
        });
    }

    while let Some(result) = tasks.join_next().await {
        match result {
            Ok((name, Err(err))) => {
                warn!(
                    target: "marketplace_snapshot",
                    name = %name,
                    error = %err,
                    "failed to seed source resolution"
                );
            }
            Ok((_name, Ok(()))) => {}
            Err(err) => {
                warn!(target: "marketplace_snapshot", error = %err, "source-resolution task failed");
            }
        }
    }
}

fn remote_source_candidates(
    market_result: MarketplaceResult,
    normalized_name: &str,
) -> Vec<ResolveSourceCandidate> {
    let mut seen_sources = HashSet::new();
    let mut candidates = Vec::new();

    for skill in market_result.skills {
        let Some(skill_name) = normalize_skill_name(&skill.name) else {
            continue;
        };
        if skill_name != normalized_name || skill.git_url.trim().is_empty() {
            continue;
        }

        let Some(source) = skill
            .source
            .as_deref()
            .and_then(normalize_source)
            .or_else(|| {
                extract_github_source_from_url(&skill.git_url)
                    .and_then(|value| normalize_source(&value))
            })
        else {
            continue;
        };

        if seen_sources.insert(source.clone()) {
            candidates.push(ResolveSourceCandidate {
                source,
                git_url: skill.git_url,
                installs: skill.stars,
            });
        }
    }

    candidates.sort_by(|left, right| {
        right
            .installs
            .cmp(&left.installs)
            .then_with(|| left.source.cmp(&right.source))
    });
    candidates
}

async fn resolve_skill_sources_remote_fallback(
    requests: &[ResolveSkillRequest],
    named_sources: &HashMap<String, String>,
    preferred_repos: &HashSet<String>,
) -> Result<HashMap<String, String>> {
    let mut resolved = HashMap::new();
    for request in requests {
        if let Some(url) = named_sources.get(&request.normalized_name) {
            resolved.insert(request.original_name.clone(), url.clone());
        }
    }

    let unresolved = unresolved_normalized_names(requests, &resolved, named_sources);
    if unresolved.is_empty() {
        return Ok(resolved);
    }

    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(3));
    let mut tasks = tokio::task::JoinSet::new();

    for name in unresolved {
        let permit =
            semaphore.clone().acquire_owned().await.map_err(|err| {
                anyhow!("Failed to acquire remote source-resolution permit: {err}")
            })?;
        tasks.spawn(async move {
            let _permit = permit;
            let result = remote::search_skills_sh(&name, RESOLVE_SOURCE_REMOTE_LIMIT).await;
            (name, result)
        });
    }

    let mut candidates_by_name: HashMap<String, Vec<ResolveSourceCandidate>> = HashMap::new();
    while let Some(result) = tasks.join_next().await {
        match result {
            Ok((name, Ok(market_result))) => {
                let candidates = remote_source_candidates(market_result, &name);
                if !candidates.is_empty() {
                    candidates_by_name.insert(name, candidates);
                }
            }
            Ok((name, Err(err))) => {
                warn!(
                    target: "marketplace_snapshot",
                    name = %name,
                    error = %err,
                    "remote source fallback failed"
                );
            }
            Err(err) => {
                warn!(target: "marketplace_snapshot", error = %err, "remote source fallback task join error");
            }
        }
    }

    for request in requests {
        if resolved.contains_key(&request.original_name) {
            continue;
        }

        if let Some(url) = choose_source_candidate(
            candidates_by_name
                .get(&request.normalized_name)
                .map(Vec::as_slice),
            preferred_repos,
        ) {
            resolved.insert(request.original_name.clone(), url);
        }
    }

    Ok(resolved)
}

pub async fn resolve_skill_sources_local_first(
    names: &[String],
    existing_sources: &HashMap<String, String>,
) -> Result<HashMap<String, String>> {
    let requests = normalize_resolve_requests(names);
    if requests.is_empty() {
        return Ok(HashMap::new());
    }

    let named_sources = existing_named_sources(existing_sources);
    let preferred_repos = preferred_source_repos(existing_sources);

    let initial = with_conn(|conn| {
        resolve_skill_sources_from_snapshot(conn, &requests, &named_sources, &preferred_repos)
    });

    let mut resolved = match initial {
        Ok(resolved) => resolved,
        Err(err) => {
            warn!(target: "marketplace_snapshot", error = %err, "source resolution local read failed");
            return resolve_skill_sources_remote_fallback(
                &requests,
                &named_sources,
                &preferred_repos,
            )
            .await;
        }
    };

    let unresolved = unresolved_normalized_names(&requests, &resolved, &named_sources);
    if unresolved.is_empty() {
        return Ok(resolved);
    }

    seed_resolution_names(&unresolved).await;

    match with_conn(|conn| {
        resolve_skill_sources_from_snapshot(conn, &requests, &named_sources, &preferred_repos)
    }) {
        Ok(after_seed) => {
            resolved.extend(after_seed);
            Ok(resolved)
        }
        Err(err) => {
            warn!(
                target: "marketplace_snapshot",
                error = %err,
                "source resolution local re-read failed after seed"
            );
            resolve_skill_sources_remote_fallback(&requests, &named_sources, &preferred_repos).await
        }
    }
}

fn build_fts_query(query: &str) -> Option<String> {
    let tokens = tokenize_query(query);
    if tokens.is_empty() {
        None
    } else {
        Some(
            tokens
                .into_iter()
                .map(|token| format!("{token}*"))
                .collect::<Vec<_>>()
                .join(" OR "),
        )
    }
}

fn tokenize_query(query: &str) -> Vec<String> {
    let mut cleaned = String::with_capacity(query.len());
    for ch in query.chars() {
        if ch.is_alphanumeric() {
            cleaned.push(ch.to_ascii_lowercase());
        } else {
            cleaned.push(' ');
        }
    }

    cleaned
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .map(|token| token.trim_matches('/').to_string())
        .filter(|token| !token.is_empty())
        .collect()
}

fn refresh_fts_entry_in_tx(tx: &Transaction<'_>, skill_key: &str) -> Result<()> {
    let row = tx
        .query_row(
            "SELECT
                s.skill_key,
                s.name,
                s.description,
                COALESCE(d.summary, ''),
                COALESCE(s.publisher_name, ''),
                COALESCE(s.repo_name, '')
             FROM marketplace_skill s
             LEFT JOIN marketplace_skill_detail d ON d.skill_key = s.skill_key
             WHERE s.skill_key = ?1",
            [skill_key],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )
        .optional()
        .context("Failed to load marketplace skill for FTS refresh")?;

    tx.execute(
        "DELETE FROM marketplace_skill_fts WHERE skill_key = ?1",
        [skill_key],
    )
    .context("Failed to delete marketplace FTS entry")?;

    if let Some((skill_key, name, description, summary, publisher_name, repo_name)) = row {
        tx.execute(
            "INSERT INTO marketplace_skill_fts (
                skill_key,
                name,
                description,
                summary,
                publisher_name,
                repo_name
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                skill_key,
                name,
                description,
                summary,
                publisher_name,
                repo_name
            ],
        )
        .context("Failed to insert marketplace FTS entry")?;
    }

    Ok(())
}

// ── Pack seeding ──────────────────────────────────────────────────

/// Public type returned by pack search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplacePack {
    pub pack_key: String,
    pub source: String,
    pub name: String,
    pub description: String,
    pub skill_count: i64,
    pub author: Option<String>,
    pub git_url: String,
    pub installs: i64,
}

/// Insert or update a pack entry and link its skills.
/// Called after syncing a repo that contains a skillpack.toml.
pub fn upsert_pack(
    pack_key: &str,
    source: &str,
    name: &str,
    description: &str,
    author: Option<&str>,
    git_url: &str,
    skill_keys: &[(String, String)], // (skill_key, skill_name)
) -> Result<()> {
    with_conn(|conn| {
        let tx = conn
            .unchecked_transaction()
            .context("Failed to start pack upsert transaction")?;

        tx.execute(
            "INSERT INTO marketplace_pack (
                pack_key, source, name, description, skill_count, author, git_url, installs, last_seen_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8)
            ON CONFLICT(pack_key) DO UPDATE SET
                source = excluded.source,
                name = excluded.name,
                description = CASE WHEN excluded.description <> '' THEN excluded.description ELSE marketplace_pack.description END,
                skill_count = excluded.skill_count,
                author = COALESCE(excluded.author, marketplace_pack.author),
                git_url = excluded.git_url,
                last_seen_at = excluded.last_seen_at",
            params![
                pack_key,
                source,
                name,
                description,
                skill_keys.len() as i64,
                author,
                git_url,
                now_rfc3339(),
            ],
        )
        .context("Failed to upsert marketplace pack")?;

        // Clear old skill links and re-insert
        tx.execute(
            "DELETE FROM marketplace_pack_skill WHERE pack_key = ?1",
            [pack_key],
        )
        .context("Failed to clear pack skill links")?;

        for (skill_key, skill_name) in skill_keys {
            tx.execute(
                "INSERT OR IGNORE INTO marketplace_pack_skill (pack_key, skill_key, skill_name)
                 VALUES (?1, ?2, ?3)",
                params![pack_key, skill_key, skill_name],
            )
            .context("Failed to insert pack skill link")?;
        }

        // Refresh pack FTS
        tx.execute(
            "DELETE FROM marketplace_pack_fts WHERE pack_key = ?1",
            [pack_key],
        )
        .context("Failed to delete pack FTS entry")?;

        tx.execute(
            "INSERT INTO marketplace_pack_fts (pack_key, name, description, author)
             VALUES (?1, ?2, ?3, ?4)",
            params![pack_key, name, description, author.unwrap_or("")],
        )
        .context("Failed to insert pack FTS entry")?;

        tx.commit().context("Failed to commit pack upsert")?;
        Ok(())
    })
}

/// Search for packs matching a query string.
pub fn search_packs_local(query: &str, limit: u32) -> Result<Vec<MarketplacePack>> {
    let limit = (limit.max(1)).min(50) as i64;
    let normalized = query.trim().to_ascii_lowercase();

    if normalized.is_empty() {
        return list_packs_local(limit as u32);
    }

    let Some(fts_query) = build_fts_query(&normalized) else {
        return Ok(Vec::new());
    };

    with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT p.pack_key, p.source, p.name, p.description, p.skill_count,
                    p.author, p.git_url, p.installs
             FROM marketplace_pack_fts fts
             JOIN marketplace_pack p ON p.pack_key = fts.pack_key
             WHERE marketplace_pack_fts MATCH ?1
             ORDER BY bm25(marketplace_pack_fts, 10.0, 4.0, 1.0) ASC, p.installs DESC
             LIMIT ?2",
        )?;

        let packs = stmt
            .query_map(params![fts_query, limit], |row| {
                Ok(MarketplacePack {
                    pack_key: row.get(0)?,
                    source: row.get(1)?,
                    name: row.get(2)?,
                    description: row.get(3)?,
                    skill_count: row.get(4)?,
                    author: row.get(5)?,
                    git_url: row.get(6)?,
                    installs: row.get(7)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(packs)
    })
}

/// List all known packs, ordered by installs descending.
pub fn list_packs_local(limit: u32) -> Result<Vec<MarketplacePack>> {
    let limit = (limit.max(1)).min(50) as i64;
    with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT pack_key, source, name, description, skill_count,
                    author, git_url, installs
             FROM marketplace_pack
             ORDER BY installs DESC, name ASC
             LIMIT ?1",
        )?;

        let packs = stmt
            .query_map([limit], |row| {
                Ok(MarketplacePack {
                    pack_key: row.get(0)?,
                    source: row.get(1)?,
                    name: row.get(2)?,
                    description: row.get(3)?,
                    skill_count: row.get(4)?,
                    author: row.get(5)?,
                    git_url: row.get(6)?,
                    installs: row.get(7)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(packs)
    })
}

fn upsert_skill_in_tx(
    tx: &Transaction<'_>,
    skill: &Skill,
    synced_at: &str,
) -> Result<Option<String>> {
    let source = skill
        .source
        .as_deref()
        .and_then(normalize_source)
        .or_else(|| {
            extract_github_source_from_url(&skill.git_url)
                .and_then(|value| normalize_source(&value))
        });
    let Some(source) = source else {
        return Ok(None);
    };

    let Some(name) = normalize_skill_name(&skill.name) else {
        return Ok(None);
    };
    let Some(skill_key) = build_skill_key(&source, &name) else {
        return Ok(None);
    };
    let (publisher_name, repo_name) = split_source(&source);
    let git_url = if skill.git_url.trim().is_empty() {
        format!("https://github.com/{source}")
    } else {
        skill.git_url.clone()
    };
    let author = skill.author.clone().unwrap_or_else(|| source.clone());
    let installs = skill.stars as i64;

    tx.execute(
        "INSERT INTO marketplace_skill (
            skill_key,
            source,
            name,
            git_url,
            author,
            publisher_name,
            repo_name,
            description,
            installs,
            last_seen_remote_at,
            last_list_sync_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)
        ON CONFLICT(skill_key) DO UPDATE SET
            source = excluded.source,
            name = excluded.name,
            git_url = excluded.git_url,
            author = excluded.author,
            publisher_name = excluded.publisher_name,
            repo_name = excluded.repo_name,
            description = CASE
                WHEN excluded.description <> '' THEN excluded.description
                ELSE marketplace_skill.description
            END,
            installs = MAX(marketplace_skill.installs, excluded.installs),
            last_seen_remote_at = excluded.last_seen_remote_at,
            last_list_sync_at = excluded.last_list_sync_at",
        params![
            skill_key,
            source,
            name,
            git_url,
            author,
            publisher_name,
            repo_name,
            skill.description,
            installs,
            synced_at
        ],
    )
    .context("Failed to upsert marketplace skill snapshot row")?;

    refresh_fts_entry_in_tx(tx, &skill_key)?;
    Ok(Some(skill_key))
}

fn upsert_skill_identity_in_tx(
    tx: &Transaction<'_>,
    source: &str,
    name: &str,
    installs: u32,
    synced_at: &str,
) -> Result<Option<String>> {
    let source = match normalize_source(source) {
        Some(value) => value,
        None => return Ok(None),
    };
    let name = match normalize_skill_name(name) {
        Some(value) => value,
        None => return Ok(None),
    };
    let (publisher_name, repo_name) = split_source(&source);
    let skill_key = build_skill_key(&source, &name).expect("normalized key");
    let git_url = format!("https://github.com/{source}");

    tx.execute(
        "INSERT INTO marketplace_skill (
            skill_key,
            source,
            name,
            git_url,
            author,
            publisher_name,
            repo_name,
            description,
            installs,
            last_seen_remote_at,
            last_list_sync_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, '', ?8, ?9, ?9)
        ON CONFLICT(skill_key) DO UPDATE SET
            source = excluded.source,
            name = excluded.name,
            git_url = excluded.git_url,
            author = excluded.author,
            publisher_name = excluded.publisher_name,
            repo_name = excluded.repo_name,
            installs = MAX(marketplace_skill.installs, excluded.installs),
            last_seen_remote_at = excluded.last_seen_remote_at,
            last_list_sync_at = excluded.last_list_sync_at",
        params![
            skill_key,
            source,
            name,
            git_url,
            source,
            publisher_name,
            repo_name,
            installs as i64,
            synced_at
        ],
    )
    .context("Failed to upsert marketplace repo-skill identity row")?;

    refresh_fts_entry_in_tx(tx, &skill_key)?;
    Ok(Some(skill_key))
}

fn upsert_detail_in_tx(
    tx: &Transaction<'_>,
    source: &str,
    name: &str,
    details: &MarketplaceSkillDetails,
    synced_at: &str,
) -> Result<()> {
    let Some(skill_key) = build_skill_key(source, name) else {
        return Ok(());
    };

    let audits_json = serde_json::to_string(&details.security_audits)
        .context("Failed to serialize marketplace security audits")?;
    tx.execute(
        "INSERT INTO marketplace_skill_detail (
            skill_key,
            summary,
            readme,
            weekly_installs,
            github_stars,
            first_seen,
            security_audits_json,
            last_detail_sync_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        ON CONFLICT(skill_key) DO UPDATE SET
            summary = excluded.summary,
            readme = excluded.readme,
            weekly_installs = excluded.weekly_installs,
            github_stars = excluded.github_stars,
            first_seen = excluded.first_seen,
            security_audits_json = excluded.security_audits_json,
            last_detail_sync_at = excluded.last_detail_sync_at",
        params![
            skill_key,
            details.summary,
            details.readme,
            details.weekly_installs,
            details.github_stars.map(|value| value as i64),
            details.first_seen,
            audits_json,
            synced_at
        ],
    )
    .context("Failed to upsert marketplace detail snapshot row")?;
    refresh_fts_entry_in_tx(tx, &skill_key)?;
    Ok(())
}

fn delete_listing_scope_in_tx(tx: &Transaction<'_>, scope: &str) -> Result<()> {
    tx.execute(
        "DELETE FROM marketplace_listing WHERE listing_type = ?1",
        [scope],
    )
    .with_context(|| format!("Failed to clear marketplace listing scope: {scope}"))?;
    Ok(())
}

fn cleanup_stale_skills_in_tx(tx: &Transaction<'_>) -> Result<()> {
    let installed_markers = installed_markers();
    let cutoff = (Utc::now() - Duration::days(STALE_SKILL_RETENTION_DAYS)).to_rfc3339();

    let mut stmt = tx
        .prepare(
            "SELECT skill_key, name
             FROM marketplace_skill
             WHERE last_seen_remote_at IS NOT NULL
               AND last_seen_remote_at < ?1",
        )
        .context("Failed to prepare stale marketplace skill query")?;

    let rows = stmt
        .query_map([cutoff], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .context("Failed to scan stale marketplace skills")?;

    for row in rows {
        let (skill_key, name) = row.context("Failed to decode stale marketplace skill row")?;
        if installed_markers.contains(&skill_key)
            || installed_markers.contains(&name.to_ascii_lowercase())
        {
            continue;
        }

        tx.execute(
            "DELETE FROM marketplace_listing WHERE skill_key = ?1",
            [skill_key.as_str()],
        )
        .context("Failed to delete stale marketplace listing rows")?;
        tx.execute(
            "DELETE FROM marketplace_repo_skill WHERE skill_key = ?1",
            [skill_key.as_str()],
        )
        .context("Failed to delete stale marketplace repo-skill rows")?;
        tx.execute(
            "DELETE FROM marketplace_skill_detail WHERE skill_key = ?1",
            [skill_key.as_str()],
        )
        .context("Failed to delete stale marketplace detail row")?;
        tx.execute(
            "DELETE FROM marketplace_skill_fts WHERE skill_key = ?1",
            [skill_key.as_str()],
        )
        .context("Failed to delete stale marketplace FTS row")?;
        tx.execute(
            "DELETE FROM marketplace_skill WHERE skill_key = ?1",
            [skill_key.as_str()],
        )
        .context("Failed to delete stale marketplace skill row")?;
    }

    Ok(())
}

async fn apply_installed_state(mut skills: Vec<Skill>) -> Vec<Skill> {
    let installed_skills = match load_installed_skills().await {
        Ok(skills) => skills,
        Err(err) => {
            warn!(target: "marketplace_snapshot", error = %err, "failed to load installed snapshot");
            return skills;
        }
    };

    let mut by_key = HashMap::new();
    let mut by_name = HashMap::new();
    for skill in installed_skills {
        let state = InstalledSkillState {
            installed: true,
            update_available: skill.update_available,
            skill_type: skill.skill_type.clone(),
            tree_hash: skill.tree_hash.clone(),
            agent_links: skill.agent_links.clone(),
        };

        if let Some(source) = skill.source.as_deref() {
            if let Some(skill_key) = build_skill_key(source, &skill.name) {
                by_key.insert(skill_key, state.clone());
            }
        }
        by_name.insert(skill.name.to_ascii_lowercase(), state);
    }

    for skill in &mut skills {
        let skill_key = skill
            .source
            .as_deref()
            .and_then(|source| build_skill_key(source, &skill.name));
        let state = skill_key
            .as_deref()
            .and_then(|key| by_key.get(key))
            .or_else(|| by_name.get(&skill.name.to_ascii_lowercase()));

        if let Some(state) = state {
            skill.installed = state.installed;
            skill.update_available = state.update_available;
            skill.skill_type = state.skill_type.clone();
            skill.tree_hash = state.tree_hash.clone();
            skill.agent_links = state.agent_links.clone();
        }
    }

    skills
}

fn empty_details() -> MarketplaceSkillDetails {
    MarketplaceSkillDetails {
        summary: None,
        readme: None,
        weekly_installs: None,
        github_stars: None,
        first_seen: None,
        security_audits: Vec::new(),
    }
}

pub async fn get_leaderboard_local(category: &str) -> Result<LocalFirstResult<Vec<Skill>>> {
    let scope = leaderboard_scope(category);
    let local = with_conn(|conn| {
        let data = load_leaderboard_snapshot(conn, &scope)?;
        let seed_state = sync_seed_state(conn, &scope)?;
        let fresh = is_scope_fresh_conn(conn, &scope)?;
        let updated_at = scope_updated_at(conn, &scope)?;
        Ok((data, seed_state, fresh, updated_at))
    });

    match local {
        Ok((data, _, fresh, updated_at)) if !data.is_empty() => {
            let data = apply_installed_state(data).await;
            Ok(LocalFirstResult {
                data,
                snapshot_status: if fresh {
                    SnapshotStatus::Fresh
                } else {
                    SnapshotStatus::Stale
                },
                snapshot_updated_at: updated_at,
            })
        }
        Ok((_, ScopeSeedState::Synced, _, updated_at)) => Ok(LocalFirstResult {
            data: Vec::new(),
            snapshot_status: SnapshotStatus::Miss,
            snapshot_updated_at: updated_at,
        }),
        Ok((_, ScopeSeedState::NeverSynced, _, _)) => {
            if sync_scope_leaderboard(category).await.is_ok() {
                let reseeded = with_conn(|conn| {
                    let data = load_leaderboard_snapshot(conn, &scope)?;
                    let updated_at = scope_updated_at(conn, &scope)?;
                    Ok((data, updated_at))
                })?;
                return Ok(LocalFirstResult {
                    data: apply_installed_state(reseeded.0).await,
                    snapshot_status: SnapshotStatus::Seeding,
                    snapshot_updated_at: reseeded.1,
                });
            }

            Ok(LocalFirstResult {
                data: Vec::new(),
                snapshot_status: SnapshotStatus::RemoteError,
                snapshot_updated_at: None,
            })
        }
        Err(err) => {
            warn!(target: "marketplace_snapshot", error = %err, "leaderboard local read failed");
            match remote::get_skills_sh_leaderboard(category).await {
                Ok(skills) => Ok(LocalFirstResult {
                    data: apply_installed_state(skills).await,
                    snapshot_status: SnapshotStatus::ErrorFallback,
                    snapshot_updated_at: None,
                }),
                Err(_remote_err) => Ok(LocalFirstResult {
                    data: Vec::new(),
                    snapshot_status: SnapshotStatus::RemoteError,
                    snapshot_updated_at: None,
                }),
            }
        }
    }
}

pub async fn search_local(query: &str, limit: Option<u32>) -> Result<LocalFirstResult<Vec<Skill>>> {
    let limit = limit.unwrap_or(50).clamp(1, 200);
    let local = with_conn(|conn| {
        let (data, updated_at) = load_search_snapshot(conn, query, limit)?;
        let has_any = any_skill_rows(conn)?;
        Ok((data, updated_at, has_any))
    });

    match local {
        Ok((data, updated_at, _)) if !data.is_empty() => Ok(LocalFirstResult {
            data: apply_installed_state(data).await,
            snapshot_status: SnapshotStatus::Fresh,
            snapshot_updated_at: updated_at,
        }),
        Ok((_, updated_at, true)) => Ok(LocalFirstResult {
            data: Vec::new(),
            snapshot_status: SnapshotStatus::Miss,
            snapshot_updated_at: updated_at,
        }),
        Ok((_, _, false)) => {
            if seed_search_results(query, limit).await.is_ok() {
                let reseeded = with_conn(|conn| load_search_snapshot(conn, query, limit))?;
                return Ok(LocalFirstResult {
                    data: apply_installed_state(reseeded.0).await,
                    snapshot_status: SnapshotStatus::Seeding,
                    snapshot_updated_at: reseeded.1,
                });
            }
            Ok(LocalFirstResult {
                data: Vec::new(),
                snapshot_status: SnapshotStatus::RemoteError,
                snapshot_updated_at: None,
            })
        }
        Err(err) => {
            warn!(target: "marketplace_snapshot", error = %err, "search local read failed");
            match remote::search_skills_sh(query, limit).await {
                Ok(result) => Ok(LocalFirstResult {
                    data: apply_installed_state(result.skills).await,
                    snapshot_status: SnapshotStatus::ErrorFallback,
                    snapshot_updated_at: None,
                }),
                Err(_) => Ok(LocalFirstResult {
                    data: Vec::new(),
                    snapshot_status: SnapshotStatus::RemoteError,
                    snapshot_updated_at: None,
                }),
            }
        }
    }
}

pub async fn get_publishers_local() -> Result<LocalFirstResult<Vec<OfficialPublisher>>> {
    let scope = "official_publishers";
    let local: Result<(Vec<OfficialPublisher>, ScopeSeedState, bool, Option<String>)> =
        with_conn(|conn| {
            let data = load_publishers_snapshot(conn)?;
            let seed_state = sync_seed_state(conn, scope)?;
            let fresh = is_scope_fresh_conn(conn, scope)?;
            let updated_at = scope_updated_at(conn, scope)?;
            Ok((data, seed_state, fresh, updated_at))
        });

    match local {
        Ok((data, _, fresh, updated_at)) if !data.is_empty() => Ok(LocalFirstResult {
            data,
            snapshot_status: if fresh {
                SnapshotStatus::Fresh
            } else {
                SnapshotStatus::Stale
            },
            snapshot_updated_at: updated_at,
        }),
        Ok((_, ScopeSeedState::Synced, _, updated_at)) => Ok(LocalFirstResult {
            data: Vec::new(),
            snapshot_status: SnapshotStatus::Miss,
            snapshot_updated_at: updated_at,
        }),
        Ok((_, ScopeSeedState::NeverSynced, _, _)) => {
            if sync_scope_publishers().await.is_ok() {
                let reseeded: (Vec<OfficialPublisher>, Option<String>) = with_conn(|conn| {
                    let data = load_publishers_snapshot(conn)?;
                    let updated_at = scope_updated_at(conn, scope)?;
                    Ok((data, updated_at))
                })?;
                return Ok(LocalFirstResult {
                    data: reseeded.0,
                    snapshot_status: SnapshotStatus::Seeding,
                    snapshot_updated_at: reseeded.1,
                });
            }
            Ok(LocalFirstResult {
                data: Vec::new(),
                snapshot_status: SnapshotStatus::RemoteError,
                snapshot_updated_at: None,
            })
        }
        Err(err) => {
            warn!(target: "marketplace_snapshot", error = %err, "publishers local read failed");
            match remote::get_official_publishers().await {
                Ok(publishers) => Ok(LocalFirstResult {
                    data: publishers,
                    snapshot_status: SnapshotStatus::ErrorFallback,
                    snapshot_updated_at: None,
                }),
                Err(_) => Ok(LocalFirstResult {
                    data: Vec::new(),
                    snapshot_status: SnapshotStatus::RemoteError,
                    snapshot_updated_at: None,
                }),
            }
        }
    }
}

pub async fn get_publisher_repos_local(
    publisher_name: &str,
) -> Result<LocalFirstResult<Vec<PublisherRepo>>> {
    let publisher_name = publisher_name.trim().to_ascii_lowercase();
    let scope = format!("publisher_repos:{publisher_name}");
    let local: Result<(Vec<PublisherRepo>, ScopeSeedState, bool, Option<String>)> =
        with_conn(|conn| {
            let data = load_publisher_repos_snapshot(conn, &publisher_name)?;
            let seed_state = sync_seed_state(conn, &scope)?;
            let fresh = is_scope_fresh_conn(conn, &scope)?;
            let updated_at = scope_updated_at(conn, &scope)?;
            Ok((data, seed_state, fresh, updated_at))
        });

    match local {
        Ok((data, _, fresh, updated_at)) if !data.is_empty() => Ok(LocalFirstResult {
            data,
            snapshot_status: if fresh {
                SnapshotStatus::Fresh
            } else {
                SnapshotStatus::Stale
            },
            snapshot_updated_at: updated_at,
        }),
        Ok((_, ScopeSeedState::Synced, _, updated_at)) => Ok(LocalFirstResult {
            data: Vec::new(),
            snapshot_status: SnapshotStatus::Miss,
            snapshot_updated_at: updated_at,
        }),
        Ok((_, ScopeSeedState::NeverSynced, _, _)) => {
            if sync_scope_publisher_repos(&publisher_name).await.is_ok() {
                let reseeded: (Vec<PublisherRepo>, Option<String>) = with_conn(|conn| {
                    let data = load_publisher_repos_snapshot(conn, &publisher_name)?;
                    let updated_at = scope_updated_at(conn, &scope)?;
                    Ok((data, updated_at))
                })?;
                return Ok(LocalFirstResult {
                    data: reseeded.0,
                    snapshot_status: SnapshotStatus::Seeding,
                    snapshot_updated_at: reseeded.1,
                });
            }

            Ok(LocalFirstResult {
                data: Vec::new(),
                snapshot_status: SnapshotStatus::RemoteError,
                snapshot_updated_at: None,
            })
        }
        Err(err) => {
            warn!(target: "marketplace_snapshot", error = %err, "publisher repos local read failed");
            match remote::get_publisher_repos(&publisher_name).await {
                Ok(repos) => Ok(LocalFirstResult {
                    data: repos,
                    snapshot_status: SnapshotStatus::ErrorFallback,
                    snapshot_updated_at: None,
                }),
                Err(_) => Ok(LocalFirstResult {
                    data: Vec::new(),
                    snapshot_status: SnapshotStatus::RemoteError,
                    snapshot_updated_at: None,
                }),
            }
        }
    }
}

pub async fn get_repo_skills_local(source: &str) -> Result<LocalFirstResult<Vec<Skill>>> {
    let source = normalize_source(source).ok_or_else(|| anyhow!("Invalid repo source"))?;
    let scope = format!("repo_skills:{source}");
    let local: Result<(Vec<Skill>, ScopeSeedState, bool, Option<String>)> = with_conn(|conn| {
        let data = load_repo_skills_snapshot(conn, &source)?;
        let seed_state = sync_seed_state(conn, &scope)?;
        let fresh = is_scope_fresh_conn(conn, &scope)?;
        let updated_at = scope_updated_at(conn, &scope)?;
        Ok((data, seed_state, fresh, updated_at))
    });

    match local {
        Ok((data, _, fresh, updated_at)) if !data.is_empty() => Ok(LocalFirstResult {
            data: apply_installed_state(data).await,
            snapshot_status: if fresh {
                SnapshotStatus::Fresh
            } else {
                SnapshotStatus::Stale
            },
            snapshot_updated_at: updated_at,
        }),
        Ok((_, ScopeSeedState::Synced, _, updated_at)) => Ok(LocalFirstResult {
            data: Vec::new(),
            snapshot_status: SnapshotStatus::Miss,
            snapshot_updated_at: updated_at,
        }),
        Ok((_, ScopeSeedState::NeverSynced, _, _)) => {
            if sync_scope_repo_skills(&source).await.is_ok() {
                let reseeded: (Vec<Skill>, Option<String>) = with_conn(|conn| {
                    let data = load_repo_skills_snapshot(conn, &source)?;
                    let updated_at = scope_updated_at(conn, &scope)?;
                    Ok((data, updated_at))
                })?;
                return Ok(LocalFirstResult {
                    data: apply_installed_state(reseeded.0).await,
                    snapshot_status: SnapshotStatus::Seeding,
                    snapshot_updated_at: reseeded.1,
                });
            }

            Ok(LocalFirstResult {
                data: Vec::new(),
                snapshot_status: SnapshotStatus::RemoteError,
                snapshot_updated_at: None,
            })
        }
        Err(err) => {
            warn!(target: "marketplace_snapshot", error = %err, "repo skills local read failed");
            let (publisher_name, repo_name) = split_source(&source);
            match remote::get_publisher_repo_skills(&publisher_name, &repo_name).await {
                Ok(skills) => {
                    let data = skills
                        .into_iter()
                        .map(|skill| {
                            skill_from_snapshot_row(
                                source.clone(),
                                skill.name,
                                format!("https://github.com/{source}"),
                                Some(source.clone()),
                                String::new(),
                                skill.installs,
                                Some(now_rfc3339()),
                                None,
                            )
                        })
                        .collect();
                    Ok(LocalFirstResult {
                        data: apply_installed_state(data).await,
                        snapshot_status: SnapshotStatus::ErrorFallback,
                        snapshot_updated_at: None,
                    })
                }
                Err(_) => Ok(LocalFirstResult {
                    data: Vec::new(),
                    snapshot_status: SnapshotStatus::RemoteError,
                    snapshot_updated_at: None,
                }),
            }
        }
    }
}

pub async fn get_skill_detail_local(
    source: &str,
    name: &str,
) -> Result<LocalFirstResult<MarketplaceSkillDetails>> {
    let source = normalize_source(source).ok_or_else(|| anyhow!("Invalid skill source"))?;
    let name = normalize_skill_name(name).ok_or_else(|| anyhow!("Invalid skill name"))?;
    let scope =
        skill_detail_scope(&source, &name).ok_or_else(|| anyhow!("Invalid detail scope"))?;
    let skill_key = build_skill_key(&source, &name).expect("normalized skill detail key");

    let local = with_conn(|conn| {
        let data = load_skill_detail_snapshot(conn, &skill_key)?;
        let seed_state = sync_seed_state(conn, &scope)?;
        let fresh = is_scope_fresh_conn(conn, &scope)?;
        let updated_at = scope_updated_at(conn, &scope)?;
        Ok((data, seed_state, fresh, updated_at))
    });

    match local {
        Ok((Some(data), _, fresh, updated_at)) => Ok(LocalFirstResult {
            data,
            snapshot_status: if fresh {
                SnapshotStatus::Fresh
            } else {
                SnapshotStatus::Stale
            },
            snapshot_updated_at: updated_at,
        }),
        Ok((None, ScopeSeedState::Synced, _, updated_at)) => Ok(LocalFirstResult {
            data: empty_details(),
            snapshot_status: SnapshotStatus::Miss,
            snapshot_updated_at: updated_at,
        }),
        Ok((None, ScopeSeedState::NeverSynced, _, _)) => {
            if sync_scope_skill_detail(&source, &name).await.is_ok() {
                let reseeded = with_conn(|conn| {
                    let data = load_skill_detail_snapshot(conn, &skill_key)?;
                    let updated_at = scope_updated_at(conn, &scope)?;
                    Ok((data, updated_at))
                })?;
                return Ok(LocalFirstResult {
                    data: reseeded.0.unwrap_or_else(empty_details),
                    snapshot_status: SnapshotStatus::Seeding,
                    snapshot_updated_at: reseeded.1,
                });
            }

            Ok(LocalFirstResult {
                data: empty_details(),
                snapshot_status: SnapshotStatus::RemoteError,
                snapshot_updated_at: None,
            })
        }
        Err(err) => {
            warn!(target: "marketplace_snapshot", error = %err, "detail local read failed");
            match remote::fetch_marketplace_skill_details(&source, &name).await {
                Ok(details) => Ok(LocalFirstResult {
                    data: details,
                    snapshot_status: SnapshotStatus::ErrorFallback,
                    snapshot_updated_at: None,
                }),
                Err(_) => Ok(LocalFirstResult {
                    data: empty_details(),
                    snapshot_status: SnapshotStatus::RemoteError,
                    snapshot_updated_at: None,
                }),
            }
        }
    }
}

pub async fn ai_search_local(
    keywords: &[String],
    limit: Option<u32>,
) -> Result<LocalFirstResult<AiKeywordSearchResult>> {
    async fn load_ai_search_snapshot(
        keywords: &[String],
        limit: u32,
    ) -> Result<(Vec<Skill>, HashMap<String, Vec<String>>, Option<String>)> {
        let mut skill_map: HashMap<String, Skill> = HashMap::new();
        let mut keyword_skill_map: HashMap<String, Vec<String>> = HashMap::new();
        let mut latest_updated_at: Option<String> = None;

        for keyword in keywords {
            let local = with_conn(|conn| load_search_snapshot(conn, keyword, limit))?;
            let mut names = Vec::new();
            for skill in local.0 {
                let key = skill
                    .source
                    .as_deref()
                    .and_then(|source| build_skill_key(source, &skill.name))
                    .unwrap_or_else(|| skill.name.to_ascii_lowercase());

                names.push(skill.name.clone());
                let entry = skill_map.entry(key).or_insert_with(|| skill.clone());
                if skill.stars > entry.stars {
                    *entry = skill;
                }
            }
            if !names.is_empty() {
                keyword_skill_map.insert(keyword.clone(), names);
            }
            if latest_updated_at.as_ref().is_none_or(|current| {
                local
                    .1
                    .as_ref()
                    .is_some_and(|candidate| candidate > current)
            }) {
                latest_updated_at = local.1;
            }
        }

        let mut skills: Vec<Skill> = skill_map.into_values().collect();
        skills.sort_by(|left, right| {
            right
                .stars
                .cmp(&left.stars)
                .then_with(|| left.name.cmp(&right.name))
        });
        for (index, skill) in skills.iter_mut().enumerate() {
            skill.rank = Some((index + 1) as u32);
        }

        Ok::<(Vec<Skill>, HashMap<String, Vec<String>>, Option<String>), anyhow::Error>((
            skills,
            keyword_skill_map,
            latest_updated_at,
        ))
    }

    let keywords = normalize_keywords(keywords);
    let limit = limit.unwrap_or(50).clamp(1, 200);
    let mut snapshot_status = SnapshotStatus::Fresh;
    let mut loaded = load_ai_search_snapshot(&keywords, limit).await?;
    let snapshot_rows = with_conn(skill_row_count)?;

    if loaded.0.is_empty() && !with_conn(any_skill_rows)? {
        for keyword in &keywords {
            seed_search_results(keyword, limit).await?;
        }
        loaded = load_ai_search_snapshot(&keywords, limit).await?;
        snapshot_status = SnapshotStatus::Seeding;
    } else if !keywords.is_empty()
        && loaded.0.len() < AI_SEARCH_REMOTE_SEED_MIN_HITS
        && snapshot_rows < AI_SEARCH_LOW_COVERAGE_ROWS
    {
        for keyword in &keywords {
            seed_search_results(keyword, limit).await?;
        }
        loaded = load_ai_search_snapshot(&keywords, limit).await?;
        snapshot_status = SnapshotStatus::Seeding;
    }

    let skills = apply_installed_state(loaded.0).await;
    Ok(LocalFirstResult {
        data: AiKeywordSearchResult {
            total_count: skills.len() as u32,
            skills,
            keyword_skill_map: loaded.1,
        },
        snapshot_status: if keywords.is_empty() {
            SnapshotStatus::Miss
        } else {
            snapshot_status
        },
        snapshot_updated_at: loaded.2,
    })
}

fn normalize_keywords(keywords: &[String]) -> Vec<String> {
    keywords
        .iter()
        .map(|keyword| keyword.trim())
        .filter(|keyword| !keyword.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

pub async fn sync_marketplace_scope(scope: &str) -> Result<()> {
    match parse_scope(scope)? {
        ScopeSpec::Leaderboard { category } => sync_scope_leaderboard(&category).await,
        ScopeSpec::OfficialPublishers => sync_scope_publishers().await,
        ScopeSpec::PublisherRepos { publisher_name } => {
            sync_scope_publisher_repos(&publisher_name).await
        }
        ScopeSpec::RepoSkills { source } => sync_scope_repo_skills(&source).await,
        ScopeSpec::SkillDetail { source, name } => sync_scope_skill_detail(&source, &name).await,
        ScopeSpec::SearchSeed { query } => seed_search_results(&query, SEARCH_SEED_LIMIT).await,
    }
}

pub async fn sync_scope_leaderboard(category: &str) -> Result<()> {
    let scope = leaderboard_scope(category);
    mark_scope_attempt(&scope)?;
    let synced_at = now_rfc3339();
    let skills = remote::get_skills_sh_leaderboard(category)
        .await
        .with_context(|| format!("Failed to fetch remote leaderboard: {category}"))?;

    let write_result: Result<()> = with_conn(|conn| {
        let tx = conn
            .unchecked_transaction()
            .context("Failed to start leaderboard snapshot transaction")?;

        mark_scope_attempt_in_tx(&tx, &scope)?;
        delete_listing_scope_in_tx(&tx, &scope)?;

        for (index, skill) in skills.iter().enumerate() {
            if let Some(skill_key) = upsert_skill_in_tx(&tx, skill, &synced_at)? {
                tx.execute(
                    "INSERT INTO marketplace_listing (listing_type, skill_key, rank, updated_at)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![scope, skill_key, (index + 1) as i64, synced_at],
                )
                .context("Failed to insert marketplace listing row")?;
            }
        }

        cleanup_stale_skills_in_tx(&tx)?;
        mark_scope_success_in_tx(&tx, &scope)?;
        tx.commit()
            .context("Failed to commit leaderboard snapshot transaction")?;
        Ok(())
    });

    if let Err(err) = write_result {
        let _ = mark_scope_error(&scope, &err.to_string());
        return Err(err);
    }

    Ok(())
}

pub async fn sync_scope_publishers() -> Result<()> {
    let scope = "official_publishers";
    mark_scope_attempt(scope)?;
    let synced_at = now_rfc3339();
    let publishers = remote::get_official_publishers()
        .await
        .context("Failed to fetch remote official publishers")?;

    let write_result: Result<()> = with_conn(|conn| {
        let tx = conn
            .unchecked_transaction()
            .context("Failed to start publisher snapshot transaction")?;

        mark_scope_attempt_in_tx(&tx, scope)?;
        tx.execute("DELETE FROM marketplace_publisher", [])
            .context("Failed to clear publisher snapshot table")?;

        for publisher in &publishers {
            tx.execute(
                "INSERT INTO marketplace_publisher (
                    publisher_name,
                    repo_count,
                    skill_count,
                    url,
                    updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    publisher.name.to_ascii_lowercase(),
                    publisher.repo_count as i64,
                    publisher.skill_count as i64,
                    publisher.url,
                    synced_at
                ],
            )
            .context("Failed to upsert publisher snapshot row")?;
        }

        mark_scope_success_in_tx(&tx, scope)?;
        tx.commit()
            .context("Failed to commit publisher snapshot transaction")?;
        Ok(())
    });

    if let Err(err) = write_result {
        let _ = mark_scope_error(scope, &err.to_string());
        return Err(err);
    }

    Ok(())
}

pub async fn sync_scope_publisher_repos(publisher_name: &str) -> Result<()> {
    let publisher_name = publisher_name.trim().to_ascii_lowercase();
    let scope = format!("publisher_repos:{publisher_name}");
    mark_scope_attempt(&scope)?;
    let synced_at = now_rfc3339();
    let repos = remote::get_publisher_repos(&publisher_name)
        .await
        .with_context(|| format!("Failed to fetch repos for publisher {publisher_name}"))?;

    let write_result: Result<()> = with_conn(|conn| {
        let tx = conn
            .unchecked_transaction()
            .context("Failed to start publisher-repo snapshot transaction")?;

        mark_scope_attempt_in_tx(&tx, &scope)?;
        tx.execute(
            "DELETE FROM marketplace_repo WHERE publisher_name = ?1",
            [publisher_name.as_str()],
        )
        .context("Failed to clear publisher repo snapshot rows")?;

        for repo in &repos {
            tx.execute(
                "INSERT INTO marketplace_repo (
                    source,
                    publisher_name,
                    repo_name,
                    skill_count,
                    installs,
                    installs_label,
                    url,
                    updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                ON CONFLICT(source) DO UPDATE SET
                    publisher_name = excluded.publisher_name,
                    repo_name = excluded.repo_name,
                    skill_count = excluded.skill_count,
                    installs = excluded.installs,
                    installs_label = excluded.installs_label,
                    url = excluded.url,
                    updated_at = excluded.updated_at",
                params![
                    repo.source.to_ascii_lowercase(),
                    publisher_name,
                    repo.repo.to_ascii_lowercase(),
                    repo.skill_count as i64,
                    repo.installs as i64,
                    repo.installs_label,
                    repo.url,
                    synced_at
                ],
            )
            .context("Failed to upsert publisher repo snapshot row")?;

            if !repo.skills.is_empty() {
                tx.execute(
                    "DELETE FROM marketplace_repo_skill WHERE source = ?1",
                    [repo.source.to_ascii_lowercase()],
                )
                .context("Failed to clear embedded repo-skill snapshot rows")?;

                for (index, skill) in repo.skills.iter().enumerate() {
                    if let Some(skill_key) = upsert_skill_identity_in_tx(
                        &tx,
                        &repo.source,
                        &skill.name,
                        skill.installs,
                        &synced_at,
                    )? {
                        tx.execute(
                            "INSERT INTO marketplace_repo_skill (
                                source,
                                skill_key,
                                installs,
                                rank,
                                updated_at
                            ) VALUES (?1, ?2, ?3, ?4, ?5)
                            ON CONFLICT(source, skill_key) DO UPDATE SET
                                installs = excluded.installs,
                                rank = excluded.rank,
                                updated_at = excluded.updated_at",
                            params![
                                repo.source.to_ascii_lowercase(),
                                skill_key,
                                skill.installs as i64,
                                (index + 1) as i64,
                                synced_at
                            ],
                        )
                        .context("Failed to upsert embedded repo-skill snapshot row")?;
                    }
                }
            }
        }

        mark_scope_success_in_tx(&tx, &scope)?;
        tx.commit()
            .context("Failed to commit publisher-repo snapshot transaction")?;
        Ok(())
    });

    if let Err(err) = write_result {
        let _ = mark_scope_error(&scope, &err.to_string());
        return Err(err);
    }

    Ok(())
}

pub async fn sync_scope_repo_skills(source: &str) -> Result<()> {
    let source = normalize_source(source).ok_or_else(|| anyhow!("Invalid repo source"))?;
    let scope = format!("repo_skills:{source}");
    mark_scope_attempt(&scope)?;
    let synced_at = now_rfc3339();
    let (publisher_name, repo_name) = split_source(&source);
    let skills = remote::get_publisher_repo_skills(&publisher_name, &repo_name)
        .await
        .with_context(|| format!("Failed to fetch repo skills for {source}"))?;

    let write_result: Result<()> = with_conn(|conn| {
        let tx = conn
            .unchecked_transaction()
            .context("Failed to start repo-skill snapshot transaction")?;

        mark_scope_attempt_in_tx(&tx, &scope)?;
        tx.execute(
            "DELETE FROM marketplace_repo_skill WHERE source = ?1",
            [source.as_str()],
        )
        .context("Failed to clear repo-skill snapshot rows")?;

        for (index, skill) in skills.iter().enumerate() {
            if let Some(skill_key) =
                upsert_skill_identity_in_tx(&tx, &source, &skill.name, skill.installs, &synced_at)?
            {
                tx.execute(
                    "INSERT INTO marketplace_repo_skill (
                        source,
                        skill_key,
                        installs,
                        rank,
                        updated_at
                    ) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        source,
                        skill_key,
                        skill.installs as i64,
                        (index + 1) as i64,
                        synced_at
                    ],
                )
                .context("Failed to insert repo-skill snapshot row")?;
            }
        }

        mark_scope_success_in_tx(&tx, &scope)?;
        tx.commit()
            .context("Failed to commit repo-skill snapshot transaction")?;
        Ok(())
    });

    if let Err(err) = write_result {
        let _ = mark_scope_error(&scope, &err.to_string());
        return Err(err);
    }

    Ok(())
}

pub async fn sync_scope_skill_detail(source: &str, name: &str) -> Result<()> {
    let source = normalize_source(source).ok_or_else(|| anyhow!("Invalid skill source"))?;
    let name = normalize_skill_name(name).ok_or_else(|| anyhow!("Invalid skill name"))?;
    let scope =
        skill_detail_scope(&source, &name).ok_or_else(|| anyhow!("Invalid detail scope"))?;
    mark_scope_attempt(&scope)?;
    let synced_at = now_rfc3339();
    let details = remote::fetch_marketplace_skill_details(&source, &name)
        .await
        .with_context(|| format!("Failed to fetch marketplace detail for {source}/{name}"))?;

    let write_result: Result<()> = with_conn(|conn| {
        let tx = conn
            .unchecked_transaction()
            .context("Failed to start skill-detail snapshot transaction")?;

        mark_scope_attempt_in_tx(&tx, &scope)?;
        let _ = upsert_skill_identity_in_tx(&tx, &source, &name, 0, &synced_at)?;
        upsert_detail_in_tx(&tx, &source, &name, &details, &synced_at)?;
        mark_scope_success_in_tx(&tx, &scope)?;
        tx.commit()
            .context("Failed to commit skill-detail snapshot transaction")?;
        Ok(())
    });

    if let Err(err) = write_result {
        let _ = mark_scope_error(&scope, &err.to_string());
        return Err(err);
    }

    Ok(())
}

async fn seed_search_results(query: &str, limit: u32) -> Result<()> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(());
    }

    let synced_at = now_rfc3339();
    let result = remote::search_skills_sh(trimmed, limit)
        .await
        .with_context(|| {
            format!("Failed to seed marketplace search results for query '{trimmed}'")
        })?;

    with_conn(|conn| {
        let tx = conn
            .unchecked_transaction()
            .context("Failed to start marketplace search seed transaction")?;

        for skill in &result.skills {
            let _ = upsert_skill_in_tx(&tx, skill, &synced_at)?;
        }

        cleanup_stale_skills_in_tx(&tx)?;
        tx.commit()
            .context("Failed to commit marketplace search seed transaction")?;
        Ok(())
    })
}

fn mark_scope_attempt(scope: &str) -> Result<()> {
    with_conn(|conn| {
        let tx = conn
            .unchecked_transaction()
            .context("Failed to start scope-attempt transaction")?;
        mark_scope_attempt_in_tx(&tx, scope)?;
        tx.commit()
            .context("Failed to commit scope-attempt transaction")?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::{
        ResolveSkillRequest, SNAPSHOT_SCHEMA_VERSION, SnapshotRuntimeConfig, build_skill_key,
        configure_runtime, create_connection, leaderboard_scope, load_leaderboard_snapshot,
        load_search_snapshot, load_skill_detail_snapshot, mark_scope_success_in_tx, now_rfc3339,
        resolve_skill_sources_from_snapshot, scope_updated_at, upsert_skill_identity_in_tx,
    };
    use rusqlite::Connection;
    use std::collections::{HashMap, HashSet};
    use std::path::Path;
    use std::sync::{Mutex, OnceLock};

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
            || -> super::InstalledSkillsFuture { Box::pin(async { Ok(Vec::new()) }) },
        ));
        f(&temp_root);
    }

    fn open_raw_conn(path: &Path) -> Connection {
        Connection::open(path).expect("open raw sqlite")
    }

    #[test]
    fn migrates_legacy_marketplace_cache_into_snapshot_schema() {
        with_temp_data_root(|temp_root| {
            let path = temp_root.join("marketplace.db");
            let conn = open_raw_conn(&path);
            conn.execute_batch(
                "CREATE TABLE marketplace_cache (
                    key TEXT PRIMARY KEY,
                    description TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );
                INSERT INTO marketplace_cache (key, description, updated_at)
                VALUES ('openai/skills/screenshot', 'Capture screenshots', '2026-01-01T00:00:00Z');",
            )
            .expect("seed legacy marketplace cache");
            drop(conn);

            let conn = create_connection().expect("create migrated marketplace connection");
            let version: i64 = conn
                .pragma_query_value(None, "user_version", |row| row.get(0))
                .expect("read user_version");
            assert_eq!(version, SNAPSHOT_SCHEMA_VERSION);

            let description: String = conn
                .query_row(
                    "SELECT description FROM marketplace_skill WHERE skill_key = 'openai/skills/screenshot'",
                    [],
                    |row| row.get(0),
                )
                .expect("read migrated description");
            assert_eq!(description, "Capture screenshots");
        });
    }

    #[test]
    fn leaderboard_round_trip_reads_inserted_rows() {
        with_temp_data_root(|_| {
            let conn = create_connection().expect("create marketplace connection");
            let tx = conn.unchecked_transaction().expect("start tx");
            let synced_at = now_rfc3339();
            let scope = leaderboard_scope("all");

            let skill_key =
                upsert_skill_identity_in_tx(&tx, "openai/skills", "screenshot", 42, &synced_at)
                    .expect("upsert snapshot skill")
                    .expect("skill key");
            tx.execute(
                "INSERT INTO marketplace_listing (listing_type, skill_key, rank, updated_at)
                 VALUES (?1, ?2, 1, ?3)",
                rusqlite::params![scope, skill_key, synced_at],
            )
            .expect("insert leaderboard row");
            mark_scope_success_in_tx(&tx, &scope).expect("mark scope success");
            tx.commit().expect("commit leaderboard snapshot");

            let rows = load_leaderboard_snapshot(&conn, &scope).expect("load leaderboard snapshot");
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].name, "screenshot");
            assert_eq!(rows[0].rank, Some(1));
            assert!(
                scope_updated_at(&conn, &scope)
                    .expect("scope updated at")
                    .is_some()
            );
        });
    }

    #[test]
    fn search_prefers_exact_name_match_before_description_only_match() {
        with_temp_data_root(|_| {
            let conn = create_connection().expect("create marketplace connection");
            let tx = conn.unchecked_transaction().expect("start tx");
            let synced_at = now_rfc3339();

            let exact_key =
                upsert_skill_identity_in_tx(&tx, "openai/skills", "search", 10, &synced_at)
                    .expect("upsert exact")
                    .expect("exact key");
            tx.execute(
                "UPDATE marketplace_skill SET description = 'Exact skill' WHERE skill_key = ?1",
                [exact_key.as_str()],
            )
            .expect("update exact description");
            super::refresh_fts_entry_in_tx(&tx, &exact_key).expect("refresh exact fts");

            let desc_key =
                upsert_skill_identity_in_tx(&tx, "openai/skills", "assistant", 999, &synced_at)
                    .expect("upsert description")
                    .expect("description key");
            tx.execute(
                "UPDATE marketplace_skill SET description = 'search helper utility' WHERE skill_key = ?1",
                [desc_key.as_str()],
            )
            .expect("update desc description");
            super::refresh_fts_entry_in_tx(&tx, &desc_key).expect("refresh desc fts");
            tx.commit().expect("commit search fixtures");

            let results = load_search_snapshot(&conn, "search", 10)
                .expect("run search snapshot")
                .0;
            assert_eq!(results.len(), 2);
            assert_eq!(results[0].name, "search");
        });
    }

    #[test]
    fn search_handles_hyphenated_query_tokens() {
        with_temp_data_root(|_| {
            let conn = create_connection().expect("create marketplace connection");
            let tx = conn.unchecked_transaction().expect("start tx");
            let synced_at = now_rfc3339();

            let skill_key = upsert_skill_identity_in_tx(
                &tx,
                "openai/skills",
                "ui-design-system",
                12,
                &synced_at,
            )
            .expect("upsert hyphenated skill")
            .expect("hyphenated skill key");
            tx.execute(
                "UPDATE marketplace_skill SET description = 'Design polished UI systems' WHERE skill_key = ?1",
                [skill_key.as_str()],
            )
            .expect("update hyphenated description");
            super::refresh_fts_entry_in_tx(&tx, &skill_key).expect("refresh hyphenated fts");
            tx.commit().expect("commit hyphenated fixtures");

            let results = load_search_snapshot(&conn, "ui-design", 10)
                .expect("run hyphenated search")
                .0;
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].name, "ui-design-system");
        });
    }

    #[test]
    fn source_resolution_prefers_repo_affinity_before_popularity() {
        with_temp_data_root(|_| {
            let conn = create_connection().expect("create marketplace connection");
            let tx = conn.unchecked_transaction().expect("start tx");
            let synced_at = now_rfc3339();

            upsert_skill_identity_in_tx(&tx, "openai/skills", "search", 10, &synced_at)
                .expect("upsert preferred repo");
            upsert_skill_identity_in_tx(&tx, "vercel/ai", "search", 999, &synced_at)
                .expect("upsert popular repo");
            tx.commit().expect("commit source fixtures");

            let requests = vec![ResolveSkillRequest {
                original_name: "search".to_string(),
                normalized_name: "search".to_string(),
            }];
            let named_sources = HashMap::new();
            let preferred_repos = HashSet::from(["openai/skills".to_string()]);

            let resolved = resolve_skill_sources_from_snapshot(
                &conn,
                &requests,
                &named_sources,
                &preferred_repos,
            )
            .expect("resolve from snapshot");

            assert_eq!(
                resolved.get("search"),
                Some(&"https://github.com/openai/skills".to_string())
            );
        });
    }

    #[test]
    fn source_resolution_requires_unique_top_candidate_when_ambiguous() {
        with_temp_data_root(|_| {
            let conn = create_connection().expect("create marketplace connection");
            let tx = conn.unchecked_transaction().expect("start tx");
            let synced_at = now_rfc3339();

            upsert_skill_identity_in_tx(&tx, "openai/skills", "search", 100, &synced_at)
                .expect("upsert first candidate");
            upsert_skill_identity_in_tx(&tx, "vercel/ai", "search", 100, &synced_at)
                .expect("upsert second candidate");
            tx.commit().expect("commit ambiguous fixtures");

            let requests = vec![ResolveSkillRequest {
                original_name: "search".to_string(),
                normalized_name: "search".to_string(),
            }];

            let resolved = resolve_skill_sources_from_snapshot(
                &conn,
                &requests,
                &HashMap::new(),
                &HashSet::new(),
            )
            .expect("resolve ambiguous snapshot");

            assert!(resolved.get("search").is_none());
        });
    }

    #[test]
    fn detail_snapshot_reads_cached_payload() {
        with_temp_data_root(|_| {
            let conn = create_connection().expect("create marketplace connection");
            let tx = conn.unchecked_transaction().expect("start tx");
            let synced_at = now_rfc3339();
            let skill_key =
                upsert_skill_identity_in_tx(&tx, "openai/skills", "screenshot", 0, &synced_at)
                    .expect("upsert detail identity")
                    .expect("detail key");
            tx.execute(
                "INSERT INTO marketplace_skill_detail (
                    skill_key,
                    summary,
                    readme,
                    weekly_installs,
                    github_stars,
                    first_seen,
                    security_audits_json,
                    last_detail_sync_at
                ) VALUES (?1, 'summary', '# readme', '1.2K', 12, 'Apr 1, 2026', '[]', ?2)",
                rusqlite::params![skill_key, synced_at],
            )
            .expect("insert detail snapshot");
            tx.commit().expect("commit detail snapshot");

            let detail = load_skill_detail_snapshot(
                &conn,
                &build_skill_key("openai/skills", "screenshot").expect("detail key"),
            )
            .expect("load detail snapshot")
            .expect("detail exists");

            assert_eq!(detail.summary.as_deref(), Some("summary"));
            assert_eq!(detail.github_stars, Some(12));
        });
    }
}
