//! Anthropic Claude translation service.

use std::future::Future;

use crate::core::ai_provider::config::AiConfig;
use crate::core::translation_api::config::TranslationProviderSettings;
use crate::core::translation_api::services::TranslationProvider;
use crate::core::translation_api::{TranslationError, TranslationResult};

pub struct ClaudeService {
    api_key: String,
    settings: TranslationProviderSettings,
}

impl ClaudeService {
    pub fn new(ai_config: &AiConfig) -> Self {
        Self {
            api_key: ai_config.translation_api.claude_key.clone(),
            settings: ai_config.translation_api.claude_settings.clone(),
        }
    }
}

impl TranslationProvider for ClaudeService {
    fn name(&self) -> &'static str {
        "claude"
    }
    fn label(&self) -> &'static str {
        "Claude"
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
            if api_key.is_empty() {
                return Err(TranslationError::MissingApiKey("Claude".into()));
            }

            let model = if settings.model.trim().is_empty() {
                "claude-sonnet-4-7"
            } else {
                &settings.model
            };
            let base_url = if settings.base_url.trim().is_empty() {
                "https://api.anthropic.com".to_string()
            } else {
                settings.base_url.trim_end_matches('/').to_string()
            };

            let prompt = format!(
                "Translate the following text from {} to {} (respond with ONLY the translated text):\n\n{}",
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
                "max_tokens": 4096,
            });

            let client = crate::core::ai_provider::http_client::get_http_client()?;
            let resp = client
                .post(format!("{}/v1/messages", base_url))
                .header("Content-Type", "application/json")
                .header("x-api-key", &api_key)
                .header("anthropic-version", "2023-06-01")
                .json(&body)
                .send()
                .await?;

            let status = resp.status();
            let body_text = resp.text().await?;

            if !status.is_success() {
                return Err(TranslationError::ApiError("Claude".into(), body_text));
            }

            let parsed: serde_json::Value = serde_json::from_str(&body_text)
                .map_err(|e| TranslationError::ParseError(e.to_string()))?;

            let translated = parsed["content"][0]["text"]
                .as_str()
                .ok_or_else(|| TranslationError::ParseError("Missing text content".into()))?
                .trim()
                .to_string();

            Ok(TranslationResult::new(translated, "claude", text.len()))
        }
    }
}
