//! Circuit breaker for provider-level fault isolation.
//!
//! Inspired by cc-switch/proxy/circuit_breaker.rs.
//! Prevents repeatedly calling providers that are known to be down.
//!
//! Storage: `~/.skillstar/state/circuit_breakers.json`

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Circuit breaker state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CircuitState {
    /// Normal operation — requests pass through.
    Closed,
    /// Too many failures — requests are blocked.
    Open,
    /// Recovery timeout elapsed — single probe allowed.
    HalfOpen,
}

impl Default for CircuitState {
    fn default() -> Self {
        CircuitState::Closed
    }
}

/// A provider's circuit breaker record.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CircuitBreakerRecord {
    /// Current circuit state.
    pub state: CircuitState,
    /// Consecutive failure count since last success.
    pub failure_count: u32,
    /// Last failure Unix timestamp (seconds).
    pub last_failure: Option<i64>,
    /// When the circuit last transitioned to Open.
    pub opened_at: Option<i64>,
    /// Whether the half-open probe succeeded.
    pub half_open_success: bool,
}

/// Threshold of consecutive failures before opening the circuit.
const FAILURE_THRESHOLD: u32 = 5;

/// How long (seconds) the circuit stays Open before transitioning to HalfOpen.
const RECOVERY_TIMEOUT_SECS: i64 = 60;

/// How long (seconds) a HalfOpen probe result lingers before closing again.
const HALF_OPEN_RESET_SECS: i64 = 30;

impl CircuitBreakerRecord {
    pub fn new() -> Self {
        Self {
            state: CircuitState::Closed,
            failure_count: 0,
            last_failure: None,
            opened_at: None,
            half_open_success: false,
        }
    }

    /// Whether the circuit allows requests.
    pub fn is_available(&self) -> bool {
        let now = chrono::Utc::now().timestamp();
        match self.state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                if let Some(opened_at) = self.opened_at {
                    if now >= opened_at + RECOVERY_TIMEOUT_SECS {
                        return true; // transition to HalfOpen below
                    }
                }
                false
            }
            CircuitState::HalfOpen => true,
        }
    }

    /// Returns the state after applying recovery timeout logic.
    pub fn current_state(&self) -> CircuitState {
        if self.state == CircuitState::Open {
            let now = chrono::Utc::now().timestamp();
            if let Some(opened_at) = self.opened_at {
                if now >= opened_at + RECOVERY_TIMEOUT_SECS {
                    return CircuitState::HalfOpen;
                }
            }
        }
        self.state
    }

    /// Record a successful request — reset failure count and close the circuit.
    pub fn record_success(&mut self) {
        self.state = CircuitState::Closed;
        self.failure_count = 0;
        self.last_failure = None;
        self.opened_at = None;
        self.half_open_success = false;
    }

    /// Record a failed request — increment failure count and maybe open the circuit.
    pub fn record_failure(&mut self) {
        self.failure_count += 1;
        self.last_failure = Some(chrono::Utc::now().timestamp());

        if self.failure_count >= FAILURE_THRESHOLD {
            self.state = CircuitState::Open;
            self.opened_at = Some(chrono::Utc::now().timestamp());
        }
    }

    /// Transition from HalfOpen to Closed on success, or back to Open on failure.
    pub fn record_half_open_result(&mut self, success: bool) {
        self.half_open_success = success;
        if success {
            self.record_success();
        } else {
            self.state = CircuitState::Open;
            self.opened_at = Some(chrono::Utc::now().timestamp());
            self.failure_count = 1;
        }
    }
}

impl Default for CircuitBreakerRecord {
    fn default() -> Self {
        Self::new()
    }
}

/// In-memory circuit breaker state (keyed by "app_id/provider_id").
type CircuitMap = Arc<Mutex<HashMap<String, CircuitBreakerRecord>>>;

lazy_static::lazy_static! {
    static ref CIRCUIT_MAP: CircuitMap = Arc::new(Mutex::new(HashMap::new()));
}

fn store_path() -> PathBuf {
    skillstar_infra::paths::state_dir().join("circuit_breakers.json")
}

