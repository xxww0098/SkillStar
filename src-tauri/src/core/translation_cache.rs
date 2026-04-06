use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, params};
use std::collections::HashMap;
#[cfg(test)]
use std::path::PathBuf;
#[cfg(not(test))]
use std::sync::LazyLock;
use std::sync::RwLock;
use tracing::{debug, info};

const CACHE_SCHEMA_VERSION: &str = "v1";

// ── In-memory cache for short translations ──────────────────────────
//
// Keyed by (source_hash, normalized_target_language) → CachedTranslation.
// Eliminates DB roundtrips during list_skills (which builds 100+ skills
// concurrently on blocking threads, each needing its description translation).

static SHORT_TRANSLATION_MEM: std::sync::LazyLock<
    RwLock<HashMap<(String, String), CachedTranslation>>,
> = std::sync::LazyLock::new(|| RwLock::new(HashMap::new()));

const CORRUPTED_MARKERS: &[&str] = &[
    "PLEASE SELECT TWO DISTINCT LANGUAGES",
    "MYMEMORY WARNING:",
    "LIMIT EXCEEDED",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranslationKind {
    Short,
    Skill,
    SkillSection,
    Summary,
}

impl TranslationKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Short => "short",
            Self::Skill => "skill",
            Self::SkillSection => "skill_section",
            Self::Summary => "summary",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedTranslation {
    pub translated_text: String,
    pub source_provider: Option<String>,
}

#[cfg(test)]
fn db_path() -> PathBuf {
    super::paths::translation_db_path()
}

fn migrate_schema(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS translation_cache (
            cache_key TEXT PRIMARY KEY,
            kind TEXT NOT NULL,
            target_language TEXT NOT NULL,
            source_hash TEXT NOT NULL,
            translated_text TEXT NOT NULL,
            source_provider TEXT,
            updated_at TEXT NOT NULL
        )",
        (),
    )
    .context("Failed to initialize translation_cache table")?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_translation_cache_target_kind
        ON translation_cache(target_language, kind)",
        (),
    )
    .context("Failed to initialize translation_cache indexes")?;
    Ok(())
}

/// Ensure schema migration runs exactly once via the pool.
#[cfg(not(test))]
static SCHEMA_READY: LazyLock<()> = LazyLock::new(|| {
    let conn = super::db_pool::translation_pool()
        .get()
        .expect("translation cache DB pool connection for schema migration");
    migrate_schema(&conn).expect("translation cache schema migration failed");
});

/// Test-only: open a standalone connection with full schema migration.
#[cfg(test)]
fn create_connection() -> Result<Connection> {
    let path = db_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("Failed to create translation cache directory")?;
    }
    let conn = Connection::open(&path).context("Failed to open translation cache db")?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")
        .context("Failed to set WAL mode")?;
    migrate_schema(&conn)?;
    Ok(conn)
}

fn with_conn<F, T>(f: F) -> Result<T>
where
    F: FnOnce(&Connection) -> Result<T>,
{
    #[cfg(not(test))]
    {
        LazyLock::force(&SCHEMA_READY);
        let conn = super::db_pool::translation_pool()
            .get()
            .map_err(|e| anyhow::anyhow!("Failed to get translation pool connection: {e}"))?;
        f(&conn)
    }
    #[cfg(test)]
    {
        let conn = create_connection()?;
        f(&conn)
    }
}

fn normalize_target_language(target_language: &str) -> String {
    let trimmed = target_language.trim();
    if trimmed.is_empty() {
        "en".to_string()
    } else {
        trimmed.to_ascii_lowercase()
    }
}

fn source_hash(source_text: &str) -> String {
    super::util::sha256_hex(source_text.as_bytes())
}

fn build_cache_key(kind: TranslationKind, target_language: &str, source_text_hash: &str) -> String {
    format!(
        "{}::{}::{}::{}",
        kind.as_str(),
        normalize_target_language(target_language),
        source_text_hash,
        CACHE_SCHEMA_VERSION
    )
}

fn is_corrupted_translation(text: &str) -> bool {
    CORRUPTED_MARKERS.iter().any(|marker| text.contains(marker))
}

