//! Per-provider model catalogs, stored outside the provider store.
//!
//! ## Why these moved out of the store
//!
//! v3 kept the whole discovered catalog in `provider.meta.model_catalog`,
//! including each model's untouched upstream JSON in `ModelCatalogEntry.raw`.
//! For a relay that answers `/v1/models` with several hundred models, that is a
//! few hundred kilobytes of pretty-printed JSON re-serialized into
//! `model_providers.json` on every single write — in the same file that holds
//! the user's credentials and bindings, and with no cap on its size.
//!
//! A catalog is refetchable; a binding is not. They do not belong in the same
//! file, and they do not deserve the same durability guarantees. So the v3 → v4
//! migration lifts each catalog out ([`super::migrate::ExtractedCatalog`]) and
//! this module is where it lands: one file per provider under
//! `<data_root>/cache/model_catalog/<provider_id>.json`.
//!
//! ## Why this exists in WP-2A rather than WP-2B
//!
//! WP-2B owns the *source strategy* — the embedded snapshot, the models.dev
//! refresh, and the precedence between them. This module owns only the L2 tier
//! that already existed: what one provider's own `/v1/models` returned. It is
//! here because the OpenCode writer reads it to build each model's `name` /
//! `limit` / `cost` block, so leaving the read side unimplemented while the
//! migration removed the write side would silently strip that metadata out of
//! everyone's `opencode.json`.
//!
//! Failures are deliberately soft. A missing or unreadable catalog yields an
//! empty list, exactly as `catalog_from_meta` did for a provider that had never
//! been fetched — a config file that writes without model metadata is degraded,
//! one that refuses to write at all is broken.

use super::types::ModelCatalogEntry;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tracing::warn;

/// `<data_root>/cache/model_catalog/` — one JSON file per provider.
pub fn catalog_cache_dir() -> PathBuf {
    // In tests the real data root is unreachable, override or not. A test that
    // forgets its guard should write into a throwaway directory, not into the
    // developer's `~/.skillstar/cache` — the cost of the mistake is otherwise
    // paid outside the test process, where nothing will report it.
    #[cfg(test)]
    return test_cache_override().unwrap_or_else(test_sandbox_cache_dir);
    #[cfg(not(test))]
    skillstar_core::infra::paths::data_root()
        .join("cache")
        .join("model_catalog")
}

// The test override is **thread-local**, not an env var.
//
// `SKILLSTAR_DATA_DIR` is process-global, and libtest runs one test per thread:
// a test that sets it re-roots every other test running at that moment, and a
// test whose temp dir is dropped leaves the survivors pointing at a directory
// that no longer exists. That failure mode is not hypothetical — it is the same
// one the tool-sync sandbox home was rewritten to avoid.
#[cfg(test)]
thread_local! {
    static TEST_CACHE_OVERRIDE: std::cell::RefCell<Option<PathBuf>> =
        const { std::cell::RefCell::new(None) };
}

/// Install (or clear) this thread's catalog cache dir, returning the previous.
#[cfg(test)]
pub(crate) fn set_test_cache_override(dir: Option<PathBuf>) -> Option<PathBuf> {
    TEST_CACHE_OVERRIDE.with(|slot| std::mem::replace(&mut *slot.borrow_mut(), dir))
}

#[cfg(test)]
fn test_cache_override() -> Option<PathBuf> {
    TEST_CACHE_OVERRIDE.with(|slot| slot.borrow().clone())
}

