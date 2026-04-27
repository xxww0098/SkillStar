//! Tauri commands for model configuration (Claude Code / Codex / OpenCode).

use crate::core::app_shell::refresh_tray_menu;
use crate::core::infra::error::AppError;
use crate::core::model_config::{
    claude, cloud_sync, codex, codex_accounts, codex_oauth, opencode, providers, speedtest,
};
use crate::core::path_env::command_with_path;

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

fn build_dashboard(
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

#[tauri::command]
pub async fn get_provider_health_dashboard(
    app_id: String,
) -> Result<ProviderHealthDashboard, AppError> {
    let (providers, _) = providers::get_providers(&app_id)
        .map_err(|e| AppError::Other(format!("Get providers error: {e}")))?;

    let health_results = skillstar_model_config::health::get_cached_health_for_app(&app_id);
    let quotas = skillstar_model_config::quota::get_cached_quotas_for_app(&app_id);

    Ok(build_dashboard(
        &app_id,
        &providers,
        &health_results,
        &quotas,
        chrono::Utc::now().timestamp(),
    ))
}

#[tauri::command]
pub async fn refresh_provider_health_dashboard(
    app_id: String,
) -> Result<ProviderHealthDashboard, AppError> {
    let (providers, _) = providers::get_providers(&app_id)
        .map_err(|e| AppError::Other(format!("Get providers error: {e}")))?;

    let provider_list: Vec<providers::ProviderEntry> = providers.values().cloned().collect();

    // Run health checks and quota fetches concurrently
    let (health_results, quotas) = tokio::join!(
        skillstar_model_config::health::check_all_providers_health(&app_id, &provider_list),
        skillstar_model_config::quota::fetch_all_quotas(&app_id, &provider_list),
    );

    Ok(build_dashboard(
        &app_id,
        &providers,
        &health_results,
        &quotas,
        chrono::Utc::now().timestamp(),
    ))
}

#[tauri::command]
pub async fn get_provider_usage_tracker(
    app_id: String,
    history_days: Option<usize>,
    history_limit: Option<usize>,
) -> Result<AppUsageTrackerPayload, AppError> {
    let tracker = skillstar_model_config::usage_tracker::get_app_usage_tracker(
        &app_id,
        history_days.unwrap_or(7),
        history_limit.unwrap_or(20),
    )
    .map_err(|e| AppError::Other(format!("Get usage tracker error: {e}")))?;
    Ok(tracker.into())
}

#[tauri::command]
pub async fn get_provider_usage_summary(
    app_id: String,
    provider_id: String,
    history_days: Option<usize>,
    history_limit: Option<usize>,
) -> Result<Option<ProviderUsageSummaryPayload>, AppError> {
    let summary = skillstar_model_config::usage_tracker::get_provider_usage_summary(
        &app_id,
        &provider_id,
        history_days.unwrap_or(7),
        history_limit.unwrap_or(20),
    )
    .map_err(|e| AppError::Other(format!("Get provider usage summary error: {e}")))?;
    Ok(summary.map(Into::into))
}

#[tauri::command]
pub async fn export_model_cloud_sync_snapshot(
    app_id: String,
) -> Result<cloud_sync::CloudSyncSnapshot, AppError> {
    cloud_sync::export_app_cloud_sync_snapshot(&app_id)
        .map_err(|e| AppError::Other(format!("Export model cloud sync snapshot error: {e}")))
}

#[tauri::command]
pub async fn import_model_cloud_sync_snapshot(
    input: ImportCloudSyncSnapshotInput,
) -> Result<cloud_sync::CloudSyncImportReport, AppError> {
    cloud_sync::import_app_cloud_sync_snapshot(input.snapshot, input.mode)
        .map_err(|e| AppError::Other(format!("Import model cloud sync snapshot error: {e}")))
}

// ── Status ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_model_config_status() -> Result<ModelConfigStatus, AppError> {
    Ok(ModelConfigStatus {
        claude_config_exists: claude::config_exists(),
        claude_config_path: claude::config_path_string(),
        codex_config_exists: codex::config_exists(),
        codex_config_path: codex::config_path_string(),
        opencode_config_exists: opencode::config_exists(),
        opencode_config_path: opencode::config_path_string(),
    })
}

