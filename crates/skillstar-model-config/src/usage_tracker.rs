//! Provider-centric usage tracking built on top of cached quota state and quota telemetry logs.
//!
//! Surface area:
//! - current usage summaries per app/provider from the unified provider state store
//! - lightweight historical timeline reconstructed from `provider-quota-YYYY-MM-DD.log`

use anyhow::{Context, Result};
use chrono::{Local, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::{provider_states, providers, quota};
use skillstar_infra::paths;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUsageSnapshot {
    pub provider_id: String,
    pub provider_name: String,
    pub app_id: String,
    pub usage_percent: Option<i32>,
    pub remaining: Option<String>,
    pub reset_time: Option<String>,
    pub plan_name: Option<String>,
    pub fetched_at: Option<i64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUsageHistoryPoint {
    pub provider_id: String,
    pub app_id: String,
    pub usage_percent: Option<i32>,
    pub remaining: Option<String>,
    pub reset_time: Option<String>,
    pub plan_name: Option<String>,
    pub fetched_at: i64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUsageSummary {
    pub app_id: String,
    pub provider_id: String,
    pub provider_name: String,
    pub provider_category: String,
    pub current: ProviderUsageSnapshot,
    pub history: Vec<ProviderUsageHistoryPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppUsageTracker {
    pub app_id: String,
    pub provider_count: usize,
    pub refreshed_at: i64,
    pub entries: Vec<ProviderUsageSummary>,
}

fn provider_quota_log_path_for_date(date: NaiveDate) -> PathBuf {
    let stamp = date.format("%Y-%m-%d");
    paths::logs_dir().join(format!("provider-quota-{stamp}.log"))
}

fn today_local() -> NaiveDate {
    Local::now().date_naive()
}

fn collect_history_from_paths(
    log_paths: &[PathBuf],
    app_id: &str,
    provider_id: &str,
    limit: usize,
) -> Vec<ProviderUsageHistoryPoint> {
    let mut entries = Vec::new();

    for path in log_paths {
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let Ok(parsed) = serde_json::from_str::<quota::ProviderQuota>(trimmed) else {
                continue;
            };

            if parsed.app_id != app_id || parsed.provider_id != provider_id {
                continue;
            }

            entries.push(ProviderUsageHistoryPoint {
                provider_id: parsed.provider_id,
                app_id: parsed.app_id,
                usage_percent: parsed.usage_percent,
                remaining: parsed.remaining,
                reset_time: parsed.reset_time,
                plan_name: parsed.plan_name,
                fetched_at: parsed.fetched_at,
                error: parsed.error,
            });
        }
    }

    entries.sort_by(|a, b| b.fetched_at.cmp(&a.fetched_at));
    if entries.len() > limit {
        entries.truncate(limit);
    }
    entries
}

fn history_paths_for_days(history_days: usize) -> Vec<PathBuf> {
    let span = history_days.max(1);
    let today = today_local();
    (0..span)
        .map(|offset| today - chrono::Days::new(offset as u64))
        .map(provider_quota_log_path_for_date)
        .collect()
}

fn summarize_usage_from_state(
    app_id: &str,
    provider_map: &HashMap<String, providers::ProviderEntry>,
    history_days: usize,
    history_limit: usize,
) -> Result<AppUsageTracker> {
    let states = provider_states::load()?;
    let history_paths = history_paths_for_days(history_days);
    let mut entries = Vec::with_capacity(provider_map.len());

    for (provider_id, provider) in provider_map {
        let cached = states
            .quotas
            .quotas
            .get(&quota::composite_key(app_id, provider_id));

        entries.push(ProviderUsageSummary {
            app_id: app_id.to_string(),
            provider_id: provider_id.clone(),
            provider_name: provider.name.clone(),
            provider_category: provider.category.clone(),
            current: ProviderUsageSnapshot {
                provider_id: provider_id.clone(),
                provider_name: provider.name.clone(),
                app_id: app_id.to_string(),
                usage_percent: cached.and_then(|q| q.usage_percent),
                remaining: cached.and_then(|q| q.remaining.clone()),
                reset_time: cached.and_then(|q| q.reset_time.clone()),
                plan_name: cached.and_then(|q| q.plan_name.clone()),
                fetched_at: cached.map(|q| q.fetched_at),
                error: cached.and_then(|q| q.error.clone()),
            },
            history: collect_history_from_paths(&history_paths, app_id, provider_id, history_limit),
        });
    }

    entries.sort_by(|a, b| {
        let a_score = a.current.usage_percent.unwrap_or(-1);
        let b_score = b.current.usage_percent.unwrap_or(-1);
        b_score
            .cmp(&a_score)
            .then_with(|| a.provider_name.cmp(&b.provider_name))
    });

    Ok(AppUsageTracker {
        app_id: app_id.to_string(),
        provider_count: entries.len(),
        refreshed_at: chrono::Utc::now().timestamp(),
        entries,
    })
}

pub fn get_app_usage_tracker(
    app_id: &str,
    history_days: usize,
    history_limit: usize,
) -> Result<AppUsageTracker> {
    let (provider_map, _) = providers::get_providers(app_id)
        .with_context(|| format!("Failed to load providers for app {app_id}"))?;
    summarize_usage_from_state(app_id, &provider_map, history_days, history_limit.max(1))
}

pub fn get_provider_usage_summary(
    app_id: &str,
    provider_id: &str,
    history_days: usize,
    history_limit: usize,
) -> Result<Option<ProviderUsageSummary>> {
    let tracker = get_app_usage_tracker(app_id, history_days, history_limit.max(1))?;
    Ok(tracker
        .entries
        .into_iter()
        .find(|entry| entry.provider_id == provider_id))
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
        let temp_root =
            std::env::temp_dir().join(format!("skillstar-usage-tracker-{suffix}-{stamp}"));
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

    fn provider_entry(id: &str, name: &str, category: &str) -> providers::ProviderEntry {
        providers::ProviderEntry {
            id: id.to_string(),
            name: name.to_string(),
            category: category.to_string(),
            settings_config: json!({}),
            website_url: None,
            api_key_url: None,
            icon_color: None,
            notes: None,
            created_at: None,
            sort_index: None,
            meta: None,
        }
    }

    fn write_store(app_id: &str, entries: Vec<providers::ProviderEntry>) -> Result<()> {
        let mut store = providers::ProvidersStore::default();
        let app = match app_id {
            "claude" => &mut store.claude,
            "codex" => &mut store.codex,
            "opencode" => &mut store.opencode,
            "gemini" => &mut store.gemini,
            _ => &mut store.claude,
        };

        for entry in entries {
            app.providers.insert(entry.id.clone(), entry);
        }
        providers::write_store(&store)
    }

    fn write_states(quotas: Vec<quota::ProviderQuota>) -> Result<()> {
        let mut states = provider_states::ProviderStates::default();
        for quota in quotas {
            states.quotas.quotas.insert(
                quota::composite_key(&quota.app_id, &quota.provider_id),
                quota,
            );
        }
        provider_states::save(&states)
    }

    fn write_log(path: &Path, lines: &[quota::ProviderQuota]) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let body = lines
            .iter()
            .map(serde_json::to_string)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .join("\n");
        std::fs::write(path, format!("{body}\n"))?;
        Ok(())
    }

    #[test]
    fn app_usage_tracker_combines_cached_state_and_history() -> Result<()> {
        with_temp_home("summary-history", || {
            write_store(
                "codex",
                vec![
                    provider_entry("router", "Router", "aggregator"),
                    provider_entry("official", "Official", "official"),
                ],
            )?;

            write_states(vec![
                quota::ProviderQuota {
                    provider_id: "router".into(),
                    app_id: "codex".into(),
                    usage_percent: Some(65),
                    remaining: Some("$3.50 / $10".into()),
                    reset_time: Some("2026-04-24T00:00:00Z".into()),
                    plan_name: Some("Pro".into()),
                    fetched_at: 200,
                    error: None,
                },
                quota::ProviderQuota {
                    provider_id: "official".into(),
                    app_id: "codex".into(),
                    usage_percent: Some(10),
                    remaining: Some("$9 / $10".into()),
                    reset_time: None,
                    plan_name: Some("Starter".into()),
                    fetched_at: 150,
                    error: None,
                },
            ])?;

            let today_path = provider_quota_log_path_for_date(today_local());
            write_log(
                &today_path,
                &[
                    quota::ProviderQuota {
                        provider_id: "router".into(),
                        app_id: "codex".into(),
                        usage_percent: Some(50),
                        remaining: Some("$5 / $10".into()),
                        reset_time: None,
                        plan_name: Some("Pro".into()),
                        fetched_at: 100,
                        error: None,
                    },
                    quota::ProviderQuota {
                        provider_id: "router".into(),
                        app_id: "codex".into(),
                        usage_percent: Some(60),
                        remaining: Some("$4 / $10".into()),
                        reset_time: None,
                        plan_name: Some("Pro".into()),
                        fetched_at: 120,
                        error: None,
                    },
                    quota::ProviderQuota {
                        provider_id: "official".into(),
                        app_id: "codex".into(),
                        usage_percent: Some(5),
                        remaining: None,
                        reset_time: None,
                        plan_name: Some("Starter".into()),
                        fetched_at: 110,
                        error: None,
                    },
                ],
            )?;

            let tracker = get_app_usage_tracker("codex", 1, 5)?;
            assert_eq!(tracker.provider_count, 2);
            assert_eq!(tracker.entries[0].provider_id, "router");
            assert_eq!(tracker.entries[0].current.usage_percent, Some(65));
            assert_eq!(tracker.entries[0].history.len(), 2);
            assert_eq!(tracker.entries[0].history[0].fetched_at, 120);
            assert_eq!(tracker.entries[0].history[1].fetched_at, 100);
            assert_eq!(tracker.entries[1].provider_id, "official");
            Ok(())
        })
    }

    #[test]
    fn provider_usage_summary_returns_none_for_unknown_provider() -> Result<()> {
        with_temp_home("missing-provider", || {
            write_store(
                "codex",
                vec![provider_entry("router", "Router", "aggregator")],
            )?;
            write_states(vec![])?;

            let summary = get_provider_usage_summary("codex", "unknown", 1, 10)?;
            assert!(summary.is_none());
            Ok(())
        })
    }

    #[test]
    fn usage_history_respects_day_window_and_limit() -> Result<()> {
        with_temp_home("window-limit", || {
            write_store(
                "codex",
                vec![provider_entry("router", "Router", "aggregator")],
            )?;
            write_states(vec![])?;

            let today = today_local();
            let yesterday = today - chrono::Days::new(1);
            let two_days_ago = today - chrono::Days::new(2);

            write_log(
                &provider_quota_log_path_for_date(today),
                &[
                    quota::ProviderQuota {
                        provider_id: "router".into(),
                        app_id: "codex".into(),
                        usage_percent: Some(40),
                        remaining: None,
                        reset_time: None,
                        plan_name: None,
                        fetched_at: 300,
                        error: None,
                    },
                    quota::ProviderQuota {
                        provider_id: "router".into(),
                        app_id: "codex".into(),
                        usage_percent: Some(35),
                        remaining: None,
                        reset_time: None,
                        plan_name: None,
                        fetched_at: 250,
                        error: None,
                    },
                ],
            )?;
            write_log(
                &provider_quota_log_path_for_date(yesterday),
                &[quota::ProviderQuota {
                    provider_id: "router".into(),
                    app_id: "codex".into(),
                    usage_percent: Some(20),
                    remaining: None,
                    reset_time: None,
                    plan_name: None,
                    fetched_at: 200,
                    error: None,
                }],
            )?;
            write_log(
                &provider_quota_log_path_for_date(two_days_ago),
                &[quota::ProviderQuota {
                    provider_id: "router".into(),
                    app_id: "codex".into(),
                    usage_percent: Some(10),
                    remaining: None,
                    reset_time: None,
                    plan_name: None,
                    fetched_at: 100,
                    error: None,
                }],
            )?;

            let summary = get_provider_usage_summary("codex", "router", 2, 2)?
                .expect("provider summary should exist");
            assert_eq!(summary.history.len(), 2);
            assert_eq!(summary.history[0].fetched_at, 300);
            assert_eq!(summary.history[1].fetched_at, 250);
            assert!(summary.history.iter().all(|point| point.fetched_at != 100));
            Ok(())
        })
    }
}
