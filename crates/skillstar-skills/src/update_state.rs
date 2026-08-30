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

pub use skillstar_core::types::{UpstreamChange, UpstreamSuccessor};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillUpdateState {
    pub name: String,
    pub update_available: bool,
    /// Upstream removal (with the successor it was renamed into, if one was
    /// found). Independent of `update_available`, which keeps meaning
    /// "a content update is available".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_change: Option<UpstreamChange>,
}

impl SkillUpdateState {
    pub fn new(name: impl Into<String>, update_available: bool) -> Self {
        Self {
            name: name.into(),
            update_available,
            upstream_change: None,
        }
    }
}

/// What is remembered per Skill — also the on-disk value shape.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Stored {
    update_available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    upstream_change: Option<UpstreamChange>,
}

#[derive(Debug, Clone)]
struct Stamped {
    stored: Stored,
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
/// by construction, not a measurement that can go stale. It also clears any
/// recorded upstream change: the Skill was just pulled from its source.
pub fn set(name: &str, available: bool) {
    set_stamped(name, available);
}

pub fn set_stamped(name: &str, available: bool) -> u64 {
    let (revision, snapshot) = with_store_mut(|store| {
        store.revision += 1;
        let revision = store.revision;
        store.states.insert(
            name.to_string(),
            Stamped {
                stored: Stored {
                    update_available: available,
                    upstream_change: None,
                },
                revision,
            },
        );
        (revision, flat(store))
    });
    persist(&snapshot);
    revision
}

pub fn get(name: &str) -> Option<bool> {
    with_store(|store| {
        store
            .states
            .get(name)
            .map(|stored| stored.stored.update_available)
    })
}

/// The upstream removal recorded for `name`, if the last check found one.
pub fn upstream_change(name: &str) -> Option<UpstreamChange> {
    with_store(|store| {
        store
            .states
            .get(name)
            .and_then(|stored| stored.stored.upstream_change.clone())
    })
}

/// Every `(installed name, successor)` pair on record — what new-skill
/// detection uses to mark a freshly appeared Skill as a rename target.
pub fn successors() -> Vec<(String, UpstreamSuccessor)> {
    with_store(|store| {
        store
            .states
            .iter()
            .filter_map(|(name, stored)| match &stored.stored.upstream_change {
                Some(UpstreamChange::Removed {
                    successor: Some(successor),
                    ..
                }) => Some((name.clone(), successor.clone())),
                _ => None,
            })
            .collect()
    })
}

/// Drop a name that no longer exists in the library.
pub fn forget(name: &str) {
    let snapshot = with_store_mut(|store| store.states.remove(name).is_some().then(|| flat(store)));
    if let Some(snapshot) = snapshot {
        persist(&snapshot);
    }
}

pub fn restore_if_revision(name: &str, expected_revision: u64, available: Option<bool>) {
    let snapshot = with_store_mut(|store| {
        if store.states.get(name).map(|stored| stored.revision) != Some(expected_revision) {
            return None;
        }
        store.revision += 1;
        let revision = store.revision;
        match available {
            Some(available) => {
                store.states.insert(
                    name.to_string(),
                    Stamped {
                        stored: Stored {
                            update_available: available,
                            upstream_change: None,
                        },
                        revision,
                    },
                );
            }
            None => {
                store.states.remove(name);
            }
        }
        Some(flat(store))
    });
    if let Some(snapshot) = snapshot {
        persist(&snapshot);
    }
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
                            stored: Stored {
                                update_available: state.update_available,
                                upstream_change: state.upstream_change.clone(),
                            },
                            revision,
                        },
                    );
                }

                match store.states.get(&state.name) {
                    Some(stored) => SkillUpdateState {
                        name: state.name.clone(),
                        update_available: stored.stored.update_available,
                        upstream_change: stored.stored.upstream_change.clone(),
                    },
                    None => state.clone(),
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
                skill.update_available = stored.stored.update_available;
                skill.upstream_change = stored.stored.upstream_change.clone();
            }
        }
    });
}

// ── persistence ─────────────────────────────────────────────────────

fn snapshot_path() -> PathBuf {
    skillstar_core::infra::paths::state_dir().join("skill_update_states.json")
}