// ── Claude ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_claude_model_config() -> Result<serde_json::Value, AppError> {
    claude::read_settings().map_err(|e| AppError::Other(format!("Claude config read error: {e}")))
}

#[tauri::command]
pub async fn save_claude_model_config(config: serde_json::Value) -> Result<(), AppError> {
    claude::write_settings(&config)
        .map_err(|e| AppError::Other(format!("Claude config write error: {e}")))
}

// ── Codex ───────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_codex_model_config() -> Result<String, AppError> {
    codex::read_config_text().map_err(|e| AppError::Other(format!("Codex config read error: {e}")))
}

#[tauri::command]
pub async fn save_codex_model_config(config_text: String) -> Result<(), AppError> {
    codex::write_config(&config_text)
        .map_err(|e| AppError::Other(format!("Codex config write error: {e}")))
}

#[tauri::command]
pub async fn get_codex_auth() -> Result<serde_json::Value, AppError> {
    codex::read_auth().map_err(|e| AppError::Other(format!("Codex auth read error: {e}")))
}

#[tauri::command]
pub async fn save_codex_auth(
    fields: std::collections::HashMap<String, String>,
) -> Result<(), AppError> {
    codex::merge_auth_fields(&fields)
        .map_err(|e| AppError::Other(format!("Codex auth merge error: {e}")))
}

#[tauri::command]
pub async fn get_codex_auth_status() -> Result<codex::CodexAuthStatus, AppError> {
    codex::read_auth_status().map_err(|e| AppError::Other(format!("Codex auth status error: {e}")))
}

// ── Codex OAuth + Multi-Account ────────────────────────────────────

#[tauri::command]
pub async fn codex_oauth_start(
    app: tauri::AppHandle,
) -> Result<codex_oauth::OAuthLoginStartResponse, AppError> {
    codex_oauth::start_oauth_login(app)
        .await
        .map_err(|e| AppError::Other(format!("OAuth start error: {e}")))
}

#[tauri::command]
pub async fn codex_oauth_complete(
    app: tauri::AppHandle,
    login_id: String,
) -> Result<codex_accounts::CodexAccount, AppError> {
    let tokens = codex_oauth::complete_oauth_login(&login_id)
        .await
        .map_err(|e| AppError::Other(format!("OAuth complete error: {e}")))?;

    let account = codex_accounts::create_account_from_tokens(tokens)
        .map_err(|e| AppError::Other(format!("Create account error: {e}")))?;

    let _ = refresh_tray_menu(&app);
    Ok(account)
}

#[tauri::command]
pub async fn codex_oauth_cancel(login_id: Option<String>) -> Result<(), AppError> {
    codex_oauth::cancel_oauth_flow(login_id.as_deref())
        .map_err(|e| AppError::Other(format!("OAuth cancel error: {e}")))
}

#[tauri::command]
pub async fn codex_oauth_submit_callback(
    login_id: String,
    callback_url: String,
) -> Result<(), AppError> {
    codex_oauth::submit_callback_url(&login_id, &callback_url)
        .map_err(|e| AppError::Other(format!("OAuth callback error: {e}")))
}

// ── Gemini OAuth ───────────────────────────────────────────────────

#[tauri::command]
pub async fn gemini_oauth_start()
-> Result<crate::core::model_config::gemini_oauth::GeminiOAuthStartResponse, AppError> {
    crate::core::model_config::gemini_oauth::start_login()
        .await
        .map_err(|e| AppError::Other(format!("Gemini OAuth start error: {e}")))
}

