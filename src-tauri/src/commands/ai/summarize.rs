use skillstar_core::infra::error::AppError;
use skillstar_models::ai_provider;

use super::{emit_summarize_stream_event, ensure_ai_config};

#[tauri::command]
pub async fn ai_summarize_skill(content: String) -> Result<String, AppError> {
    let config = ensure_ai_config().await?;

    let result = ai_provider::summarize_text(&config, &content).await?;

    Ok(result)
}

#[tauri::command]
pub async fn ai_summarize_skill_stream(
    window: tauri::Window,
    request_id: String,
    content: String,
) -> Result<String, AppError> {
    let config = ensure_ai_config().await?;

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
            Err(AppError::Other(message))
        }
    }
}

#[tauri::command]
pub async fn ai_test_connection() -> Result<u64, AppError> {
    let config = ensure_ai_config().await?;
    Ok(ai_provider::test_connection(&config).await?)
}

#[derive(serde::Deserialize)]
pub struct SkillMeta {
    pub name: String,
    pub description: String,
}

#[tauri::command]
pub async fn ai_pick_skills(
    prompt: String,
    skills: Vec<SkillMeta>,
) -> Result<ai_provider::SkillPickResponse, AppError> {
    let config = ensure_ai_config().await?;
    let candidates = skills
        .into_iter()
        .map(|skill| ai_provider::SkillPickCandidate {
            name: skill.name,
            description: skill.description,
        })
        .collect();

    Ok(ai_provider::pick_skills(&config, &prompt, candidates).await?)
}
