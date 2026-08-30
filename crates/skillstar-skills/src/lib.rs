//! skillstar-skills: skill library, install/update, discovery and deployment.
//!
//! Owns install/update/uninstall, lockfile, update detection, repo scan,
//! frontmatter validation, local authoring, bundles, project deployment and
//! terminal-independent skill content. Agents live in `skillstar-agents`,
//! patrol/shared channels in `skillstar-channels`, git transport in
//! `skillstar-git`, GitHub identity in `skillstar-github-auth`. Callers should
//! use the narrow public modules rather than reaching through temporary
//! re-exports.
//!
//! | Module | Responsibility |
//! |---|---|
//! | [`lockfile`] / [`update_checker`] / [`update_api`] | Installed-skill records and repo update detection |
//! | [`projects`] / [`deployment`] | Project manifest, link-copy deploy |
//! | [`validation`] / [`discovery`] / [`plugin_manifest`] | Frontmatter gate, repo scan, plugin manifests; pack-root shims |
//! | library modules | install, update, bundle, local, repo scan, groups |

pub mod content;
mod content_copy;
pub mod discovery;
pub mod git;
pub mod git_skill;
pub mod hub_entry;
pub mod lockfile;
mod pack_layout;
mod plugin_manifest;
pub mod skill_mutation;
pub mod source_resolver;
pub mod tutorial;

pub mod installed_skill;
pub mod local_skill;
pub mod repo_link;
pub mod repo_scanner;
pub mod share_install;
pub mod skill_bundle;
pub mod skill_group;
pub mod skill_hooks;
pub mod skill_install;
#[cfg(test)]
mod skill_install_removal_tests;
pub mod skill_pack;
pub mod skill_update;
pub mod update_api;
pub mod update_checker;
pub mod update_state;
pub mod validation;

// Agent / project / deployment / patrol / terminal
// (`agents` and `github_auth` now live in `skillstar-agents` /
// `skillstar-github-auth`; `git` transport/ops live in `skillstar-git`;
// `shared_channels` and `patrol` live in `skillstar-channels`)
pub mod deployment;
pub mod projects;

// ── Convenience re-exports ─────────────────────────────────────────
//
// Only exports with real external callers are kept here; everything else is
// reached through its owning module (`crate::discovery::DiscoveredSkill`,
// `crate::lockfile::LockEntry`, `skillstar_core::types::…`).

pub use discovery::discover_skills;
pub use skillstar_core::types::{Skill, SkillContent};

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
