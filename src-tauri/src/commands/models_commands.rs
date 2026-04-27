//! Tauri commands for provider/model CRUD, health, and config management.

use crate::core::app_shell::refresh_tray_menu;
use crate::core::infra::error::AppError;
use crate::core::model_config::{
    claude, cloud_sync, codex, opencode, providers,
};
use crate::core::path_env::command_with_path;

use super::models_dto::*;

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

#[tauri::command]
pub async fn get_claude_model_config() -> Result<serde_json::Value, AppError> {
    claude::read_settings().map_err(|e| AppError::Other(format!("Claude config read error: {e}")))
}

#[tauri::command]
pub async fn save_claude_model_config(config: serde_json::Value) -> Result<(), AppError> {
    claude::write_settings(&config)
        .map_err(|e| AppError::Other(format!("Claude config write error: {e}")))
}

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
pub async fn get_opencode_model_config() -> Result<serde_json::Value, AppError> {
    opencode::read_config().map_err(|e| AppError::Other(format!("OpenCode config read error: {e}")))
}

#[tauri::command]
pub async fn save_opencode_model_config(config: serde_json::Value) -> Result<(), AppError> {
    opencode::write_config(&config)
        .map_err(|e| AppError::Other(format!("OpenCode config write error: {e}")))
}

#[tauri::command]
pub async fn set_claude_setting(key: String, value: serde_json::Value) -> Result<(), AppError> {
    claude::set_field(&key, value)
        .map_err(|e| AppError::Other(format!("Claude set_field error: {e}")))
}

#[tauri::command]
pub async fn set_codex_setting(key: String, value: Option<String>) -> Result<(), AppError> {
    codex::set_field(&key, value.as_deref())
        .map_err(|e| AppError::Other(format!("Codex set_field error: {e}")))
}

#[tauri::command]
pub async fn set_opencode_setting(key: String, value: serde_json::Value) -> Result<(), AppError> {
    opencode::set_field(&key, value)
        .map_err(|e| AppError::Other(format!("OpenCode set_field error: {e}")))
}

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
