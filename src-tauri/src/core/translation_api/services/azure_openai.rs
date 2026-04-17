//! Azure OpenAI translation service.

use std::future::Future;

use crate::core::ai_provider::config::AiConfig;
use crate::core::translation_api::config::TranslationProviderSettings;
use crate::core::translation_api::services::TranslationProvider;
use crate::core::translation_api::{TranslationError, TranslationResult};

pub struct AzureOpenAIService {
    api_key: String,
    settings: TranslationProviderSettings,
}

impl AzureOpenAIService {
    pub fn new(ai_config: &AiConfig) -> Self {
        Self {
            api_key: ai_config.translation_api.azure_openai_key.clone(),
            settings: ai_config.translation_api.azure_openai_settings.clone(),
        }
    }
}

impl TranslationProvider for AzureOpenAIService {
    fn name(&self) -> &'static str {
        "azureopenai"
    }
    fn label(&self) -> &'static str {
        "Azure OpenAI"
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
                return Err(TranslationError::MissingApiKey("Azure OpenAI".into()));
            }

            // Azure OpenAI requires deployment name; use model field as deployment name
            let deployment = if settings.model.trim().is_empty() {
                "gpt-4o"
            } else {
                &settings.model
            };
            let base_url = if settings.base_url.trim().is_empty() {
                "https://{your-resource}.openai.azure.com".to_string()
            } else {
                settings.base_url.trim_end_matches('/').to_string()
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
                "messages": [{"role": "user", "content": prompt}],
                "temperature": settings.temperature,
            });

            let url = format!(
                "{}/openai/deployments/{}/chat/completions?api-version=2024-02-1",
                base_url, deployment
            );

            let client = crate::core::ai_provider::http_client::get_http_client()?;
            let resp = client
                .post(&url)
                .header("Content-Type", "application/json")
                .header("api-key", &api_key)
                .json(&body)
                .send()
                .await?;

            let status = resp.status();
            let body_text = resp.text().await?;

            if !status.is_success() {
                return Err(TranslationError::ApiError("Azure OpenAI".into(), body_text));
            }

            let parsed: serde_json::Value = serde_json::from_str(&body_text)
                .map_err(|e| TranslationError::ParseError(e.to_string()))?;

            let translated = parsed["choices"][0]["message"]["content"]
                .as_str()
                .ok_or_else(|| TranslationError::ParseError("Missing message content".into()))?
                .trim()
                .to_string();

            Ok(TranslationResult::new(
                translated,
                "azureopenai",
                text.len(),
            ))
        }
    }
}
