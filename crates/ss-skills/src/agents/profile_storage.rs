//! Persistence of per-agent user preferences, decoupled from agent definitions.
//!
//! `ProfilePrefs` (the enable/disable map + custom agents) is loaded/saved
//! through the `PrefsStore` trait, so the registry can be driven by an in-memory
//! store in tests instead of touching `~/.skillstar/config/profiles.toml`.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::custom::CustomProfileDef;

/// Persisted user preferences: per-agent enable state + user-defined agents.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct ProfilePrefs {
    /// Map of agent id → enabled.
    pub enabled: std::collections::HashMap<String, bool>,
    #[serde(default)]
    pub custom_profiles: Vec<CustomProfileDef>,
    /// Recovery-only journal keyed by the physical Global skills directory.
    ///
    /// This is deliberately directory-scoped rather than Agent-scoped: multiple
    /// profiles can point at the same physical folder, so there is no valid
    /// per-Agent ownership record for its entries.
    #[serde(default)]
    pub suspended_global_skill_names: BTreeMap<String, Vec<String>>,
}

/// Abstraction over where preferences are read from / written to.
pub(crate) trait PrefsStore {
    fn load(&self) -> ProfilePrefs;
    fn save(&self, prefs: &ProfilePrefs) -> Result<()>;
}

/// Path to the TOML configuration file storing user preferences.
fn prefs_path() -> PathBuf {
    ss_core::infra::paths::profiles_config_path()
}

/// Stable enough to share a recovery journal between profiles that resolve to
/// the same existing Global skills directory. The journal is intentionally an
/// exact recovery record, not a persistent Agent identity: if a target later
/// resolves elsewhere, it is not silently remapped.
fn global_skills_target_key(target: &Path) -> String {
    let resolved = std::fs::canonicalize(target).unwrap_or_else(|_| target.to_path_buf());
    let mut key = resolved.to_string_lossy().replace('\\', "/");
    while key.len() > 1 && key.ends_with('/') {
        key.pop();
    }
    #[cfg(windows)]
    {
        key.make_ascii_lowercase();
    }
    key
}

fn normalized_skill_names(names: &[String]) -> Vec<String> {
    names
        .iter()
        .map(|name| name.trim())
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub(crate) fn suspended_global_skill_names(target: &Path, store: &dyn PrefsStore) -> Vec<String> {
    store
        .load()
        .suspended_global_skill_names
        .get(&global_skills_target_key(target))
        .cloned()
        .unwrap_or_default()
}

pub(crate) fn replace_suspended_global_skill_names(
    target: &Path,
    names: &[String],
    store: &dyn PrefsStore,
) -> Result<()> {
    let mut prefs = store.load();
    let key = global_skills_target_key(target);
    let names = normalized_skill_names(names);
    if names.is_empty() {
        prefs.suspended_global_skill_names.remove(&key);
    } else {
        prefs.suspended_global_skill_names.insert(key, names);
    }
    store.save(&prefs)
}

/// One-shot id renames applied when loading `profiles.toml`, so a stored
/// enable state survives a builtin profile being renamed. Keys map
/// persisted id → current builtin id. When both keys exist the newer
/// (current-id) state wins and the stale key is dropped.
const PROFILE_ID_RENAMES: &[(&str, &str)] = &[("windsurf", "devin-desktop")];

fn apply_profile_id_renames(prefs: &mut ProfilePrefs) -> bool {
    let mut changed = false;
    for (old, new) in PROFILE_ID_RENAMES {
        let Some(value) = prefs.enabled.remove(*old) else {
            continue;
        };
        changed = true;
        prefs.enabled.entry((*new).to_string()).or_insert(value);
    }
    changed
}

/// Production store: `~/.skillstar/config/profiles.toml`.
pub(crate) struct TomlPrefsStore;

impl PrefsStore for TomlPrefsStore {
    fn load(&self) -> ProfilePrefs {
        let path = prefs_path();
        if !path.exists() {
            return ProfilePrefs::default();
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            return ProfilePrefs::default();
        };
        let mut prefs: ProfilePrefs = toml::from_str(&content).unwrap_or_default();
        if apply_profile_id_renames(&mut prefs)
            && let Ok(fresh) = toml::to_string_pretty(&prefs)
        {
            let _ = ss_core::infra::fs_ops::atomic_write(&path, fresh.as_bytes());
        }
        prefs
    }

    fn save(&self, prefs: &ProfilePrefs) -> Result<()> {
        let path = prefs_path();
        let content =
            toml::to_string_pretty(prefs).context("Failed to serialize profile preferences")?;
        ss_core::infra::fs_ops::atomic_write(&path, content.as_bytes())
            .context("Failed to write profile preferences")?;
        Ok(())
    }
}

/// In-memory store for tests — never touches disk or env.
#[cfg(test)]
pub(crate) struct MemPrefsStore(std::cell::RefCell<ProfilePrefs>);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windsurf_enable_state_folds_onto_devin_desktop() {
        let mut prefs = ProfilePrefs::default();
        prefs.enabled.insert("windsurf".to_string(), true);
        assert!(apply_profile_id_renames(&mut prefs));
        assert_eq!(prefs.enabled.get("devin-desktop"), Some(&true));
        assert!(!prefs.enabled.contains_key("windsurf"));
    }

    #[test]
    fn newer_devin_desktop_state_wins_over_stale_windsurf_key() {
        let mut prefs = ProfilePrefs::default();
        prefs.enabled.insert("windsurf".to_string(), true);
        prefs.enabled.insert("devin-desktop".to_string(), false);
        assert!(apply_profile_id_renames(&mut prefs));
        assert_eq!(prefs.enabled.get("devin-desktop"), Some(&false));
        assert!(!prefs.enabled.contains_key("windsurf"));
    }

    #[test]
    fn id_renames_leave_unrelated_profiles_alone() {
        let mut prefs = ProfilePrefs::default();
        prefs.enabled.insert("claude".to_string(), true);
        assert!(!apply_profile_id_renames(&mut prefs));
        assert_eq!(prefs.enabled.len(), 1);
    }
}

#[cfg(test)]
impl MemPrefsStore {
    pub fn new() -> Self {
        Self(std::cell::RefCell::new(ProfilePrefs::default()))
    }
}

#[cfg(test)]
impl PrefsStore for MemPrefsStore {
    fn load(&self) -> ProfilePrefs {
        self.0.borrow().clone()
    }
    fn save(&self, prefs: &ProfilePrefs) -> Result<()> {
        *self.0.borrow_mut() = prefs.clone();
        Ok(())
    }
}
