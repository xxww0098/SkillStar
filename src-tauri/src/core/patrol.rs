//! Background patrol — low-overhead update monitoring.
//!
//! When active, a tokio task checks installed skills in fast per-cycle batches:
//! prefetch unique repos once, then compare each skill locally. Results are
//! emitted as Tauri events so the frontend can merge them into the UI.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};
use tokio::sync::watch;
use tracing::{error, warn};

use super::{git_ops, local_skill, repo_scanner};

// ── Persistent Configuration ────────────────────────────────────────

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

fn config_path() -> std::path::PathBuf {
    super::paths::patrol_state_path()
}

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

pub fn save_config(config: &PatrolConfig) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(config)?;
    std::fs::write(&path, content)?;
    Ok(())
}

// ── Runtime Status ──────────────────────────────────────────────────

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

#[derive(Debug, Clone)]
struct HubSkillEntry {
    name: String,
    path: PathBuf,
}

// ── Patrol Manager ──────────────────────────────────────────────────

struct PatrolInner {
    enabled: bool,
    running: bool,
    cancel_tx: Option<watch::Sender<bool>>,
    skills_checked: u64,
    updates_found: u64,
    current_skill: String,
    interval_secs: u64,
}

pub struct PatrolManager {
    inner: Arc<Mutex<PatrolInner>>,
}

impl PatrolManager {
    pub fn new() -> Self {
        let config = load_config();
        Self {
            inner: Arc::new(Mutex::new(PatrolInner {
                enabled: config.enabled,
                running: false,
                cancel_tx: None,
                skills_checked: 0,
                updates_found: 0,
                current_skill: String::new(),
                interval_secs: config.interval_secs,
            })),
        }
    }

    /// Start the patrol loop. If already running, stop the existing task first.
    ///
    /// This is synchronous — it spawns the patrol loop on the tokio runtime
    /// but does not block on it.
    pub fn start(&self, app: AppHandle, interval_secs: u64) -> Result<()> {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());

        // If already running, stop the existing task first.
        if let Some(tx) = inner.cancel_tx.take() {
            let _ = tx.send(true);
        }

        inner.enabled = true;
        inner.interval_secs = interval_secs;
        inner.running = true;
        inner.skills_checked = 0;
        inner.updates_found = 0;
        inner.current_skill.clear();

        // Persist enabled state
        let _ = save_config(&PatrolConfig {
            enabled: true,
            interval_secs,
        });

        let (cancel_tx, cancel_rx) = watch::channel(false);
        inner.cancel_tx = Some(cancel_tx);

        let state = Arc::clone(&self.inner);
        tokio::spawn(patrol_loop(app, state, cancel_rx, interval_secs));

        Ok(())
    }

    /// Stop the patrol loop.
    pub fn stop(&self) {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(tx) = inner.cancel_tx.take() {
            let _ = tx.send(true);
        }
        inner.enabled = false;
        inner.running = false;
        inner.current_skill.clear();

        // Persist disabled state but keep the interval
        let _ = save_config(&PatrolConfig {
            enabled: false,
            interval_secs: inner.interval_secs,
        });
    }

    pub fn set_enabled(&self, enabled: bool) -> Result<()> {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        inner.enabled = enabled;
        let interval_secs = inner.interval_secs;
        save_config(&PatrolConfig {
            enabled,
            interval_secs,
        })
    }

    /// Get current patrol status.
    pub fn status(&self) -> PatrolStatus {
        let inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        PatrolStatus {
            enabled: inner.enabled,
            running: inner.running,
            interval_secs: inner.interval_secs,
            skills_checked: inner.skills_checked,
            updates_found: inner.updates_found,
            current_skill: inner.current_skill.clone(),
        }
    }
}

// ── Patrol Loop ─────────────────────────────────────────────────────

