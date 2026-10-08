//! The agent registry: the runtime `AgentProfile` and the engine that produces
//! the enriched profile list from specs + persisted prefs.

use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::builtin::{BuiltinSpec, builtin_agent_data};
use super::custom::CustomSpec;
use super::profile_storage::{PrefsStore, ProfilePrefs};
use super::spec::AgentSpec;

/// A single agent profile describing where its skills directory lives.
///
/// FROZEN 8-field contract: serialized across the Tauri IPC boundary and
/// mirrored by the frontend `AgentProfile` interface. Do not reorder, rename,
/// retype, or remove fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProfile {
    /// Internal identifier. Three legacy ids remain aliases of upstream names.
    pub id: String,
    /// Human-readable name shown in UI
    pub display_name: String,
    /// Icon descriptor (`lobe:<agent-id>` for built-ins, data URI for custom).
    pub icon: String,
    /// Global skills directory (absolute path)
    pub global_skills_dir: PathBuf,
    /// Project-level skills path relative to project root, e.g. ".claude/skills"
    pub project_skills_rel: String,
    /// Compatibility mirror of `enabled`; system installation is not probed.
    pub installed: bool,
    /// Whether the user has manually activated this agent in SkillStar.
    pub enabled: bool,
    /// Number of skills currently symlinked to this agent
    pub synced_count: u32,
}

impl AgentProfile {
    /// Whether this Agent supports global/user-level skills.
    pub fn has_global_skills(&self) -> bool {
        !self.global_skills_dir.as_os_str().is_empty()
    }

    /// Whether this agent supports project-level skills.
    ///
    /// Global-only custom agents have an empty `project_skills_rel`.
    pub fn has_project_skills(&self) -> bool {
        !self.project_skills_rel.is_empty()
    }
}

/// Find an agent profile by its ID from a slice of profiles.
///
/// Returns `Err` if no profile matches. This is the canonical
/// replacement for the `.find(...).ok_or_else(...)` pattern.
pub fn find_profile<'a>(profiles: &'a [AgentProfile], agent_id: &str) -> Result<&'a AgentProfile> {
    let agent_id = compatible_profile_id(agent_id);
    profiles
        .iter()
        .find(|p| p.id == agent_id)
        .ok_or_else(|| anyhow::anyhow!("Agent profile '{}' not found", agent_id))
}

/// Map standard Agent ids onto SkillStar's three legacy persisted ids, and
/// Antigravity's per-state ids onto the single profile that serves all three.
/// `windsurf` folds onto `devin-desktop`: the editor was rebranded Devin
/// Desktop in 2026-06 and SkillStar renamed the profile accordingly; the
/// id other tools still publish is `windsurf`.
pub fn compatible_profile_id(agent_id: &str) -> &str {
    match agent_id {
        "claude-code" => "claude",
        "kiro-cli" => "kiro",
        "hermes-agent" => "hermes",
        "antigravity-cli" | "antigravity-ide" => "antigravity",
        "windsurf" => "devin-desktop",
        id => id,
    }
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
        let home = ss_core::infra::paths::home_dir();
        for cp in &self.prefs.custom_profiles {
            if let Err(error) = stored_custom_is_usable(cp, &home) {
                tracing::warn!(
                    target: "agents",
                    id = %cp.id,
                    error = %error,
                    "skipping stored custom agent that fails directory validation"
                );
                continue;
            }
            specs.push(Box::new(CustomSpec(cp)));
        }
        specs
    }

    /// Produce profiles from static capabilities and persisted manual prefs.
    /// New profiles default to inactive. The frozen `installed` IPC field mirrors
    /// `enabled` for compatibility and never probes the host system.
    pub(crate) fn into_profiles(self) -> Vec<AgentProfile> {
        let home = ss_core::infra::paths::home_dir();
        let mut out = Vec::new();
        for spec in self.specs() {
            let global_skills_dir = spec.resolve_global_dir(&home);
            let enabled = self.prefs.enabled.get(spec.id()).copied().unwrap_or(false);
            let synced_count = if spec.supports_global() {
                count_managed_entries(&global_skills_dir)
            } else {
                0
            };
            let p = AgentProfile {
                id: spec.id().to_string(),
                display_name: spec.display_name().to_string(),
                icon: spec.icon().to_string(),
                global_skills_dir,
                project_skills_rel: spec.project_skills_rel().unwrap_or("").to_string(),
                installed: enabled,
                enabled,
                synced_count,
            };
            out.push(p);
        }
        out
    }
}

fn stored_custom_is_usable(
    def: &super::custom::CustomProfileDef,
    home: &std::path::Path,
) -> anyhow::Result<()> {
    let resolved = CustomSpec(def).resolve_global_dir(home);
    super::validation::validate_global_skills_dir(&resolved, home)?;
    super::validation::validate_project_skills_rel(&def.project_skills_rel)?;
    Ok(())
}

/// Count SkillStar's deployments in an Agent directory. An Agent that reads
/// the canonical root directly sees every installed Skill.
fn count_managed_entries(dir: &std::path::Path) -> u32 {
    if crate::deployment::targets_canonical_root(dir) {
        return crate::installer::installed_names().len() as u32;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            crate::deployment::owned_deployment(&entry.path(), &name).is_owned()
        })
        .count() as u32
}

/// Toggle an agent's enabled state, persisting the result.
///
/// When no explicit state exists, the profile is inactive.
pub(crate) fn toggle(id: &str, store: &dyn PrefsStore) -> Result<bool> {
    let id = compatible_profile_id(id);
    let mut prefs = store.load();
    let registry = AgentRegistry {
        prefs: prefs.clone(),
    };
    if !registry.specs().iter().any(|spec| spec.id() == id) {
        return Err(anyhow::anyhow!("Agent profile '{}' not found", id));
    }
    let current = prefs.enabled.get(id).copied().unwrap_or(false);
    let new_state = !current;
    prefs.enabled.insert(id.to_string(), new_state);
    store.save(&prefs)?;
    Ok(new_state)
}
