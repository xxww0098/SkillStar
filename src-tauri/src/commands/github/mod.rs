//! GitHub / repo and storage-management commands.
//!
//! Historically this was a single `github.rs` that had accreted several
//! unrelated concerns. It is split by concern for navigability; all command
//! names remain reachable as `commands::github::<name>` via the re-exports
//! below, so the IPC registration in `lib.rs` is unaffected.
//!
//! - [`repo`] — gh CLI status, publish, repo scan/install, new-skill detection.
//! - [`storage`] — Settings storage overview + cache/force-delete maintenance.

mod auth;
mod repo;
mod storage;

pub use auth::*;
pub use repo::*;
pub use storage::*;
