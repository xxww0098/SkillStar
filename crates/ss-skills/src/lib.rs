//! ss-skills: skill library, install/update, discovery and deployment.
//!
//! Owns install/update/uninstall, the shared skill lock, repo scan, frontmatter
//! validation, local authoring, bundles, project deployment, Agent profiles,
//! GitHub App identity, shared channels, patrol, and skill-only workflows.
//! Git transport lives in
//! `ss-git`. Callers should use the narrow public modules rather than
//! reaching through temporary re-exports.
//!
//! Install (D-081): shallow-clone the
//! source into a temp dir (`fetch`), copy chosen folders into the canonical
//! `~/.skillstar/data/skills/installed/<name>` directory (`installer`), record provenance with
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
//! | [`validation`] / [`discovery`] / [`plugin_manifest`] | Frontmatter parsing and install gate, repo scan, plugin manifests |
//! | [`team`] | Local team intelligence: BM25 recall, friction notes, skill health, digest |
//! | library modules | install, bundle, local, repo scan, groups |

pub mod agents;
pub mod channels;
pub mod content;
mod content_copy;
pub mod discovery;
pub mod fetch;
pub mod git;
pub mod git_skill;
pub mod github_auth;
pub mod health;
pub mod install_baseline;
pub mod installer;
pub mod legacy_cleanup;
mod materialize;
#[cfg(test)]
mod pack_fixture;
pub mod plugin_manifest;
pub mod skill_lock;
pub mod skill_mutation;
pub mod source_resolver;
pub mod team;
#[cfg(test)]
mod test_sandbox;
pub mod update;
mod update_check;
pub mod workflows;

pub mod installed_skill;
pub mod local_identity;
pub mod local_skill;
pub mod repo_scanner;
pub mod share_install;
pub mod skill_bundle;
pub mod skill_group;
pub mod skill_install;
pub mod skill_update;
pub mod storage_migration;
pub mod update_api;
pub mod update_state;
pub mod validation;

// Project / deployment; Git transport remains in `ss-git`.
pub mod deployment;
pub mod projects;

// ── Convenience re-exports ─────────────────────────────────────────
//
// Only exports with real external callers are kept here; everything else is
// reached through its owning module (`crate::discovery::DiscoveredSkill`,
// `crate::skill_lock`, `ss_core::types::…`).

pub use discovery::discover_skills;
pub use ss_core::types::{Skill, SkillContent};

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

/// Same lock as [`lock_test_env`], wrapped so `await_holding_lock` stays
/// quiet in async tests that must hold it across `.await` points (the env
/// stays pinned for the whole test; usage crate precedent).
#[cfg(test)]
pub(crate) struct TestEnvLock {
    _guard: std::sync::MutexGuard<'static, ()>,
}

#[cfg(test)]
pub(crate) fn lock_test_env_async() -> TestEnvLock {
    TestEnvLock {
        _guard: lock_test_env(),
    }
}