#[tauri::command]
pub async fn gemini_oauth_complete(
    login_id: String,
) -> Result<crate::core::model_config::gemini_oauth::GeminiOAuthCompletePayload, AppError> {
    crate::core::model_config::gemini_oauth::complete_login(&login_id)
        .await
        .map_err(|e| AppError::Other(format!("Gemini OAuth complete error: {e}")))
}

#[tauri::command]
pub fn gemini_oauth_cancel(login_id: Option<String>) -> Result<(), AppError> {
    crate::core::model_config::gemini_oauth::cancel_login(login_id.as_deref())
        .map_err(|e| AppError::Other(format!("Gemini OAuth cancel error: {e}")))
}

#[tauri::command]
pub fn gemini_oauth_submit_callback(
    login_id: String,
    callback_url: String,
) -> Result<(), AppError> {
    crate::core::model_config::gemini_oauth::submit_callback_url(&login_id, &callback_url)
        .map_err(|e| AppError::Other(format!("Gemini OAuth callback error: {e}")))
}

#[tauri::command]
pub async fn list_codex_accounts() -> Result<Vec<codex_accounts::CodexAccount>, AppError> {
    Ok(codex_accounts::list_accounts())
}

#[tauri::command]
pub async fn get_current_codex_account_id() -> Result<Option<String>, AppError> {
    Ok(codex_accounts::get_current_account_id())
}

#[tauri::command]
pub async fn switch_codex_account(
    app: tauri::AppHandle,
    account_id: String,
) -> Result<codex_accounts::CodexAccount, AppError> {
    // For OAuth accounts, ensure tokens are fresh before writing to auth.json.
    // Stale tokens would cause Codex CLI to prompt for re-login.
    if let Some(account) = codex_accounts::load_account(&account_id) {
        if account.auth_mode == "oauth"
            && codex_oauth::is_token_expired(&account.tokens.access_token)
        {
            if let Some(ref refresh_token) = account.tokens.refresh_token {
                match codex_oauth::refresh_access_token(refresh_token).await {
                    Ok(new_tokens) => {
                        let mut refreshed = account.clone();
                        refreshed.tokens = codex_accounts::CodexTokens {
                            id_token: new_tokens.id_token,
                            access_token: new_tokens.access_token,
                            refresh_token: new_tokens.refresh_token,
                        };
                        // Re-extract plan_type from refreshed token
                        if let Ok((_, _, plan_type, _)) =
                            codex_accounts::extract_user_info(&refreshed.tokens.id_token)
                        {
                            if plan_type.is_some() {
                                refreshed.plan_type = plan_type;
                            }
                        }
                        codex_accounts::save_account(&refreshed)
                            .map_err(|e| AppError::Other(format!("Save refreshed tokens: {e}")))?;
                        tracing::info!("Codex OAuth token refreshed during account switch");
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Token refresh failed during switch (will use stale): {}",
                            e
                        );
                    }
                }
            }
        }
    }

    let result = codex_accounts::switch_account(&account_id)
        .map_err(|e| AppError::Other(format!("Switch account error: {e}")))?;

    let _ = refresh_tray_menu(&app);
    Ok(result)
}

#[tauri::command]
pub async fn delete_codex_account(account_id: String) -> Result<(), AppError> {
    codex_accounts::delete_account(&account_id)
        .map_err(|e| AppError::Other(format!("Delete account error: {e}")))
}

#[tauri::command]
pub async fn refresh_codex_quota(
    account_id: String,
) -> Result<codex_accounts::CodexQuota, AppError> {
    codex_accounts::refresh_account_quota(&account_id)
        .await
        .map_err(|e| AppError::Other(format!("Quota refresh error: {e}")))
}

#[tauri::command]
pub async fn refresh_all_codex_quotas()
-> Result<Vec<(String, Result<codex_accounts::CodexQuota, String>)>, AppError> {
    Ok(codex_accounts::refresh_all_quotas().await)
}

