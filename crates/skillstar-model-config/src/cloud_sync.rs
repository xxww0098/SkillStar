//! Versioned cloud-sync primitives for provider/model state.
//!
//! This is intentionally narrow: it snapshots and restores a single app's
//! provider registry plus the unified cached provider state that already exists
//! on disk (health, quotas, circuit breakers). Higher-level remote transport,
//! account linking, and UI workflows can build on this contract later.

use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::{
    circuit_breaker::CircuitBreakerRecord,
    health::ProviderHealth,
    provider_states::{self, ProviderStates},
    providers::{self, AppProviders, ProvidersStore},
    quota::ProviderQuota,
};

pub const CLOUD_SYNC_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CloudSyncScope {
    ProviderModelState,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CloudSyncMergeMode {
    Replace,
    Merge,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudSyncProviderState {
    pub health: Vec<ProviderHealth>,
    pub quotas: Vec<ProviderQuota>,
    pub circuit_breakers: HashMap<String, CircuitBreakerRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudSyncSnapshot {
    pub schema_version: u32,
    pub scope: CloudSyncScope,
    pub app_id: String,
    pub exported_at: i64,
    pub providers: AppProviders,
    pub state: CloudSyncProviderState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudSyncImportReport {
    pub app_id: String,
    pub mode: CloudSyncMergeMode,
    pub provider_count: usize,
    pub health_count: usize,
    pub quota_count: usize,
    pub circuit_breaker_count: usize,
}

fn ensure_supported_app(app_id: &str) -> Result<()> {
    match app_id {
        "claude" | "codex" | "opencode" | "gemini" => Ok(()),
        _ => bail!("Unsupported app_id for cloud sync: {app_id}"),
    }
}

fn app_providers<'a>(store: &'a ProvidersStore, app_id: &str) -> Result<&'a AppProviders> {
    ensure_supported_app(app_id)?;
    Ok(match app_id {
        "claude" => &store.claude,
        "codex" => &store.codex,
        "opencode" => &store.opencode,
        "gemini" => &store.gemini,
        _ => unreachable!(),
    })
}

fn app_providers_mut<'a>(
    store: &'a mut ProvidersStore,
    app_id: &str,
) -> Result<&'a mut AppProviders> {
    ensure_supported_app(app_id)?;
    Ok(match app_id {
        "claude" => &mut store.claude,
        "codex" => &mut store.codex,
        "opencode" => &mut store.opencode,
        "gemini" => &mut store.gemini,
        _ => unreachable!(),
    })
}

fn composite_key(app_id: &str, provider_id: &str) -> String {
    format!("{app_id}/{provider_id}")
}

fn filter_state_for_app(states: &ProviderStates, app_id: &str) -> CloudSyncProviderState {
    let health = states
        .health
        .results
        .values()
        .filter(|entry| entry.app_id == app_id)
        .cloned()
        .collect();

    let quotas = states
        .quotas
        .quotas
        .values()
        .filter(|entry| entry.app_id == app_id)
        .cloned()
        .collect();

    let circuit_breakers = states
        .circuit_breakers
        .iter()
        .filter_map(|(key, value)| {
            key.strip_prefix(&format!("{app_id}/"))
                .map(|provider_id| (provider_id.to_string(), value.clone()))
        })
        .collect();

    CloudSyncProviderState {
        health,
        quotas,
        circuit_breakers,
    }
}

fn replace_state_for_app(
    states: &mut ProviderStates,
    app_id: &str,
    snapshot: &CloudSyncProviderState,
) {
    states
        .health
        .results
        .retain(|_, entry| entry.app_id != app_id);
    states
        .quotas
        .quotas
        .retain(|_, entry| entry.app_id != app_id);
    states
        .circuit_breakers
        .retain(|key, _| !key.starts_with(&format!("{app_id}/")));

    merge_state_for_app(states, app_id, snapshot);
}

