//! AI command module — split into domain-specific submodules.
//!
//! - `summarize`: summarization and AI connection test

pub mod summarize;

use serde::Serialize;
use skillstar_core::infra::error::AppError;
use skillstar_models::ai_provider;
use tauri::Emitter;

// ── Shared Types ────────────────────────────────────────────────────

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiStreamPayload {
    request_id: String,
    event: String,
    delta: Option<String>,
    message: Option<String>,
}

// ── Shared Helpers ──────────────────────────────────────────────────

fn emit_ai_stream_event(
    window: &tauri::Window,
    channel: &str,
    request_id: &str,
    event: &str,
    delta: Option<String>,
    message: Option<String>,
) -> Result<(), AppError> {
    let payload = AiStreamPayload {
        request_id: request_id.to_string(),
        event: event.to_string(),
        delta,
        message,
    };

    window
        .emit(channel, payload)
        .map_err(|e| AppError::Other(format!("Failed to emit {} event: {}", channel, e)))
}

fn emit_summarize_stream_event(
    window: &tauri::Window,
    request_id: &str,
    event: &str,
    delta: Option<String>,
    message: Option<String>,
) -> Result<(), AppError> {
    emit_ai_stream_event(
        window,
        "ai://summarize-stream",
        request_id,
        event,
        delta,
        message,
    )
}

/// Shared across command modules (summarize here, marketplace AI search).
/// The two error strings are matched verbatim by the frontend (`formatAiErrorMessage`).
pub(crate) async fn ensure_ai_config() -> Result<ai_provider::AiConfig, AppError> {
    let config = ai_provider::load_config_async().await;
    if !config.enabled {
        return Err(AppError::Other(
            "AI provider is disabled. Please enable it in Settings.".to_string(),
        ));
    }
    let config = ai_provider::resolve_runtime_config(&config)?;
    if config.api_key.trim().is_empty() && config.api_format != ai_provider::ApiFormat::Local {
        return Err(AppError::Other(
            "AI provider is not configured. Please choose a Models provider or local model in Settings.".to_string(),
        ));
    }
    Ok(config)
}

// ── Config Commands (stay in mod.rs, too small to warrant a file) ───

#[tauri::command]
pub async fn get_ai_config() -> Result<ai_provider::AiConfig, AppError> {
    Ok(ai_provider::load_config_async().await)
}

#[tauri::command]
pub async fn save_ai_config(config: ai_provider::AiConfig) -> Result<(), AppError> {
    Ok(ai_provider::save_config(&config)?)
}