#[tauri::command]
pub async fn refresh_gemini_quota(
    app_id: String,
    provider_id: String,
) -> Result<crate::core::model_config::gemini_quota::GeminiQuota, AppError> {
    crate::core::model_config::gemini_quota::refresh_gemini_quota(&app_id, &provider_id)
        .await
        .map_err(|e| AppError::Other(format!("Gemini quota refresh error: {e}")))
}

#[tauri::command]
pub async fn add_codex_api_key_account(
    app: tauri::AppHandle,
    api_key: String,
    api_base_url: Option<String>,
) -> Result<codex_accounts::CodexAccount, AppError> {
    let account = codex_accounts::create_api_key_account(api_key, api_base_url)
        .map_err(|e| AppError::Other(format!("Add API key account error: {e}")))?;

    codex_accounts::switch_account(&account.id)
        .map_err(|e| AppError::Other(format!("Switch account error: {e}")))?;

    let _ = refresh_tray_menu(&app);
    Ok(account)
}

// ── OpenCode ────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_opencode_model_config() -> Result<serde_json::Value, AppError> {
    opencode::read_config().map_err(|e| AppError::Other(format!("OpenCode config read error: {e}")))
}

#[tauri::command]
pub async fn save_opencode_model_config(config: serde_json::Value) -> Result<(), AppError> {
    opencode::write_config(&config)
        .map_err(|e| AppError::Other(format!("OpenCode config write error: {e}")))
}

// ── Instant behavior-field writes (BehaviorStrip) ──────────────────

/// Set a single behavior field in Claude settings.json.
/// Key is a top-level JSON key (e.g. "effortLevel", "alwaysThinkingEnabled").
#[tauri::command]
pub async fn set_claude_setting(key: String, value: serde_json::Value) -> Result<(), AppError> {
    claude::set_field(&key, value)
        .map_err(|e| AppError::Other(format!("Claude set_field error: {e}")))
}

/// Set a single behavior field in Codex config.toml.
/// Key supports dot paths (e.g. "features.fast_mode").
/// Value is an optional TOML-encoded string. If None, the field is removed.
#[tauri::command]
pub async fn set_codex_setting(key: String, value: Option<String>) -> Result<(), AppError> {
    codex::set_field(&key, value.as_deref())
        .map_err(|e| AppError::Other(format!("Codex set_field error: {e}")))
}

/// Set a single behavior field in OpenCode opencode.json.
/// Key supports dot paths (e.g. "permission.edit").
#[tauri::command]
pub async fn set_opencode_setting(key: String, value: serde_json::Value) -> Result<(), AppError> {
    opencode::set_field(&key, value)
        .map_err(|e| AppError::Other(format!("OpenCode set_field error: {e}")))
}

// ── Speed Test ──────────────────────────────────────────────────────

#[tauri::command]
pub async fn test_model_endpoints(
    urls: Vec<String>,
    timeout_secs: Option<u64>,
) -> Result<Vec<speedtest::EndpointLatency>, AppError> {
    speedtest::test_endpoints(urls, timeout_secs)
        .await
        .map_err(|e| AppError::Other(format!("Endpoint test error: {e}")))
}

// ── Provider List (cc-switch style multi-provider management) ──────

#[tauri::command]
pub async fn get_model_providers(app_id: String) -> Result<ProvidersResponse, AppError> {
    let (p, current) = providers::get_providers(&app_id)
        .map_err(|e| AppError::Other(format!("Get providers error: {e}")))?;
    Ok(ProvidersResponse {
        providers: p,
        current,
    })
}

#[tauri::command]
pub async fn get_model_provider_presets(
    app_id: String,
) -> Result<ProviderPresetsResponse, AppError> {
    Ok(ProviderPresetsResponse {
        presets: providers::get_provider_presets(&app_id),
    })
}

#[tauri::command]
pub async fn switch_model_provider(
    app: tauri::AppHandle,
    app_id: String,
    provider_id: String,
) -> Result<(), AppError> {
    providers::switch_provider(&app_id, &provider_id)
        .map_err(|e| AppError::Other(format!("Switch provider error: {e}")))?;
    let _ = refresh_tray_menu(&app);
    Ok(())
}

