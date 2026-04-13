//! Persistent storage for dismissed new-skill notifications.
//!
//! When the user dismisses a ghost card (new skill available in a cached repo),
//! we record the dismissal key here so it won't reappear until a genuinely new
//! skill is added.
//!
//! File location: `~/.skillstar/state/dismissed_new_skills.json`

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Default, Serialize, Deserialize)]
struct DismissedStore {
    /// Set of dismissed keys in the format "repo_source/skill_id"
    dismissed: HashSet<String>,
}

fn store_path() -> std::path::PathBuf {
    crate::core::infra::paths::state_dir().join("dismissed_new_skills.json")
}

fn load_store() -> DismissedStore {
    let path = store_path();
    if !path.exists() {
        return DismissedStore::default();
    }
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

fn save_store(store: &DismissedStore) -> Result<()> {
    let path = store_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(store)?;
    std::fs::write(&path, content)?;
    Ok(())
}

/// Load all dismissed new-skill keys.
pub fn load_dismissed() -> Vec<String> {
    load_store().dismissed.into_iter().collect()
}

/// Dismiss a new-skill notification by key (format: "repo_source/skill_id").
pub fn dismiss(key: &str) -> Result<()> {
    let mut store = load_store();
    store.dismissed.insert(key.to_string());
    save_store(&store)
}

/// Dismiss multiple new-skill notifications at once.
pub fn dismiss_batch(keys: &[String]) -> Result<()> {
    let mut store = load_store();
    for key in keys {
        store.dismissed.insert(key.clone());
    }
    save_store(&store)
}

/// Remove dismissed entries that are no longer valid (the skill was installed
/// or the repo was removed). Call with the set of currently valid keys.
#[allow(dead_code)]
pub fn prune_stale(valid_keys: &HashSet<String>) {
    let mut store = load_store();
    let before = store.dismissed.len();
    store.dismissed.retain(|k| valid_keys.contains(k));
    if store.dismissed.len() != before {
        let _ = save_store(&store);
    }
}