fn merge_state_for_app(
    states: &mut ProviderStates,
    app_id: &str,
    snapshot: &CloudSyncProviderState,
) {
    for health in &snapshot.health {
        let mut entry = health.clone();
        entry.app_id = app_id.to_string();
        states
            .health
            .results
            .insert(composite_key(app_id, &entry.provider_id), entry);
    }

    for quota in &snapshot.quotas {
        let mut entry = quota.clone();
        entry.app_id = app_id.to_string();
        states
            .quotas
            .quotas
            .insert(composite_key(app_id, &entry.provider_id), entry);
    }

    for (provider_id, breaker) in &snapshot.circuit_breakers {
        states
            .circuit_breakers
            .insert(composite_key(app_id, provider_id), breaker.clone());
    }
}

pub fn export_app_cloud_sync_snapshot(app_id: &str) -> Result<CloudSyncSnapshot> {
    ensure_supported_app(app_id)?;

    let provider_store = providers::read_store()?;
    let states = provider_states::load()?;

    Ok(CloudSyncSnapshot {
        schema_version: CLOUD_SYNC_SCHEMA_VERSION,
        scope: CloudSyncScope::ProviderModelState,
        app_id: app_id.to_string(),
        exported_at: chrono::Utc::now().timestamp(),
        providers: app_providers(&provider_store, app_id)?.clone(),
        state: filter_state_for_app(&states, app_id),
    })
}

