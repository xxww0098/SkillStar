//! Application use cases for the Usage subscription tracker, split by use
//! case: read projections, CRUD, usage refresh, OAuth flows, account
//! switching, the gateway's 401 self-heal, and the consumption summary, over
//! a shared helper module.

mod crud;
mod helpers;
mod oauth;
mod projections;
mod reauth;
mod refresh;
mod summary;
mod switching;

pub use crud::*;
pub use oauth::*;
pub use projections::*;
pub use reauth::HealingBook;
pub use refresh::*;
pub use summary::get_consumption_summary;
pub use switching::*;

// Sibling modules inside `usage` address these helpers as
// `service::{fill_active, map_err}` (see `token_import`), so the paths stay
// stable across the split.
pub(super) use helpers::{fill_active, map_err};

// The tests module globs `super::*`; these re-imports keep the crate items
// and private helpers resolvable from there without widening visibility.
#[cfg(test)]
use super::dto::*;
#[cfg(test)]
use chrono::Utc;
#[cfg(test)]
use helpers::{append_network_hint, mark_credentials_rotated, network_hint_targets};
#[cfg(test)]
use refresh::refresh_failure;
#[cfg(test)]
use skillstar_usage::catalog::AuthMode;
#[cfg(test)]
use skillstar_usage::subscription::{BillingCycle, Subscription};
#[cfg(test)]
use skillstar_core::config::proxy;
#[cfg(test)]
use skillstar_usage::{UsageError, crypto, storage};

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;
