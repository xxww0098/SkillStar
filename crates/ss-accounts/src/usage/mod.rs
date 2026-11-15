//! Quota capability for the accounts crate.
//!
//! Owns the quota snapshot model and the per-agent fetchers, and refreshes the
//! tokens it needs through `crate::oauth`. Account records and persistence live
//! in `record` and `store`.

pub mod credentials;
pub mod error;
pub mod fetch;
pub mod snapshot;

pub use credentials::{Credentials, UsageOutcome};
pub use error::{UsageError, UsageResult};
pub use snapshot::{Credits, UsageSnapshot, UsageUnit, UsageWindow};

/// Read one account's quota.
pub fn read(agent: &str, creds: &Credentials) -> UsageResult<UsageOutcome> {
    fetch::fetch(agent, creds)
}