pub fn import_app_cloud_sync_snapshot(
    snapshot: CloudSyncSnapshot,
    mode: CloudSyncMergeMode,
) -> Result<CloudSyncImportReport> {
    if snapshot.schema_version != CLOUD_SYNC_SCHEMA_VERSION {
        return Err(anyhow!(
            "Unsupported cloud sync schema version: {}",
            snapshot.schema_version
        ));
    }
    if snapshot.scope != CloudSyncScope::ProviderModelState {
        return Err(anyhow!("Unsupported cloud sync scope"));
    }
    ensure_supported_app(&snapshot.app_id)?;

    let app_id = snapshot.app_id.clone();
    let provider_count = snapshot.providers.providers.len();
    let health_count = snapshot.state.health.len();
    let quota_count = snapshot.state.quotas.len();
    let circuit_breaker_count = snapshot.state.circuit_breakers.len();

    let mut provider_store = providers::read_store()?;
    let target_app = app_providers_mut(&mut provider_store, &app_id)?;
    match mode {
        CloudSyncMergeMode::Replace => {
            *target_app = snapshot.providers.clone();
        }
        CloudSyncMergeMode::Merge => {
            for (provider_id, provider) in &snapshot.providers.providers {
                target_app
                    .providers
                    .insert(provider_id.clone(), provider.clone());
            }
            if let Some(current) = snapshot.providers.current.clone() {
                target_app.current = Some(current);
            }
        }
    }
    providers::write_store(&provider_store)?;

    let mut states = provider_states::load().unwrap_or_default();
    match mode {
        CloudSyncMergeMode::Replace => replace_state_for_app(&mut states, &app_id, &snapshot.state),
        CloudSyncMergeMode::Merge => merge_state_for_app(&mut states, &app_id, &snapshot.state),
    }
    provider_states::save(&states)?;

    Ok(CloudSyncImportReport {
        app_id,
        mode,
        provider_count,
        health_count,
        quota_count,
        circuit_breaker_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use serde_json::json;
    use std::ffi::OsStr;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn env_lock() -> &'static std::sync::Mutex<()> {
        crate::test_env_lock()
    }

    fn set_env<K: AsRef<OsStr>, V: AsRef<OsStr>>(key: K, value: V) {
        unsafe { std::env::set_var(key, value) }
    }

    fn remove_env<K: AsRef<OsStr>>(key: K) {
        unsafe { std::env::remove_var(key) }
    }

    fn with_temp_home<F>(suffix: &str, f: F) -> Result<()>
    where
        F: FnOnce() -> Result<()>,
    {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let temp_root = std::env::temp_dir().join(format!("skillstar-cloud-sync-{suffix}-{stamp}"));
        let previous_home = std::env::var_os("HOME");
        let previous_test_home = std::env::var_os("SKILLSTAR_TEST_HOME");
        let previous_test_config = std::env::var_os("SKILLSTAR_TEST_CONFIG_DIR");
        let previous_data_dir = std::env::var_os("SKILLSTAR_DATA_DIR");
        let previous_hub_dir = std::env::var_os("SKILLSTAR_HUB_DIR");
        set_env("HOME", temp_root.join("home"));
        set_env("SKILLSTAR_TEST_HOME", temp_root.join("home"));
        set_env("SKILLSTAR_TEST_CONFIG_DIR", temp_root.join("config"));
        set_env("SKILLSTAR_DATA_DIR", temp_root.join("data"));
        set_env("SKILLSTAR_HUB_DIR", temp_root.join("data").join("hub"));
        #[cfg(windows)]
        let previous_userprofile = std::env::var_os("USERPROFILE");
        #[cfg(windows)]
        set_env("USERPROFILE", temp_root.join("home"));

        let result = f();

        match previous_home {
            Some(value) => set_env("HOME", value),
            None => remove_env("HOME"),
        }
        match previous_test_home {
            Some(value) => set_env("SKILLSTAR_TEST_HOME", value),
            None => remove_env("SKILLSTAR_TEST_HOME"),
        }
        match previous_test_config {
            Some(value) => set_env("SKILLSTAR_TEST_CONFIG_DIR", value),
            None => remove_env("SKILLSTAR_TEST_CONFIG_DIR"),
        }
        match previous_data_dir {
            Some(value) => set_env("SKILLSTAR_DATA_DIR", value),
            None => remove_env("SKILLSTAR_DATA_DIR"),
        }
        match previous_hub_dir {
            Some(value) => set_env("SKILLSTAR_HUB_DIR", value),
            None => remove_env("SKILLSTAR_HUB_DIR"),
        }
        #[cfg(windows)]
        match previous_userprofile {
            Some(value) => set_env("USERPROFILE", value),
            None => remove_env("USERPROFILE"),
        }

        let _ = std::fs::remove_dir_all(&temp_root);
        result
    }

    fn provider_entry(id: &str, name: &str) -> providers::ProviderEntry {
        providers::ProviderEntry {
            id: id.to_string(),
            name: name.to_string(),
            category: "custom".to_string(),
            settings_config: json!({"env": {}}),
            website_url: None,
            api_key_url: None,
            icon_color: None,
            notes: None,
            created_at: None,
            sort_index: None,
            meta: None,
        }
    }

    fn write_provider_store(
        app_id: &str,
        current: Option<&str>,
        entries: Vec<providers::ProviderEntry>,
    ) -> Result<()> {
        let mut store = providers::read_store()?;
        let app = app_providers_mut(&mut store, app_id)?;
        app.providers = entries
            .into_iter()
            .map(|entry| (entry.id.clone(), entry))
            .collect();
        app.current = current.map(str::to_string);
        providers::write_store(&store)
    }

    fn write_state_fixture(app_id: &str, provider_id: &str, label: &str) -> Result<()> {
        let mut states = provider_states::load().unwrap_or_default();
        states.health.results.insert(
            composite_key(app_id, provider_id),
            ProviderHealth {
                provider_id: provider_id.to_string(),
                app_id: app_id.to_string(),
                url: format!("https://{label}.example.com"),
                latency_ms: Some(123),
                status: Some(200),
                health_status: crate::health::HealthStatus::Healthy,
                checked_at: 100,
                error: None,
            },
        );
        states.quotas.quotas.insert(
            composite_key(app_id, provider_id),
            ProviderQuota {
                provider_id: provider_id.to_string(),
                app_id: app_id.to_string(),
                usage_percent: Some(40),
                remaining: Some(format!("{label}-remaining")),
                reset_time: Some("2026-04-24T00:00:00Z".into()),
                plan_name: Some(label.to_string()),
                fetched_at: 101,
                error: None,
            },
        );
        states.circuit_breakers.insert(
            composite_key(app_id, provider_id),
            CircuitBreakerRecord {
                state: crate::circuit_breaker::CircuitState::Closed,
                failure_count: 0,
                last_failure: None,
                opened_at: None,
                half_open_success: false,
            },
        );
        provider_states::save(&states)
    }

    #[test]
    fn export_snapshot_scopes_to_single_app() -> Result<()> {
        with_temp_home("export", || {
            write_provider_store(
                "codex",
                Some("router"),
                vec![provider_entry("router", "Router")],
            )?;
            write_provider_store(
                "claude",
                Some("official"),
                vec![provider_entry("official", "Official")],
            )?;
            write_state_fixture("codex", "router", "codex")?;
            write_state_fixture("claude", "official", "claude")?;

            let snapshot = export_app_cloud_sync_snapshot("codex")?;
            assert_eq!(snapshot.schema_version, CLOUD_SYNC_SCHEMA_VERSION);
            assert_eq!(snapshot.scope, CloudSyncScope::ProviderModelState);
            assert_eq!(snapshot.app_id, "codex");
            assert_eq!(snapshot.providers.current.as_deref(), Some("router"));
            assert_eq!(snapshot.providers.providers.len(), 1);
            assert_eq!(snapshot.state.health.len(), 1);
            assert_eq!(snapshot.state.quotas.len(), 1);
            assert_eq!(snapshot.state.circuit_breakers.len(), 1);
            assert!(snapshot.state.circuit_breakers.contains_key("router"));
            Ok(())
        })
    }

    #[test]
    fn import_replace_overwrites_existing_app_state() -> Result<()> {
        with_temp_home("replace", || {
            write_provider_store(
                "codex",
                Some("old"),
                vec![provider_entry("old", "Old Provider")],
            )?;
            write_state_fixture("codex", "old", "old")?;

            let snapshot = CloudSyncSnapshot {
                schema_version: CLOUD_SYNC_SCHEMA_VERSION,
                scope: CloudSyncScope::ProviderModelState,
                app_id: "codex".into(),
                exported_at: 999,
                providers: AppProviders {
                    providers: vec![("new".into(), provider_entry("new", "New Provider"))]
                        .into_iter()
                        .collect(),
                    current: Some("new".into()),
                },
                state: CloudSyncProviderState {
                    health: vec![ProviderHealth {
                        provider_id: "new".into(),
                        app_id: "codex".into(),
                        url: "https://new.example.com".into(),
                        latency_ms: Some(5),
                        status: Some(200),
                        health_status: crate::health::HealthStatus::Healthy,
                        checked_at: 11,
                        error: None,
                    }],
                    quotas: vec![ProviderQuota {
                        provider_id: "new".into(),
                        app_id: "codex".into(),
                        usage_percent: Some(12),
                        remaining: Some("fresh".into()),
                        reset_time: None,
                        plan_name: Some("cloud".into()),
                        fetched_at: 12,
                        error: None,
                    }],
                    circuit_breakers: vec![(
                        "new".into(),
                        CircuitBreakerRecord {
                            state: crate::circuit_breaker::CircuitState::Open,
                            failure_count: 6,
                            last_failure: Some(10),
                            opened_at: Some(10),
                            half_open_success: false,
                        },
                    )]
                    .into_iter()
                    .collect(),
                },
            };

            let report = import_app_cloud_sync_snapshot(snapshot, CloudSyncMergeMode::Replace)?;
            assert_eq!(report.provider_count, 1);
            assert_eq!(report.health_count, 1);

            let exported = export_app_cloud_sync_snapshot("codex")?;
            assert!(exported.providers.providers.contains_key("new"));
            assert!(!exported.providers.providers.contains_key("old"));
            assert_eq!(exported.providers.current.as_deref(), Some("new"));
            assert_eq!(exported.state.health[0].provider_id, "new");
            assert_eq!(exported.state.quotas[0].provider_id, "new");
            assert!(exported.state.circuit_breakers.contains_key("new"));
            assert!(!exported.state.circuit_breakers.contains_key("old"));
            Ok(())
        })
    }

    #[test]
    fn import_merge_preserves_existing_entries() -> Result<()> {
        with_temp_home("merge", || {
            write_provider_store(
                "codex",
                Some("local"),
                vec![provider_entry("local", "Local Provider")],
            )?;
            write_state_fixture("codex", "local", "local")?;

            let snapshot = CloudSyncSnapshot {
                schema_version: CLOUD_SYNC_SCHEMA_VERSION,
                scope: CloudSyncScope::ProviderModelState,
                app_id: "codex".into(),
                exported_at: 999,
                providers: AppProviders {
                    providers: vec![("cloud".into(), provider_entry("cloud", "Cloud Provider"))]
                        .into_iter()
                        .collect(),
                    current: Some("cloud".into()),
                },
                state: CloudSyncProviderState {
                    health: vec![ProviderHealth {
                        provider_id: "cloud".into(),
                        app_id: "codex".into(),
                        url: "https://cloud.example.com".into(),
                        latency_ms: Some(9),
                        status: Some(200),
                        health_status: crate::health::HealthStatus::Healthy,
                        checked_at: 21,
                        error: None,
                    }],
                    quotas: vec![ProviderQuota {
                        provider_id: "cloud".into(),
                        app_id: "codex".into(),
                        usage_percent: Some(22),
                        remaining: Some("cloud-remaining".into()),
                        reset_time: None,
                        plan_name: Some("cloud-plan".into()),
                        fetched_at: 22,
                        error: None,
                    }],
                    circuit_breakers: vec![("cloud".into(), CircuitBreakerRecord::default())]
                        .into_iter()
                        .collect(),
                },
            };

            import_app_cloud_sync_snapshot(snapshot, CloudSyncMergeMode::Merge)?;
            let exported = export_app_cloud_sync_snapshot("codex")?;

            assert!(exported.providers.providers.contains_key("local"));
            assert!(exported.providers.providers.contains_key("cloud"));
            assert_eq!(exported.providers.current.as_deref(), Some("cloud"));
            assert_eq!(exported.state.health.len(), 2);
            assert_eq!(exported.state.quotas.len(), 2);
            assert!(exported.state.circuit_breakers.contains_key("local"));
            assert!(exported.state.circuit_breakers.contains_key("cloud"));
            Ok(())
        })
    }

    #[test]
    fn export_rejects_unknown_app_id() -> Result<()> {
        with_temp_home("unknown-app", || {
            let err = export_app_cloud_sync_snapshot("unknown-app").unwrap_err();
            assert!(err.to_string().contains("Unsupported app_id"));
            Ok(())
        })
    }

    #[test]
    fn import_rejects_unsupported_schema_version() -> Result<()> {
        with_temp_home("bad-schema", || {
            let snapshot = CloudSyncSnapshot {
                schema_version: CLOUD_SYNC_SCHEMA_VERSION + 1,
                scope: CloudSyncScope::ProviderModelState,
                app_id: "codex".into(),
                exported_at: 999,
                providers: AppProviders::default(),
                state: CloudSyncProviderState {
                    health: Vec::new(),
                    quotas: Vec::new(),
                    circuit_breakers: HashMap::new(),
                },
            };

            let err =
                import_app_cloud_sync_snapshot(snapshot, CloudSyncMergeMode::Merge).unwrap_err();
            assert!(
                err.to_string()
                    .contains("Unsupported cloud sync schema version")
            );
            Ok(())
        })
    }
}
