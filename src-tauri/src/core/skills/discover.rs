//! Pure filesystem SKILL.md discovery.
//!
//! All implementation is in `skillstar_skill_core::discovery`.
//! This module re-exports it for `crate::skill_discover::*` access.

pub use skillstar_skill_core::discovery::{
    DiscoveredSkill, dedupe_discovered_skills, discover_skills, source_priority,
};
