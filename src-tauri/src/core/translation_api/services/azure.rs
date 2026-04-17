//! Azure Cognitive Services Translator v3 API service.

use std::future::Future;

use crate::core::ai_provider::config::AiConfig;
use crate::core::translation_api::services::TranslationProvider;
use crate::core::translation_api::{TranslationError, TranslationResult, normalize_lang};

pub struct AzureTranslateService {
    _marker: std::marker::PhantomData<AiConfig>,
}

impl AzureTranslateService {
    pub fn new(_ai_config: &AiConfig) -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }
}

impl TranslationProvider for AzureTranslateService {
    fn name(&self) -> &'static str {
        "azure"
    }
    fn label(&self) -> &'static str {
        "Azure Translator"
    }

    fn translate(
        &self,
        text: &str,
        source_lang: &str,
        target_lang: &str,
    ) -> impl Future<Output = Result<TranslationResult, TranslationError>> + Send {
        async move {
            let config = crate::core::ai_provider::load_config().translation_api;
            let api_key = config.azure_key.clone();
            let region = config.azure_region.clone();

            if api_key.is_empty() {
                return Err(TranslationError::MissingApiKey("Azure Translator".into()));
            }

            let source = if source_lang == "auto" {
                "auto"
            } else {
                source_lang
            };
            let target = normalize_lang("azure", target_lang, true);

            let body = serde_json::json!([{
                "text": text
            }]);

            let endpoint = format!(
                "https://api.cognitive.microsofttranslator.com/translate?api-version=3.0&from={}&to={}",
                source, target
            );

            let mut req = crate::core::ai_provider::http_client::get_http_client()?
                .post(&endpoint)
                .header("Content-Type", "application/json")
                .header("Ocp-Apim-Subscription-Key", &api_key);

            if !region.is_empty() {
                req = req.header("Ocp-Apim-Subscription-Region", &region);
            }

            let resp = req.json(&body).send().await?;
            let status = resp.status();
            let body_text = resp.text().await?;

            if !status.is_success() {
                return Err(TranslationError::ApiError(
                    "Azure Translator".into(),
                    body_text,
                ));
            }

            let parsed: serde_json::Value = serde_json::from_str(&body_text)
                .map_err(|e| TranslationError::ParseError(e.to_string()))?;

            let translated = parsed[0]["translations"][0]["text"]
                .as_str()
                .ok_or_else(|| TranslationError::ParseError("Missing translatedText".into()))?
                .to_string();

            Ok(TranslationResult::new(translated, "azure", text.len()))
        }
    }
}
