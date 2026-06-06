//! Session-scoped translation cache.
//!
//! Same text + same target language + same model → same translation. Keyed by
//! a blake3 hash of those three inputs. Bounded LRU-ish (we just drop oldest
//! half when capacity is exceeded) with a TTL so stale entries from previous
//! days don't accumulate in long-running sessions.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

const MAX_ENTRIES: usize = 1024;
const TTL: Duration = Duration::from_secs(60 * 60); // 1 hour

struct Entry {
    translation: String,
    inserted_at: Instant,
}

static CACHE: LazyLock<Mutex<HashMap<[u8; 32], Entry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn key(text: &str, target_lang: &str, model: &str) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(text.as_bytes());
    hasher.update(b"\x1f"); // unit separator
    hasher.update(target_lang.as_bytes());
    hasher.update(b"\x1f");
    hasher.update(model.as_bytes());
    *hasher.finalize().as_bytes()
}

pub fn get(text: &str, target_lang: &str, model: &str) -> Option<String> {
    let k = key(text, target_lang, model);
    let mut guard = CACHE.lock().ok()?;
    match guard.get(&k) {
        Some(e) if e.inserted_at.elapsed() < TTL => Some(e.translation.clone()),
        Some(_) => {
            // Expired — clean up while we're here.
            guard.remove(&k);
            None
        }
        None => None,
    }
}

pub fn insert(text: &str, target_lang: &str, model: &str, translation: String) {
    let k = key(text, target_lang, model);
    let Ok(mut guard) = CACHE.lock() else { return };
    if guard.len() >= MAX_ENTRIES {
        // Drop the oldest half — cheap and good enough for a session cache.
        let mut entries: Vec<_> = guard
            .iter()
            .map(|(k, v)| (*k, v.inserted_at))
            .collect();
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

#[cfg(test)]
pub(crate) fn clear_for_tests() {
    if let Ok(mut guard) = CACHE.lock() {
        guard.clear();
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
