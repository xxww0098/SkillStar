//! The effective price of one catalog model (spec slice 08).
//!
//! [`effective_price`] resolves three tiers, first hit wins:
//!
//! 1. the top-level `prices` map of `model_gateway.json` — the user's
//!    override, read through the typed store container, exact
//!    `"<catalog>/<model>"` key first, then the `"<catalog>"` wildcard;
//! 2. the models.dev cache cost of that catalog entry;
//! 3. nothing — the model is unpriced.
//!
//! An explicit zero price is a price: an override row exists as soon as its
//! key is in the map, and a catalog cost counts as soon as it names one
//! unit. The units left out bill zero.

use crate::catalog::schema::CatalogCost;
use crate::ledger::TokenCounts;
use crate::store::doc::{ModelGatewayDoc, PriceRow};

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

/// The effective price of `catalog/model`.
///
/// The tier-1 lookup reads the store leniently, so a broken
/// `model_gateway.json` reads as no overrides and the catalog tier decides.
pub fn effective_price(catalog: &str, model: &str) -> Option<ModelCost> {
    let doc = ModelGatewayDoc::open_lenient();
    let exact = format!("{catalog}/{model}");
    if let Some(row) = doc
        .prices()
        .get(exact.as_str())
        .or_else(|| doc.prices().get(catalog))
    {
        return Some(ModelCost::from_row(row));
    }
    crate::catalog::entry(catalog, model)
        .and_then(|entry| entry.cost.as_ref().and_then(ModelCost::from_catalog))
}
