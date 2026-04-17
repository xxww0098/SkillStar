//! OpenAI-compatible translation service (OpenAI, Perplexity, Groq, OpenRouter, SiliconFlow).

use std::future::Future;

use crate::core::ai_provider::config::AiConfig;
use crate::core::translation_api::services::TranslationProvider;
use crate::core::translation_api::{TranslationError, TranslationResult};

pub struct OpenAICompatService {
    api_key: String,
    provider: String,
    base_url: String,
    settings: crate::core::translation_api::config::TranslationProviderSettings,
}

fn chat_completions_endpoint(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with("/chat/completions") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/chat/completions")
    }
}

impl OpenAICompatService {
    pub fn new(ai_config: &AiConfig, provider: &str, base_url: &str) -> Self {
        let key_field = match provider {
            "openai" => &ai_config.translation_api.openai_key,
            "perplexity" => &ai_config.translation_api.perplexity_key,
            "siliconflow" => &ai_config.translation_api.siliconflow_key,
            "groq" => &ai_config.translation_api.groq_key,
            "openrouter" => &ai_config.translation_api.openrouter_key,
            _ => {
                return Self {
                    api_key: String::new(),
                    provider: provider.to_string(),
                    base_url: base_url.to_string(),
                    settings: Default::default(),
                };
            }
        };
        let settings = match provider {
            "openai" => &ai_config.translation_api.openai_settings,
            "perplexity" => &ai_config.translation_api.perplexity_settings,
            "siliconflow" => &ai_config.translation_api.siliconflow_settings,
            "groq" => &ai_config.translation_api.groq_settings,
            "openrouter" => &ai_config.translation_api.openrouter_settings,
            _ => {
                return Self {
                    api_key: key_field.clone(),
                    provider: provider.to_string(),
                    base_url: base_url.to_string(),
                    settings: Default::default(),
                };
            }
        };
        Self {
            api_key: key_field.clone(),
            provider: provider.to_string(),
            base_url: if settings.base_url.is_empty() {
                base_url.to_string()
            } else {
                settings.base_url.clone()
            },
            settings: settings.clone(),
        }
    }

    fn do_translate(
        &self,
        text: &str,
        source_lang: &str,
        target_lang: &str,
    ) -> Result<String, TranslationError> {
        if self.api_key.is_empty() {
            return Err(TranslationError::MissingApiKey(self.provider.clone()));
        }
        let model = if self.settings.model.is_empty() {
            match self.provider.as_str() {
                "openai" => "gpt-4o",
                "perplexity" => "sonar",
                "groq" => "llama-3.3-70b-versatile",
                "openrouter" => "anthropic/claude-3.5-haiku",
                "siliconflow" => "Qwen/Qwen2.5-7B-Instruct",
                _ => "gpt-4o",
            }
        } else {
            &self.settings.model
        };

        let prompt = format!(
            "Translate the following text from {} to {} (respond with ONLY the translated text, no explanations):\n\n{}",
            if source_lang == "auto" {
                "the detected language"
            } else {
                source_lang
            },
            target_lang,
            text
        );

        Ok(serde_json::json!({
            "model": model,
            "messages": [{"role": "user", "content": prompt}],
            "temperature": self.settings.temperature,
        })
        .to_string())
    }
}

impl TranslationProvider for OpenAICompatService {
    fn name(&self) -> &'static str {
        match self.provider.as_str() {
            "openai" => "openai",
            "perplexity" => "perplexity",
            "siliconflow" => "siliconflow",
            "groq" => "groq",
            "openrouter" => "openrouter",
            _ => "openai",
        }
    }
    fn label(&self) -> &'static str {
        match self.provider.as_str() {
            "openai" => "OpenAI",
            "perplexity" => "Perplexity",
            "siliconflow" => "SiliconFlow",
            "groq" => "Groq",
            "openrouter" => "OpenRouter",
            _ => "OpenAI-compatible",
        }
    }

    fn translate(
        &self,
        text: &str,
        source_lang: &str,
        target_lang: &str,
    ) -> impl Future<Output = Result<TranslationResult, TranslationError>> + Send {
        let api_key = self.api_key.clone();
        let base_url = self.base_url.clone();
        let endpoint_url = chat_completions_endpoint(&base_url);
        let body_json = self.do_translate(text, source_lang, target_lang);
        async move {
            let body_json = body_json?;
            let client = crate::core::ai_provider::http_client::get_http_client()?;
            let resp = client
                .post(endpoint_url)
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {api_key}"))
                .body(body_json)
                .send()
                .await?;

            let status = resp.status();
            let body_text = resp.text().await?;

            if !status.is_success() {
                return Err(TranslationError::ApiError(
                    "OpenAI-compatible".into(),
                    body_text,
                ));
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
                "openai_compat",
                text.len(),
            ))
        }
    }
}
