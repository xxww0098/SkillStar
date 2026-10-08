//! The effective price of a model id (lifted read-only from the retired
//! gateway domain). Session rows name a model, not the provider that
//! served it, so [`effective_price_by_model`] is the only lookup.
//!
//! Two tiers, first hit wins:
//!
//! 1. the top-level `prices` map of `config/model_gateway.json` — the
//!    user's override file. The first `"<catalog>/<model>"` key whose
//!    model half matches wins. A catalog-only wildcard does not: the
//!    session row does not say which catalog it belongs to. The file is
//!    a leftover of the removed model gateway; it is read leniently so a
//!    user's price overrides keep billing, but nothing writes it anymore;
//! 2. the models.dev cache
//!    (`<data_root>/cache/gateway-catalog/models.dev.json`), scanned in
//!    provider order. The cache is also a leftover: this module never
//!    refreshes it, so prices freeze at whatever the last sync stored
//!    (D-082).
//!
//! An explicit zero price is a price: an override row exists as soon as
//! its key is in the map, and a catalog cost counts as soon as it names
//! one unit. The units left out bill zero.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use ss_core::infra::paths::{config_dir, data_root};

/// Unit prices for one model, USD per million tokens. Reasoning tokens are
/// billed inside `output`, the way the usage fetchers fold the vendor
/// vocabularies together.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ModelCost {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
}

impl ModelCost {
    /// The cost of one turn in USD: Σ(token count × unit price) / 1e6.
    ///
    /// `tokens.reasoning` is already part of `tokens.output`, so it is
    /// deliberately not billed a second time.
    pub fn cost(&self, tokens: &TokenCounts) -> f64 {
        (tokens.input as f64 * self.input
            + tokens.output as f64 * self.output
            + tokens.cache_read as f64 * self.cache_read
            + tokens.cache_write as f64 * self.cache_write)
            / 1_000_000.0
    }

    /// One override row: every unit the row leaves out bills zero.
    fn from_row(row: &PriceRow) -> Self {
        Self {
            input: row.input,
            output: row.output,
            cache_read: row.cache_read,
            cache_write: row.cache_write,
        }
    }

    /// One catalog cost, when it names at least one unit. A cost with every
    /// unit absent carries no price at all (unpriced), not a zero price.
    fn from_catalog(cost: &CatalogCost) -> Option<Self> {
        let priced = cost.input.is_some()
            || cost.output.is_some()
            || cost.cache_read.is_some()
            || cost.cache_write.is_some();
        priced.then(|| Self {
            input: cost.input.unwrap_or_default(),
            output: cost.output.unwrap_or_default(),
            cache_read: cost.cache_read.unwrap_or_default(),
            cache_write: cost.cache_write.unwrap_or_default(),
        })
    }
}

/// The four token counts every consumption row bills over, plus the
/// reasoning count the session vocabulary folds into `output` (kept for
/// wire compatibility; session files always report it as 0).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenCounts {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub reasoning: u64,
}

/// One `prices` row of the override file.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
struct PriceRow {
    #[serde(default)]
    input: f64,
    #[serde(default)]
    output: f64,
    #[serde(default)]
    cache_read: f64,
    #[serde(default)]
    cache_write: f64,
}

/// The only slice of `model_gateway.json` this module reads.
#[derive(Clone, Debug, Default, Deserialize)]
struct PricesDoc {
    #[serde(default)]
    prices: BTreeMap<String, PriceRow>,
}

/// The only slice of a models.dev cache entry this module reads.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
struct CatalogCost {
    #[serde(default)]
    input: Option<f64>,
    #[serde(default)]
    output: Option<f64>,
    #[serde(default)]
    cache_read: Option<f64>,
    #[serde(default)]
    cache_write: Option<f64>,
}

/// The effective price of a model id whose provider is not known — the
/// session-file world, where a row names a model but not the provider
/// that served it.
///
/// Overrides first: the first `<catalog>/<model>` key whose model half
/// matches wins (BTreeMap order; model ids are unique enough across
/// providers that collisions are not worth an order rule). Then the
/// models.dev cache is scanned in provider order and the first provider
/// listing that model with a cost wins. A model nobody prices answers
/// `None` — unpriced, never free.
pub fn effective_price_by_model(model: &str) -> Option<ModelCost> {
    if model.is_empty() || model.contains('/') {
        return None;
    }
    let overrides = price_overrides_at(&config_dir().join("model_gateway.json"));
    if let Some(cost) = resolve_override_by_model(&overrides, model) {
        return Some(cost);
    }
    catalog_cost_by_model(&models_dev_cache_path(), model)
        .as_ref()
        .and_then(ModelCost::from_catalog)
}

