//! Git operations façade.
//!
//! Transport/tree/ops/history are owned by `ss-git` and re-exported
//! here for callers that already depend on `ss-skills`.
//! [`gh_manager`] and its REST client [`gh_rest`] stay in this crate because
//! they are coupled to content and the lockfile.

pub use ss_git::*;

pub mod gh_manager;
pub mod gh_rest;

#[cfg(test)]
mod gh_publish_tests;
