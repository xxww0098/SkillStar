//! Single owner of "does this skill have an update available?".
//!
//! Three producers answer that question: the batch refresh, the background
//! patrol, and the completion of an update itself. They used to write to
//! different places — the batch refresh to a JSON snapshot plus an in-process
//! cache, patrol only to a Tauri event, an update to the cache — so a patrol
//! finding vanished on restart and disagreed with the snapshot in the meantime.
//! The UI compensated with hand-written race guards.
//!
//! Everything now writes here, and staleness is resolved here rather than in
//! the UI. Each name carries the revision at which it was last written; a scan
//! commits with the revision it *started* at and silently loses to anything
//! written while it was running. That is the case the guards existed for: a
//! refresh that began before a pull must not re-assert the update badge the
//! pull just cleared.
//!
//! Revisions are process-local and deliberately not persisted — the webview
//! and this process start and stop together, so a revision can never appear to
//! travel backwards from a reader's point of view.

use serde::{Deserialize, Serialize};
use skillstar_core::types::Skill;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{LazyLock, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillUpdateState {
    pub name: String,
    pub update_available: bool,
}

#[derive(Debug, Clone, Copy)]
struct Stamped {
    available: bool,
    revision: u64,
}

#[derive(Default)]
struct Store {
    states: HashMap<String, Stamped>,
    revision: u64,
    hydrated: bool,
}

static STORE: LazyLock<RwLock<Store>> = LazyLock::new(|| RwLock::new(Store::default()));

// ── interface ───────────────────────────────────────────────────────

/// The revision a scan should record before it starts checking.
///
/// Pass it back to [`commit_scan`] so findings that were overtaken by an
/// update are dropped instead of overwriting it.
pub fn stamp() -> u64 {
    with_store(|store| store.revision)
}

/// Record a definitive result, overriding anything a scan may report later.
///
/// Used when an update has actually been applied — that answer is authoritative
/// by construction, not a measurement that can go stale.
pub fn set(name: &str, available: bool) {
    let snapshot = with_store_mut(|store| {
        store.revision += 1;
        let revision = store.revision;
        store.states.insert(
            name.to_string(),
            Stamped {
                available,
                revision,
            },
        );
        flat(store)
    });
    persist(&snapshot);
}

/// Commit the findings of a scan that started at `since`.
///
/// Names written after the scan started keep the newer value. Returns the
/// effective state of every submitted name, so callers report what is true
/// rather than what they happened to measure.
pub fn commit_scan(since: u64, states: &[SkillUpdateState]) -> Vec<SkillUpdateState> {
    let (effective, snapshot) = with_store_mut(|store| {
        store.revision += 1;
        let revision = store.revision;

        let effective = states
            .iter()
            .map(|state| {
                let overtaken = store
                    .states
                    .get(&state.name)
                    .is_some_and(|stored| stored.revision > since);

                if !overtaken {
                    store.states.insert(
                        state.name.clone(),
                        Stamped {
                            available: state.update_available,
                            revision,
                        },
                    );
                }

                SkillUpdateState {
                    name: state.name.clone(),
                    update_available: store
                        .states
                        .get(&state.name)
                        .map(|stored| stored.available)
                        .unwrap_or(state.update_available),
                }
            })
            .collect::<Vec<_>>();

        (effective, flat(store))
    });

    persist(&snapshot);
    effective
}

/// Stamp the known update state onto a freshly-built skill list.
pub fn apply_to(skills: &mut [Skill]) {
    with_store(|store| {
        for skill in skills.iter_mut() {
            if let Some(stored) = store.states.get(&skill.name) {
                skill.update_available = stored.available;
            }
        }
    });
}

// ── persistence ─────────────────────────────────────────────────────

fn snapshot_path() -> PathBuf {
    skillstar_core::infra::paths::state_dir().join("skill_update_states.json")
}

/// The on-disk shape stays a plain `name -> bool` map: revisions are
/// process-local, so persisting them would be meaningless on next launch.
fn flat(store: &Store) -> HashMap<String, bool> {
    store
        .states
        .iter()
        .map(|(name, stored)| (name.clone(), stored.available))
        .collect()
}

fn load_snapshot() -> HashMap<String, bool> {
    let path = snapshot_path();
    let Ok(content) = std::fs::read_to_string(&path) else {
        return HashMap::new();
    };

    serde_json::from_str::<HashMap<String, bool>>(&content).unwrap_or_else(|err| {
        tracing::warn!(target: "skill_update_state", path = %path.display(), error = %err, "failed to read skill update snapshot");
        HashMap::new()
    })
}

fn persist(states: &HashMap<String, bool>) {
    let path = snapshot_path();
    let Ok(content) = serde_json::to_string(states) else {
        return;
    };

    if let Err(err) = skillstar_core::infra::fs_ops::atomic_write(&path, content.as_bytes()) {
        tracing::warn!(target: "skill_update_state", path = %path.display(), error = %err, "failed to write skill update snapshot");
    }
}

/// Pull the persisted snapshot in on first access. Everything lands at
/// revision 0, so the first scan of the session may overwrite all of it.
fn hydrate(store: &mut Store) {
    if store.hydrated {
        return;
    }
    store.hydrated = true;
    for (name, available) in load_snapshot() {
        store.states.entry(name).or_insert(Stamped {
            available,
            revision: 0,
        });
    }
}

fn with_store<T>(read: impl FnOnce(&Store) -> T) -> T {
    // Hydration mutates, so take the write lock and downgrade by reading.
    with_store_mut(|store| read(store))
}

fn with_store_mut<T>(write: impl FnOnce(&mut Store) -> T) -> T {
    let mut store = STORE
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    hydrate(&mut store);
    write(&mut store)
}

#[cfg(test)]
pub(crate) fn reset_for_test() {
    let mut store = STORE
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *store = Store {
        hydrated: true,
        ..Default::default()
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use skillstar_core::types::{SkillCategory, SkillType};

    fn state(name: &str, available: bool) -> SkillUpdateState {
        SkillUpdateState {
            name: name.to_string(),
            update_available: available,
        }
    }

    fn skill(name: &str) -> Skill {
        Skill {
            name: name.to_string(),
            description: String::new(),
            localized_description: None,
            skill_type: SkillType::Hub,
            stars: 0,
            installed: true,
            update_available: false,
            last_updated: String::new(),
            git_url: String::new(),
            tree_hash: None,
            category: SkillCategory::None,
            author: None,
            topics: Vec::new(),
            agent_links: Some(Vec::new()),
            rank: None,
            source: None,
        }
    }

    /// Sandbox the snapshot file so tests never touch the real state dir.
    fn sandbox() -> (std::sync::MutexGuard<'static, ()>, tempfile::TempDir) {
        let guard = crate::lock_test_env();
        let temp = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
        }
        reset_for_test();
        (guard, temp)
    }

    #[test]
    fn a_scan_that_started_before_an_update_loses_to_it() {
        let (_guard, _temp) = sandbox();

        // A refresh begins while "alpha" still has an update pending.
        let scan_started = stamp();

        // The update lands first and clears the badge.
        set("alpha", false);

        // The refresh now reports what it measured before the pull.
        let effective = commit_scan(scan_started, &[state("alpha", true)]);

        assert_eq!(
            effective[0].update_available, false,
            "a stale scan must not re-assert the badge the update just cleared"
        );
    }

    #[test]
    fn a_scan_with_nothing_racing_it_is_applied() {
        let (_guard, _temp) = sandbox();

        let scan_started = stamp();
        let effective = commit_scan(scan_started, &[state("alpha", true)]);

        assert_eq!(effective[0].update_available, true);

        let mut skills = [skill("alpha")];
        apply_to(&mut skills);
        assert!(skills[0].update_available);
    }

    #[test]
    fn staleness_is_decided_per_name_not_for_the_whole_scan() {
        let (_guard, _temp) = sandbox();

        let scan_started = stamp();
        set("alpha", false);

        let effective = commit_scan(scan_started, &[state("alpha", true), state("beta", true)]);

        assert_eq!(effective[0].update_available, false, "alpha was overtaken");
        assert_eq!(
            effective[1].update_available, true,
            "beta was not, and must still be applied"
        );
    }

    #[test]
    fn results_survive_a_restart_through_the_snapshot() {
        let (_guard, _temp) = sandbox();

        // Patrol finds an update and records it.
        commit_scan(stamp(), &[state("alpha", true)]);

        // Next launch: fresh store, same data dir.
        reset_for_test_unhydrated();
        let mut skills = [skill("alpha")];
        apply_to(&mut skills);

        assert!(
            skills[0].update_available,
            "a finding must outlive the process that made it"
        );
    }

    #[test]
    fn unknown_skills_keep_whatever_they_were_built_with() {
        let (_guard, _temp) = sandbox();

        commit_scan(stamp(), &[state("alpha", true)]);

        let mut skills = [skill("never-scanned")];
        apply_to(&mut skills);
        assert!(!skills[0].update_available);
    }

    /// Simulate a fresh process against the same data dir.
    fn reset_for_test_unhydrated() {
        let mut store = STORE
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *store = Store::default();
    }
}
