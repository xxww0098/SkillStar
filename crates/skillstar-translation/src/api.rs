use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TranslationError {
    #[error("provider `{0}` not found in registry")]
    ProviderNotFound(String),

    #[error("API key required for `{0}` but none configured")]
    MissingApiKey(String),

    #[error("HTTP error from `{0}`: {1}")]
    HttpError(String, String),

    #[error("API error from `{0}`: {1}")]
    ApiError(String, String),

    #[error("timeout after {0}s")]
    Timeout(u64),

    #[error("rate limited by `{0}`, retry after {1}s")]
    RateLimited(String, u64),

    #[error("parsing error: {0}")]
    ParseError(String),

    #[error("unknown: {0}")]
    Unknown(String),
}

impl From<reqwest::Error> for TranslationError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            TranslationError::Timeout(45)
        } else {
            TranslationError::HttpError("unknown".into(), e.to_string())
        }
    }
}

impl From<anyhow::Error> for TranslationError {
    fn from(e: anyhow::Error) -> Self {
        TranslationError::Unknown(e.to_string())
    }
}

pub const TRADITIONAL_PROVIDERS: &[&str] = &["deepl", "deeplx", "mymemory"];

pub const ALL_PROVIDERS: &[&str] = &["deepl", "deeplx", "mymemory"];

pub fn normalize_lang(provider: &str, code: &str, target: bool) -> String {
    match provider {
        "deepl" => match code {
            "zh" if target => "ZH-HANS".to_string(),
            "zh-hant" => "ZH-HANT".to_string(),
            "en" => "EN-US".to_string(),
            "pt-br" | "pt-pt" => "PT".to_string(),
            "fil" => "TL".to_string(),
            _ => code.to_uppercase(),
        },
        _ => code.to_string(),
    }
}

pub fn is_traditional_provider(provider: &str) -> bool {
    TRADITIONAL_PROVIDERS.contains(&provider)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationResult {
    pub translated_text: String,
    pub detected_lang: Option<String>,
    pub provider: String,
    pub chars_consumed: usize,
}

impl TranslationResult {
    pub fn new(translated_text: String, provider: &str, chars_consumed: usize) -> Self {
        Self {
            translated_text,
            detected_lang: None,
            provider: provider.to_string(),
            chars_consumed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_lang_deepl_target_zh() {
        assert_eq!(normalize_lang("deepl", "zh", true), "ZH-HANS");
    }

    #[test]
    fn normalize_lang_deepl_source_zh() {
        assert_eq!(normalize_lang("deepl", "zh", false), "ZH");
    }

    #[test]
    fn normalize_lang_deepl_en() {
        assert_eq!(normalize_lang("deepl", "en", true), "EN-US");
        assert_eq!(normalize_lang("deepl", "en", false), "EN-US");
    }

    #[test]
    fn normalize_lang_deepl_zh_hant() {
        assert_eq!(normalize_lang("deepl", "zh-hant", true), "ZH-HANT");
        assert_eq!(normalize_lang("deepl", "zh-hant", false), "ZH-HANT");
    }

    #[test]
    fn normalize_lang_deepl_pt_variants() {
        assert_eq!(normalize_lang("deepl", "pt-br", true), "PT");
        assert_eq!(normalize_lang("deepl", "pt-pt", true), "PT");
    }

    #[test]
    fn normalize_lang_deepl_fil() {
        assert_eq!(normalize_lang("deepl", "fil", true), "TL");
    }

    #[test]
    fn normalize_lang_deepl_unknown_uppercases() {
        assert_eq!(normalize_lang("deepl", "ko", true), "KO");
        assert_eq!(normalize_lang("deepl", "ja", false), "JA");
    }

    #[test]
    fn normalize_lang_non_deepl_passes_through() {
        assert_eq!(normalize_lang("deeplx", "zh", true), "zh");
        assert_eq!(normalize_lang("mymemory", "EN-US", false), "EN-US");
    }

    #[test]
    fn is_traditional_provider_recognizes_core_providers() {
        assert!(is_traditional_provider("deepl"));
        assert!(is_traditional_provider("deeplx"));
        assert!(is_traditional_provider("mymemory"));
    }

    #[test]
    fn is_traditional_provider_rejects_unknown() {
        assert!(!is_traditional_provider("openai"));
        assert!(!is_traditional_provider("claude"));
        assert!(!is_traditional_provider(""));
    }

    #[test]
    fn translation_error_display_messages() {
        assert_eq!(
            TranslationError::ProviderNotFound("foo".into()).to_string(),
            "provider `foo` not found in registry"
        );
        assert_eq!(
            TranslationError::MissingApiKey("DeepL".into()).to_string(),
            "API key required for `DeepL` but none configured"
        );
        assert_eq!(
            TranslationError::Timeout(30).to_string(),
            "timeout after 30s"
        );
    }
}
