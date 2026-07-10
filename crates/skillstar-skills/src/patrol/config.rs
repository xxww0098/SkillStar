//! Persistent configuration for background patrol.

use skillstar_core::infra::paths::patrol_state_path;

/// Patrol state file path (from infra).
pub fn config_path() -> std::path::PathBuf {
    patrol_state_path()
}

/// Load patrol configuration from disk.
pub fn load_config() -> PatrolConfig {
    let path = config_path();
    if !path.exists() {
        return PatrolConfig::default();
    }
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

/// Save patrol configuration to disk.
pub fn save_config(config: &PatrolConfig) -> anyhow::Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(config)?;
    std::fs::write(&path, content)?;
    Ok(())
}

// ── Re-export types from types module ──────────────────────────────────

pub use crate::patrol::types::PatrolConfig;

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn with_temp_dir<F>(suffix: &str, f: F)
    where
        F: FnOnce(std::path::PathBuf) -> anyhow::Result<()>,
    {
        // Share the crate-wide env lock so patrol tests serialize with the
        // `projects` tests — they all mutate the process-global
        // `SKILLSTAR_DATA_DIR`, so a per-module lock would let them race.
        let _guard = crate::projects::lock_test_env();

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp_root = std::env::temp_dir().join(format!("skillstar-patrol-{}-{}", suffix, stamp));
        let data_dir = temp_root.join("data");
        std::fs::create_dir_all(&data_dir).unwrap();

        // SAFETY: we hold the lock and restore/cleanup below
        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", &data_dir);
        }

        let result = f(data_dir);

        // SAFETY: restore env
        unsafe {
            std::env::remove_var("SKILLSTAR_DATA_DIR");
        }
        let _ = std::fs::remove_dir_all(&temp_root);

        result.unwrap();
    }

    #[test]
    fn load_config_returns_default_when_file_missing() {
        with_temp_dir("missing", |_data_dir| {
            // Config file does not exist — should return default
            let config = load_config();
            assert!(!config.enabled);
            assert_eq!(config.interval_secs, 30);
            Ok(())
        });
    }

    #[test]
    fn load_config_returns_default_when_json_invalid() {
        with_temp_dir("invalid-json", |data_dir| {
            // Write invalid JSON to the patrol state file
            let state_dir = data_dir.join("state");
            std::fs::create_dir_all(&state_dir).unwrap();
            std::fs::write(state_dir.join("patrol.json"), "{ invalid json }").unwrap();

            // Should fall back to default
            let config = load_config();
            assert!(!config.enabled);
            assert_eq!(config.interval_secs, 30);
            Ok(())
        });
    }

    #[test]
    fn save_load_roundtrip() {
        with_temp_dir("roundtrip", |_data_dir| {
            // Save a non-default config
            let original = PatrolConfig {
                enabled: true,
                interval_secs: 60,
            };
            save_config(&original).unwrap();

            // Load it back
            let loaded = load_config();
            assert!(loaded.enabled);
            assert_eq!(loaded.interval_secs, 60);
            Ok(())
        });
    }
}
