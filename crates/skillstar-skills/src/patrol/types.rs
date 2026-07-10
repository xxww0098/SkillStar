//! Pure data types for patrol configuration, status, and events.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ── Persistent Configuration ────────────────────────────────────────────

/// Patrol configuration persisted to `~/.skillstar/state/patrol.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatrolConfig {
    /// Whether patrol was last marked active.
    pub enabled: bool,
    /// Per-skill check interval in seconds.
    pub interval_secs: u64,
}

impl Default for PatrolConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_secs: 30,
        }
    }
}

// ── Runtime Status ─────────────────────────────────────────────────────

/// Current patrol runtime status returned to callers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatrolStatus {
    pub enabled: bool,
    pub running: bool,
    pub interval_secs: u64,
    pub skills_checked: u64,
    pub updates_found: u64,
    /// Name of the skill currently being checked (empty when idle).
    pub current_skill: String,
}

/// Event payload emitted after each single-skill check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatrolCheckEvent {
    pub name: String,
    pub update_available: bool,
    pub skills_checked: u64,
    pub updates_found: u64,
}

/// Lightweight hub skill entry used by `collect_hub_skills` in the Tauri crate.
#[derive(Debug, Clone)]
pub struct HubSkillEntry {
    pub name: String,
    pub path: PathBuf,
}
