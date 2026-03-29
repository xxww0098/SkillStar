use crate::core::ai_provider;
use serde::Serialize;
use tauri::Emitter;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AiStreamPayload {
    request_id: String,
    event: String,
    delta: Option<String>,
    message: Option<String>,
}

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

fn emit_translate_stream_event(
    window: &tauri::Window,
    request_id: &str,
    event: &str,
    delta: Option<String>,
    message: Option<String>,
) -> Result<(), String> {
    emit_ai_stream_event(
        window,
        "ai://translate-stream",
        request_id,
        event,
        delta,
        message,
    )
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

#[tauri::command]
pub async fn get_ai_config() -> Result<ai_provider::AiConfig, String> {
    Ok(ai_provider::load_config())
}

#[tauri::command]
pub async fn save_ai_config(config: ai_provider::AiConfig) -> Result<(), String> {
    ai_provider::save_config(&config).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ai_translate_skill(content: String) -> Result<String, String> {
    let config = ai_provider::load_config();
    if !config.enabled {
        return Err("AI provider is disabled. Please enable it in Settings.".to_string());
    }
    if config.api_key.trim().is_empty() {
        return Err(
            "AI provider is not configured. Please set up your API key in Settings.".to_string(),
        );
    }
    ai_provider::translate_text(&config, &content)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ai_translate_skill_stream(
    window: tauri::Window,
    request_id: String,
    content: String,
) -> Result<String, String> {
    let config = ai_provider::load_config();
    if !config.enabled {
        return Err("AI provider is disabled. Please enable it in Settings.".to_string());
    }
    if config.api_key.trim().is_empty() {
        return Err(
            "AI provider is not configured. Please set up your API key in Settings.".to_string(),
        );
    }

    let _ = emit_translate_stream_event(&window, &request_id, "start", None, None);

    let mut on_delta = |delta: &str| -> anyhow::Result<()> {
        emit_translate_stream_event(&window, &request_id, "delta", Some(delta.to_string()), None)
            .map_err(anyhow::Error::msg)
    };

    match ai_provider::translate_text_streaming(&config, &content, &mut on_delta).await {
        Ok(result) => {
            let _ = emit_translate_stream_event(&window, &request_id, "complete", None, None);
            Ok(result)
        }
        Err(err) => {
            let message = err.to_string();
            let _ = emit_translate_stream_event(
                &window,
                &request_id,
                "error",
                None,
                Some(message.clone()),
            );
            Err(message)
        }
    }
}

#[tauri::command]
pub async fn ai_translate_short_text_stream(
    window: tauri::Window,
    request_id: String,
    content: String,
) -> Result<String, String> {
    let config = ai_provider::load_config();
    if !config.enabled {
        return Err("AI provider is disabled. Please enable it in Settings.".to_string());
    }
    if config.api_key.trim().is_empty() {
        return Err(
            "AI provider is not configured. Please set up your API key in Settings.".to_string(),
        );
    }

    let _ = emit_translate_stream_event(&window, &request_id, "start", None, None);

    let mut on_delta = |delta: &str| -> anyhow::Result<()> {
        emit_translate_stream_event(&window, &request_id, "delta", Some(delta.to_string()), None)
            .map_err(anyhow::Error::msg)
    };

    match ai_provider::translate_short_text_streaming(&config, &content, &mut on_delta).await {
        Ok(result) => {
            let _ = emit_translate_stream_event(&window, &request_id, "complete", None, None);
            Ok(result)
        }
        Err(err) => {
            let message = err.to_string();
            let _ = emit_translate_stream_event(
                &window,
                &request_id,
                "error",
                None,
                Some(message.clone()),
            );
            Err(message)
        }
    }
}

#[tauri::command]
pub async fn ai_summarize_skill(content: String) -> Result<String, String> {
    let config = ai_provider::load_config();
    if !config.enabled {
        return Err("AI provider is disabled. Please enable it in Settings.".to_string());
    }
    if config.api_key.trim().is_empty() {
        return Err(
            "AI provider is not configured. Please set up your API key in Settings.".to_string(),
        );
    }
    ai_provider::summarize_text(&config, &content)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ai_summarize_skill_stream(
    window: tauri::Window,
    request_id: String,
    content: String,
) -> Result<String, String> {
    let config = ai_provider::load_config();
    if !config.enabled {
        return Err("AI provider is disabled. Please enable it in Settings.".to_string());
    }
    if config.api_key.trim().is_empty() {
        return Err(
            "AI provider is not configured. Please set up your API key in Settings.".to_string(),
        );
    }

    let _ = emit_summarize_stream_event(&window, &request_id, "start", None, None);

    let mut on_delta = |delta: &str| -> anyhow::Result<()> {
        emit_summarize_stream_event(&window, &request_id, "delta", Some(delta.to_string()), None)
            .map_err(anyhow::Error::msg)
    };

    match ai_provider::summarize_text_streaming(&config, &content, &mut on_delta).await {
        Ok(result) => {
            let _ = emit_summarize_stream_event(&window, &request_id, "complete", None, None);
            Ok(result)
        }
        Err(err) => {
            let message = err.to_string();
            let _ = emit_summarize_stream_event(
                &window,
                &request_id,
                "error",
                None,
                Some(message.clone()),
            );
            Err(message)
        }
    }
}

#[tauri::command]
pub async fn ai_test_connection() -> Result<String, String> {
    let config = ai_provider::load_config();
    if !config.enabled {
        return Err("AI provider is disabled. Please enable it in Settings.".to_string());
    }
    if config.api_key.trim().is_empty() {
        return Err("API key is empty".to_string());
    }
    ai_provider::test_connection(&config)
        .await
        .map_err(|e| e.to_string())
}

#[derive(serde::Deserialize)]
pub struct SkillMeta {
    pub name: String,
    pub description: String,
}

#[tauri::command]
pub async fn ai_pick_skills(prompt: String, skills: Vec<SkillMeta>) -> Result<Vec<String>, String> {
    let config = ai_provider::load_config();
    if !config.enabled {
        return Err("AI provider is disabled. Please enable it in Settings.".to_string());
    }
    if config.api_key.trim().is_empty() {
        return Err(
            "AI provider is not configured. Please set up your API key in Settings.".to_string(),
        );
    }

    // Build a YAML-like catalog from skill metadata
    let catalog = skills
        .iter()
        .map(|s| format!("- name: {}\n  description: {}", s.name, s.description))
        .collect::<Vec<_>>()
        .join("\n");

    ai_provider::pick_skills(&config, &prompt, &catalog)
        .await
        .map_err(|e| e.to_string())
}
