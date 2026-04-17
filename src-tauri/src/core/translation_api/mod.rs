#![allow(dead_code)]

//! # Translation API Integration
//!
//! Unified translation service supporting:
//! - Traditional APIs: DeepL, DeepLX, Google Translate, Azure Translate, GTX
//! - AI LLM Providers: DeepSeek, Claude, OpenAI, Gemini, Perplexity,
//!   Azure OpenAI, SiliconFlow, Groq, OpenRouter, Nvidia NIM, Custom LLM
//!
//! ## Architecture
//!
//! ```
//! TranslationServiceFactory ──creates──> dyn TranslationProvider
//!                                              │
//!                           ┌─────────────────┼─────────────────┐
//!                      Traditional          LLM (OpenAI compat)    LLM (Anthropic)
//!                           │                      │                    │
//!                        DeepL               DeepSeek               Claude
//!                     Google                  Perplexity            Gemini
//!                      Azure                  Groq                  (native)
//!                       GTX                    OpenRouter
//!                                            Siliconflow
//!                                             Nvidia
//!                                          AzureOpenAI
//! ```

pub mod config;
pub mod markdown;
pub mod router;
pub mod services;

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ── Error Types ────────────────────────────────────────────────────

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

// ── Provider Names ─────────────────────────────────────────────────

/// All supported translation provider identifiers.
/// Used for UI selection and factory lookup.
pub const TRADITIONAL_PROVIDERS: &[&str] = &[
    "deepl",  // DeepL Pro/Free
    "deeplx", // DeepLX free endpoint
    "google", // Google Translate v2
    "azure",  // Azure Translator v3
    "gtx",    // Google Translate AJAX (free, rate-limited)
];

pub const LLM_PROVIDERS: &[&str] = &[
    "deepseek",
    "claude",
    "openai",
    "gemini",
    "perplexity",
    "azureopenai",
    "siliconflow",
    "groq",
    "openrouter",
    "nvidia",
    "customllm",
];

pub const ALL_PROVIDERS: &[&str] = &[
    // Traditional first (free options at top)
    "gtx",
    "deepl",
    "deeplx",
    "google",
    "azure",
    // LLM providers
    "deepseek",
    "claude",
    "openai",
    "gemini",
    "perplexity",
    "azureopenai",
    "siliconflow",
    "groq",
    "openrouter",
    "nvidia",
    "customllm",
];

// ── Language Codes ─────────────────────────────────────────────────

/// Normalize a SkillStar language code to a provider-specific code.
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
        "google" | "gtx" => code.to_string(),
        "azure" => code.to_string(),
        // LLM providers use standard codes
        _ => code.to_string(),
    }
}

/// Returns true if the provider is a traditional (non-LLM) API.
pub fn is_traditional_provider(provider: &str) -> bool {
    TRADITIONAL_PROVIDERS.contains(&provider)
}

/// Returns true if the provider is an LLM-based translation service.
pub fn is_llm_provider(provider: &str) -> bool {
    LLM_PROVIDERS.contains(&provider)
}

// ── Translation Result ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationResult {
    pub translated_text: String,
    /// Detected source language (for "auto" source)
    pub detected_lang: Option<String>,
    /// Provider that served this translation
    pub provider: String,
    /// Characters consumed (for quota tracking)
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

// ── Service Factory ─────────────────────────────────────────────────
