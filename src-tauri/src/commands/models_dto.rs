//! Shared DTOs and helpers for model configuration commands.

use crate::core::infra::error::AppError;
use crate::core::model_config::{
    claude, cloud_sync, codex, opencode, providers,
};

/// Status of all three model config files.
#[derive(serde::Serialize)]
pub struct ModelConfigStatus {
    pub claude_config_exists: bool,
    pub claude_config_path: String,
    pub codex_config_exists: bool,
    pub codex_config_path: String,
    pub opencode_config_exists: bool,
    pub opencode_config_path: String,
}

/// Provider list response.
#[derive(serde::Serialize)]
pub struct ProvidersResponse {
    pub providers: std::collections::HashMap<String, providers::ProviderEntry>,
    pub current: Option<String>,
}

/// Built-in preset registry response.
#[derive(serde::Serialize)]
pub struct ProviderPresetsResponse {
    pub presets: Vec<providers::ProviderEntry>,
}

// ── Health Dashboard ────────────────────────────────────────────────

/// Combined health + quota state for a single provider.
#[derive(serde::Serialize)]
pub struct DashboardProviderEntry {
    pub provider_id: String,
    pub name: String,
    pub health_status: String,
    pub latency_ms: Option<u64>,
    pub checked_at: Option<i64>,
    pub usage_percent: Option<i32>,
    pub remaining: Option<String>,
    pub reset_time: Option<String>,
    pub plan_name: Option<String>,
    pub error: Option<String>,
}