/// Tier 1 of the by-model lookup: the first `<catalog>/<model>` key whose
/// model half matches (BTreeMap order; model ids are unique enough across
/// providers that collisions are not worth an order rule).
fn resolve_override_by_model(doc: &PricesDoc, model: &str) -> Option<ModelCost> {
    let suffix = format!("/{model}");
    doc.prices
        .range(suffix.clone()..)
        .find(|(key, _)| key.ends_with(suffix.as_str()))
        .map(|(_, row)| ModelCost::from_row(row))
}

/// The first cached provider that lists `model` with a cost, in BTreeMap
/// provider order.
fn catalog_cost_by_model(path: &Path, model: &str) -> Option<CatalogCost> {
    let cache: BTreeMap<String, serde_json::Value> =
        serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    cache
        .values()
        .filter_map(|provider| provider.get("models"))
        .filter_map(|models| models.get(model))
        .filter_map(|entry| entry.get("cost"))
        .cloned()
        .filter_map(|cost| serde_json::from_value::<CatalogCost>(cost).ok())
        .find(|cost| {
            cost.input.is_some()
                || cost.output.is_some()
                || cost.cache_read.is_some()
                || cost.cache_write.is_some()
        })
}

/// `<data_root>/cache/gateway-catalog/models.dev.json`, the cache the
/// retired gateway last wrote.
pub fn models_dev_cache_path() -> PathBuf {
    data_root()
        .join("cache")
        .join("gateway-catalog")
        .join("models.dev.json")
}

/// The override document at `path`; missing or broken reads as empty.
fn price_overrides_at(path: &Path) -> PricesDoc {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, body: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    #[test]
    fn broken_or_missing_files_read_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        let broken = dir.path().join("model_gateway.json");
        write(&broken, "{\"prices\":"); // truncated JSON
        assert!(price_overrides_at(&broken).prices.is_empty());
        assert!(
            price_overrides_at(&dir.path().join("absent.json"))
                .prices
                .is_empty()
        );
    }

    #[test]
    fn a_catalog_cost_must_name_a_unit() {
        let named = CatalogCost {
            output: Some(2.0),
            ..CatalogCost::default()
        };
        assert_eq!(
            ModelCost::from_catalog(&named),
            Some(ModelCost {
                output: 2.0,
                ..ModelCost::default()
            })
        );
        assert_eq!(ModelCost::from_catalog(&CatalogCost::default()), None);
    }

    #[test]
    fn cost_bills_the_four_units_over_a_million() {
        let cost = ModelCost {
            input: 0.27,
            output: 1.10,
            cache_read: 0.07,
            cache_write: 0.0,
        };
        let tokens = TokenCounts {
            input: 200,
            output: 40,
            cache_read: 10,
            cache_write: 0,
            reasoning: 0,
        };
        assert!(
            (cost.cost(&tokens) - (200.0 * 0.27 + 40.0 * 1.10 + 10.0 * 0.07) / 1e6).abs() < 1e-12
        );
    }

    #[test]
    fn by_model_lookup_scans_overrides_then_the_cache() {
        let dir = tempfile::tempdir().unwrap();
        let overrides = dir.path().join("model_gateway.json");
        let cache = dir.path().join("models.dev.json");
        write(
            &overrides,
            r#"{"prices":{"zai/glm-4.7":{"input":0.6},"deepseek":{"output":5.0}}}"#,
        );
        write(
            &cache,
            r#"{"zai":{"models":{"glm-4.7":{"cost":{"input":0.5}}}},
                "deepseek":{"models":{"deepseek-chat":{"cost":{"input":0.27,"output":1.10}},"free":{}}}}"#,
        );

        // Override rows win regardless of which provider the cache lists.
        let doc = price_overrides_at(&overrides);
        assert_eq!(
            resolve_override_by_model(&doc, "glm-4.7"),
            Some(ModelCost {
                input: 0.6,
                ..ModelCost::default()
            })
        );
        // A wildcard-only override never matches by model: the catalog it
        // belongs to is unknown in the session world.
        assert_eq!(resolve_override_by_model(&doc, "deepseek-chat"), None);
        assert_eq!(
            catalog_cost_by_model(&cache, "glm-4.7").unwrap().input,
            Some(0.5)
        );
        // Cache-only model: priced by the provider that lists it.
        assert_eq!(
            catalog_cost_by_model(&cache, "deepseek-chat")
                .unwrap()
                .output,
            Some(1.10)
        );
        // A model listed without a cost names no price.
        assert!(catalog_cost_by_model(&cache, "free").is_none());
        assert!(catalog_cost_by_model(&cache, "absent").is_none());
        assert!(catalog_cost_by_model(&cache, "a/b").is_none());
    }
}
