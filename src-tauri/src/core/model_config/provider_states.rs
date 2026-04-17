//! Unified provider state store — combines health, quota, and circuit-breaker
//! state into a single JSON file to reduce file count.
//!
//! Storage: `~/.skillstar/state/provider_states.json`
//!
//! Old individual files are migrated lazily on first load:
//!   - `provider_health.json`   → merged into `health.results`
//!   - `provider_quota.json`    → merged into `quotas.quotas`
//!   - `circuit_breakers.json` → merged into `circuit_breakers`
//!
//! All other modules (`health`, `quota`, `circuit_breaker`) delegate
//! read/write to this module so there is exactly one file on disk.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use super::atomic_write;
use crate::core::infra::paths;

// ── Key types (re-exported so callers don't need to know internal shape) ──

pub use super::circuit_breaker::{CircuitBreakerRecord, CircuitState};
pub use super::health::{HealthStore, ProviderHealth};
pub use super::quota::{ProviderQuota, QuotaStore};

// ── Unified store ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProviderStates {
    #[serde(default)]
    pub health: HealthStore,
    #[serde(default)]
    pub quotas: QuotaStore,
    /// Keyed by "app_id/provider_id"
    #[serde(default)]
    pub circuit_breakers: HashMap<String, CircuitBreakerRecord>,
}

fn store_path() -> PathBuf {
    paths::state_dir().join("provider_states.json")
}

/// Old individual state file paths for migration.
fn old_health_path() -> PathBuf {
    paths::state_dir().join("provider_health.json")
}
fn old_quota_path() -> PathBuf {
    paths::state_dir().join("provider_quota.json")
}
fn old_breaker_path() -> PathBuf {
    paths::state_dir().join("circuit_breakers.json")
}

// ── Load (with one-time migration from old files) ───────────────────────

static MIGRATED: std::sync::LazyLock<std::sync::Mutex<bool>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(false));

/// Load the unified store, migrating from old individual files if needed.
/// Migration runs exactly once per process.
pub fn load() -> Result<ProviderStates> {
    // Run migration guard once
    {
        let _guard = MIGRATED.lock().unwrap();
    }

    let path = store_path();

    // Fast path: if unified file exists, use it
    if path.exists() {
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        let text = text.trim_start_matches('\u{FEFF}');
        return serde_json::from_str(&text)
            .with_context(|| format!("Failed to parse {}", path.display()));
    }

    // Migration path: collect from old individual files
    let mut states = ProviderStates::default();

    if old_health_path().exists() {
        if let Ok(text) = std::fs::read_to_string(old_health_path()) {
            if let Ok(store) = serde_json::from_str::<HealthStore>(&text) {
                states.health = store;
            }
        }
    }

    if old_quota_path().exists() {
        if let Ok(text) = std::fs::read_to_string(old_quota_path()) {
            if let Ok(store) = serde_json::from_str::<QuotaStore>(&text) {
                states.quotas = store;
            }
        }
    }

    if old_breaker_path().exists() {
        if let Ok(text) = std::fs::read_to_string(old_breaker_path()) {
            if let Ok(map) = serde_json::from_str::<HashMap<String, CircuitBreakerRecord>>(&text) {
                states.circuit_breakers = map;
            }
        }
    }

    // Persist migrated state
    if let Err(e) = save(&states) {
        tracing::warn!("Failed to persist migrated provider states: {}", e);
    }

    Ok(states)
}

/// Persist the unified store.
pub fn save(states: &ProviderStates) -> Result<()> {
    let path = store_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(states)?;
    atomic_write(&path, json.as_bytes())
}

// ── Convenience accessors used by health / quota / circuit_breaker modules ──

pub fn get_health(app_id: &str, provider_id: &str) -> Option<ProviderHealth> {
    let key = format!("{}/{}", app_id, provider_id);
    load().ok()?.health.results.get(&key).cloned()
}

pub fn get_quota(app_id: &str, provider_id: &str) -> Option<ProviderQuota> {
    let key = format!("{}/{}", app_id, provider_id);
    load().ok()?.quotas.quotas.get(&key).cloned()
}

pub fn get_circuit(app_id: &str, provider_id: &str) -> Option<CircuitBreakerRecord> {
    let key = format!("{}/{}", app_id, provider_id);
    load().ok()?.circuit_breakers.get(&key).cloned()
}
