//! Skill management modules.
//!
//! Child modules import shared infrastructure via `crate::core::{…}` or the
//! `super::…` re-exports below (for example `git_ops`, `sync`, `translation_cache`).

pub mod installed_skill;
pub mod local_skill;
pub mod discover;
pub mod repo_scanner;
pub mod skill_bundle;
pub mod skill_group;
pub mod skill_install;
pub mod skill_pack;
#[allow(dead_code)]
pub mod skill_update;
pub mod update_checker;

// ── Re-exports for `use super::{…}` inside skill submodules ───────────

pub use crate::core::ai::translation_cache;
pub use crate::core::git::ops as git_ops;
pub use crate::core::git::source_resolver;
pub use crate::core::projects::agents as agent_profile;
pub use crate::core::projects::sync;
pub use discover as skill_discover;
pub use super::lockfile;
pub use super::project_manifest;
pub use super::security_scan;
pub use super::skill;
