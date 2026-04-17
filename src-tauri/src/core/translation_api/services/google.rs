//! Google Translate v2 API service.

use std::future::Future;

use crate::core::ai_provider::config::AiConfig;
use crate::core::translation_api::services::TranslationProvider;
use crate::core::translation_api::{TranslationError, TranslationResult, normalize_lang};

pub struct GoogleTranslateService {
    _marker: std::marker::PhantomData<AiConfig>,
}

impl GoogleTranslateService {
    pub fn new(_ai_config: &AiConfig) -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }
}

impl TranslationProvider for GoogleTranslateService {
    fn name(&self) -> &'static str {
        "google"
    }
    fn label(&self) -> &'static str {
        "Google Translate"
    }

    fn translate(
        &self,
        text: &str,
        source_lang: &str,
        target_lang: &str,
    ) -> impl Future<Output = Result<TranslationResult, TranslationError>> + Send {
        async move {
            let api_key = crate::core::ai_provider::load_config()
                .translation_api
                .google_key
                .clone();
            if api_key.is_empty() {
                return Err(TranslationError::MissingApiKey("Google Translate".into()));
            }

            let source = if source_lang == "auto" {
                "auto"
            } else {
                source_lang
            };
            let target = normalize_lang("google", target_lang, true);

            let params = [
                ("key", api_key.as_str()),
                ("q", text),
                ("source", source),
                ("target", &target),
                ("format", "html"),
            ];

            let client = crate::core::ai_provider::http_client::get_http_client()?;
            let resp = client
                .post("https://translation.googleapis.com/language/translate/v2")
                .header("Content-Type", "application/x-www-form-urlencoded")
                .form(&params)
                .send()
                .await?;

            let status = resp.status();
            let body_text = resp.text().await?;

            if !status.is_success() {
                return Err(TranslationError::ApiError(
                    "Google Translate".into(),
                    body_text,
                ));
            }

            let parsed: serde_json::Value = serde_json::from_str(&body_text)
                .map_err(|e| TranslationError::ParseError(e.to_string()))?;

            let translated = parsed["data"]["translations"][0]["translatedText"]
                .as_str()
                .ok_or_else(|| TranslationError::ParseError("Missing translatedText".into()))?
                .to_string();

            Ok(TranslationResult::new(translated, "google", text.len()))
        }
    }
}