#[tauri::command]
pub async fn add_model_provider(
    app: tauri::AppHandle,
    app_id: String,
    provider: providers::ProviderEntry,
) -> Result<(), AppError> {
    providers::add_provider(&app_id, provider)
        .map_err(|e| AppError::Other(format!("Add provider error: {e}")))?;
    let _ = refresh_tray_menu(&app);
    Ok(())
}

#[tauri::command]
pub async fn update_model_provider(
    app: tauri::AppHandle,
    app_id: String,
    provider: providers::ProviderEntry,
) -> Result<(), AppError> {
    providers::update_provider(&app_id, provider)
        .map_err(|e| AppError::Other(format!("Update provider error: {e}")))?;
    let _ = refresh_tray_menu(&app);
    Ok(())
}

#[tauri::command]
pub async fn delete_model_provider(
    app: tauri::AppHandle,
    app_id: String,
    provider_id: String,
) -> Result<(), AppError> {
    providers::delete_provider(&app_id, &provider_id)
        .map_err(|e| AppError::Other(format!("Delete provider error: {e}")))?;
    let _ = refresh_tray_menu(&app);
    Ok(())
}

#[tauri::command]
pub async fn reorder_model_providers(
    app: tauri::AppHandle,
    app_id: String,
    provider_ids: Vec<String>,
) -> Result<(), AppError> {
    providers::reorder_providers(&app_id, provider_ids)
        .map_err(|e| AppError::Other(format!("Reorder providers error: {e}")))?;
    let _ = refresh_tray_menu(&app);
    Ok(())
}

// ── Fetch Models from Endpoint ─────────────────────────────────────

/// Model entry returned from `/v1/models`.
#[derive(serde::Serialize)]
pub struct ModelListEntry {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owned_by: Option<String>,
}

/// Fetch available models from an OpenAI-compatible endpoint.
///
/// Sends GET to `base_url + "/models"` (or raw `base_url` when `is_full_url` is true).
/// Parses the standard `{ data: [{ id, owned_by }] }` response format.
#[tauri::command]
pub async fn fetch_endpoint_models(
    base_url: String,
    api_key: Option<String>,
    is_full_url: Option<bool>,
) -> Result<Vec<ModelListEntry>, AppError> {
    let url = if is_full_url.unwrap_or(false) {
        base_url.trim_end_matches('/').to_string()
    } else {
        format!("{}/models", base_url.trim_end_matches('/'))
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| AppError::Other(format!("HTTP client error: {e}")))?;

    let mut req = client.get(&url);
    if let Some(key) = &api_key {
        if !key.is_empty() {
            req = req.bearer_auth(key);
        }
    }

    let resp = req
        .send()
        .await
        .map_err(|e| AppError::Other(format!("Request failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!(
            "Models endpoint returned {status}: {body}"
        )));
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| AppError::Other(format!("Failed to parse response: {e}")))?;

    // Parse OpenAI format: { "data": [{ "id": "...", "owned_by": "..." }] }
    let entries = body
        .get("data")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    let id = item.get("id")?.as_str()?.to_string();
                    let owned_by = item
                        .get("owned_by")
                        .and_then(|o| o.as_str())
                        .map(|s| s.to_string());
                    Some(ModelListEntry { id, owned_by })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(entries)
}

// ── OpenCode CLI Models ────────────────────────────────────────────

