//! skillstar-skills: skill library + agents + projects + patrol + terminal.
//!
//! Owns install/update/uninstall, lockfile, update detection, local authoring,
//! agent registry, project deployment, patrol check logic, and Launch Deck
//! terminal helpers. Callers should use the narrow public modules rather than
//! reaching through temporary re-exports.
//!
//! | Module | Responsibility |
//! |---|---|
//! | [`lockfile`] / internal update checker | Installed-skill records and repo update detection |
//! | [`agents`] / [`projects`] / [`deployment`] | Agent profiles, project manifest, link-copy deploy |
//! | [`patrol`] | Patrol config/types and execution |
//! | library modules | install, update, bundle, local, repo scan, discovery, groups |

pub mod content;
mod content_copy;
mod discovery;
mod frontmatter;
pub mod git;
pub mod git_skill;
pub mod github_auth;
pub mod lockfile;
mod shared;
pub mod source_resolver;
pub mod tutorial;

pub mod installed_skill;
pub mod local_skill;
pub(crate) mod repo_link;
pub mod repo_scanner;
pub mod share_install;
pub mod shared_channels;
pub mod skill_bundle;
pub mod skill_group;
pub mod skill_install;
pub mod skill_pack;
pub mod skill_update;
mod update_checker;
pub mod update_state;

// Agent / project / deployment / patrol / terminal (merged from former skillstar-projects)
pub mod agents;
pub mod deployment;
pub mod patrol;
pub mod projects;

// ── Convenience re-exports ─────────────────────────────────────────

pub use discovery::{DiscoveredSkill, discover_skills};
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