pub fn get_cached_translation(
    kind: TranslationKind,
    target_language: &str,
    source_text: &str,
) -> Result<Option<CachedTranslation>> {
    let hash = source_hash(source_text);
    let normalized_lang = normalize_target_language(target_language);

    // Fast path: check in-memory cache for Short translations.
    if kind == TranslationKind::Short {
        if let Ok(mem) = SHORT_TRANSLATION_MEM.read() {
            if let Some(cached) = mem.get(&(hash.clone(), normalized_lang.clone())) {
                return Ok(Some(cached.clone()));
            }
        }
    }

    let cache_key = build_cache_key(kind, &normalized_lang, &hash);

    let result = with_conn(|conn| {
        let entry = conn
            .query_row(
                "SELECT translated_text, source_provider FROM translation_cache WHERE cache_key = ?1",
                params![cache_key],
                |row| {
                    Ok(CachedTranslation {
                        translated_text: row.get(0)?,
                        source_provider: row.get(1)?,
                    })
                },
            )
            .optional()
            .context("Failed to query translation cache")?;

        if let Some(e) = entry {
            if is_corrupted_translation(&e.translated_text) {
                let _ = conn.execute(
                    "DELETE FROM translation_cache WHERE cache_key = ?1",
                    params![cache_key],
                );
                return Ok(None);
            }
            return Ok(Some(e));
        }

        Ok(None)
    })?;

    // Warm the in-memory cache on DB hit for Short translations.
    if kind == TranslationKind::Short {
        if let Some(ref cached) = result {
            if let Ok(mut mem) = SHORT_TRANSLATION_MEM.write() {
                mem.insert((hash, normalized_lang), cached.clone());
            }
        }
    }

    Ok(result)
}

pub fn upsert_translation(
    kind: TranslationKind,
    target_language: &str,
    source_text: &str,
    translated_text: &str,
    source_provider: Option<&str>,
) -> Result<()> {
    if translated_text.trim().is_empty() {
        debug!(target: "translate", kind = kind.as_str(), "cache upsert SKIP empty text");
        return Ok(());
    }

    // Write-side corruption guard — reject known bad translations before
    // they reach the DB / memory cache.
    if is_corrupted_translation(translated_text) {
        debug!(target: "translate", kind = kind.as_str(), "cache upsert SKIP corrupted");
        return Ok(());
    }

    let normalized_lang = normalize_target_language(target_language);
    let hash = source_hash(source_text);
    let cache_key = build_cache_key(kind, &normalized_lang, &hash);
    let now = Utc::now().to_rfc3339();
    let provider = source_provider
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    with_conn(|conn| {
        conn.execute(
            "INSERT INTO translation_cache (
                cache_key,
                kind,
                target_language,
                source_hash,
                translated_text,
                source_provider,
                updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(cache_key) DO UPDATE SET
                translated_text = excluded.translated_text,
                source_provider = excluded.source_provider,
                updated_at = excluded.updated_at",
            params![
                cache_key,
                kind.as_str(),
                normalized_lang,
                hash,
                translated_text,
                provider,
                now
            ],
        )
        .context("Failed to upsert translation cache entry")?;

        Ok(())
    })?;

    // Update in-memory cache for Short translations.
    if kind == TranslationKind::Short {
        if let Ok(mut mem) = SHORT_TRANSLATION_MEM.write() {
            mem.insert(
                (hash, normalized_lang),
                CachedTranslation {
                    translated_text: translated_text.to_string(),
                    source_provider: provider,
                },
            );
        }
    }

    Ok(())
}

#[allow(dead_code)]
pub fn clear_cache() -> Result<usize> {
    // Clear in-memory cache first.
    if let Ok(mut mem) = SHORT_TRANSLATION_MEM.write() {
        mem.clear();
    }

    with_conn(|conn| {
        let deleted = conn
            .execute("DELETE FROM translation_cache", [])
            .context("Failed to clear translation cache rows")?;

        Ok(deleted.max(0))
    })
}

// ── LRU Cache Cleanup ───────────────────────────────────────────────

/// Maximum number of cache entries before LRU eviction kicks in.
const MAX_CACHE_ENTRIES: i64 = 10_000;

/// Entries older than this many days are considered stale.
const STALE_ENTRY_DAYS: i64 = 90;

