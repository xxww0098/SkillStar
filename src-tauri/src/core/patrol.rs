//! Tauri adapter for background patrol.
//!
//! Domain check/prefetch/collect logic lives in `skillstar_skills::patrol`.
//! This module only owns State, tokio spawn, cancellation, and event emit.

use anyhow::Result;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};
use tokio::sync::watch;
use tracing::{error, warn};

use skillstar_skills::patrol::{self, load_config, save_config};
use skillstar_skills::update_state;

// Re-export types used by commands
pub use skillstar_skills::patrol::{PatrolCheckEvent, PatrolConfig, PatrolStatus};

// ── Patrol Manager ──────────────────────────────────────────────────────

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
    pub fn start(&self, app: AppHandle, interval_secs: u64) -> Result<()> {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());

        if let Some(tx) = inner.cancel_tx.take() {
            let _ = tx.send(true);
        }

        inner.enabled = true;
        inner.interval_secs = interval_secs;
        inner.running = true;
        inner.skills_checked = 0;
        inner.updates_found = 0;
        inner.current_skill.clear();

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

    pub fn stop(&self) {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(tx) = inner.cancel_tx.take() {
            let _ = tx.send(true);
        }
        inner.enabled = false;
        inner.running = false;
        inner.current_skill.clear();

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

// ── Patrol Loop (tokio + Emitter only) ──────────────────────────────────

async fn patrol_loop(
    app: AppHandle,
    state: Arc<Mutex<PatrolInner>>,
    mut cancel_rx: watch::Receiver<bool>,
    interval_secs: u64,
) {
    let interval = std::time::Duration::from_secs(interval_secs);
    let per_skill_delay = std::time::Duration::from_millis(10);

    loop {
        let skills = match tokio::task::spawn_blocking(patrol::collect_hub_skills).await {
            Ok(Ok(entries)) => entries,
            Ok(Err(e)) => {
                error!(target: "patrol", error = %e, "failed to list skills");
                tokio::select! {
                    _ = tokio::time::sleep(interval) => continue,
                    _ = cancel_rx.changed() => break,
                }
            }
            Err(e) => {
                error!(target: "patrol", error = %e, "failed to list skills (join)");
                tokio::select! {
                    _ = tokio::time::sleep(interval) => continue,
                    _ = cancel_rx.changed() => break,
                }
            }
        };

        if skills.is_empty() {
            tokio::select! {
                _ = tokio::time::sleep(interval) => continue,
                _ = cancel_rx.changed() => break,
            }
        }

        let skill_paths: Vec<PathBuf> = skills.iter().map(|entry| entry.path.clone()).collect();
        let failed_fetch_roots: Arc<std::collections::HashSet<PathBuf>> =
            match tokio::task::spawn_blocking(move || patrol::prefetch_failed_repos(&skill_paths))
                .await
            {
                Ok(failed) => Arc::new(failed),
                Err(err) => {
                    warn!(target: "patrol", error = %err, "failed to prefetch repos");
                    Arc::new(std::collections::HashSet::new())
                }
            };

        for entry in &skills {
            if *cancel_rx.borrow() {
                break;
            }

            {
                let mut inner = state.lock().unwrap_or_else(|p| p.into_inner());
                inner.current_skill = entry.name.clone();
            }

            let skill_name = entry.name.clone();
            let skill_path = entry.path.clone();
            let failed_roots = Arc::clone(&failed_fetch_roots);
            // Stamped before the check so a finding overtaken by an update
            // applied mid-check is dropped rather than re-asserting the badge.
            let checked_from = update_state::stamp();
            let update_result = tokio::task::spawn_blocking(move || {
                patrol::check_skill_update_local(&skill_name, &skill_path, &failed_roots)
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

            let Some(update_available) = update_result else {
                tokio::select! {
                    _ = tokio::time::sleep(per_skill_delay) => {},
                    _ = cancel_rx.changed() => break,
                }
                continue;
            };

            // Write through before emitting: the event is a notification, not
            // the record. Patrol findings used to live only in the event, so
            // they vanished on restart and drifted from the persisted snapshot.
            let update_available = update_state::commit_scan(
                checked_from,
                &[update_state::SkillUpdateState {
                    name: entry.name.clone(),
                    update_available,
                }],
            )
            .first()
            .map(|state| state.update_available)
            .unwrap_or(update_available);

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

            let _ = app.emit("patrol://skill-checked", &event);

            tokio::select! {
                _ = tokio::time::sleep(per_skill_delay) => {},
                _ = cancel_rx.changed() => break,
            }
        }

        if *cancel_rx.borrow() {
            break;
        }

        let new_skills_result =
            tokio::task::spawn_blocking(patrol::detect_new_skills_in_cached_repos).await;

        if let Ok(new_skills) = new_skills_result
            && !new_skills.is_empty()
        {
            let _ = app.emit("patrol://new-skills-detected", &new_skills);
        }

        tokio::select! {
            _ = tokio::time::sleep(interval) => {},
            _ = cancel_rx.changed() => break,
        }
    }

    let mut inner = state.lock().unwrap_or_else(|p| p.into_inner());
    inner.running = false;
    inner.current_skill.clear();
    inner.cancel_tx = None;
}
