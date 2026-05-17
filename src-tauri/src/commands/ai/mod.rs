//! AI command module — split into domain-specific submodules.
//!
//! - `summarize`: summarization, AI connection test, skill pick

pub mod summarize;

use skillstar_ai::ai_provider;
use serde::Serialize;
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
) -> Result<(), String> {
    let payload = AiStreamPayload {
        request_id: request_id.to_string(),
        event: event.to_string(),
        delta,
        message,
    };

    window
        .emit(channel, payload)
        .map_err(|e| format!("Failed to emit {} event: {}", channel, e))
}

fn emit_summarize_stream_event(
    window: &tauri::Window,
    request_id: &str,
    event: &str,
    delta: Option<String>,
    message: Option<String>,
) -> Result<(), String> {
    emit_ai_stream_event(
        window,
        "ai://summarize-stream",
        request_id,
        event,
        delta,
        message,
    )
}

async fn ensure_ai_config() -> Result<ai_provider::AiConfig, String> {
    let config = ai_provider::load_config_async().await;
    if !config.enabled {
        return Err("AI provider is disabled. Please enable it in Settings.".to_string());
    }
    let config = ai_provider::resolve_runtime_config(&config).map_err(|e| e.to_string())?;
    if config.api_key.trim().is_empty() && config.api_format != ai_provider::ApiFormat::Local {
        return Err(
            "AI provider is not configured. Please choose a Models provider or local model in Settings.".to_string(),
        );
    }
    Ok(config)
}

/// Public wrapper for other command modules that need AI config validation.
pub async fn ensure_ai_config_pub() -> Result<ai_provider::AiConfig, String> {
    ensure_ai_config().await
}

// ── Config Commands (stay in mod.rs, too small to warrant a file) ───

#[tauri::command]
pub async fn get_ai_config() -> Result<ai_provider::AiConfig, String> {
    Ok(ai_provider::load_config_async().await)
}

#[tauri::command]
pub async fn save_ai_config(config: ai_provider::AiConfig) -> Result<(), String> {
    ai_provider::save_config(&config).map_err(|e| e.to_string())
}
