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

const CACHE_SCHEMA_VERSION: &str = "v2";

static SHORT_TRANSLATION_MEM: std::sync::LazyLock<
    RwLock<HashMap<(String, String, String), CachedTranslation>>,
> = std::sync::LazyLock::new(|| RwLock::new(HashMap::new()));

const CORRUPTED_MARKERS: &[&str] = &[
    "PLEASE SELECT TWO DISTINCT LANGUAGES",
    "MYMEMORY WARNING:",
    "LIMIT EXCEEDED",
    "rate limit",
    "RATE LIMIT",
    "timeout",
    "TIMEOUT",
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
    if let Ok(path) = std::env::var("SKILLSTAR_TRANSLATION_DB_PATH") {
        return PathBuf::from(path);
    }
    skillstar_infra::paths::translation_db_path()
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

#[cfg(not(test))]
static SCHEMA_READY: LazyLock<()> = LazyLock::new(|| {
    let conn = skillstar_infra::db_pool::translation_pool()
        .get()
        .expect("translation cache DB pool connection for schema migration");
    migrate_schema(&conn).expect("translation cache schema migration failed");
});

#[cfg(test)]
fn create_connection() -> Result<Connection> {
    let path = db_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("Failed to create translation cache directory")?;
    }
    let conn = Connection::open(&path).context("Failed to open translation cache db")?;
    conn.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA busy_timeout=5000;")
        .context("Failed to configure translation cache test DB")?;
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
        let conn = skillstar_infra::db_pool::translation_pool()
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
    skillstar_infra::util::sha256_hex(source_text.as_bytes())
}

fn translation_looks_translated_for_cache(
    target_language: &str,
    source: &str,
    result: &str,
) -> bool {
    if result.trim().is_empty() {
        return false;
    }

    if source == result {
        return false;
    }

    let target_needs_cjk = matches!(
        target_language,
        "zh-CN" | "zh-TW" | "zh-cn" | "zh-tw" | "ja" | "ko"
    );

    if target_needs_cjk {
        let has_target_script = result.chars().any(|c| {
            ('\u{4E00}'..='\u{9FFF}').contains(&c)
                || ('\u{3400}'..='\u{4DBF}').contains(&c)
                || ('\u{F900}'..='\u{FAFF}').contains(&c)
                || ('\u{3040}'..='\u{309F}').contains(&c)
                || ('\u{30A0}'..='\u{30FF}').contains(&c)
                || ('\u{AC00}'..='\u{D7AF}').contains(&c)
        });
        if !has_target_script {
            return false;
        }
    }

    true
}

fn normalize_provider_identity(provider_identity: Option<&str>) -> String {
    provider_identity
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("default")
        .to_ascii_lowercase()
}