async fn patrol_loop(
    app: AppHandle,
    state: Arc<Mutex<PatrolInner>>,
    mut cancel_rx: watch::Receiver<bool>,
    interval_secs: u64,
) {
    let interval = std::time::Duration::from_secs(interval_secs);
    let per_skill_delay = std::time::Duration::from_millis(10);

    loop {
        // Collect hub skills to check this cycle.
        let skills = match collect_hub_skills().await {
            Ok(entries) => entries,
            Err(e) => {
                error!(target: "patrol", error = %e, "failed to list skills");
                // Wait before retrying
                tokio::select! {
                    _ = tokio::time::sleep(interval) => continue,
                    _ = cancel_rx.changed() => break,
                }
            }
        };

        if skills.is_empty() {
            // Nothing to check — wait one full interval then retry
            tokio::select! {
                _ = tokio::time::sleep(interval) => continue,
                _ = cancel_rx.changed() => break,
            }
        }

        // Fetch once per unique repo root to avoid per-skill network fetches.
        let skill_paths: Vec<PathBuf> = skills.iter().map(|entry| entry.path.clone()).collect();
        let failed_fetch_roots: Arc<std::collections::HashSet<PathBuf>> =
            match tokio::task::spawn_blocking(move || {
                repo_scanner::prefetch_unique_repos(&skill_paths)
            })
            .await
            {
                Ok(failed) => Arc::new(failed),
                Err(err) => {
                    warn!(target: "patrol", error = %err, "failed to prefetch repos");
                    Arc::new(std::collections::HashSet::new())
                }
            };

        for entry in &skills {
            // Check for cancellation before each skill
            if *cancel_rx.borrow() {
                break;
            }

            // Update current_skill
            {
                let mut inner = state.lock().unwrap_or_else(|p| p.into_inner());
                inner.current_skill = entry.name.clone();
            }

            let skill_name = entry.name.clone();
            let skill_path = entry.path.clone();
            let failed_roots = Arc::clone(&failed_fetch_roots);
            // Check this skill locally after cycle prefetch.
            let update_result = tokio::task::spawn_blocking(move || {
                check_skill_update_local(&skill_name, &skill_path, &failed_roots)
            })
            .await
            .unwrap_or_else(|err| {
                warn!(
                    target: "patrol",
                    skill = %entry.name,
                    error = %err,
                    "update check task failed"
                );
                Some(false)
            });

            // None = fetch failed for this skill's repo; skip emitting so the
            // frontend preserves the existing update badge.
            let Some(update_available) = update_result else {
                // Still do the per-skill delay to keep cadence.
                tokio::select! {
                    _ = tokio::time::sleep(per_skill_delay) => {},
                    _ = cancel_rx.changed() => break,
                }
                continue;
            };

            // Update counters
            let event = {
                let mut inner = state.lock().unwrap_or_else(|p| p.into_inner());
                inner.skills_checked += 1;
                if update_available {
                    inner.updates_found += 1;
                }
                PatrolCheckEvent {
                    name: entry.name.clone(),
                    update_available,
                    skills_checked: inner.skills_checked,
                    updates_found: inner.updates_found,
                }
            };

            // Emit event to frontend
            let _ = app.emit("patrol://skill-checked", &event);

            // Keep a tiny inter-skill pause so UI updates remain smooth.
            tokio::select! {
                _ = tokio::time::sleep(per_skill_delay) => {},
                _ = cancel_rx.changed() => break,
            }
        }

        // Check for cancellation after a full cycle
        if *cancel_rx.borrow() {
            break;
        }

        // After checking all skills, detect new uninstalled skills in fetched repos.
        // This is cheap because repos were already fetched during prefetch_unique_repos().
        let new_skills_result =
            tokio::task::spawn_blocking(repo_scanner::detect_new_skills_in_cached_repos).await;

        if let Ok(new_skills) = new_skills_result {
            if !new_skills.is_empty() {
                let _ = app.emit("patrol://new-skills-detected", &new_skills);
            }
        }

        // Wait between patrol cycles.
        tokio::select! {
            _ = tokio::time::sleep(interval) => {},
            _ = cancel_rx.changed() => break,
        }
    }

    // Clean up: mark as stopped
    let mut inner = state.lock().unwrap_or_else(|p| p.into_inner());
    inner.running = false;
    inner.current_skill.clear();
    inner.cancel_tx = None;
}

fn check_skill_update_local(
    skill_name: &str,
    skill_path: &Path,
    failed_fetch_roots: &std::collections::HashSet<PathBuf>,
) -> Option<bool> {
    if repo_scanner::is_repo_cached_skill(skill_path) {
        return repo_scanner::check_repo_skill_update_local(skill_path, failed_fetch_roots);
    }

    // Fallback for non-repo-cached hub skills.
    let _ = git_ops::ensure_worktree_checked_out(skill_path);
    match git_ops::check_update(skill_path) {
        Ok(update_available) => Some(update_available),
        Err(err) => {
            warn!(target: "patrol", skill = %skill_name, error = %err, "check failed");
            Some(false)
        }
    }
}

/// Collect all installed hub (non-local) skills and their paths.
///
/// Uses a lightweight directory scan instead of `list_installed_skills` to
/// avoid the overhead of parsing every SKILL.md on each patrol cycle.
async fn collect_hub_skills() -> Result<Vec<HubSkillEntry>> {
    let skills_dir = super::paths::hub_skills_dir();
    tokio::task::spawn_blocking(move || {
        let entries = match std::fs::read_dir(&skills_dir) {
            Ok(e) => e,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => return Err(anyhow::anyhow!("Failed to read skills directory: {}", err)),
        };

        let mut skills = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name.is_empty() {
                continue;
            }
            // Skip local skills — they have no git remote to check
            if local_skill::is_local_skill(&name) {
                continue;
            }
            skills.push(HubSkillEntry { name, path });
        }
        skills.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(skills)
    })
    .await
    .map_err(|e| anyhow::anyhow!("Task join error: {}", e))?
}
