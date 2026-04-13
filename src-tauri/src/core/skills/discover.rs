//! Pure filesystem SKILL.md discovery.
//!
//! All implementation is in `skillstar_skill_core::discovery`.
//! This module re-exports it for `crate::skill_discover::*` access.

pub use skillstar_skill_core::discovery::{
    discover_skills, dedupe_discovered_skills, source_priority,
    DiscoveredSkill,
};
