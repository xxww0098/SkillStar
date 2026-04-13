//! Shared skill types and helpers.
//!
//! All implementation moved to `skillstar_skill_core::shared` (which re-exports
//! from `marketplace_core`).

pub use skillstar_marketplace_core::{
    OfficialPublisher, Skill, SkillCategory, SkillType, extract_github_source_from_url,
};
pub use skillstar_skill_core::shared::{
    parse_skill_content, extract_skill_description, SkillContent,
};
