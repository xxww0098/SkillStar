//! The models.dev catalog cache.
//!
//! [`cache`] owns the file. The rest of this module owns the only typed
//! parse of it: every reader, in this crate and in the app, goes through
//! [`catalog_ids`], [`serves`], [`entry`], or [`effort_values`].

pub(crate) mod cache;
pub(crate) mod ids;
pub(crate) mod schema;

pub use ids::catalog_ids;

use schema::{Catalog, CatalogModel};

/// The parsed cache. A missing or unreadable file reads as empty.
pub(crate) fn load() -> Catalog {
    serde_json::from_slice(&cache::models_dev_load()).unwrap_or_default()
}

/// Whether the cache lists `provider/model`.
///
/// `group` is a reserved provider name (saved groups live in
/// `model_gateway.json`), an empty side is no id, and a `/` inside the
/// model half would blur the flattened `provider/model` form.
pub fn serves(provider: &str, model: &str) -> bool {
    entry(provider, model).is_some()
}

/// The cached entry for `provider/model`, when the cache lists it.
pub fn entry(provider: &str, model: &str) -> Option<CatalogModel> {
    if provider.is_empty() || provider == "group" || model.is_empty() || model.contains('/') {
        return None;
    }
    load()
        .providers
        .get(provider)?
        .models
        .get(model)
        .cloned()
}

/// Effort levels the entry offers: `reasoning_options[type=effort].values`.
pub fn effort_values(provider: &str, model: &str) -> Vec<String> {
    entry(provider, model)
        .map(|entry| {
            entry
                .reasoning_options
                .iter()
                .find(|option| option.r#type == "effort")
                .map(|option| option.values.clone())
                .unwrap_or_default()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_cache(body: &[u8]) {
        let path = cache::models_dev_cache_path();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    /// Env-sandboxed catalog lookups; the cache path follows the data root.
    fn with_env_cache(body: &[u8], probe: impl FnOnce()) {
        let _lock = crate::TEST_PATH_ENV_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let root = std::env::temp_dir().join(format!(
            "skillstar-catalog-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let home = root.join("home");
        let data = root.join("data");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&data).unwrap();
        let pairs = [
            ("HOME", Some(home.clone())),
            ("USERPROFILE", Some(home.clone())),
            ("SKILLSTAR_TOOL_SYNC_HOME", Some(home.clone())),
            ("SKILLSTAR_DATA_DIR", Some(data.clone())),
        ];
        let saved: Vec<_> = pairs
            .iter()
            .map(|(key, value)| {
                let previous = std::env::var_os(key);
                // SAFETY: tests touching these vars hold the lock above.
                unsafe { std::env::set_var(key, value.clone().unwrap()) };
                (*key, previous)
            })
            .collect();
        write_cache(body);
        probe();
        for (key, previous) in saved {
            // SAFETY: see above.
            unsafe {
                match previous {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            };
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn serves_and_entry_follow_the_cache() {
        with_env_cache(
            br#"{"openai":{"models":{"gpt-test":{"name":"GPT Test","reasoning_options":[{"type":"effort","values":["low","high"]}]}}}}"#,
            || {
                assert!(serves("openai", "gpt-test"));
                assert!(!serves("openai", "missing"));
                assert!(!serves("", "gpt-test"));
                assert!(!serves("group", "gpt-test"));
                assert!(!serves("openai", ""));
                assert!(!serves("openai", "a/b"));
                let entry = entry("openai", "gpt-test").unwrap();
                assert_eq!(entry.name, "GPT Test");
                assert_eq!(
                    effort_values("openai", "gpt-test"),
                    vec!["low".to_string(), "high".to_string()]
                );
                assert!(effort_values("openai", "missing").is_empty());
                assert_eq!(
                    catalog_ids(),
                    vec!["openai/gpt-test".to_string()]
                );
            },
        );
    }

    #[test]
    fn a_broken_cache_reads_as_empty() {
        let truncated = br#"{"openai":{"models":"#[0..20].to_vec(); // not valid json
        with_env_cache(&truncated, || {
            assert!(catalog_ids().is_empty());
            assert!(!serves("openai", "gpt-test"));
            assert!(effort_values("openai", "gpt-test").is_empty());
        });
    }
}