/// Clean up stale and excess translation cache entries.
///
/// 1. Deletes entries older than `STALE_ENTRY_DAYS`.
/// 2. If still over `MAX_CACHE_ENTRIES`, keeps only the most recent entries.
///
/// Returns the total number of entries removed.
pub fn cleanup_stale_entries() -> Result<usize> {
    // Clear in-memory cache — it will be re-warmed lazily.
    if let Ok(mut mem) = SHORT_TRANSLATION_MEM.write() {
        mem.clear();
    }

    with_conn(|conn| {
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(STALE_ENTRY_DAYS)).to_rfc3339();

        // Phase 1: delete entries older than STALE_ENTRY_DAYS
        let deleted_stale: usize = conn
            .execute(
                "DELETE FROM translation_cache WHERE updated_at < ?1",
                params![cutoff],
            )
            .context("Failed to delete stale translations")?;

        // Phase 2: if still over limit, keep the most recent MAX_CACHE_ENTRIES
        let total: i64 = conn
            .query_row("SELECT COUNT(1) FROM translation_cache", [], |row| {
                row.get(0)
            })
            .context("Failed to count translation cache entries")?;

        let deleted_lru: usize = if total > MAX_CACHE_ENTRIES {
            conn.execute(
                "DELETE FROM translation_cache WHERE cache_key NOT IN (
                    SELECT cache_key FROM translation_cache
                    ORDER BY updated_at DESC LIMIT ?1
                )",
                params![MAX_CACHE_ENTRIES],
            )
            .context("Failed to LRU-evict excess translations")?
        } else {
            0
        };

        let total_removed = deleted_stale + deleted_lru;
        if total_removed > 0 {
            info!(target: "translate", stale = deleted_stale, lru = deleted_lru, "cleanup removed {total_removed} cache entries");
        }

        Ok(total_removed)
    })
}

/// Remove cached "short" translations that target a CJK language but contain
/// no CJK/kana/hangul characters (i.e. the AI or MyMemory returned the text
/// untranslated).
fn cleanup_untranslated_entries() -> Result<usize> {
    with_conn(|conn| {
        // Fetch candidates: short translations for CJK target languages
        let mut stmt = conn
            .prepare(
                "SELECT cache_key, translated_text FROM translation_cache
             WHERE kind = 'short'
               AND (target_language IN ('zh-cn', 'zh-tw', 'ja', 'ko'))",
            )
            .context("Failed to prepare untranslated cleanup query")?;

        let rows: Vec<(String, String)> = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .context("Failed to query untranslated candidates")?
            .filter_map(|r| r.ok())
            .collect();

        let mut removed = 0usize;
        for (cache_key, text) in &rows {
            let has_target_chars = text.chars().any(|c| {
                // CJK Unified Ideographs
                ('\u{4E00}'..='\u{9FFF}').contains(&c)
                    || ('\u{3400}'..='\u{4DBF}').contains(&c)
                    || ('\u{F900}'..='\u{FAFF}').contains(&c)
                    // Japanese kana
                    || ('\u{3040}'..='\u{30FF}').contains(&c)
                    // Hangul syllables
                    || ('\u{AC00}'..='\u{D7AF}').contains(&c)
            });
            if !has_target_chars {
                let _ = conn.execute(
                    "DELETE FROM translation_cache WHERE cache_key = ?1",
                    params![cache_key],
                );
                removed += 1;
            }
        }

        if removed > 0 {
            info!(target: "translate", removed, "cleaned up untranslated CJK entries");
        }
        Ok(removed)
    })
}

/// Run once at startup: clean stale entries. Safe to call multiple times
/// (only does real work on the first call per process).
pub fn startup_cleanup() {
    static DONE: std::sync::Once = std::sync::Once::new();
    DONE.call_once(|| {
        if let Err(e) = cleanup_stale_entries() {
            tracing::warn!(target: "translate", "startup cache cleanup failed: {e}");
        }
        if let Err(e) = cleanup_untranslated_entries() {
            tracing::warn!(target: "translate", "startup untranslated cleanup failed: {e}");
        }
    });
}

