use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;
use sha2::{Digest, Sha256};
use tracing::debug;

use crate::error::{Error, Result};
use crate::types::TranslationResult;

/// SQLite-backed translation cache, keyed by content hash + target language + model.
pub struct TranslationCache {
    conn: Mutex<Connection>,
}

/// A cached translation entry.
#[derive(Debug, Clone)]
pub struct CachedTranslation {
    pub translated_text: String,
    pub confidence: f64,
}

impl TranslationCache {
    /// Open (or create) a cache database at the given path.
    pub fn open(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::Cache(e.to_string()))?;
        }
        let conn = Connection::open(db_path).map_err(|e| Error::Cache(e.to_string()))?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             CREATE TABLE IF NOT EXISTS translation_cache (
                 cache_key   TEXT PRIMARY KEY,
                 segment_id  TEXT NOT NULL,
                 source_text TEXT NOT NULL,
                 target_lang TEXT NOT NULL,
                 model       TEXT NOT NULL,
                 translated_text TEXT NOT NULL,
                 confidence  REAL NOT NULL DEFAULT 0.0,
                 created_at  TEXT NOT NULL DEFAULT (datetime('now'))
             );
             CREATE INDEX IF NOT EXISTS idx_cache_key ON translation_cache(cache_key);",
        )
        .map_err(|e| Error::Cache(e.to_string()))?;

        Ok(Self { conn: Mutex::new(conn) })
    }

    /// Open an in-memory cache (useful for testing or one-off runs).
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory().map_err(|e| Error::Cache(e.to_string()))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS translation_cache (
                 cache_key   TEXT PRIMARY KEY,
                 segment_id  TEXT NOT NULL,
                 source_text TEXT NOT NULL,
                 target_lang TEXT NOT NULL,
                 model       TEXT NOT NULL,
                 translated_text TEXT NOT NULL,
                 confidence  REAL NOT NULL DEFAULT 0.0,
                 created_at  TEXT NOT NULL DEFAULT (datetime('now'))
             );",
        )
        .map_err(|e| Error::Cache(e.to_string()))?;

        Ok(Self { conn: Mutex::new(conn) })
    }

    /// Look up a cached translation.
    pub fn get(&self, source_text: &str, target_lang: &str, model: &str) -> Option<CachedTranslation> {
        let key = Self::compute_key(source_text, target_lang, model);
        let conn = self.conn.lock().ok()?;
        conn
            .query_row(
                "SELECT translated_text, confidence FROM translation_cache WHERE cache_key = ?1",
                [&key],
                |row| {
                    Ok(CachedTranslation {
                        translated_text: row.get(0)?,
                        confidence: row.get(1)?,
                    })
                },
            )
            .ok()
    }

    /// Store a translation result in the cache.
    pub fn put(
        &self,
        source_text: &str,
        segment_id: &str,
        target_lang: &str,
        model: &str,
        result: &TranslationResult,
    ) {
        let key = Self::compute_key(source_text, target_lang, model);
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(e) => {
                debug!("Cache lock failed: {e}");
                return;
            }
        };
        let res = conn.execute(
            "INSERT OR REPLACE INTO translation_cache
             (cache_key, segment_id, source_text, target_lang, model, translated_text, confidence)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                key,
                segment_id,
                source_text,
                target_lang,
                model,
                result.translated_text,
                result.confidence,
            ],
        );
        if let Err(e) = res {
            debug!("Cache write failed: {e}");
        }
    }

    /// Batch store multiple translations.
    pub fn put_batch(
        &self,
        items: &[(String, String, TranslationResult)], // (source_text, segment_id, result)
        target_lang: &str,
        model: &str,
    ) {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(e) => {
                debug!("Cache batch lock failed: {e}");
                return;
            }
        };
        // Use a transaction for batch writes.
        let tx = match conn.unchecked_transaction() {
            Ok(tx) => tx,
            Err(e) => {
                debug!("Cache batch transaction failed: {e}");
                return;
            }
        };

        for (source_text, segment_id, result) in items {
            let key = Self::compute_key(source_text, target_lang, model);
            let _ = tx.execute(
                "INSERT OR REPLACE INTO translation_cache
                 (cache_key, segment_id, source_text, target_lang, model, translated_text, confidence)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    key,
                    segment_id,
                    source_text,
                    target_lang,
                    model,
                    result.translated_text,
                    result.confidence,
                ],
            );
        }
        let _ = tx.commit();
    }

    /// Compute a cache key from source text, target language, and model.
    fn compute_key(source_text: &str, target_lang: &str, model: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(source_text.as_bytes());
        hasher.update(b"|");
        hasher.update(target_lang.as_bytes());
        hasher.update(b"|");
        hasher.update(model.as_bytes());
        let hash = hasher.finalize();
        hash.iter().map(|b| format!("{b:02x}")).collect::<String>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_roundtrip() {
        let cache = TranslationCache::in_memory().unwrap();
        let result = TranslationResult {
            segment_id: "body-0".into(),
            translated_text: "你好世界".into(),
            notes: vec![],
            applied_terms: Default::default(),
            confidence: 0.95,
        };

        cache.put("Hello World", "body-0", "chinese", "gpt-4o", &result);
        let cached = cache.get("Hello World", "chinese", "gpt-4o").unwrap();
        assert_eq!(cached.translated_text, "你好世界");
        assert!((cached.confidence - 0.95).abs() < f64::EPSILON);
    }

    #[test]
    fn cache_miss() {
        let cache = TranslationCache::in_memory().unwrap();
        assert!(cache.get("missing", "en", "gpt-4o").is_none());
    }

    #[test]
    fn cache_overwrite() {
        let cache = TranslationCache::in_memory().unwrap();
        let r1 = TranslationResult {
            segment_id: "s".into(),
            translated_text: "v1".into(),
            notes: vec![],
            applied_terms: Default::default(),
            confidence: 0.5,
        };
        let r2 = TranslationResult {
            segment_id: "s".into(),
            translated_text: "v2".into(),
            notes: vec![],
            applied_terms: Default::default(),
            confidence: 0.9,
        };
        cache.put("text", "s", "en", "model", &r1);
        cache.put("text", "s", "en", "model", &r2);
        let cached = cache.get("text", "en", "model").unwrap();
        assert_eq!(cached.translated_text, "v2");
    }
}
