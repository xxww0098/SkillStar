//! Git operations façade.
//!
//! Transport/tree/ops/history are owned by `skillstar-git` and re-exported
//! here for callers that already depend on `skillstar-skills`.
//! [`gh_manager`] and its REST client [`gh_rest`] stay in this crate because
//! they are coupled to content and the lockfile.

pub use skillstar_git::*;

pub mod gh_manager;
pub mod gh_rest;

#[cfg(test)]
mod gh_publish_tests;