/// Per-process throwaway cache dir: the unit-test default when no guard is
/// installed. Shared across threads, so a test that depends on its *contents*
/// still needs its own [`set_test_cache_override`]; this only guarantees that
/// nothing escapes into the real data root.
#[cfg(test)]
fn test_sandbox_cache_dir() -> PathBuf {
    use std::sync::LazyLock;
    static DIR: LazyLock<PathBuf> = LazyLock::new(|| {
        let dir =
            std::env::temp_dir().join(format!("skillstar-catalog-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        dir
    });
    DIR.clone()
}

/// The cache file for one provider.
///
/// Provider ids are UUIDv4s or fixed slugs, but this is a path built from a
/// value that reaches us through IPC, so any separator or `..` segment is
/// flattened rather than trusted.
pub fn catalog_cache_path(provider_id: &str) -> PathBuf {
    catalog_cache_dir().join(format!("{}.json", sanitize_id(provider_id)))
}

fn sanitize_id(provider_id: &str) -> String {
    let safe: String = provider_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if safe.is_empty() {
        "provider".to_string()
    } else {
        safe
    }
}

/// Read one provider's cached catalog, or an empty list.
///
/// Never an error: see the module note on why a degraded write beats a refused
/// one. A parse failure is logged so a corrupt cache file is diagnosable
/// instead of merely quiet.
pub fn read_catalog(provider_id: &str) -> Vec<ModelCatalogEntry> {
    read_catalog_at(&catalog_cache_path(provider_id))
}

/// Same, against an explicit path (what the tests and the writers use).
pub fn read_catalog_at(path: &Path) -> Vec<ModelCatalogEntry> {
    if !path.exists() {
        return Vec::new();
    }
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) => {
            warn!("failed to read model catalog {}: {e}", path.display());
            return Vec::new();
        }
    };
    match serde_json::from_str::<Vec<ModelCatalogEntry>>(&text) {
        Ok(entries) => entries,
        Err(e) => {
            warn!("malformed model catalog {}: {e}", path.display());
            Vec::new()
        }
    }
}

/// Write one provider's catalog, replacing whatever was there.
pub fn write_catalog(provider_id: &str, entries: &[ModelCatalogEntry]) -> Result<()> {
    write_catalog_at(&catalog_cache_path(provider_id), entries)
}

/// Same, against an explicit path.
pub fn write_catalog_at(path: &Path, entries: &[ModelCatalogEntry]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let json =
        serde_json::to_string_pretty(entries).context("failed to serialize model catalog")?;
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, json.as_bytes())
        .with_context(|| format!("failed to write {}", temp.display()))?;
    std::fs::rename(&temp, path)
        .with_context(|| format!("failed to rename {} to {}", temp.display(), path.display()))?;
    Ok(())
}

/// Persist the catalogs the v3 → v4 migration lifted out of the store.
///
/// Returns the number written. A failure here is reported to the caller as a
/// warning string rather than an error: the store migration has already
/// committed by this point, and a catalog that failed to land is refetchable
/// with one click, so aborting would trade a recoverable gap for an
/// unrecoverable one.
pub fn persist_extracted(
    catalogs: &[super::migrate::ExtractedCatalog],
    warnings: &mut Vec<String>,
) -> usize {
    let mut written = 0;
    for catalog in catalogs {
        let entries: Vec<ModelCatalogEntry> = match serde_json::from_value(catalog.raw.clone()) {
            Ok(entries) => entries,
            Err(e) => {
                warnings.push(format!(
                        "provider {}: cached model catalog was not in the expected shape and was dropped ({e})",
                        catalog.provider_id
                    ));
                continue;
            }
        };
        match write_catalog(&catalog.provider_id, &entries) {
            Ok(()) => written += 1,
            Err(e) => warnings.push(format!(
                "provider {}: model catalog could not be cached ({e}); it will be refetched on demand",
                catalog.provider_id
            )),
        }
    }
    written
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str) -> ModelCatalogEntry {
        ModelCatalogEntry {
            id: id.to_string(),
            display_name: Some(format!("{id} (display)")),
            context_length: Some(128_000),
            ..ModelCatalogEntry::default()
        }
    }

    #[test]
    fn a_catalog_round_trips_through_the_cache_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("p1.json");
        let entries = vec![entry("model-a"), entry("model-b")];

        write_catalog_at(&path, &entries).unwrap();

        assert_eq!(read_catalog_at(&path), entries);
    }

    #[test]
    fn a_missing_or_corrupt_catalog_reads_as_empty_rather_than_failing() {
        let tmp = tempfile::TempDir::new().unwrap();
        let missing = tmp.path().join("never-written.json");
        assert!(read_catalog_at(&missing).is_empty());

        let corrupt = tmp.path().join("corrupt.json");
        std::fs::write(&corrupt, "{not json").unwrap();
        assert!(
            read_catalog_at(&corrupt).is_empty(),
            "a config file written without model metadata is degraded; refusing to write is broken"
        );
    }

    #[test]
    fn a_provider_id_cannot_escape_the_cache_directory() {
        let path = catalog_cache_path("../../etc/passwd");
        assert_eq!(
            path.parent().unwrap(),
            catalog_cache_dir(),
            "a traversal in the id must not move the file out of the cache dir"
        );
        assert!(!path.to_string_lossy().contains(".."));
    }
}
