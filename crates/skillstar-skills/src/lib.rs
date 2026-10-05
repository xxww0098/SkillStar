//! skillstar-skills: skill library, install/update, discovery and deployment.
//!
//! Owns install/update/uninstall, the shared skill lock, repo scan, frontmatter
//! validation, local authoring, bundles, project deployment, Agent profiles,
//! GitHub App identity, and terminal-independent skill content. patrol/shared
//! channels live in `skillstar-channels`; git transport lives in
//! `skillstar-git`. Callers should use the narrow public modules rather than
//! reaching through temporary re-exports.
//!
//! Install follows the vercel `npx skills` pipeline (D-081): shallow-clone the
//! source into a temp dir (`fetch`), copy chosen folders into the canonical
//! `~/.agents/skills/<name>` directory (`installer`), record provenance with
//! the upstream tree SHA in `.skill-lock.json` (`skill_lock`), and refresh by
//! overwrite-reinstall when the tree SHA moves (`update`). Agent deploy and
//! project-vs-global scope stay at the caller.
//!
//! | Module | Responsibility |
//! |---|---|
//! | [`skill_lock`] / [`update`] / [`update_api`] | Shared lock records and upstream tree-SHA update detection |
//! | [`projects`] / [`deployment`] | Project manifest, link-copy deploy |
//! | [`agents`] | Agent spec, registry, custom profiles, activation prefs |
//! | [`github_auth`] | GitHub App device flow, token store, API credential |
//! | [`validation`] / [`discovery`] / [`plugin_manifest`] | Frontmatter install gate (via `skill-spec`), repo scan, plugin manifests |
//! | [`team`] | Local team intelligence: BM25 recall, friction notes, skill health, digest |
//! | library modules | install, bundle, local, repo scan, groups |

pub mod agents;
pub mod content;
mod content_copy;
pub mod discovery;
pub mod fetch;
pub mod git;
pub mod git_skill;
pub mod github_auth;
pub mod installer;
pub mod legacy_cleanup;
pub mod plugin_manifest;
#[cfg(test)]
mod pack_fixture;
pub mod skill_lock;
pub mod skill_mutation;
pub mod source_resolver;
pub mod team;
pub mod update;

pub mod installed_skill;
pub mod local_identity;
pub mod local_skill;
pub mod repo_scanner;
pub mod share_install;
pub mod skill_bundle;
pub mod skill_group;
pub mod skill_install;
pub mod skill_update;
pub mod update_api;
pub mod update_state;
pub mod validation;

// project / deployment (`shared_channels` and `patrol` live in
// `skillstar-channels`; `git` transport/ops live in `skillstar-git`)
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
