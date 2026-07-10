//! Translation cache.
//!
//! Same text + same target language + same model → same translation. A small
//! in-memory layer keeps hot session reads cheap; a SQLite layer under
//! `~/.skillstar/db/translation_cache.db` makes skill translations reusable
//! across app restarts.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use rusqlite::{Connection, OptionalExtension, params};

const MEMORY_MAX_ENTRIES: usize = 2048;
const TTL: Duration = Duration::from_secs(60 * 60); // 1 hour
const CACHE_SCHEMA_VERSION: &str = "skillstar-ai.translate.v2";

struct Entry {
    translation: String,
    inserted_at: Instant,
}

static CACHE: LazyLock<Mutex<HashMap<[u8; 32], Entry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static DB: LazyLock<Mutex<Option<Connection>>> = LazyLock::new(|| Mutex::new(None));

#[derive(Debug, Clone, Copy)]
enum CacheKind {
    Segment,
    Document,
}

impl CacheKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Segment => "segment",
            Self::Document => "document",
        }
    }
}

fn key(kind: CacheKind, text: &str, target_lang: &str, model: &str) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(CACHE_SCHEMA_VERSION.as_bytes());
    hasher.update(b"\x1f");
    hasher.update(kind.as_str().as_bytes());
    hasher.update(b"\x1f");
    hasher.update(text.as_bytes());
    hasher.update(b"\x1f"); // unit separator
    hasher.update(target_lang.as_bytes());
    hasher.update(b"\x1f");
    hasher.update(model.as_bytes());
    *hasher.finalize().as_bytes()
}

fn key_hex(key: &[u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for byte in key {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

pub fn get(text: &str, target_lang: &str, model: &str) -> Option<String> {
    get_kind(CacheKind::Segment, text, target_lang, model)
}

pub fn insert(text: &str, target_lang: &str, model: &str, translation: String) {
    insert_kind(CacheKind::Segment, text, target_lang, model, translation);
}

pub fn get_document(markdown: &str, target_lang: &str, model: &str) -> Option<String> {
    get_kind(CacheKind::Document, markdown, target_lang, model)
}

pub fn insert_document(markdown: &str, target_lang: &str, model: &str, translation: String) {
    insert_kind(
        CacheKind::Document,
        markdown,
        target_lang,
        model,
        translation,
    );
}

fn get_kind(kind: CacheKind, text: &str, target_lang: &str, model: &str) -> Option<String> {
    let k = key(kind, text, target_lang, model);
    let mut guard = CACHE.lock().ok()?;
    match guard.get(&k) {
        Some(e) if e.inserted_at.elapsed() < TTL => Some(e.translation.clone()),
        Some(_) => {
            // Expired — clean up while we're here.
            guard.remove(&k);
            None
        }
        None => {
            drop(guard);
            let hit = persistent_get(&k)?;
            remember(k, hit.clone());
            Some(hit)
        }
    }
}

fn insert_kind(kind: CacheKind, text: &str, target_lang: &str, model: &str, translation: String) {
    let k = key(kind, text, target_lang, model);
    remember(k, translation.clone());
    persistent_insert(&k, kind, target_lang, model, &translation);
}

fn remember(k: [u8; 32], translation: String) {
    let Ok(mut guard) = CACHE.lock() else { return };
    if guard.len() >= MEMORY_MAX_ENTRIES {
        // Drop the oldest half — cheap and good enough for a session cache.
        let mut entries: Vec<_> = guard.iter().map(|(k, v)| (*k, v.inserted_at)).collect();
        entries.sort_by_key(|(_, t)| *t);
        let drop_count = entries.len() / 2;
        for (key, _) in entries.into_iter().take(drop_count) {
            guard.remove(&key);
        }
    }
    guard.insert(
        k,
        Entry {
            translation,
            inserted_at: Instant::now(),
        },
    );
}

fn persistent_get(k: &[u8; 32]) -> Option<String> {
    let key = key_hex(k);
    with_conn(|conn| {
        let hit = conn
            .query_row(
                "SELECT translation FROM translation_cache WHERE cache_key = ?1",
                [&key],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if hit.is_some() {
            let _ = conn.execute(
                "UPDATE translation_cache
                    SET hit_count = hit_count + 1, last_used_at = unixepoch()
                  WHERE cache_key = ?1",
                [&key],
            );
        }
        Ok(hit)
    })
    .flatten()
}

fn persistent_insert(
    k: &[u8; 32],
    kind: CacheKind,
    target_lang: &str,
    model: &str,
    translation: &str,
) {
    let key = key_hex(k);
    let _ = with_conn(|conn| {
        conn.execute(
            "INSERT INTO translation_cache (
                cache_key, kind, target_lang, model, translation,
                created_at, updated_at, last_used_at, hit_count
             ) VALUES (?1, ?2, ?3, ?4, ?5, unixepoch(), unixepoch(), unixepoch(), 0)
             ON CONFLICT(cache_key) DO UPDATE SET
                translation = excluded.translation,
                updated_at = unixepoch(),
                last_used_at = unixepoch()",
            params![key, kind.as_str(), target_lang, model, translation],
        )?;
        Ok(())
    });
}

fn with_conn<T>(f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> Option<T> {
    let mut guard = DB.lock().ok()?;
    if guard.is_none() {
        *guard = open_conn().ok();
    }
    let conn = guard.as_ref()?;
    f(conn).ok()
}

fn open_conn() -> rusqlite::Result<Connection> {
    let db_dir = skillstar_core::infra::paths::db_dir();
    let _ = std::fs::create_dir_all(&db_dir);
    let conn = Connection::open(db_dir.join("translation_cache.db"))?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA busy_timeout=5000;
         CREATE TABLE IF NOT EXISTS translation_cache (
            cache_key TEXT PRIMARY KEY,
            kind TEXT NOT NULL,
            target_lang TEXT NOT NULL,
            model TEXT NOT NULL,
            translation TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            last_used_at INTEGER NOT NULL,
            hit_count INTEGER NOT NULL DEFAULT 0
         );
         CREATE INDEX IF NOT EXISTS idx_translation_cache_kind_target_model
            ON translation_cache(kind, target_lang, model);
         CREATE INDEX IF NOT EXISTS idx_translation_cache_last_used
            ON translation_cache(last_used_at);",
    )?;
    Ok(conn)
}

#[cfg(test)]
pub(crate) fn clear_for_tests() {
    if let Ok(mut guard) = CACHE.lock() {
        guard.clear();
    }
    if let Ok(mut guard) = DB.lock() {
        *guard = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_get_roundtrip() {
        clear_for_tests();
        insert("hello", "zh-CN", "gpt-4o", "你好".into());
        assert_eq!(get("hello", "zh-CN", "gpt-4o"), Some("你好".to_string()));
    }

    #[test]
    fn miss_on_different_lang() {
        clear_for_tests();
        insert("hello", "zh-CN", "gpt-4o", "你好".into());
        assert_eq!(get("hello", "ja", "gpt-4o"), None);
    }

    #[test]
    fn miss_on_different_model() {
        clear_for_tests();
        insert("hello", "zh-CN", "gpt-4o", "你好".into());
        assert_eq!(get("hello", "zh-CN", "claude-sonnet"), None);
    }
}