/// On disk this is `name -> { update_available, upstream_change? }`; revisions
/// are process-local, so persisting them would be meaningless on next launch.
/// Snapshots written before upstream changes existed were `name -> bool` and
/// still load.
fn flat(store: &Store) -> HashMap<String, Stored> {
    store
        .states
        .iter()
        .map(|(name, stored)| (name.clone(), stored.stored.clone()))
        .collect()
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Persisted {
    Legacy(bool),
    Full(Stored),
}

fn load_snapshot() -> HashMap<String, Stored> {
    let path = snapshot_path();
    let Ok(content) = std::fs::read_to_string(&path) else {
        return HashMap::new();
    };

    match serde_json::from_str::<HashMap<String, Persisted>>(&content) {
        Ok(states) => states
            .into_iter()
            .map(|(name, persisted)| {
                let stored = match persisted {
                    Persisted::Legacy(update_available) => Stored {
                        update_available,
                        upstream_change: None,
                    },
                    Persisted::Full(stored) => stored,
                };
                (name, stored)
            })
            .collect(),
        Err(err) => {
            tracing::warn!(target: "skill_update_state", path = %path.display(), error = %err, "failed to read skill update snapshot");
            HashMap::new()
        }
    }
}

fn persist(states: &HashMap<String, Stored>) {
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
    for (name, stored) in load_snapshot() {
        store.states.entry(name).or_insert(Stamped {
            stored,
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

pub fn reset_for_test() {
    let mut store = STORE
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *store = Store {
        hydrated: true,
        ..Default::default()
    };
}

/// Forget the in-memory store so the next access hydrates from disk again.
#[cfg(test)]
fn reload_for_test() {
    let mut store = STORE
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *store = Store::default();
}

#[cfg(test)]
mod tests {
    use super::*;
    use skillstar_core::types::{SkillCategory, SkillType};

    fn state(name: &str, available: bool) -> SkillUpdateState {
        SkillUpdateState::new(name, available)
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
            upstream_change: None,
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

        assert!(
            !effective[0].update_available,
            "a stale scan must not re-assert the badge the update just cleared"
        );
    }

    #[test]
    fn a_scan_with_nothing_racing_it_is_applied() {
        let (_guard, _temp) = sandbox();

        let scan_started = stamp();
        let effective = commit_scan(scan_started, &[state("alpha", true)]);

        assert!(effective[0].update_available);

        let mut skills = [skill("alpha")];
        apply_to(&mut skills);
        assert!(skills[0].update_available);
    }

    #[test]
    fn rollback_restore_only_replaces_the_transaction_revision() {
        let (_guard, _temp) = sandbox();
        set("alpha", true);
        let transaction_revision = set_stamped("alpha", false);
        restore_if_revision("alpha", transaction_revision, Some(true));
        assert_eq!(get("alpha"), Some(true));

        let later_revision = set_stamped("alpha", false);
        set("alpha", true);
        restore_if_revision("alpha", later_revision, Some(false));
        assert_eq!(get("alpha"), Some(true));
    }

    #[test]
    fn staleness_is_decided_per_name_not_for_the_whole_scan() {
        let (_guard, _temp) = sandbox();

        let scan_started = stamp();
        set("alpha", false);

        let effective = commit_scan(scan_started, &[state("alpha", true), state("beta", true)]);

        assert!(!effective[0].update_available, "alpha was overtaken");
        assert!(
            effective[1].update_available,
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

    fn successor() -> UpstreamSuccessor {
        UpstreamSuccessor {
            skill_id: "gamma-spec".into(),
            folder_path: "skills/engineering/gamma-spec".into(),
            description: "Gamma, renamed".into(),
            similarity: Some(92),
        }
    }

    fn removed(successor: Option<UpstreamSuccessor>) -> UpstreamChange {
        UpstreamChange::Removed {
            suggested_local_name: "gamma.local".into(),
            successor,
        }
    }

    #[test]
    fn upstream_changes_persist_and_legacy_snapshots_still_load() {
        let (_guard, _temp) = sandbox();

        // A snapshot written before upstream changes existed is a bare bool map.
        let path = snapshot_path();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"alpha":true,"beta":false}"#).unwrap();
        reload_for_test();
        assert_eq!(get("alpha"), Some(true));
        assert_eq!(get("beta"), Some(false));
        assert_eq!(upstream_change("alpha"), None);

        // A removal with its successor survives a restart.
        commit_scan(
            stamp(),
            &[SkillUpdateState {
                name: "gamma".into(),
                update_available: false,
                upstream_change: Some(removed(Some(successor()))),
            }],
        );
        reload_for_test();
        assert_eq!(upstream_change("gamma"), Some(removed(Some(successor()))));
        assert_eq!(successors(), vec![("gamma".to_string(), successor())]);
        assert_eq!(
            get("alpha"),
            Some(true),
            "legacy entries are kept alongside"
        );

        let mut skills = [skill("gamma")];
        apply_to(&mut skills);
        assert!(!skills[0].update_available);
        assert_eq!(skills[0].upstream_change, Some(removed(Some(successor()))));

        // An applied update is authoritative and clears the change; a removed
        // library entry is forgotten outright.
        set("gamma", false);
        assert_eq!(upstream_change("gamma"), None);
        forget("gamma");
        reload_for_test();
        assert_eq!(get("gamma"), None);
        assert!(successors().is_empty());
    }
}