/// Aggregated health dashboard for an app.
#[derive(serde::Serialize)]
pub struct ProviderHealthDashboard {
    pub app_id: String,
    pub entries: Vec<DashboardProviderEntry>,
    pub summary: DashboardSummary,
    pub refreshed_at: i64,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUsageSnapshotPayload {
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

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUsageHistoryPointPayload {
    pub provider_id: String,
    pub app_id: String,
    pub usage_percent: Option<i32>,
    pub remaining: Option<String>,
    pub reset_time: Option<String>,
    pub plan_name: Option<String>,
    pub fetched_at: i64,
    pub error: Option<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUsageSummaryPayload {
    pub app_id: String,
    pub provider_id: String,
    pub provider_name: String,
    pub provider_category: String,
    pub current: ProviderUsageSnapshotPayload,
    pub history: Vec<ProviderUsageHistoryPointPayload>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUsageTrackerPayload {
    pub app_id: String,
    pub provider_count: usize,
    pub refreshed_at: i64,
    pub entries: Vec<ProviderUsageSummaryPayload>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportCloudSyncSnapshotInput {
    pub snapshot: cloud_sync::CloudSyncSnapshot,
    pub mode: cloud_sync::CloudSyncMergeMode,
}

impl From<skillstar_model_config::usage_tracker::ProviderUsageSnapshot>
    for ProviderUsageSnapshotPayload
{
    fn from(value: skillstar_model_config::usage_tracker::ProviderUsageSnapshot) -> Self {
        Self {
            provider_id: value.provider_id,
            provider_name: value.provider_name,
            app_id: value.app_id,
            usage_percent: value.usage_percent,
            remaining: value.remaining,
            reset_time: value.reset_time,
            plan_name: value.plan_name,
            fetched_at: value.fetched_at,
            error: value.error,
        }
    }
}

impl From<skillstar_model_config::usage_tracker::ProviderUsageHistoryPoint>
    for ProviderUsageHistoryPointPayload
{
    fn from(value: skillstar_model_config::usage_tracker::ProviderUsageHistoryPoint) -> Self {
        Self {
            provider_id: value.provider_id,
            app_id: value.app_id,
            usage_percent: value.usage_percent,
            remaining: value.remaining,
            reset_time: value.reset_time,
            plan_name: value.plan_name,
            fetched_at: value.fetched_at,
            error: value.error,
        }
    }
}

impl From<skillstar_model_config::usage_tracker::ProviderUsageSummary>
    for ProviderUsageSummaryPayload
{
    fn from(value: skillstar_model_config::usage_tracker::ProviderUsageSummary) -> Self {
        Self {
            app_id: value.app_id,
            provider_id: value.provider_id,
            provider_name: value.provider_name,
            provider_category: value.provider_category,
            current: value.current.into(),
            history: value.history.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<skillstar_model_config::usage_tracker::AppUsageTracker> for AppUsageTrackerPayload {
    fn from(value: skillstar_model_config::usage_tracker::AppUsageTracker) -> Self {
        Self {
            app_id: value.app_id,
            provider_count: value.provider_count,
            refreshed_at: value.refreshed_at,
            entries: value.entries.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(serde::Serialize)]
pub struct DashboardSummary {
    pub total: usize,
    pub healthy: usize,
    pub degraded: usize,
    pub unreachable: usize,
    pub unknown: usize,
}

pub fn build_dashboard(
    app_id: &str,
    providers: &std::collections::HashMap<String, providers::ProviderEntry>,
    health_results: &[skillstar_model_config::health::ProviderHealth],
    quotas: &[skillstar_model_config::quota::ProviderQuota],
    refreshed_at: i64,
) -> ProviderHealthDashboard {
    let mut entries = Vec::with_capacity(providers.len());
    let mut summary = DashboardSummary {
        total: providers.len(),
        healthy: 0,
        degraded: 0,
        unreachable: 0,
        unknown: 0,
    };

    for (id, provider) in providers {
        let health = health_results.iter().find(|h| h.provider_id == *id);
        let quota = quotas.iter().find(|q| q.provider_id == *id);

        let status = health
            .map(|h| match h.health_status {
                skillstar_model_config::health::HealthStatus::Healthy => "healthy",
                skillstar_model_config::health::HealthStatus::Degraded => "degraded",
                skillstar_model_config::health::HealthStatus::Unreachable => "unreachable",
                skillstar_model_config::health::HealthStatus::Unknown => "unknown",
            })
            .unwrap_or("unknown");

        match status {
            "healthy" => summary.healthy += 1,
            "degraded" => summary.degraded += 1,
            "unreachable" => summary.unreachable += 1,
            _ => summary.unknown += 1,
        }

        entries.push(DashboardProviderEntry {
            provider_id: id.clone(),
            name: provider.name.clone(),
            health_status: status.to_string(),
            latency_ms: health.and_then(|h| h.latency_ms),
            checked_at: health.map(|h| h.checked_at),
            usage_percent: quota.and_then(|q| q.usage_percent),
            remaining: quota.and_then(|q| q.remaining.clone()),
            reset_time: quota.and_then(|q| q.reset_time.clone()),
            plan_name: quota.and_then(|q| q.plan_name.clone()),
            error: health.and_then(|h| h.error.clone()),
        });
    }

    // Sort: unhealthy first, then by name
    entries.sort_by(|a, b| {
        let a_score = match a.health_status.as_str() {
            "unreachable" => 0,
            "degraded" => 1,
            "unknown" => 2,
            _ => 3,
        };
        let b_score = match b.health_status.as_str() {
            "unreachable" => 0,
            "degraded" => 1,
            "unknown" => 2,
            _ => 3,
        };
        a_score.cmp(&b_score).then_with(|| a.name.cmp(&b.name))
    });

    ProviderHealthDashboard {
        app_id: app_id.to_string(),
        entries,
        summary,
        refreshed_at,
    }
}

/// Model entry returned from `/v1/models`.
#[derive(serde::Serialize)]
pub struct ModelListEntry {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owned_by: Option<String>,
}

/// Resolve a known config file key to its filesystem path.
/// Only whitelisted keys are allowed to prevent arbitrary file access.
pub fn resolve_config_path(file_key: &str) -> Result<std::path::PathBuf, AppError> {
    match file_key {
        "claude" => Ok(claude::settings_path()),
        "codex_config" => Ok(codex::config_toml_path()),
        "opencode" => Ok(opencode::config_path()),
        _ => Err(AppError::Other(format!(
            "Unknown config file key: {file_key}"
        ))),
    }
}
