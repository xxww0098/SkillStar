//! Project management: registration, manifest, agent profiles, and skill sync.

pub mod agents;
pub mod sync;

/// Project manifest CRUD operations.
#[allow(unused_imports)]
pub use super::project_manifest as manifest;
