//! Persistence of per-agent user preferences, decoupled from agent definitions.
//!
//! `ProfilePrefs` (the enable/disable map + custom agents) is loaded/saved
//! through the `PrefsStore` trait, so the registry can be driven by an in-memory
//! store in tests instead of touching `~/.skillstar/config/profiles.toml`.

use std::path::PathBuf;

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
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content =
            toml::to_string_pretty(prefs).context("Failed to serialize profile preferences")?;
        std::fs::write(&path, content).context("Failed to write profile preferences")?;
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
