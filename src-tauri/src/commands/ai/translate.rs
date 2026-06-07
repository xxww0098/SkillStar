use skillstar_ai::ai_provider;
use tauri::Emitter;

use super::{emit_translate_pipeline_event, ensure_ai_config};

/// Non-streaming translation: returns the translated SKILL.md content.
///
/// For users who don't need progress events. The streaming command emits
/// pipeline-progress events alongside returning the same final result.
#[tauri::command]
pub async fn ai_translate_skill(content: String) -> Result<String, String> {
    let config = ensure_ai_config().await?;
    ai_provider::translate::translate_skill(&config, &content, |_| {})
        .await
        .map_err(|e| e.to_string())
}

/// Streaming translation: emits `ai://translate-stream` events with the
/// pipeline phase + batch progress, and returns the translated content.
///
/// Event payload shape (camelCase, matches AiStreamPayload + pipelineProgress):
///   { requestId, event: "start" | "progress" | "complete" | "error",
///     pipelineProgress?: { phase, current, total },
///     metrics?: { model, tps, completionTokens, elapsedMs, ... },
///     message?: string }
#[tauri::command]
pub async fn ai_translate_skill_stream(
    window: tauri::Window,
    request_id: String,
    content: String,
    force_refresh: Option<bool>,
) -> Result<TranslateSkillStreamResponse, String> {
    let config = ensure_ai_config().await?;

    let _ = emit_translate_pipeline_event(&window, &request_id, "start", None, None, None);

    let window_clone = window.clone();
    let request_id_clone = request_id.clone();
    let on_progress = move |progress: ai_provider::translate::PipelineProgress| {
        let _ = emit_translate_pipeline_event(
            &window_clone,
            &request_id_clone,
            "progress",
            Some(progress),
            None,
            None,
        );
    };

    let options = ai_provider::translate::TranslateOptions {
        force_refresh: force_refresh.unwrap_or(false),
    };

    match ai_provider::translate::translate_skill_with_report(
        &config,
        &content,
        options,
        on_progress,
    )
    .await
    {
        Ok(result) => {
            let metrics = result.metrics.clone();
            let _ = emit_translate_pipeline_event(
                &window,
                &request_id,
                "complete",
                None,
                Some(metrics.clone()),
                None,
            );
            Ok(TranslateSkillStreamResponse {
                content: result.content,
                metrics,
            })
        }
        Err(err) => {
            let message = err.to_string();
            let _ = emit_translate_pipeline_event(
                &window,
                &request_id,
                "error",
                None,
                None,
                Some(message.clone()),
            );
            Err(message)
        }
    }
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TranslateSkillStreamResponse {
    pub content: String,
    pub metrics: ai_provider::translate::TranslationMetrics,
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(super) struct TranslatePipelinePayload {
    pub request_id: String,
    pub event: String,
    pub pipeline_progress: Option<ai_provider::translate::PipelineProgress>,
    pub metrics: Option<ai_provider::translate::TranslationMetrics>,
    pub message: Option<String>,
}

pub(super) fn emit_translate_pipeline_event_impl(
    window: &tauri::Window,
    request_id: &str,
    event: &str,
    progress: Option<ai_provider::translate::PipelineProgress>,
    metrics: Option<ai_provider::translate::TranslationMetrics>,
    message: Option<String>,
) -> Result<(), String> {
    let payload = TranslatePipelinePayload {
        request_id: request_id.to_string(),
        event: event.to_string(),
        pipeline_progress: progress,
        metrics,
        message,
    };
    window
        .emit("ai://translate-stream", payload)
        .map_err(|e| format!("Failed to emit translate event: {e}"))
}
