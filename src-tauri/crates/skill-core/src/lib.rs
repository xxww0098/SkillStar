//! Skill-core: self-contained skill management primitives.
//!
//! This crate owns the pure, dependency-free skill management logic:
//!
//! | Module | Responsibility |
//! |---|---|
//! | [`discovery`] | Pure filesystem SKILL.md scanning (priority + full-depth) |
//! | [`lockfile`] | Lockfile persistence (installed skill records) |
//! | [`shared`] | Shared types (`Skill`, `SkillContent`) and helpers |
//! | [`source_resolver`] | URL normalization and comparison |

pub mod discovery;
pub mod lockfile;
pub mod shared;
pub mod source_resolver;

// ── Convenience re-exports ─────────────────────────────────────────

pub use discovery::{
    DiscoveredSkill, PRIORITY_SKILL_DIRS, dedupe_discovered_skills, discover_skills,
    find_all_skill_md_files, source_priority,
};
pub use lockfile::{LockEntry, Lockfile};
pub use shared::{
    Skill, SkillCategory, SkillContent, SkillType, extract_github_source_from_url,
    extract_skill_description, parse_skill_content,
};