/// Run `opencode models` directly and return list of models
#[tauri::command]
pub async fn get_opencode_cli_models() -> Result<Vec<String>, AppError> {
    let output = tauri::async_runtime::spawn_blocking(|| {
        command_with_path("opencode").arg("models").output()
    })
    .await
    .map_err(|e| AppError::Other(format!("Task panic: {e}")))??;

    if !output.status.success() {
        return Err(AppError::Other(format!(
            "opencode models error: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let mut models = Vec::new();
    for line in stdout_str.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            models.push(trimmed.to_string());
        }
    }

    Ok(models)
}

fn get_opencode_auth_path() -> std::path::PathBuf {
    opencode::auth_json_path()
}

#[tauri::command]
pub async fn open_opencode_config_dir() -> Result<(), AppError> {
    let dir = opencode::config_dir();
    super::open_folder(dir.to_string_lossy().to_string()).await
}

#[tauri::command]
pub async fn open_opencode_auth_dir() -> Result<(), AppError> {
    let auth = opencode::auth_json_path();
    let dir = auth
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| auth.clone());
    super::open_folder(dir.to_string_lossy().to_string()).await
}

#[tauri::command]
pub fn gemini_oauth_is_configured() -> bool {
    !crate::core::model_config::gemini_oauth::gemini_oauth_client_id().is_empty()
}

#[tauri::command]
pub async fn get_opencode_auth_providers() -> Result<serde_json::Value, AppError> {
    let path = get_opencode_auth_path();
    let mut parsed: serde_json::Value = if path.exists() {
        let content = std::fs::read_to_string(&path)
            .map_err(|e| AppError::Other(format!("Failed to read auth.json: {}", e)))?;
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    // Augment with environment variables detected by opencode CLI
    if let Ok(output) = command_with_path("opencode")
        .arg("providers")
        .arg("list")
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut obj = parsed.as_object_mut().cloned().unwrap_or_default();

        // Map known env vars to OpenCode CLI provider IDs
        let env_mapping: Vec<(&str, &str)> = vec![
            ("DASHSCOPE_API_KEY", "alibaba"),
            ("OPENAI_API_KEY", "openai"),
            ("ANTHROPIC_API_KEY", "anthropic"),
            ("MOONSHOT_API_KEY", "moonshot"),
            ("ZHIPU_API_KEY", "zhipu"),
            ("DEEPSEEK_API_KEY", "deepseek"),
            ("ARK_API_KEY", "bytedance"), // Volcengine
            ("MINIMAX_API_KEY", "minimax"),
        ];

        for line in stdout.lines() {
            if line.contains('●') {
                for &(env_var, provider_id) in &env_mapping {
                    if line.contains(env_var) {
                        let is_ignored = obj
                            .get(provider_id)
                            .and_then(|v| v.get("type"))
                            .map_or(false, |t| t == "env_ignored");

                        if !obj.contains_key(provider_id) || is_ignored {
                            if !is_ignored {
                                obj.insert(
                                    provider_id.to_string(),
                                    serde_json::json!({ "type": "env", "key": env_var }),
                                );
                            }
                        }
                    }
                }
            }
        }
        parsed = serde_json::Value::Object(obj);
    }

    // Read from ~/.config/opencode/opencode.json for "custom" providers
    if let Ok(oc_config) = crate::core::model_config::opencode::read_config() {
        if let Some(providers) = oc_config.get("provider").and_then(|v| v.as_object()) {
            let mut obj = parsed.as_object_mut().cloned().unwrap_or_default();
            for (key, val) in providers {
                let mut p = serde_json::json!({
                    "type": "custom",
                    "baseURL": "",
                    "key": ""
                });

                if let Some(options) = val.get("options").and_then(|v| v.as_object()) {
                    if let Some(key_str) = options.get("apiKey").and_then(|v| v.as_str()) {
                        p["key"] = serde_json::json!(key_str);
                    }
                    if let Some(base_str) = options.get("baseURL").and_then(|v| v.as_str()) {
                        p["baseURL"] = serde_json::json!(base_str);
                    }
                }

                if !obj.contains_key(key) {
                    obj.insert(key.clone(), p);
                }
            }
            parsed = serde_json::Value::Object(obj);
        }
    }

    Ok(parsed)
}

#[tauri::command]
pub async fn add_opencode_auth_provider(provider: String, key: String) -> Result<(), AppError> {
    let path = get_opencode_auth_path();
    let mut parsed = if path.exists() {
        let content = std::fs::read_to_string(&path)
            .map_err(|e| AppError::Other(format!("Failed to read auth.json: {}", e)))?;
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    if let Some(obj) = parsed.as_object_mut() {
        obj.insert(provider, serde_json::json!({ "type": "api", "key": key }));
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    std::fs::write(&path, serde_json::to_string_pretty(&parsed).unwrap())
        .map_err(|e| AppError::Other(format!("Failed to write auth.json: {}", e)))?;
    Ok(())
}

#[tauri::command]
pub async fn remove_opencode_auth_provider(
    provider: String,
    is_env: Option<bool>,
    is_custom: Option<bool>,
) -> Result<(), AppError> {
    if is_custom.unwrap_or(false) {
        crate::core::model_config::opencode::set_field(
            &format!("provider.{}", provider),
            serde_json::Value::Null,
        )?;
        return Ok(());
    }

    let path = get_opencode_auth_path();
    let content = if path.exists() {
        std::fs::read_to_string(&path)
            .map_err(|e| AppError::Other(format!("Failed to read auth.json: {}", e)))?
    } else {
        "{}".to_string()
    };

    let mut parsed: serde_json::Value =
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}));

    if let Some(obj) = parsed.as_object_mut() {
        if is_env.unwrap_or(false) {
            // Unset internally to stop CLI from seeing it right now
            // We previously removed the env var from the Tauri process here, but
            // std::env::remove_var is unsafe and removing it doesn't affect
            // already running OpenCode CLI instances.
            // The "env_ignored" flag in auth.json handles the persistence.

            // Mark as ignored in auth.json to persist the deletion across restarts
            obj.insert(
                provider.clone(),
                serde_json::json!({ "type": "env_ignored" }),
            );
        } else {
            obj.remove(&provider);
        }
    }

    std::fs::write(&path, serde_json::to_string_pretty(&parsed).unwrap())
        .map_err(|e| AppError::Other(format!("Failed to write auth.json: {}", e)))?;
    Ok(())
}

