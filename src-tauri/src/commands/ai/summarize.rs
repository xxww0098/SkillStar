use skillstar_core::infra::error::AppError;
use skillstar_models::ai_provider;

use super::{emit_summarize_stream_event, ensure_ai_config};

fn locale_or_default(locale: Option<&str>) -> &str {
    match locale.map(str::trim).filter(|s| !s.is_empty()) {
        Some(code) => code,
        None => "zh-CN",
    }
}

#[tauri::command]
pub async fn ai_summarize_skill(
    content: String,
    locale: Option<String>,
) -> Result<String, AppError> {
    let config = ensure_ai_config().await?;
    let locale = locale_or_default(locale.as_deref());

    let result = ai_provider::summarize_text(&config, &content, locale).await?;

    Ok(result)
}

#[tauri::command]
pub async fn ai_summarize_skill_stream(
    window: tauri::Window,
    request_id: String,
    content: String,
    locale: Option<String>,
) -> Result<String, AppError> {
    let config = ensure_ai_config().await?;
    let locale = locale_or_default(locale.as_deref());

    let _ = emit_summarize_stream_event(&window, &request_id, "start", None, None);

    let mut on_delta = |delta: &str| -> anyhow::Result<()> {
        emit_summarize_stream_event(&window, &request_id, "delta", Some(delta.to_string()), None)
            .map_err(anyhow::Error::msg)
    };

    match ai_provider::summarize_text_streaming(&config, &content, locale, &mut on_delta).await {
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
            Err(AppError::Other(message))
        }
    }
}

#[tauri::command]
pub async fn ai_test_connection() -> Result<u64, AppError> {
    let config = ensure_ai_config().await?;
    Ok(ai_provider::test_connection(&config).await?)
}
