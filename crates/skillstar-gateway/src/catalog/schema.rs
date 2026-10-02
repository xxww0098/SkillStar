//! Typed view of the models.dev api.json document.
//!
//! The catalog is a read-only cache of a third-party document, not a store
//! the gateway writes back: unknown fields are ignored, and a row that does
//! not parse reads as its default instead of blanking every other row. The
//! hand-written `Deserialize` impls keep that skip-local behavior, which the
//! derived path cannot express: a single malformed provider or model would
//! otherwise fail the whole map.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer};

/// One entry of `reasoning_options`: a `type` tag and the values it offers.
#[derive(Deserialize, Clone, Default)]
pub(crate) struct ReasoningOption {
    #[serde(default)]
    pub r#type: String,
    #[serde(default)]
    pub values: Vec<String>,
}

/// Per-minute and per-request limits as models.dev reports them.
///
/// No consumer reads them yet; they exist for the cost/limit projections
/// the evolution slices build on top of this parse.
#[derive(Deserialize, Clone, Default)]
#[allow(dead_code)]
pub(crate) struct CatalogLimit {
    #[serde(default)]
    pub requests: Option<u64>,
    #[serde(default)]
    pub tokens: Option<u64>,
    #[serde(default)]
    pub context: Option<u64>,
    #[serde(default)]
    pub output: Option<u64>,
}

/// Price per million tokens as models.dev reports them.
///
/// No consumer reads them yet; they exist for the cost projections the
/// evolution slices build on top of this parse.
#[derive(Deserialize, Clone, Default)]
#[allow(dead_code)]
pub(crate) struct CatalogCost {
    #[serde(default)]
    pub input: Option<f64>,
    #[serde(default)]
    pub output: Option<f64>,
    #[serde(default)]
    pub cache_read: Option<f64>,
    #[serde(default)]
    pub cache_write: Option<f64>,
}

/// One model row of one provider. Only the fields consumers read are
/// declared; everything else in the cached row is ignored.
#[derive(Deserialize, Clone, Default)]
#[allow(dead_code)]
pub(crate) struct CatalogModel {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    /// Catalog-side family. A different concept from the `family` field on
    /// provider and group rows in `model_gateway.json`.
    #[serde(default)]
    pub family: Option<String>,
    #[serde(default, deserialize_with = "loose")]
    pub limit: Option<CatalogLimit>,
    #[serde(default, deserialize_with = "loose")]
    pub cost: Option<CatalogCost>,
    #[serde(default, deserialize_with = "loose")]
    pub reasoning_options: Vec<ReasoningOption>,
}

/// One provider row: its models, keyed by model id.
#[derive(Default)]
pub(crate) struct CatalogProvider {
    pub(crate) models: BTreeMap<String, CatalogModel>,
}

/// The whole document: providers keyed by provider id.
#[derive(Default)]
pub(crate) struct Catalog {
    pub(crate) providers: BTreeMap<String, CatalogProvider>,
}

impl<'de> Deserialize<'de> for Catalog {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let rows = BTreeMap::<String, serde_json::Value>::deserialize(deserializer)?;
        let providers = rows
            .into_iter()
            .map(|(name, row)| {
                let provider = CatalogProvider::deserialize(row).unwrap_or_default();
                (name, provider)
            })
            .collect();
        Ok(Self { providers })
    }
}

impl<'de> Deserialize<'de> for CatalogProvider {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let row = serde_json::Value::deserialize(deserializer)?;
        // A provider whose `models` is missing or not an object keeps its
        // entry with no models, which is the old skip: no ids, no lookup hit.
        let models = row
            .get("models")
            .cloned()
            .and_then(|models| {
                BTreeMap::<String, serde_json::Value>::deserialize(models).ok()
            })
            .unwrap_or_default();
        let models = models
            .into_iter()
            .map(|(id, row)| {
                let model = CatalogModel::deserialize(row).unwrap_or_default();
                (id, model)
            })
            .collect();
        Ok(Self { models })
    }
}

/// Deserialize a present field leniently: malformed reads as the default.
fn loose<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(T::deserialize(deserializer).unwrap_or_default())
}
