use std::future::Future;

use skillstar_ai::ai_provider::config::AiConfig;

use crate::api::{TranslationError, TranslationResult, normalize_lang};
use crate::services::TranslationProvider;

const DEEPLX_FREE_ENDPOINT: &str = "https://deeplx.owo.network/translate";

fn get_deepl_key(ai_config: &AiConfig) -> Result<String, TranslationError> {
    let key = ai_config.translation_api.deepl_key.trim();
    if key.is_empty() {
        Err(TranslationError::MissingApiKey("DeepL".to_string()))
    } else {
        Ok(key.to_string())
    }
}

pub struct DeepLService {
    _marker: std::marker::PhantomData<AiConfig>,
}

impl DeepLService {
    pub fn new(_ai_config: &AiConfig) -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }
}

impl TranslationProvider for DeepLService {
    fn name(&self) -> &'static str {
        "deepl"
    }

    fn label(&self) -> &'static str {
        "DeepL"
    }

    fn translate(
        &self,
        text: &str,
        source_lang: &str,
        target_lang: &str,
    ) -> impl Future<Output = Result<TranslationResult, TranslationError>> + Send {
        async move {
            let api_key = get_deepl_key(&skillstar_ai::ai_provider::load_config())?;
            let target = normalize_lang("deepl", target_lang, true);
            let source = if source_lang == "auto" {
                None
            } else {
                Some(normalize_lang("deepl", source_lang, false))
            };

            let body = serde_json::json!({
                "text": [text],
                "target_lang": target,
                "source_lang": source,
                "tag_handling": "html",
            });

            let client = skillstar_ai::ai_provider::http_client::get_http_client()?;
            let resp = client
                .post("https://api.deepl.com/v2/translate")
                .header("Content-Type", "application/json")
                .header("Authorization", format!("DeepL-Auth-Key {api_key}"))
                .json(&body)
                .send()
                .await?;

            let status = resp.status();
            let body_text = resp.text().await?;

            if !status.is_success() {
                let msg = serde_json::from_str::<serde_json::Value>(&body_text)
                    .ok()
                    .and_then(|v| {
                        v.get("message")
                            .and_then(|m| m.as_str())
                            .map(std::string::ToString::to_string)
                    })
                    .unwrap_or_else(|| body_text.clone());
                return Err(TranslationError::ApiError("DeepL".to_string(), msg));
            }

            let parsed: serde_json::Value = serde_json::from_str(&body_text)
                .map_err(|e| TranslationError::ParseError(e.to_string()))?;

            let translated = parsed["translations"][0]["text"]
                .as_str()
                .ok_or_else(|| TranslationError::ParseError("Missing translations[0].text".into()))?
                .to_string();

            Ok(TranslationResult::new(translated, "deepl", text.len()))
        }
    }
}

pub struct DeepLXService {
    url: String,
    api_key: Option<String>,
}

impl DeepLXService {
    pub fn new(ai_config: &AiConfig) -> Self {
        let url = {
            let configured = ai_config.translation_api.deeplx_url.trim();
            if configured.is_empty() {
                DEEPLX_FREE_ENDPOINT.to_string()
            } else {
                configured.to_string()
            }
        };
        let api_key = {
            let configured = ai_config.translation_api.deeplx_key.trim();
            if configured.is_empty() {
                None
            } else {
                Some(configured.to_string())
            }
        };
        Self { url, api_key }
    }
}

impl TranslationProvider for DeepLXService {
    fn name(&self) -> &'static str {
        "deeplx"
    }

    fn label(&self) -> &'static str {
        "DeepLX (Free)"
    }

    fn translate(
        &self,
        text: &str,
        source_lang: &str,
        target_lang: &str,
    ) -> impl Future<Output = Result<TranslationResult, TranslationError>> + Send {
        async move {
            let target = normalize_lang("deepl", target_lang, true);
            let source = if source_lang == "auto" {
                None
            } else {
                Some(normalize_lang("deepl", source_lang, false))
            };

            let mut body = serde_json::json!({
                "text": text,
                "target_lang": target,
            });
            if let Some(s) = source {
                body["source_lang"] = serde_json::json!(s);
            }

            let client = skillstar_ai::ai_provider::http_client::get_http_client()?;
            let mut request = client
                .post(&self.url)
                .header("Content-Type", "application/json");

            if let Some(api_key) = self.api_key.as_deref() {
                request = request.header("Authorization", format!("Bearer {api_key}"));
                request = request.header("X-API-Key", api_key);
                request = request.header("api-key", api_key);
            }

            let resp = request.json(&body).send().await?;

            let status = resp.status();
            let body_text = resp.text().await?;

            if !status.is_success() {
                return Err(TranslationError::ApiError(
                    "DeepLX".to_string(),
                    body_text.clone(),
                ));
            }

            let parsed: serde_json::Value = serde_json::from_str(&body_text)
                .map_err(|e| TranslationError::ParseError(e.to_string()))?;

            let translated = parsed["data"]
                .as_str()
                .ok_or_else(|| TranslationError::ParseError("Missing data field".into()))?
                .to_string();

            Ok(TranslationResult::new(translated, "deeplx", text.len()))
        }
    }
}

#[cfg(test)]
mod tests {
    use skillstar_ai::ai_provider::config::AiConfig;

    use super::DeepLXService;

    fn empty_config() -> AiConfig {
        AiConfig::default()
    }

    fn config_with_deeplx(url: &str, key: &str) -> AiConfig {
        let mut config = AiConfig::default();
        config.translation_api.deeplx_url = url.to_string();
        config.translation_api.deeplx_key = key.to_string();
        config
    }

    #[test]
    fn deeplx_service_uses_default_endpoint_when_empty() {
        let config = empty_config();
        let service = DeepLXService::new(&config);
        assert_eq!(service.url, "https://deeplx.owo.network/translate");
        assert!(service.api_key.is_none());
    }

    #[test]
    fn deeplx_service_uses_configured_url() {
        let config = config_with_deeplx("https://custom.example.com/translate", "");
        let service = DeepLXService::new(&config);
        assert_eq!(service.url, "https://custom.example.com/translate");
    }

    #[test]
    fn deeplx_service_uses_configured_key() {
        let config = config_with_deeplx("", "my-secret-key");
        let service = DeepLXService::new(&config);
        assert_eq!(service.api_key, Some("my-secret-key".to_string()));
    }

    #[test]
    fn deeplx_service_trims_whitespace() {
        let mut config = AiConfig::default();
        config.translation_api.deeplx_url = "  https://example.com  ".to_string();
        config.translation_api.deeplx_key = "  key  ".to_string();
        let service = DeepLXService::new(&config);
        assert_eq!(service.url, "https://example.com");
        assert_eq!(service.api_key, Some("key".to_string()));
    }

    #[test]
    fn deeplx_service_preserves_free_endpoint_when_url_is_whitespace() {
        let mut config = AiConfig::default();
        config.translation_api.deeplx_url = "   ".to_string();
        let service = DeepLXService::new(&config);
        assert_eq!(service.url, "https://deeplx.owo.network/translate");
    }
}
