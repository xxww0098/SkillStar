//! Actions and catalog lookup for the accounts page.

use ss_usage::catalog::{CatalogEntry, catalog};

/// User actions dispatchable from an account row or confirm dialog.
#[derive(Clone, Debug)]
pub enum AccountAction {
    Activate(String),
    Refresh(String),
    ResetQuota(String, ss_usage::subscription::ResetWindow),
    Delete(String),
}

/// Look up provider display metadata from the catalog.
pub fn find_catalog_entry(catalog_id: &str) -> Option<CatalogEntry> {
    catalog().into_iter().find(|entry| entry.id == catalog_id)
}