// ── Raw config file read/write ─────────────────────────────────────

/// Resolve a known config file key to its filesystem path.
/// Only whitelisted keys are allowed to prevent arbitrary file access.
fn resolve_config_path(file_key: &str) -> Result<std::path::PathBuf, AppError> {
    match file_key {
        "claude" => Ok(claude::settings_path()),
        "codex_config" => Ok(codex::config_toml_path()),
        "opencode" => Ok(opencode::config_path()),
        _ => Err(AppError::Other(format!(
            "Unknown config file key: {file_key}"
        ))),
    }
}

#[tauri::command]
pub async fn read_model_config_text(file_key: String) -> Result<String, AppError> {
    let path = resolve_config_path(&file_key)?;
    if !path.exists() {
        return Ok(String::new());
    }
    std::fs::read_to_string(&path)
        .map_err(|e| AppError::Other(format!("Failed to read {}: {e}", path.display())))
}

#[tauri::command]
pub async fn write_model_config_text(file_key: String, content: String) -> Result<(), AppError> {
    let path = resolve_config_path(&file_key)?;
    // Ensure parent dir exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::Other(format!("Failed to create dir: {e}")))?;
    }
    std::fs::write(&path, &content)
        .map_err(|e| AppError::Other(format!("Failed to write {}: {e}", path.display())))
}

#[tauri::command]
pub async fn format_model_config_text(content: String, is_toml: bool) -> Result<String, AppError> {
    if is_toml {
        let val: toml::Value = toml::from_str(&content)
            .map_err(|e| AppError::Other(format!("Invalid TOML: {}", e)))?;
        Ok(toml::to_string_pretty(&val).unwrap_or_else(|_| toml::to_string(&val).unwrap()))
    } else {
        let val: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| AppError::Other(format!("Invalid JSON: {}", e)))?;
        Ok(serde_json::to_string_pretty(&val).unwrap())
    }
}
