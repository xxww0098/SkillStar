//! Skill update mode.
//!
//! Settings shows one switch: on is manual, off is automatic. Automatic mode
//! also stores how often to check. The GUI process owns the monitor
//! (`ss-app::skill_wake`); this file is the only preference, so the monitor
//! does not depend on the shell. Manual mode leaves updates to the explicit
//! update entries.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Spacing used when the file omits the interval or names one outside
/// [`INTERVAL_CHOICES_MINUTES`].
///
/// One hour is the unset value. Turning automatic mode on still checks
/// immediately; this only spaces the runs after that, and it matches the
/// hourly channel auto-upgrade cadence.
pub const DEFAULT_INTERVAL_MINUTES: u64 = 60;

/// Intervals the Settings control can store. Anything else loads as
/// [`DEFAULT_INTERVAL_MINUTES`].
pub const INTERVAL_CHOICES_MINUTES: [u64; 5] = [15, 30, 60, 360, 1_440];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SkillUpdateConfig {
    /// `true`: check and apply Skill updates in the background.
    /// `false` (default): manual updates only. The Settings switch is the
    /// inverse: on means manual.
    pub auto_update: bool,
    /// Minutes between automatic checks. Only values in
    /// [`INTERVAL_CHOICES_MINUTES`] are kept; see [`resolve_interval_minutes`].
    #[serde(default = "default_interval_minutes")]
    pub interval_minutes: u64,
}

impl Default for SkillUpdateConfig {
    fn default() -> Self {
        Self {
            auto_update: false,
            interval_minutes: DEFAULT_INTERVAL_MINUTES,
        }
    }
}

fn default_interval_minutes() -> u64 {
    DEFAULT_INTERVAL_MINUTES
}

/// Map a stored or edited interval onto a choice the scheduler will honor.
pub fn resolve_interval_minutes(minutes: u64) -> u64 {
    if INTERVAL_CHOICES_MINUTES.contains(&minutes) {
        minutes
    } else {
        DEFAULT_INTERVAL_MINUTES
    }
}

fn config_path() -> PathBuf {
    crate::infra::paths::skill_updates_config_path()
}

pub fn load_config() -> Result<SkillUpdateConfig> {
    let path = config_path();
    if !path.exists() {
        return Ok(SkillUpdateConfig::default());
    }
    let content = std::fs::read_to_string(&path)?;
    let mut config: SkillUpdateConfig = serde_json::from_str(&content).unwrap_or_default();
    config.interval_minutes = resolve_interval_minutes(config.interval_minutes);
    Ok(config)
}

pub fn save_config(config: &SkillUpdateConfig) -> Result<()> {
    let path = config_path();
    let content = serde_json::to_string_pretty(config)?;
    crate::infra::fs_ops::atomic_write(&path, content.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{SkillUpdateConfig, load_config, save_config};
    use tempfile::TempDir;

    #[test]
    fn load_config_defaults_to_manual_when_missing() {
        let _guard = crate::config::test_env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp = TempDir::new().unwrap();

        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
        }

        let config = load_config().unwrap();
        assert!(!config.auto_update, "manual updates are the default");

        unsafe {
            std::env::remove_var("SKILLSTAR_DATA_DIR");
        }
    }

    #[test]
    fn save_and_load_config_roundtrip() {
        let _guard = crate::config::test_env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp = TempDir::new().unwrap();

        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
        }

        save_config(&SkillUpdateConfig {
            auto_update: true,
            interval_minutes: 360,
        })
        .unwrap();
        let saved = load_config().unwrap();
        assert!(saved.auto_update);
        assert_eq!(saved.interval_minutes, 360);

        save_config(&SkillUpdateConfig {
            auto_update: false,
            interval_minutes: super::DEFAULT_INTERVAL_MINUTES,
        })
        .unwrap();
        let saved = load_config().unwrap();
        assert!(!saved.auto_update);
        assert_eq!(saved.interval_minutes, super::DEFAULT_INTERVAL_MINUTES);

        unsafe {
            std::env::remove_var("SKILLSTAR_DATA_DIR");
        }
    }

    #[test]
    fn malformed_config_falls_back_to_manual() {
        let _guard = crate::config::test_env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp = TempDir::new().unwrap();

        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
        }

        let path = super::config_path();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{not json").unwrap();
        assert!(!load_config().unwrap().auto_update);

        unsafe {
            std::env::remove_var("SKILLSTAR_DATA_DIR");
        }
    }

    #[test]
    fn a_missing_interval_stays_on_the_default() {
        let _guard = crate::config::test_env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp = TempDir::new().unwrap();

        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
        }

        let path = super::config_path();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"auto_update":true}"#).unwrap();
        let config = load_config().unwrap();
        assert!(config.auto_update);
        assert_eq!(config.interval_minutes, super::DEFAULT_INTERVAL_MINUTES);

        unsafe {
            std::env::remove_var("SKILLSTAR_DATA_DIR");
        }
    }

    #[test]
    fn an_unknown_interval_falls_back_to_the_default() {
        let _guard = crate::config::test_env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp = TempDir::new().unwrap();

        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
        }

        let path = super::config_path();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"auto_update":true,"interval_minutes":7}"#).unwrap();
        assert_eq!(
            load_config().unwrap().interval_minutes,
            super::DEFAULT_INTERVAL_MINUTES
        );

        unsafe {
            std::env::remove_var("SKILLSTAR_DATA_DIR");
        }
    }
}
