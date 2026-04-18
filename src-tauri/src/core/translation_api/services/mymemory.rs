use std::future::Future;

use crate::core::ai_provider::config::AiConfig;
use crate::core::translation_api::services::TranslationProvider;
use crate::core::translation_api::{TranslationError, TranslationResult, normalize_lang};

pub struct MyMemoryService {
    _marker: std::marker::PhantomData<AiConfig>,
}

impl MyMemoryService {
    pub fn new(_ai_config: &AiConfig) -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }

    /// Return a persistent `de` email for MyMemory API usage.
    ///
    /// MyMemory gives anonymous users 5000 words/day. By sending a stable `de`
    /// parameter the quota is tracked per-email instead of per-IP, which is more
    /// reliable for desktop apps behind NATs. The email is generated once and
    /// stored at `~/.skillstar/.mymemory_de`.
    fn get_mymemory_de() -> String {
        use std::fs;
        let path = crate::core::infra::paths::mymemory_disabled_path();
        if let Ok(email) = fs::read_to_string(&path) {
            let trimmed = email.trim().to_string();
            if !trimmed.is_empty() {
                return trimmed;
            }
        }
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let id = uuid::Uuid::new_v4();
        let email = format!("{}@skillstar.local", id);
        let _ = fs::write(&path, &email);
        email
    }

    async fn get_mymemory_de_async() -> String {
        tokio::task::spawn_blocking(Self::get_mymemory_de)
            .await
            .unwrap_or_default()
    }
}

impl TranslationProvider for MyMemoryService {
    fn name(&self) -> &'static str {
        "mymemory"
    }

    fn label(&self) -> &'static str {
        "MyMemory (Free Fallback)"
    }

    fn translate(
        &self,
        text: &str,
        source_lang: &str,
        target_lang: &str,
    ) -> impl Future<Output = Result<TranslationResult, TranslationError>> + Send {
        async move {
            let target = normalize_lang("mymemory", target_lang, true);
            let source = if source_lang == "auto" {
                "autodetect".to_string()
            } else {
                normalize_lang("mymemory", source_lang, false)
            };

            let langpair = format!("{}|{}", source, target);
            let de = Self::get_mymemory_de_async().await;

            let mut params: Vec<(&str, &str)> = vec![("q", text), ("langpair", &langpair)];
            if !de.trim().is_empty() {
                params.push(("de", &de));
            }

            let url = reqwest::Url::parse_with_params("https://api.mymemory.translated.net/get", &params)
                .map_err(|e| TranslationError::ParseError(e.to_string()))?;

            let client = crate::core::ai_provider::http_client::get_http_client()
                .map_err(|e| TranslationError::Unknown(e.to_string()))?;

            let resp = tokio::time::timeout(std::time::Duration::from_secs(15), client.get(url).send())
                .await
                .map_err(|_| TranslationError::Timeout(15))?
                .map_err(|e| TranslationError::HttpError("MyMemory".to_string(), e.to_string()))?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body_text = resp.text().await.unwrap_or_default();
                return Err(TranslationError::ApiError(
                    "MyMemory".to_string(),
                    format!("{} - {}", status, body_text),
                ));
            }

            let payload = resp
                .json::<serde_json::Value>()
                .await
                .map_err(|e| TranslationError::ParseError(e.to_string()))?;

            let api_status = payload
                .get("responseStatus")
                .and_then(|v| v.as_i64())
                .unwrap_or(200);
            if api_status != 200 {
                let details = payload
                    .get("responseDetails")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown error");
                return Err(TranslationError::ApiError(
                    "MyMemory".to_string(),
                    format!("Status {} - {}", api_status, details),
                ));
            }

            let translated = payload
                .pointer("/responseData/translatedText")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .unwrap_or("");

            if translated.is_empty() {
                return Err(TranslationError::Unknown("MyMemory returned empty translation".to_string()));
            }

            if translated.contains("PLEASE SELECT TWO DISTINCT LANGUAGES")
                || translated.contains("MYMEMORY WARNING:")
                || translated.contains("LIMIT EXCEEDED")
            {
                return Err(TranslationError::ApiError("MyMemory".to_string(), translated.to_string()));
            }

            Ok(TranslationResult::new(translated.to_string(), "mymemory", text.len()))
        }
    }
}