/// Load persisted circuit breaker records from disk (delegated to provider_states).
pub fn load_breakers() -> HashMap<String, CircuitBreakerRecord> {
    crate::provider_states::load()
        .map(|s| s.circuit_breakers)
        .unwrap_or_default()
}

/// Persist circuit breakers to disk (delegated to provider_states).
fn save_breakers(breakers: &HashMap<String, CircuitBreakerRecord>) {
    let mut states = crate::provider_states::load().unwrap_or_default();
    states.circuit_breakers = breakers.clone();
    if let Err(e) = crate::provider_states::save(&states) {
        tracing::warn!("Failed to persist circuit breakers: {}", e);
    }
}

/// Get the composite key for a provider.
fn key(app_id: &str, provider_id: &str) -> String {
    format!("{}/{}", app_id, provider_id)
}

/// Synchronously check if a provider's circuit allows requests.
/// Loads persisted state if not in memory.
pub async fn is_provider_available(app_id: &str, provider_id: &str) -> bool {
    let k = key(app_id, provider_id);
    let mut map = CIRCUIT_MAP.lock().await;

    // Ensure we have the latest state from disk
    if !map.contains_key(&k) {
        let persisted = load_breakers();
        for (pk, pv) in persisted {
            map.insert(pk, pv);
        }
    }

    if let Some(breaker) = map.get(&k) {
        breaker.is_available()
    } else {
        true
    }
}

/// Check circuit state for a provider.
pub async fn get_circuit_state(app_id: &str, provider_id: &str) -> CircuitState {
    let k = key(app_id, provider_id);
    let map = CIRCUIT_MAP.lock().await;
    map.get(&k)
        .map(|b| b.current_state())
        .unwrap_or(CircuitState::Closed)
}

/// Record a successful call for a provider.
pub async fn record_success(app_id: &str, provider_id: &str) {
    let k = key(app_id, provider_id);
    let mut map = CIRCUIT_MAP.lock().await;
    let breaker = map.entry(k.clone()).or_default();
    breaker.record_success();
    save_breakers(&map);
}

/// Record a failed call for a provider.
pub async fn record_failure(app_id: &str, provider_id: &str) {
    let k = key(app_id, provider_id);
    let mut map = CIRCUIT_MAP.lock().await;
    let breaker = map.entry(k.clone()).or_default();
    breaker.record_failure();
    tracing::warn!(
        "Circuit breaker: {} failures for {} — state={:?}",
        breaker.failure_count,
        k,
        breaker.state
    );
    save_breakers(&map);
}

/// Record a half-open probe result.
pub async fn record_half_open_result(app_id: &str, provider_id: &str, success: bool) {
    let k = key(app_id, provider_id);
    let mut map = CIRCUIT_MAP.lock().await;
    let breaker = map.entry(k.clone()).or_default();
    breaker.record_half_open_result(success);
    save_breakers(&map);
}

/// Get all circuit breaker records for an app.
pub async fn get_breakers_for_app(app_id: &str) -> Vec<(String, CircuitBreakerRecord)> {
    let map = CIRCUIT_MAP.lock().await;
    map.iter()
        .filter(|(k, _)| k.starts_with(&format!("{}/", app_id)))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// Get the full record for a single provider.
pub async fn get_record(app_id: &str, provider_id: &str) -> Option<CircuitBreakerRecord> {
    let k = key(app_id, provider_id);
    let map = CIRCUIT_MAP.lock().await;
    map.get(&k).cloned()
}

/// Reset (close) circuit breakers for a provider — useful after manual reconnect.
pub async fn reset_breaker(app_id: &str, provider_id: &str) {
    let k = key(app_id, provider_id);
    let mut map = CIRCUIT_MAP.lock().await;
    if let Some(breaker) = map.get_mut(&k) {
        breaker.record_success();
        save_breakers(&map);
    }
}

/// Get a snapshot of all breakers for an app (used by commands).
pub async fn get_all_for_app(app_id: &str) -> Vec<(String, CircuitBreakerRecord)> {
    let map = CIRCUIT_MAP.lock().await;
    map.iter()
        .filter(|(k, _)| k.starts_with(&format!("{}/", app_id)))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}
