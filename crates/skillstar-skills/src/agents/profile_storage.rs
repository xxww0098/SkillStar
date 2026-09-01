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
    skillstar_core::infra::paths::profiles_config_path()
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
        toml::from_str(&content).unwrap_or_default()
    }

    fn save(&self, prefs: &ProfilePrefs) -> Result<()> {
        let path = prefs_path();
        let content =
            toml::to_string_pretty(prefs).context("Failed to serialize profile preferences")?;
        skillstar_core::infra::fs_ops::atomic_write(&path, content.as_bytes())
            .context("Failed to write profile preferences")?;
        Ok(())
    }
}

/// In-memory store for tests — never touches disk or env.
#[cfg(test)]
pub(crate) struct MemPrefsStore(std::cell::RefCell<ProfilePrefs>);

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
