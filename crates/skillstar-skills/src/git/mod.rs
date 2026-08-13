//! Git operations façade.
//!
//! Transport/tree/ops/history are owned by `skillstar-git` and re-exported
//! here for callers that already depend on `skillstar-skills`.
//! [`gh_manager`] stays in this crate because it is coupled to
//! content and the lockfile.

pub use skillstar_git::*;

pub mod gh_manager;
