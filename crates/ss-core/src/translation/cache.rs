//! Persistent translation cache.
//!
//! Entries are rebuildable from the source text and the current engine, so
//! the file lives under `cache/`. The in-memory map reloads when the data
//! root changes, which is what tests do.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::config::{self, TranslationConfig};

#[derive(Default, Serialize, Deserialize)]
struct File {
    entries: HashMap<String, String>,
}

struct Store {
    root: PathBuf,
    entries: HashMap<String, String>,
}

static MEMORY: Mutex<Option<Store>> = Mutex::new(None);

fn with_store<T>(body: impl FnOnce(&mut Store) -> T) -> T {
    let mut guard = MEMORY
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = crate::infra::paths::data_root();
    let stale = guard.as_ref().is_none_or(|store| store.root != root);
    if stale {
        *guard = Some(Store::load(root));
    }
    body(guard.as_mut().expect("store"))
}

impl Store {
    fn load(root: PathBuf) -> Self {
        let path = crate::infra::paths::translation_cache_path();
        let entries = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<File>(&text).ok())
            .map(|file| file.entries)
            .unwrap_or_default();
        Self { root, entries }
    }

    fn save(&self) {
        let file = File {
            entries: self.entries.clone(),
        };
        let Ok(content) = serde_json::to_string(&file) else {
            return;
        };
        if let Err(error) = crate::infra::fs_ops::atomic_write(
            &crate::infra::paths::translation_cache_path(),
            content.as_bytes(),
        ) {
            tracing::warn!("failed to write translation cache: {error}");
        }
    }
}

pub fn cache_key(source: &str, target: &str, config: &TranslationConfig) -> String {
    format!("{}\u{1e}{source}", config.scope(target))
}

pub fn lookup(source: &str, target: &str) -> Option<String> {
    let config = config::load_config().unwrap_or_default();
    let key = cache_key(source, target, &config);
    with_store(|store| store.entries.get(&key).cloned())
}

pub fn remember(source: &str, target: &str, translated: &str) {
    let config = config::load_config().unwrap_or_default();
    let key = cache_key(source, target, &config);
    with_store(|store| {
        store.entries.insert(key, translated.to_string());
        store.save();
    });
}

#[cfg(test)]
mod tests {
    use super::{lookup, remember};
    use crate::translation::config::{Engine, TranslationConfig, save_config};
    use tempfile::TempDir;

    #[test]
    fn remembered_text_survives_a_reload_of_the_same_root() {
        let _guard = crate::config::test_env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp = TempDir::new().unwrap();
        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
        }

        save_config(&TranslationConfig {
            engine: Engine::Machine,
            ..TranslationConfig::default()
        })
        .unwrap();
        remember("Hello", "zh-CN", "你好");
        assert_eq!(lookup("Hello", "zh-CN").as_deref(), Some("你好"));
        assert!(lookup("Hello", "en").is_none());

        {
            let mut guard = super::MEMORY
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *guard = None;
        }
        assert_eq!(lookup("Hello", "zh-CN").as_deref(), Some("你好"));

        unsafe {
            std::env::remove_var("SKILLSTAR_DATA_DIR");
        }
    }
}