/// Bulk-load all Short translations for a given target language into a
/// HashMap keyed by `source_hash`.  Used by `list_installed_skills` to
/// avoid N+1 DB queries (one per skill) during listing.
///
/// Also warms the in-memory cache so subsequent individual lookups are
/// zero-cost.
pub fn preload_short_translations(
    target_language: &str,
) -> Result<HashMap<String, CachedTranslation>> {
    let normalized_lang = normalize_target_language(target_language);
    debug!(target: "translate", lang = %normalized_lang, "preload_short_translations");

    let map = with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT source_hash, translated_text, source_provider
                 FROM translation_cache
                 WHERE kind = 'short' AND target_language = ?1",
            )
            .context("Failed to prepare short translation preload query")?;

        let rows = stmt
            .query_map(params![normalized_lang], |row| {
                let hash: String = row.get(0)?;
                let text: String = row.get(1)?;
                let provider: Option<String> = row.get(2)?;
                Ok((hash, text, provider))
            })
            .context("Failed to execute short translation preload query")?;

        let mut result = HashMap::new();
        for row in rows {
            if let Ok((hash, text, provider)) = row {
                if !text.trim().is_empty() && !is_corrupted_translation(&text) {
                    result.insert(
                        hash,
                        CachedTranslation {
                            translated_text: text,
                            source_provider: provider,
                        },
                    );
                }
            }
        }

        Ok(result)
    })?;

    // Warm in-memory cache with the bulk-loaded data.
    if let Ok(mut mem) = SHORT_TRANSLATION_MEM.write() {
        for (hash, cached) in &map {
            mem.insert((hash.clone(), normalized_lang.clone()), cached.clone());
        }
    }
    info!(target: "translate", entries = map.len(), "preload_short_translations done");

    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::{TranslationKind, clear_cache, get_cached_translation, upsert_translation};

    fn with_temp_data_root<F: FnOnce()>(f: F) {
        let _guard = crate::core::test_env_lock().lock().expect("lock test env");
        let temp = tempfile::tempdir().expect("create temp dir");
        let key = "SKILLSTAR_DATA_DIR";
        let previous = std::env::var(key).ok();

        // SAFETY: test-only env mutation guarded by global mutex.
        unsafe {
            std::env::set_var(key, temp.path());
        }

        f();

        // SAFETY: restore env var after test in the same critical section.
        unsafe {
            match previous {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }

    #[test]
    fn round_trip_short_translation_cache() {
        with_temp_data_root(|| {
            clear_cache().expect("clear cache");

            upsert_translation(
                TranslationKind::Short,
                "zh-CN",
                "Hello world",
                "hello-world-zh",
                Some("ai"),
            )
            .expect("write cache");

            let hit = get_cached_translation(TranslationKind::Short, "zh-CN", "Hello world")
                .expect("query cache")
                .expect("entry should exist");

            assert_eq!(hit.translated_text, "hello-world-zh");
            assert_eq!(hit.source_provider.as_deref(), Some("ai"));
        });
    }

    #[test]
    fn different_target_languages_do_not_collide() {
        with_temp_data_root(|| {
            clear_cache().expect("clear cache");

            upsert_translation(
                TranslationKind::Short,
                "zh-CN",
                "Hello world",
                "hello-world-zh",
                Some("ai"),
            )
            .expect("write zh cache");
            upsert_translation(
                TranslationKind::Short,
                "ja",
                "Hello world",
                "hello-world-ja",
                Some("mymemory"),
            )
            .expect("write ja cache");

            let zh = get_cached_translation(TranslationKind::Short, "zh-CN", "Hello world")
                .expect("query zh")
                .expect("zh entry");
            let ja = get_cached_translation(TranslationKind::Short, "ja", "Hello world")
                .expect("query ja")
                .expect("ja entry");

            assert_eq!(zh.translated_text, "hello-world-zh");
            assert_eq!(ja.translated_text, "hello-world-ja");
            assert_eq!(zh.source_provider.as_deref(), Some("ai"));
            assert_eq!(ja.source_provider.as_deref(), Some("mymemory"));
        });
    }

    #[test]
    fn upsert_overwrites_existing_entry_for_same_key() {
        with_temp_data_root(|| {
            clear_cache().expect("clear cache");

            upsert_translation(
                TranslationKind::Skill,
                "zh-CN",
                "# Title\nHello",
                "# Title\nhello-zh-v1",
                None,
            )
            .expect("write v1");
            upsert_translation(
                TranslationKind::Skill,
                "zh-CN",
                "# Title\nHello",
                "# Title\nhello-zh-v2",
                None,
            )
            .expect("write v2");

            let hit = get_cached_translation(TranslationKind::Skill, "zh-CN", "# Title\nHello")
                .expect("query cache")
                .expect("entry should exist");
            assert_eq!(hit.translated_text, "# Title\nhello-zh-v2");
        });
    }

    #[test]
    fn clear_cache_returns_removed_count() {
        with_temp_data_root(|| {
            clear_cache().expect("clear cache");

            upsert_translation(TranslationKind::Short, "zh-CN", "One", "one-zh", Some("ai"))
                .expect("write one");
            upsert_translation(TranslationKind::Skill, "zh-CN", "Two", "two-zh", None)
                .expect("write two");

            let removed = clear_cache().expect("clear cache");
            assert_eq!(removed, 2);

            let one =
                get_cached_translation(TranslationKind::Short, "zh-CN", "One").expect("query one");
            let two =
                get_cached_translation(TranslationKind::Skill, "zh-CN", "Two").expect("query two");
            assert!(one.is_none());
            assert!(two.is_none());
        });
    }
}
