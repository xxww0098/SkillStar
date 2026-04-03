use crate::core::ai_provider;
use crate::core::translation_cache::{self, TranslationKind};

use super::{emit_summarize_stream_event, ensure_ai_config};

#[tauri::command]
pub async fn ai_summarize_skill(content: String) -> Result<String, String> {
    let config = ensure_ai_config().await?;

    // Check cache
    if let Ok(Some(cached)) = translation_cache::get_cached_translation(
        TranslationKind::Summary,
        &config.target_language,
        &content,
    ) {
        return Ok(cached.translated_text);
    }

    let result = ai_provider::summarize_text(&config, &content)
        .await
        .map_err(|e| e.to_string())?;

    let _ = translation_cache::upsert_translation(
        TranslationKind::Summary,
        &config.target_language,
        &content,
        &result,
        Some("ai"),
    );

    Ok(result)
}

#[tauri::command]
pub async fn ai_summarize_skill_stream(
    window: tauri::Window,
    request_id: String,
    content: String,
) -> Result<String, String> {
    let config = ensure_ai_config().await?;

    let _ = emit_summarize_stream_event(&window, &request_id, "start", None, None);

    if let Ok(Some(cached)) = translation_cache::get_cached_translation(
        TranslationKind::Summary,
        &config.target_language,
        &content,
    ) {
        let _ = emit_summarize_stream_event(&window, &request_id, "complete", None, None);
        return Ok(cached.translated_text);
    }

    let mut on_delta = |delta: &str| -> anyhow::Result<()> {
        emit_summarize_stream_event(&window, &request_id, "delta", Some(delta.to_string()), None)
            .map_err(anyhow::Error::msg)
    };

    match ai_provider::summarize_text_streaming(&config, &content, &mut on_delta).await {
        Ok(result) => {
            let _ = translation_cache::upsert_translation(
                TranslationKind::Summary,
                &config.target_language,
                &content,
                &result,
                Some("ai"),
            );
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
pub async fn ai_test_connection() -> Result<u64, String> {
    let config = ensure_ai_config().await?;
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
pub async fn ai_pick_skills(
    prompt: String,
    skills: Vec<SkillMeta>,
) -> Result<ai_provider::SkillPickResponse, String> {
    let config = ensure_ai_config().await?;
    let candidates = skills
        .into_iter()
        .map(|skill| ai_provider::SkillPickCandidate {
            name: skill.name,
            description: skill.description,
        })
        .collect();

    ai_provider::pick_skills(&config, &prompt, candidates)
        .await
        .map_err(|e| e.to_string())
}
