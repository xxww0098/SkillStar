//! AI configuration types, serde helpers, and defaults.

use serde::{Deserialize, Deserializer, Serialize};

use crate::translation_config::{TranslationApiConfig, TranslationSettings};

// ── Configuration ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ApiFormat {
    Openai,
    Anthropic,
    Local,
}

impl ApiFormat {
    pub(crate) fn parse_loose(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "openai" => Self::Openai,
            "anthropic" => Self::Anthropic,
            "local" => Self::Local,
            _ => Self::Openai,
        }
    }
}

impl Default for ApiFormat {
    fn default() -> Self {
        Self::Openai
    }
}

fn deserialize_api_format<'de, D>(deserializer: D) -> Result<ApiFormat, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = String::deserialize(deserializer)?;
    Ok(ApiFormat::parse_loose(&raw))
}

/// Per-format saved preset (base_url, api_key, model).
/// When the user switches api_format, the active fields are swapped from/to
/// the corresponding preset so each format remembers its own values.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FormatPreset {
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AiProviderRef {
    #[serde(default)]
    pub app_id: String,
    #[serde(default)]
    pub provider_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    pub enabled: bool,
    #[serde(default, deserialize_with = "deserialize_api_format")]
    pub api_format: ApiFormat,
    #[serde(default)]
    pub provider_ref: Option<AiProviderRef>,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub target_language: String,
    /// Model context window in K tokens (e.g. 128 = 128K tokens).
    /// All scan parameters are auto-derived from this value.
    #[serde(default = "default_context_window_k")]
    pub context_window_k: u32,
    /// Override: 0 = auto-derive from context_window_k
    #[serde(default)]
    pub max_concurrent_requests: u32,
    /// Override: 0 = auto-derive from context_window_k
    #[serde(default)]
    pub chunk_char_limit: usize,
    /// Override: 0 = auto-derive from context_window_k
    #[serde(default)]
    pub scan_max_response_tokens: u32,
    /// Optional anonymous telemetry for security scan quality metrics.
    /// When enabled, SkillStar only records aggregate run stats (no skill names/content).
    #[serde(default = "default_security_scan_telemetry_enabled")]
    pub security_scan_telemetry_enabled: bool,
    /// Saved preset for OpenAI-compatible format.
    #[serde(default)]
    pub openai_preset: FormatPreset,
    /// Saved preset for Anthropic Messages format.
    #[serde(default)]
    pub anthropic_preset: FormatPreset,
    /// Saved preset for Local (Ollama) format.
    #[serde(default)]
    pub local_preset: FormatPreset,
    /// All traditional translation API keys and provider settings.
    #[serde(default)]
    pub translation_api: TranslationApiConfig,
    /// Unified routing + target language settings for translation features.
    #[serde(default)]
    pub translation_settings: TranslationSettings,
    // Legacy fields — absorbed on load, not written back.
    #[serde(default, skip_serializing)]
    pub short_text_priority: Option<serde_json::Value>,
}

fn default_context_window_k() -> u32 {
    128
}

fn default_security_scan_telemetry_enabled() -> bool {
    false
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            api_format: ApiFormat::default(),
            provider_ref: None,
            base_url: String::new(),
            api_key: String::new(),
            model: "gpt-5.4".to_string(),
            target_language: "zh-CN".to_string(),
            context_window_k: default_context_window_k(),
            max_concurrent_requests: 4,
            chunk_char_limit: 0,
            scan_max_response_tokens: 0,
            security_scan_telemetry_enabled: default_security_scan_telemetry_enabled(),
            openai_preset: FormatPreset::default(),
            anthropic_preset: FormatPreset::default(),
            local_preset: FormatPreset {
                base_url: "http://127.0.0.1:11434/v1".to_string(),
                model: "llama3.1:8b".to_string(),
                ..Default::default()
            },
            translation_api: TranslationApiConfig::default(),
            translation_settings: TranslationSettings::default(),
            short_text_priority: None,
        }
    }
}
