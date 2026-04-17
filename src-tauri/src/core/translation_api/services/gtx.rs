//! Google Translate AJAX (GTX) free endpoint service.
//! Uses the public free API — no API key required.
//! Rate-limited: ~100 chars/request, ~100 requests/day.

use std::future::Future;

use crate::core::translation_api::services::TranslationProvider;
use crate::core::translation_api::{TranslationError, TranslationResult, normalize_lang};

pub struct GtxFreeService;

impl GtxFreeService {
    pub fn new() -> Self {
        Self
    }
}

impl TranslationProvider for GtxFreeService {
    fn name(&self) -> &'static str {
        "gtx"
    }
    fn label(&self) -> &'static str {
        "GTX Web (Free)"
    }

    fn translate(
        &self,
        text: &str,
        source_lang: &str,
        target_lang: &str,
    ) -> impl Future<Output = Result<TranslationResult, TranslationError>> + Send {
        async move {
            let source = if source_lang == "auto" {
                "auto"
            } else {
                source_lang
            };
            let target = normalize_lang("gtx", target_lang, true);

            let client = crate::core::ai_provider::http_client::get_http_client()?;
            let url = format!(
                "https://translate.googleapis.com/translate_a/single?client=gtx&sl={}&tl={}&dt=t&q={}",
                source,
                target,
                urlencoding::encode(text)
            );

            let resp = client.get(&url).send().await?;
            let status = resp.status();
            let body_text = resp.text().await?;

            if !status.is_success() {
                return Err(TranslationError::ApiError("GTX".into(), body_text));
            }

            // Response is a JSON array: [[translated_text, original_text, detected_lang], ...]
            let parsed: serde_json::Value = serde_json::from_str(&body_text)
                .map_err(|e| TranslationError::ParseError(e.to_string()))?;

            let translated = parsed[0][0]
                .as_str()
                .ok_or_else(|| {
                    TranslationError::ParseError("Missing translated text in GTX response".into())
                })?
                .to_string();

            Ok(TranslationResult::new(translated, "gtx", text.len()))
        }
    }
}
