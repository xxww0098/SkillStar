//! Google Gemini translation service.

use std::future::Future;

use crate::core::ai_provider::config::AiConfig;
use crate::core::translation_api::config::TranslationProviderSettings;
use crate::core::translation_api::services::TranslationProvider;
use crate::core::translation_api::{TranslationError, TranslationResult};

pub struct GeminiService {
    api_key: String,
    settings: TranslationProviderSettings,
}

impl GeminiService {
    pub fn new(ai_config: &AiConfig) -> Self {
        Self {
            api_key: ai_config.translation_api.gemini_key.clone(),
            settings: ai_config.translation_api.gemini_settings.clone(),
        }
    }
}

impl TranslationProvider for GeminiService {
    fn name(&self) -> &'static str {
        "gemini"
    }
    fn label(&self) -> &'static str {
        "Gemini"
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
                return Err(TranslationError::MissingApiKey("Gemini".into()));
            }

            let model = if settings.model.trim().is_empty() {
                "gemini-2.0-flash"
            } else {
                &settings.model
            };
            let base_url = if settings.base_url.trim().is_empty() {
                "https://generativelanguage.googleapis.com/v1beta".to_string()
            } else {
                settings.base_url.trim_end_matches('/').to_string()
            };
            let endpoint_root = if base_url.contains("/v1") {
                base_url
            } else {
                format!("{}/v1beta", base_url)
            };

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
                "contents": [{
                    "parts": [{"text": prompt}]
                }],
                "generationConfig": {
                    "temperature": settings.temperature,
                }
            });

            let url = format!(
                "{}/models/{}:generateContent?key={}",
                endpoint_root, model, api_key
            );

            let client = crate::core::ai_provider::http_client::get_http_client()?;
            let resp = client
                .post(&url)
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await?;

            let status = resp.status();
            let body_text = resp.text().await?;

            if !status.is_success() {
                return Err(TranslationError::ApiError("Gemini".into(), body_text));
            }

            let parsed: serde_json::Value = serde_json::from_str(&body_text)
                .map_err(|e| TranslationError::ParseError(e.to_string()))?;

            let translated = parsed["candidates"][0]["content"]["parts"][0]["text"]
                .as_str()
                .ok_or_else(|| TranslationError::ParseError("Missing text content".into()))?
                .trim()
                .to_string();

            Ok(TranslationResult::new(translated, "gemini", text.len()))
        }
    }
}
