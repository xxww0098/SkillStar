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

pub mod installed_skill;
pub mod local_skill;
pub mod repo_scanner;
pub mod skill_bundle;
pub mod skill_install;
pub mod skill_pack;
pub mod skill_update;
pub mod update_checker;

pub use skillstar_projects::project_manifest;

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

#[cfg(test)]
pub(crate) fn test_env_lock() -> &'static std::sync::Mutex<()> {
    use std::sync::{Mutex, OnceLock};
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[cfg(test)]
pub(crate) fn lock_test_env() -> std::sync::MutexGuard<'static, ()> {
    test_env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