fn build_cache_key(
    kind: TranslationKind,
    target_language: &str,
    source_text_hash: &str,
    provider_identity: Option<&str>,
) -> String {
    format!(
        "{}::{}::{}::{}::{}",
        kind.as_str(),
        normalize_target_language(target_language),
        source_text_hash,
        normalize_provider_identity(provider_identity),
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
    get_cached_translation_for_provider(kind, target_language, source_text, None)
}

pub fn get_cached_translation_for_provider(
    kind: TranslationKind,
    target_language: &str,
    source_text: &str,
    provider_identity: Option<&str>,
) -> Result<Option<CachedTranslation>> {
    let hash = source_hash(source_text);
    let normalized_lang = normalize_target_language(target_language);
    let normalized_provider = normalize_provider_identity(provider_identity);

    if kind == TranslationKind::Short {
        if let Ok(mem) = SHORT_TRANSLATION_MEM.read() {
            if let Some(cached) = mem.get(&(
                hash.clone(),
                normalized_lang.clone(),
                normalized_provider.clone(),
            )) {
                return Ok(Some(cached.clone()));
            }
        }
    }

    let cache_key = build_cache_key(kind, &normalized_lang, &hash, Some(&normalized_provider));

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

    if kind == TranslationKind::Short {
        if let Some(ref cached) = result {
            if let Ok(mut mem) = SHORT_TRANSLATION_MEM.write() {
                mem.insert((hash, normalized_lang, normalized_provider), cached.clone());
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
    upsert_translation_for_provider(
        kind,
        target_language,
        source_text,
        translated_text,
        source_provider,
        None,
    )
}

pub fn upsert_translation_for_provider(
    kind: TranslationKind,
    target_language: &str,
    source_text: &str,
    translated_text: &str,
    source_provider: Option<&str>,
    provider_identity: Option<&str>,
) -> Result<()> {
    if translated_text.trim().is_empty() {
        debug!(target: "translate", kind = kind.as_str(), "cache upsert SKIP empty text");
        return Ok(());
    }

    if is_corrupted_translation(translated_text) {
        debug!(target: "translate", kind = kind.as_str(), "cache upsert SKIP corrupted");
        return Ok(());
    }

    if !translation_looks_translated_for_cache(target_language, source_text, translated_text) {
        debug!(target: "translate", kind = kind.as_str(), "cache upsert SKIP untranslated");
        return Ok(());
    }

    let normalized_lang = normalize_target_language(target_language);
    let hash = source_hash(source_text);
    let normalized_provider = normalize_provider_identity(provider_identity);
    let cache_key = build_cache_key(kind, &normalized_lang, &hash, Some(&normalized_provider));
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

    if kind == TranslationKind::Short {
        if let Ok(mut mem) = SHORT_TRANSLATION_MEM.write() {
            mem.insert(
                (hash, normalized_lang, normalized_provider),
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

const MAX_CACHE_ENTRIES: i64 = 10_000;
const STALE_ENTRY_DAYS: i64 = 90;

pub fn cleanup_stale_entries() -> Result<usize> {
    if let Ok(mut mem) = SHORT_TRANSLATION_MEM.write() {
        mem.clear();
    }

    with_conn(|conn| {
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(STALE_ENTRY_DAYS)).to_rfc3339();

        let deleted_stale: usize = conn
            .execute(
                "DELETE FROM translation_cache WHERE updated_at < ?1",
                params![cutoff],
            )
            .context("Failed to delete stale translations")?;

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

fn cleanup_untranslated_entries() -> Result<usize> {
    with_conn(|conn| {
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
                ('\u{4E00}'..='\u{9FFF}').contains(&c)
                    || ('\u{3400}'..='\u{4DBF}').contains(&c)
                    || ('\u{F900}'..='\u{FAFF}').contains(&c)
                    || ('\u{3040}'..='\u{30FF}').contains(&c)
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

    info!(target: "translate", entries = map.len(), "preload_short_translations done");

    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::{
        TranslationKind, clear_cache, get_cached_translation, get_cached_translation_for_provider,
        upsert_translation, upsert_translation_for_provider,
    };

    fn with_temp_data_root<F: FnOnce()>(f: F) {
        let _guard = crate::lock_test_env();
        let temp = tempfile::tempdir().expect("create temp dir");
        let data_dir_key = "SKILLSTAR_DATA_DIR";
        let db_path_key = "SKILLSTAR_TRANSLATION_DB_PATH";
        let previous_data_dir = std::env::var(data_dir_key).ok();
        let previous_db_path = std::env::var(db_path_key).ok();
        let db_path = temp.path().join("db").join("translation.db");

        unsafe {
            std::env::set_var(data_dir_key, temp.path());
            std::env::set_var(db_path_key, &db_path);
        }

        f();

        unsafe {
            match previous_data_dir {
                Some(value) => std::env::set_var(data_dir_key, value),
                None => std::env::remove_var(data_dir_key),
            }
            match previous_db_path {
                Some(value) => std::env::set_var(db_path_key, value),
                None => std::env::remove_var(db_path_key),
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
                "你好世界",
                Some("ai"),
            )
            .expect("write cache");

            let hit = get_cached_translation(TranslationKind::Short, "zh-CN", "Hello world")
                .expect("query cache")
                .expect("entry should exist");

            assert_eq!(hit.translated_text, "你好世界");
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
                "你好世界",
                Some("ai"),
            )
            .expect("write zh cache");
            upsert_translation(
                TranslationKind::Short,
                "ja",
                "Hello world",
                "こんにちは世界",
                Some("mymemory"),
            )
            .expect("write ja cache");

            let zh = get_cached_translation(TranslationKind::Short, "zh-CN", "Hello world")
                .expect("query zh")
                .expect("zh entry");
            let ja = get_cached_translation(TranslationKind::Short, "ja", "Hello world")
                .expect("query ja")
                .expect("ja entry");

            assert_eq!(zh.translated_text, "你好世界");
            assert_eq!(ja.translated_text, "こんにちは世界");
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
                "# 标题\n你好v1",
                None,
            )
            .expect("write v1");
            upsert_translation(
                TranslationKind::Skill,
                "zh-CN",
                "# Title\nHello",
                "# 标题\n你好v2",
                None,
            )
            .expect("write v2");

            let hit = get_cached_translation(TranslationKind::Skill, "zh-CN", "# Title\nHello")
                .expect("query cache")
                .expect("entry should exist");
            assert_eq!(hit.translated_text, "# 标题\n你好v2");
        });
    }

    #[test]
    fn provider_aware_cache_keeps_engines_isolated() {
        with_temp_data_root(|| {
            clear_cache().expect("clear cache");

            upsert_translation_for_provider(
                TranslationKind::Short,
                "zh-CN",
                "Hello world",
                "你好，世界",
                Some("DeepL"),
                Some("translation_api:deepl"),
            )
            .expect("write deepl cache");
            upsert_translation_for_provider(
                TranslationKind::Short,
                "zh-CN",
                "Hello world",
                "您好，世界",
                Some("MiniMax"),
                Some("llm:codex:minimax"),
            )
            .expect("write minimax cache");
            upsert_translation_for_provider(
                TranslationKind::Short,
                "zh-CN",
                "Hello world",
                "哈喽，世界",
                Some("MyMemory"),
                Some("fallback:mymemory"),
            )
            .expect("write mymemory cache");

            let deepl = get_cached_translation_for_provider(
                TranslationKind::Short,
                "zh-CN",
                "Hello world",
                Some("translation_api:deepl"),
            )
            .expect("read deepl")
            .expect("deepl entry");
            let minimax = get_cached_translation_for_provider(
                TranslationKind::Short,
                "zh-CN",
                "Hello world",
                Some("llm:codex:minimax"),
            )
            .expect("read minimax")
            .expect("minimax entry");
            let mymemory = get_cached_translation_for_provider(
                TranslationKind::Short,
                "zh-CN",
                "Hello world",
                Some("fallback:mymemory"),
            )
            .expect("read mymemory")
            .expect("mymemory entry");

            assert_eq!(deepl.translated_text, "你好，世界");
            assert_eq!(minimax.translated_text, "您好，世界");
            assert_eq!(mymemory.translated_text, "哈喽，世界");
        });
    }

    #[test]
    fn clear_cache_returns_removed_count() {
        with_temp_data_root(|| {
            clear_cache().expect("clear cache");

            upsert_translation(TranslationKind::Short, "zh-CN", "One", "一", Some("ai"))
                .expect("write one");
            upsert_translation(TranslationKind::Skill, "zh-CN", "Two", "二", None)
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
