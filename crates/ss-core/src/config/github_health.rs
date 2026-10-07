//! Persistent circuit breaker for GitHub-family accelerators.
//!
//! A sequential five-mirror chain is still a single point of delay: a dead
//! first candidate costs a full TCP/TLS timeout on every clone. This store
//! records per-mirror outcomes under `state/github_mirror_health.json` so
//! [`super::github_mirror::candidate_mirror_urls`] can skip an open circuit.
//! Candidate order itself is the user's configured order — health never
//! reorders, it only drops currently-open circuits.
//!
//! Policy (D-050):
//! - two consecutive failures open the circuit for 20 minutes
//! - if every candidate is open, fail-open and try the original order
//! - saving a new mirror config resets the store
//! - health is rebuildable state, never a user-edited config file

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use super::github_rewrite::normalize_mirror_url;

const FAILURE_THRESHOLD: u32 = 2;
const OPEN_FOR_SECS: u64 = 20 * 60;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MirrorHealthStore {
    #[serde(default)]
    pub mirrors: HashMap<String, MirrorHealth>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MirrorHealth {
    #[serde(default)]
    pub consecutive_failures: u32,
    pub last_success_unix: Option<u64>,
    pub last_failure_unix: Option<u64>,
    pub opened_until_unix: Option<u64>,
    pub last_latency_ms: Option<u64>,
}

fn health_path() -> std::path::PathBuf {
    crate::infra::paths::github_mirror_health_path()
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn load() -> MirrorHealthStore {
    let path = health_path();
    if !path.exists() {
        return MirrorHealthStore::default();
    }
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

fn save(store: &MirrorHealthStore) -> Result<()> {
    let path = health_path();
    let content = serde_json::to_string_pretty(store)?;
    crate::infra::fs_ops::atomic_write(&path, content.as_bytes())?;
    Ok(())
}

/// Drop all recorded outcomes. Called when the user saves a new mirror config.
pub fn reset() {
    let _ = save(&MirrorHealthStore::default());
}

fn is_open(entry: Option<&MirrorHealth>, now: u64) -> bool {
    entry
        .and_then(|health| health.opened_until_unix)
        .is_some_and(|until| until > now)
}

/// Record a successful probe or git fetch through `mirror`.
pub fn record_success(mirror: &str, latency_ms: Option<u64>) {
    let Some(key) = normalize_mirror_url(mirror) else {
        return;
    };
    let mut store = load();
    let entry = store.mirrors.entry(key).or_default();
    entry.consecutive_failures = 0;
    entry.opened_until_unix = None;
    entry.last_success_unix = Some(now_unix());
    if let Some(latency) = latency_ms {
        entry.last_latency_ms = Some(latency);
    }
    let _ = save(&store);
}

/// Record a failed probe or git fetch through `mirror`.
pub fn record_failure(mirror: &str) {
    let Some(key) = normalize_mirror_url(mirror) else {
        return;
    };
    let now = now_unix();
    let mut store = load();
    let entry = store.mirrors.entry(key).or_default();
    entry.consecutive_failures = entry.consecutive_failures.saturating_add(1);
    entry.last_failure_unix = Some(now);
    if entry.consecutive_failures >= FAILURE_THRESHOLD {
        entry.opened_until_unix = Some(now.saturating_add(OPEN_FOR_SECS));
    }
    let _ = save(&store);
}

/// Filter `candidates` (already normalized, most preferred first) so hosts
/// whose circuit is currently open are skipped. The user's configured order
/// is authoritative and never re-sorted; if every candidate is open, return
/// the original order (fail-open).
pub fn rank_candidates(candidates: Vec<String>) -> Vec<String> {
    let store = load();
    let now = now_unix();
    let mut ranked: Vec<String> = Vec::new();

    for url in candidates.iter() {
        let key = normalize_mirror_url(url).unwrap_or_else(|| url.clone());
        if !is_open(store.mirrors.get(&key), now) {
            ranked.push(url.clone());
        }
    }

    if ranked.is_empty() {
        return candidates;
    }
    ranked
}

/// Snapshot used by the network doctor UI.
pub fn snapshot() -> MirrorHealthStore {
    load()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn isolate() -> (TempDir, std::sync::MutexGuard<'static, ()>) {
        let guard = crate::config::test_env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp = TempDir::new().unwrap();
        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
        }
        (temp, guard)
    }

    fn restore() {
        unsafe {
            std::env::remove_var("SKILLSTAR_DATA_DIR");
        }
    }

    #[test]
    fn two_failures_open_the_circuit_and_skip_that_candidate() {
        let (_temp, _guard) = isolate();
        let dead = "https://dead.example/";
        let live = "https://live.example/";

        record_failure(dead);
        assert_eq!(
            rank_candidates(vec![dead.into(), live.into()]),
            vec![dead.to_string(), live.to_string()],
            "a single failure must not skip the preferred host"
        );

        record_failure(dead);
        let ranked = rank_candidates(vec![dead.into(), live.into()]);
        assert_eq!(
            ranked,
            vec![live.to_string()],
            "an open circuit is skipped while a closed candidate remains"
        );

        restore();
    }

    #[test]
    fn all_open_fails_open_and_preserves_declaration_order() {
        let (_temp, _guard) = isolate();
        let first = "https://a.example/";
        let second = "https://b.example/";
        record_failure(first);
        record_failure(first);
        record_failure(second);
        record_failure(second);

        assert_eq!(
            rank_candidates(vec![first.into(), second.into()]),
            vec![first.to_string(), second.to_string()]
        );

        restore();
    }

    #[test]
    fn health_never_reorders_only_open_circuits_are_skipped() {
        let (_temp, _guard) = isolate();
        let slow = "https://slow.example/";
        let fast = "https://fast.example/";
        record_success(slow, Some(800));
        record_success(fast, Some(80));

        assert_eq!(
            rank_candidates(vec![slow.into(), fast.into()]),
            vec![slow.to_string(), fast.to_string()],
            "latency must not reorder the user's chain — the list order is the fallback order"
        );

        record_failure(slow);
        record_failure(slow);
        assert_eq!(
            rank_candidates(vec![slow.into(), fast.into()]),
            vec![fast.to_string()],
            "an open circuit is skipped without disturbing the rest of the order"
        );

        record_success(slow, Some(90));
        assert_eq!(
            rank_candidates(vec![slow.into(), fast.into()]),
            vec![slow.to_string(), fast.to_string()]
        );

        restore();
    }

    #[test]
    fn reset_drops_recorded_outcomes() {
        let (_temp, _guard) = isolate();
        record_failure("https://dead.example/");
        record_failure("https://dead.example/");
        reset();
        assert_eq!(
            rank_candidates(vec![
                "https://dead.example/".into(),
                "https://live.example/".into()
            ]),
            vec![
                "https://dead.example/".to_string(),
                "https://live.example/".to_string()
            ]
        );
        restore();
    }
}
