//! Custom LLM (user-supplied OpenAI-compatible endpoint) translation service.

use std::future::Future;

use crate::core::ai_provider::config::AiConfig;
use crate::core::translation_api::config::TranslationProviderSettings;
use crate::core::translation_api::services::TranslationProvider;
use crate::core::translation_api::{TranslationError, TranslationResult};

pub struct CustomLLMService {
    api_key: String,
    settings: TranslationProviderSettings,
}

impl CustomLLMService {
    pub fn new(ai_config: &AiConfig) -> Self {
        Self {
            api_key: ai_config.translation_api.custom_llm_key.clone(),
            settings: ai_config.translation_api.custom_llm_settings.clone(),
        }
    }
}

impl TranslationProvider for CustomLLMService {
    fn name(&self) -> &'static str {
        "customllm"
    }
    fn label(&self) -> &'static str {
        "Custom LLM"
    }

    fn translate(
        &self,
        text: &str,
        source_lang: &str,
        target_lang: &str,
    ) -> impl Future<Output = Result<TranslationResult, TranslationError>> + Send {
        let api_key = self.api_key.clone();
        let settings = self.settings.clone();
        async move {
            let base_url = settings.base_url.clone();
            let model = if settings.model.trim().is_empty() {
                "gpt-4o"
            } else {
                &settings.model
            };

            if base_url.is_empty() {
                return Err(TranslationError::ApiError(
                    "Custom LLM".into(),
                    "base_url not configured".into(),
                ));
            }
            if api_key.is_empty() {
                return Err(TranslationError::MissingApiKey("Custom LLM".into()));
            }

            let prompt = format!(
                "Translate the following text from {} to {}. Respond with ONLY the translated text:\n\n{}",
                if source_lang == "auto" {
                    "the detected language"
                } else {
                    source_lang
                },
                target_lang,
                text
            );

            let body = serde_json::json!({
                "model": model,
                "messages": [{"role": "user", "content": prompt}],
                "temperature": settings.temperature,
            });

            let client = crate::core::ai_provider::http_client::get_http_client()?;
            let resp = client
                .post(format!(
                    "{}/chat/completions",
                    base_url.trim_end_matches('/')
                ))
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {api_key}"))
                .json(&body)
                .send()
                .await?;

            let status = resp.status();
            let body_text = resp.text().await?;

            if !status.is_success() {
                return Err(TranslationError::ApiError("Custom LLM".into(), body_text));
            }

            let parsed: serde_json::Value = serde_json::from_str(&body_text)
                .map_err(|e| TranslationError::ParseError(e.to_string()))?;

            let translated = parsed["choices"][0]["message"]["content"]
                .as_str()
                .ok_or_else(|| TranslationError::ParseError("Missing message content".into()))?
                .trim()
                .to_string();

            Ok(TranslationResult::new(translated, "customllm", text.len()))
        }
    }
}
