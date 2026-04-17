//! DeepSeek translation service.

use std::future::Future;

use crate::core::ai_provider::config::AiConfig;
use crate::core::translation_api::config::TranslationProviderSettings;
use crate::core::translation_api::services::TranslationProvider;
use crate::core::translation_api::{TranslationError, TranslationResult};

pub struct DeepSeekService {
    api_key: String,
    settings: TranslationProviderSettings,
}

impl DeepSeekService {
    pub fn new(ai_config: &AiConfig) -> Self {
        Self {
            api_key: ai_config.translation_api.deepseek_key.clone(),
            settings: ai_config.translation_api.deepseek_settings.clone(),
        }
    }
}

impl TranslationProvider for DeepSeekService {
    fn name(&self) -> &'static str {
        "deepseek"
    }
    fn label(&self) -> &'static str {
        "DeepSeek"
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
                return Err(TranslationError::MissingApiKey("DeepSeek".into()));
            }

            let model = if settings.model.trim().is_empty() {
                "deepseek-chat"
            } else {
                &settings.model
            };
            let base_url = if settings.base_url.trim().is_empty() {
                "https://api.deepseek.com".to_string()
            } else {
                settings.base_url.trim_end_matches('/').to_string()
            };

            let prompt = format!(
                "Translate the following text from {} to {}:\n\n{}",
                if source_lang == "auto" {
                    "auto-detected language"
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
                .post(format!("{}/chat/completions", base_url))
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {api_key}"))
                .json(&body)
                .send()
                .await?;

            let status = resp.status();
            let body_text = resp.text().await?;

            if !status.is_success() {
                return Err(TranslationError::ApiError("DeepSeek".into(), body_text));
            }

            let parsed: serde_json::Value = serde_json::from_str(&body_text)
                .map_err(|e| TranslationError::ParseError(e.to_string()))?;

            let translated = parsed["choices"][0]["message"]["content"]
                .as_str()
                .ok_or_else(|| TranslationError::ParseError("Missing message content".into()))?
                .trim()
                .to_string();

            Ok(TranslationResult::new(translated, "deepseek", text.len()))
        }
    }
}
