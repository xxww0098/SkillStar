//! Skill management modules.
//!
//! Child modules import shared infrastructure via `crate::core::{…}` or the
//! `super::…` re-exports below (for example `git_ops`, `sync`, `translation_cache`).

pub mod discover;
pub mod installed_skill;
pub mod local_skill;
pub mod repo_scanner;
pub mod skill_bundle;
pub mod skill_group;
pub mod skill_install;
pub mod skill_pack;
#[allow(dead_code)]
pub mod skill_update;
pub mod update_checker;

// ── Re-exports for `use super::{…}` inside skill submodules ───────────

pub use super::lockfile;
pub use super::project_manifest;
pub use super::security_scan;
pub use super::skill;
pub use crate::core::ai::translation_cache;
pub use crate::core::git::ops as git_ops;
pub use crate::core::projects::agents as agent_profile;
pub use crate::core::projects::sync;

use crate::core::skill::Skill;

/// Unified lifecycle boundary for skill install / update / uninstall.
pub trait SkillManager {
    fn install_skill(&self, url: String, name: Option<String>) -> Result<Skill, String>;
    fn update_skill(&self, name: &str) -> Result<skill_update::SkillUpdateOutcome, anyhow::Error>;
    fn uninstall_skill(&self, name: &str) -> Result<(), String>;
}

/// Default implementation delegating to the existing free-function lifecycle helpers.
#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultSkillManager;

impl SkillManager for DefaultSkillManager {
    fn install_skill(&self, url: String, name: Option<String>) -> Result<Skill, String> {
        skill_install::install_skill(url, name)
    }

    fn update_skill(&self, name: &str) -> Result<skill_update::SkillUpdateOutcome, anyhow::Error> {
        skill_update::update_skill(name)
    }

    fn uninstall_skill(&self, name: &str) -> Result<(), String> {
        skill_install::uninstall_skill(name)
    }
}
