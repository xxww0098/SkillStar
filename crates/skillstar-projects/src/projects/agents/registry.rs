//! The agent registry: the runtime `AgentProfile` and the engine that produces
//! the enriched profile list from specs + persisted prefs.

use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::builtin::{BuiltinSpec, builtin_agent_data};
use super::custom::CustomSpec;
use super::detect;
use super::profile_storage::{PrefsStore, ProfilePrefs};
use super::spec::AgentSpec;

/// A single agent profile describing where its skills directory lives.
///
/// FROZEN 8-field contract: serialized across the Tauri IPC boundary and
/// mirrored by the frontend `AgentProfile` interface. Do not reorder, rename,
/// retype, or remove fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProfile {
    /// Internal identifier, e.g. "claude", "gemini"
    pub id: String,
    /// Human-readable name shown in UI
    pub display_name: String,
    /// SVG filename in /public, e.g. "claude.svg"
    pub icon: String,
    /// Global skills directory (absolute path)
    pub global_skills_dir: PathBuf,
    /// Project-level skills path relative to project root, e.g. ".claude/skills"
    pub project_skills_rel: String,
    /// Whether the agent is detected as installed on this machine
    pub installed: bool,
    /// Whether the user has enabled syncing for this agent
    pub enabled: bool,
    /// Number of skills currently symlinked to this agent
    pub synced_count: u32,
}

impl AgentProfile {
    /// Whether this agent supports project-level skills.
    ///
    /// Global-only agents (e.g. OpenClaw) have an empty `project_skills_rel`.
    pub fn has_project_skills(&self) -> bool {
        !self.project_skills_rel.is_empty()
    }
}

/// Find an agent profile by its ID from a slice of profiles.
///
/// Returns `Err` if no profile matches. This is the canonical
/// replacement for the `.find(...).ok_or_else(...)` pattern.
pub fn find_profile<'a>(profiles: &'a [AgentProfile], agent_id: &str) -> Result<&'a AgentProfile> {
    profiles
        .iter()
        .find(|p| p.id == agent_id)
        .ok_or_else(|| anyhow::anyhow!("Agent profile '{}' not found", agent_id))
}

/// Holds a snapshot of persisted prefs and turns specs into enriched profiles.
pub(crate) struct AgentRegistry {
    prefs: ProfilePrefs,
}

impl AgentRegistry {
    /// Build a registry from any prefs store (prod: `TomlPrefsStore`; tests: in-memory).
    pub(crate) fn load(store: &dyn PrefsStore) -> Self {
        Self {
            prefs: store.load(),
        }
    }

    /// All specs, built-in first (in table order) then custom (in prefs order).
    fn specs(&self) -> Vec<Box<dyn AgentSpec + '_>> {
        let mut specs: Vec<Box<dyn AgentSpec + '_>> = Vec::new();
        for d in builtin_agent_data() {
            specs.push(Box::new(BuiltinSpec(d)));
        }
        for cp in &self.prefs.custom_profiles {
            specs.push(Box::new(CustomSpec(cp)));
        }
        specs
    }

    /// Produce the enriched profile list (install status + user prefs).
    ///
    /// Reproduces the previous `get_base_profiles` + `list_profiles` merge
    /// field-for-field: built-in then custom ordering, `None` project rel → "",
    /// then `installed` → `enabled` (defaulting to `installed`) → `synced_count`.
    pub(crate) fn into_profiles(self) -> Vec<AgentProfile> {
        let home = skillstar_core::infra::paths::home_dir();
        let mut out = Vec::new();
        for spec in self.specs() {
            let global_skills_dir = spec.resolve_global_dir(&home);
            let mut p = AgentProfile {
                id: spec.id().to_string(),
                display_name: spec.display_name().to_string(),
                icon: spec.icon().to_string(),
                global_skills_dir,
                project_skills_rel: spec.project_skills_rel().unwrap_or("").to_string(),
                installed: false,
                enabled: false,
                synced_count: 0,
            };
            p.installed = detect::detect_installed(spec.as_ref(), &p.global_skills_dir);
            p.enabled = self
                .prefs
                .enabled
                .get(&p.id)
                .copied()
                .unwrap_or(p.installed);
            p.synced_count = detect::count_symlinks(&p.global_skills_dir);
            out.push(p);
        }
        out
    }

    fn default_enabled_for(&self, id: &str) -> Option<bool> {
        let home = skillstar_core::infra::paths::home_dir();
        self.specs().into_iter().find_map(|spec| {
            if spec.id() != id {
                return None;
            }
            let global_skills_dir = spec.resolve_global_dir(&home);
            Some(detect::detect_installed(spec.as_ref(), &global_skills_dir))
        })
    }
}

/// Toggle an agent's enabled state, persisting the result.
///
/// When no explicit state exists, the current state mirrors the same read-only
/// install detection used by [`AgentRegistry::into_profiles`].
pub(crate) fn toggle(id: &str, store: &dyn PrefsStore) -> Result<bool> {
    let mut prefs = store.load();
    let registry = AgentRegistry {
        prefs: prefs.clone(),
    };
    let default_enabled = registry
        .default_enabled_for(id)
        .ok_or_else(|| anyhow::anyhow!("Agent profile '{}' not found", id))?;
    let current = prefs.enabled.get(id).copied().unwrap_or(default_enabled);
    let new_state = !current;
    prefs.enabled.insert(id.to_string(), new_state);
    store.save(&prefs)?;
    Ok(new_state)
}
