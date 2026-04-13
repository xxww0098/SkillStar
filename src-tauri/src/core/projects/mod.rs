//! Project management: registration, manifest, agent profiles, and skill sync.

pub mod agents;
pub mod sync;

/// Project manifest CRUD operations.
pub use super::project_manifest as manifest;
