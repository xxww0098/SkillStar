//! Provider store for AI provider resolution and CRUD operations.
//!
//! Reads/writes `~/.skillstar/config/model_providers.json` to manage provider configurations.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::warn;
use url::Url;
use uuid::Uuid;

mod crud;
mod model_catalog;
mod presets;
mod store;
mod types;

// Re-export the full public API at the `providers::*` path so external callers
// (`skillstar_models::providers::NAME`, `crate::providers::NAME`) and downstream
// crates keep working unchanged after the split. A `pub use x::*;` glob also
// re-exports each module's `pub(crate)` helpers at crate visibility, so sibling
// submodules and the test modules can reach `store_path`, `get_app`, and the
// `default_codex_*` defaults through `super::*` / `crate::providers::*`.
pub use crud::*;
pub use model_catalog::*;
pub use presets::*;
pub use store::*;
pub use types::*;

#[cfg(test)]
mod proptests;
#[cfg(test)]
mod tests;
