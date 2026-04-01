use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
#[cfg(not(test))]
use std::sync::{LazyLock, Mutex};

const CACHE_SCHEMA_VERSION: &str = "v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranslationKind {
    Short,
    Skill,
    SkillSection,
}

impl TranslationKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Short => "short",
            Self::Skill => "skill",
            Self::SkillSection => "skill_section",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedTranslation {
    pub translated_text: String,
    pub source_provider: Option<String>,
}

fn db_path() -> PathBuf {
    super::paths::data_root().join("translation_cache.db")
}

fn create_connection() -> Result<Connection> {
    let path = db_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("Failed to create translation cache directory")?;
    }

    let conn = Connection::open(&path).context("Failed to open translation cache db")?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")
        .context("Failed to set WAL mode")?;
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

    Ok(conn)
}

/// Global singleton DB connection for production use.
/// WAL mode allows concurrent reads without blocking writes.
#[cfg(not(test))]
static DB: LazyLock<Mutex<Connection>> =
    LazyLock::new(|| Mutex::new(create_connection().expect("translation cache DB init failed")));

/// Execute a function with a DB connection.
/// Production: uses the global singleton.
/// Tests: opens a fresh connection (because tests swap data dirs via env var).
fn with_conn<F, T>(f: F) -> Result<T>
where
    F: FnOnce(&Connection) -> Result<T>,
{
    #[cfg(not(test))]
    {
        let guard = DB
            .lock()
            .map_err(|e| anyhow::anyhow!("DB lock poisoned: {e}"))?;
        f(&guard)
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
    let digest = Sha256::digest(source_text.as_bytes());
    let mut hex = String::with_capacity(64);
    const HEX_TABLE: &[u8; 16] = b"0123456789abcdef";
    for &byte in &digest {
        hex.push(HEX_TABLE[(byte >> 4) as usize] as char);
        hex.push(HEX_TABLE[(byte & 0xf) as usize] as char);
    }
    hex
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

pub fn get_cached_translation(
    kind: TranslationKind,
    target_language: &str,
    source_text: &str,
) -> Result<Option<CachedTranslation>> {
    let hash = source_hash(source_text);
    let cache_key = build_cache_key(kind, target_language, &hash);

    with_conn(|conn| {
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

        Ok(entry)
    })
}

pub fn upsert_translation(
    kind: TranslationKind,
    target_language: &str,
    source_text: &str,
    translated_text: &str,
    source_provider: Option<&str>,
) -> Result<()> {
    if translated_text.trim().is_empty() {
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
    })
}

pub fn clear_cache() -> Result<usize> {
    let path = db_path();
    if !path.exists() {
        return Ok(0);
    }

    with_conn(|conn| {
        conn.execute("DELETE FROM translation_cache", [])
            .context("Failed to clear translation cache rows")?;

        Ok(conn.changes().max(0) as usize)
    })
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
