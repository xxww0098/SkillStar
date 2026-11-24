//! The agent registry: the runtime `AgentProfile` and the engine that produces
//! the enriched profile list from specs + persisted prefs.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

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
        self.into_profiles_inner(true)
    }

    /// Produce profiles without scanning each Agent's global skills directory
    /// for `synced_count` (`synced_count` stays `0`).
    pub(crate) fn into_profile_specs(self) -> Vec<AgentProfile> {
        self.into_profiles_inner(false)
    }

    fn into_profiles_inner(self, count_entries: bool) -> Vec<AgentProfile> {
        let home = ss_core::infra::paths::home_dir();
        let specs = self.specs();
        let mut drafts: Vec<(bool, PathBuf, AgentProfile)> = Vec::with_capacity(specs.len());
        for spec in specs {
            let global_skills_dir = spec.resolve_global_dir(&home);
            let enabled = self.prefs.enabled.get(spec.id()).copied().unwrap_or(false);
            let supports_global = spec.supports_global();
            let profile = AgentProfile {
                id: spec.id().to_string(),
                display_name: spec.display_name().to_string(),
                icon: spec.icon().to_string(),
                global_skills_dir: global_skills_dir.clone(),
                project_skills_rel: spec.project_skills_rel().unwrap_or("").to_string(),
                installed: enabled,
                enabled,
                synced_count: 0,
            };
            drafts.push((supports_global, global_skills_dir, profile));
        }

        if count_entries {
            let counts = count_managed_entries_parallel(
                drafts
                    .iter()
                    .filter(|(supports_global, _, _)| *supports_global)
                    .map(|(_, dir, _)| dir.as_path()),
            );
            for (supports_global, dir, profile) in &mut drafts {
                if *supports_global {
                    profile.synced_count = counts.get(dir).copied().unwrap_or(0);
                }
            }
        }

        drafts.into_iter().map(|(_, _, profile)| profile).collect()
    }
}

/// Deduplicate `global_skills_dir` paths across profiles and scan distinct
/// existing directories in parallel.
fn count_managed_entries_parallel<'a>(
    dirs: impl Iterator<Item = &'a std::path::Path>,
) -> HashMap<PathBuf, u32> {
    let mut counts = HashMap::new();
    let mut to_scan = Vec::new();
    let mut canonical_count: Option<u32> = None;

    for dir in dirs {
        if counts.contains_key(dir) {
            continue;
        }
        if crate::deployment::targets_canonical_root(dir) {
            let n = *canonical_count
                .get_or_insert_with(|| crate::installer::installed_names().len() as u32);
            counts.insert(dir.to_path_buf(), n);
            continue;
        }
        if !dir.is_dir() {
            counts.insert(dir.to_path_buf(), 0);
            continue;
        }
        counts.insert(dir.to_path_buf(), 0);
        to_scan.push(dir.to_path_buf());
    }

    if to_scan.len() <= 1 {
        for dir in to_scan {
            let n = count_managed_entries(&dir);
            counts.insert(dir, n);
        }
        return counts;
    }

    let workers = std::thread::available_parallelism()
        .map_or(4, |n| n.get().min(8))
        .min(to_scan.len());
    let next_idx = AtomicUsize::new(0);
    let results: Vec<(PathBuf, u32)> = std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(workers);
        for _ in 0..workers {
            handles.push(scope.spawn(|| {
                let mut local = Vec::new();
                loop {
                    let idx = next_idx.fetch_add(1, Ordering::Relaxed);
                    let Some(dir) = to_scan.get(idx) else {
                        break;
                    };
                    local.push((dir.clone(), count_managed_entries(dir)));
                }
                local
            }));
        }
        handles
            .into_iter()
            .flat_map(|handle| handle.join().unwrap_or_default())
            .collect()
    });
    for (dir, n) in results {
        counts.insert(dir, n);
    }
    counts
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
            let owned = crate::deployment::owned_deployment(&entry.path(), &name);
            owned.is_owned() || owned.is_modified_copy()
        })
        .count() as u32
}

/// Whether an agent profile id exists among built-in or valid custom specs.
pub(crate) fn has_profile(id: &str, store: &dyn PrefsStore) -> bool {
    let id = compatible_profile_id(id);
    let registry = AgentRegistry::load(store);
    registry.specs().iter().any(|spec| spec.id() == id)
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
