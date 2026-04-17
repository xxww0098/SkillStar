#![allow(dead_code)]

//! Translation service implementations and factory.
//!
//! ```not_rust
//! create_provider(name, &config) -> TranslationService
//! ```

pub mod azure;
pub mod azure_openai;
pub mod claude;
pub mod custom_llm;
pub mod deepl;
pub mod deepseek;
pub mod gemini;
pub mod google;
pub mod gtx;
pub mod nvidia;
pub mod openai_compat;

use std::future::Future;

use crate::core::ai_provider::config::AiConfig;
use crate::core::model_config::providers::{self, AppProviders, ProviderEntry};
use crate::core::translation_api::{TranslationError, TranslationResult};

#[derive(Debug, Clone, Default)]
struct RuntimeProviderFields {
    api_key: Option<String>,
    base_url: Option<String>,
    model: Option<String>,
}

fn non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn parse_toml_string_field(config_text: &str, field: &str) -> Option<String> {
    for line in config_text.lines() {
        let trimmed = line.trim();
        let prefix = format!("{field} =");
        if !trimmed.starts_with(&prefix) {
            continue;
        }
        let rhs = trimmed.split_once('=')?.1.trim();
        if rhs.starts_with('"') {
            let mut chars = rhs.chars();
            let _ = chars.next();
            let mut collected = String::new();
            for ch in chars {
                if ch == '"' {
                    break;
                }
                collected.push(ch);
            }
            return non_empty(Some(&collected));
        }
    }
    None
}

fn extract_codex_runtime_fields(provider: &ProviderEntry) -> RuntimeProviderFields {
    let auth = provider
        .settings_config
        .get("auth")
        .and_then(|value| value.as_object());
    let config_text = provider
        .settings_config
        .get("config")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    let meta_base_url = provider
        .meta
        .as_ref()
        .and_then(|meta| meta.get("baseURL"))
        .and_then(|value| value.as_str());

    RuntimeProviderFields {
        api_key: non_empty(
            auth.and_then(|auth| auth.get("OPENAI_API_KEY"))
                .and_then(|value| value.as_str()),
        ),
        base_url: parse_toml_string_field(config_text, "openai_base_url")
            .or_else(|| parse_toml_string_field(config_text, "base_url"))
            .or_else(|| non_empty(meta_base_url)),
        model: parse_toml_string_field(config_text, "model"),
    }
}

fn extract_claude_runtime_fields(provider: &ProviderEntry) -> RuntimeProviderFields {
    let env = provider
        .settings_config
        .get("env")
        .and_then(|value| value.as_object());

    RuntimeProviderFields {
        api_key: non_empty(
            env.and_then(|env| env.get("ANTHROPIC_AUTH_TOKEN"))
                .and_then(|value| value.as_str()),
        )
        .or_else(|| {
            non_empty(
                env.and_then(|env| env.get("ANTHROPIC_API_KEY"))
                    .and_then(|value| value.as_str()),
            )
        }),
        base_url: non_empty(
            env.and_then(|env| env.get("ANTHROPIC_BASE_URL"))
                .and_then(|value| value.as_str()),
        ),
        model: non_empty(
            env.and_then(|env| env.get("ANTHROPIC_MODEL"))
                .and_then(|value| value.as_str()),
        )
        .or_else(|| {
            non_empty(
                env.and_then(|env| env.get("CLAUDE_CODE_MODEL"))
                    .and_then(|value| value.as_str()),
            )
        }),
    }
}

fn extract_gemini_runtime_fields(provider: &ProviderEntry) -> RuntimeProviderFields {
    let env = provider
        .settings_config
        .get("env")
        .and_then(|value| value.as_object());

    RuntimeProviderFields {
        api_key: non_empty(
            env.and_then(|env| env.get("GEMINI_API_KEY"))
                .and_then(|value| value.as_str()),
        ),
        base_url: non_empty(
            env.and_then(|env| env.get("GOOGLE_GEMINI_BASE_URL"))
                .and_then(|value| value.as_str()),
        ),
        model: non_empty(
            env.and_then(|env| env.get("GEMINI_MODEL"))
                .and_then(|value| value.as_str()),
        ),
    }
}

fn codex_provider_matches(entry: &ProviderEntry, provider_name: &str) -> bool {
    let fields = extract_codex_runtime_fields(entry);
    let id = entry.id.to_ascii_lowercase();
    let name = entry.name.to_ascii_lowercase();
    let category = entry.category.to_ascii_lowercase();
    let base_url = fields.base_url.unwrap_or_default().to_ascii_lowercase();
    let haystack = format!("{id} {name} {category} {base_url}");

    match provider_name {
        "openai" => {
            haystack.contains("openai")
                || haystack.contains("api.openai.com")
                || (base_url.is_empty() && category == "official")
        }
        "deepseek" => haystack.contains("deepseek"),
        "perplexity" => haystack.contains("perplexity"),
        "azureopenai" => {
            haystack.contains("azureopenai")
                || haystack.contains("azure_openai")
                || (haystack.contains("azure") && haystack.contains("openai"))
                || haystack.contains("openai.azure.com")
        }
        "siliconflow" => haystack.contains("siliconflow"),
        "groq" => haystack.contains("groq"),
        "openrouter" => haystack.contains("openrouter"),
        "nvidia" => haystack.contains("nvidia"),
        "customllm" => category == "custom" || id.starts_with("custom_"),
        _ => false,
    }
}

fn select_codex_provider<'a>(
    app: &'a AppProviders,
    provider_name: &str,
) -> Option<&'a ProviderEntry> {
    let current = app
        .current
        .as_deref()
        .and_then(|provider_id| app.providers.get(provider_id));

    if let Some(entry) = current.filter(|entry| codex_provider_matches(entry, provider_name)) {
        return Some(entry);
    }

    if let Some(entry) = app
        .providers
        .values()
        .find(|entry| codex_provider_matches(entry, provider_name))
    {
        return Some(entry);
    }

    match provider_name {
        "openai" | "customllm" => current,
        _ => None,
    }
}

fn resolve_runtime_provider_fields(provider_name: &str) -> Option<RuntimeProviderFields> {
    let store = providers::read_store().ok()?;

    match provider_name {
        "claude" => store
            .claude
            .current
            .as_deref()
            .and_then(|provider_id| store.claude.providers.get(provider_id))
            .map(extract_claude_runtime_fields),
        "gemini" => store
            .gemini
            .current
            .as_deref()
            .and_then(|provider_id| store.gemini.providers.get(provider_id))
            .map(extract_gemini_runtime_fields),
        "deepseek" | "openai" | "perplexity" | "azureopenai" | "siliconflow" | "groq"
        | "openrouter" | "nvidia" | "customllm" => {
            select_codex_provider(&store.codex, provider_name).map(extract_codex_runtime_fields)
        }
        _ => None,
    }
}

fn supports_runtime_provider_override(provider_name: &str) -> bool {
    matches!(
        provider_name,
        "deepseek"
            | "claude"
            | "openai"
            | "gemini"
            | "perplexity"
            | "azureopenai"
            | "siliconflow"
            | "groq"
            | "openrouter"
            | "nvidia"
            | "customllm"
    )
}

fn apply_runtime_llm_provider_config(ai_config: &AiConfig, provider_name: &str) -> AiConfig {
    if !supports_runtime_provider_override(provider_name) {
        return ai_config.clone();
    }

    let Some(fields) = resolve_runtime_provider_fields(provider_name) else {
        return ai_config.clone();
    };

    let mut config = ai_config.clone();
    let runtime_key = fields.api_key;
    let runtime_base_url = fields.base_url;
    let runtime_model = fields.model;

    let apply =
        |settings: &mut crate::core::translation_api::config::TranslationProviderSettings| {
            if let Some(base_url) = runtime_base_url.as_deref() {
                settings.base_url = base_url.to_string();
            }
            if let Some(model) = runtime_model.as_deref() {
                settings.model = model.to_string();
            }
        };

    match provider_name {
        "deepseek" => {
            if let Some(api_key) = runtime_key.as_deref() {
                config.translation_api.deepseek_key = api_key.to_string();
            }
            apply(&mut config.translation_api.deepseek_settings);
        }
        "claude" => {
            if let Some(api_key) = runtime_key.as_deref() {
                config.translation_api.claude_key = api_key.to_string();
            }
            apply(&mut config.translation_api.claude_settings);
        }
        "openai" => {
            if let Some(api_key) = runtime_key.as_deref() {
                config.translation_api.openai_key = api_key.to_string();
            }
            apply(&mut config.translation_api.openai_settings);
        }
        "gemini" => {
            if let Some(api_key) = runtime_key.as_deref() {
                config.translation_api.gemini_key = api_key.to_string();
            }
            apply(&mut config.translation_api.gemini_settings);
        }
        "perplexity" => {
            if let Some(api_key) = runtime_key.as_deref() {
                config.translation_api.perplexity_key = api_key.to_string();
            }
            apply(&mut config.translation_api.perplexity_settings);
        }
        "azureopenai" => {
            if let Some(api_key) = runtime_key.as_deref() {
                config.translation_api.azure_openai_key = api_key.to_string();
            }
            apply(&mut config.translation_api.azure_openai_settings);
        }
        "siliconflow" => {
            if let Some(api_key) = runtime_key.as_deref() {
                config.translation_api.siliconflow_key = api_key.to_string();
            }
            apply(&mut config.translation_api.siliconflow_settings);
        }
        "groq" => {
            if let Some(api_key) = runtime_key.as_deref() {
                config.translation_api.groq_key = api_key.to_string();
            }
            apply(&mut config.translation_api.groq_settings);
        }
        "openrouter" => {
            if let Some(api_key) = runtime_key.as_deref() {
                config.translation_api.openrouter_key = api_key.to_string();
            }
            apply(&mut config.translation_api.openrouter_settings);
        }
        "nvidia" => {
            if let Some(api_key) = runtime_key.as_deref() {
                config.translation_api.nvidia_key = api_key.to_string();
            }
            apply(&mut config.translation_api.nvidia_settings);
        }
        "customllm" => {
            if let Some(api_key) = runtime_key.as_deref() {
                config.translation_api.custom_llm_key = api_key.to_string();
            }
            apply(&mut config.translation_api.custom_llm_settings);
        }
        _ => {}
    }

    config
}

pub fn provider_has_runtime_configuration(provider_name: &str, ai_config: &AiConfig) -> bool {
    let effective = apply_runtime_llm_provider_config(ai_config, provider_name);
    match provider_name {
        "deeplx" => !effective.translation_api.deeplx_url.trim().is_empty(),
        "gtx" => true,
        "deepl" => !effective.translation_api.deepl_key.trim().is_empty(),
        "google" => !effective.translation_api.google_key.trim().is_empty(),
        "azure" => {
            !effective.translation_api.azure_key.trim().is_empty()
                && !effective.translation_api.azure_region.trim().is_empty()
        }
        "deepseek" => !effective.translation_api.deepseek_key.trim().is_empty(),
        "claude" => !effective.translation_api.claude_key.trim().is_empty(),
        "openai" => !effective.translation_api.openai_key.trim().is_empty(),
        "gemini" => !effective.translation_api.gemini_key.trim().is_empty(),
        "perplexity" => !effective.translation_api.perplexity_key.trim().is_empty(),
        "azureopenai" => !effective.translation_api.azure_openai_key.trim().is_empty(),
        "siliconflow" => !effective.translation_api.siliconflow_key.trim().is_empty(),
        "groq" => !effective.translation_api.groq_key.trim().is_empty(),
        "openrouter" => !effective.translation_api.openrouter_key.trim().is_empty(),
        "nvidia" => !effective.translation_api.nvidia_key.trim().is_empty(),
        "customllm" => {
            !effective
                .translation_api
                .custom_llm_settings
                .base_url
                .trim()
                .is_empty()
                && !effective.translation_api.custom_llm_key.trim().is_empty()
        }
        _ => false,
    }
}

/// Core trait implemented by all translation providers.
pub trait TranslationProvider: Send + Sync {
    fn name(&self) -> &'static str;
    fn label(&self) -> &'static str;

    fn translate(
        &self,
        text: &str,
        source_lang: &str,
        target_lang: &str,
    ) -> impl Future<Output = Result<TranslationResult, TranslationError>> + Send;

    fn translate_stream(
        &self,
        _text: &str,
        _source_lang: &str,
        _target_lang: &str,
        _on_delta: impl Fn(String) + Send + Sync + 'static,
    ) -> impl Future<Output = Result<(), TranslationError>> + Send {
        async move { Ok(()) }
    }

    fn supports_streaming(&self) -> bool {
        false
    }
}

pub enum TranslationService {
    DeepL(deepl::DeepLService),
    DeepLX(deepl::DeepLXService),
    Google(google::GoogleTranslateService),
    Azure(azure::AzureTranslateService),
    Gtx(gtx::GtxFreeService),
    DeepSeek(deepseek::DeepSeekService),
    Claude(claude::ClaudeService),
    OpenAICompat(openai_compat::OpenAICompatService),
    Gemini(gemini::GeminiService),
    AzureOpenAI(azure_openai::AzureOpenAIService),
    Nvidia(nvidia::NvidiaNIMService),
    CustomLLM(custom_llm::CustomLLMService),
}

impl TranslationService {
    pub async fn translate(
        &self,
        text: &str,
        source_lang: &str,
        target_lang: &str,
    ) -> Result<TranslationResult, TranslationError> {
        match self {
            Self::DeepL(service) => service.translate(text, source_lang, target_lang).await,
            Self::DeepLX(service) => service.translate(text, source_lang, target_lang).await,
            Self::Google(service) => service.translate(text, source_lang, target_lang).await,
            Self::Azure(service) => service.translate(text, source_lang, target_lang).await,
            Self::Gtx(service) => service.translate(text, source_lang, target_lang).await,
            Self::DeepSeek(service) => service.translate(text, source_lang, target_lang).await,
            Self::Claude(service) => service.translate(text, source_lang, target_lang).await,
            Self::OpenAICompat(service) => service.translate(text, source_lang, target_lang).await,
            Self::Gemini(service) => service.translate(text, source_lang, target_lang).await,
            Self::AzureOpenAI(service) => service.translate(text, source_lang, target_lang).await,
            Self::Nvidia(service) => service.translate(text, source_lang, target_lang).await,
            Self::CustomLLM(service) => service.translate(text, source_lang, target_lang).await,
        }
    }

    pub async fn translate_stream<F>(
        &self,
        text: &str,
        source_lang: &str,
        target_lang: &str,
        on_delta: F,
    ) -> Result<(), TranslationError>
    where
        F: Fn(String) + Send + Sync + 'static,
    {
        match self {
            Self::DeepL(service) => {
                service
                    .translate_stream(text, source_lang, target_lang, on_delta)
                    .await
            }
            Self::DeepLX(service) => {
                service
                    .translate_stream(text, source_lang, target_lang, on_delta)
                    .await
            }
            Self::Google(service) => {
                service
                    .translate_stream(text, source_lang, target_lang, on_delta)
                    .await
            }
            Self::Azure(service) => {
                service
                    .translate_stream(text, source_lang, target_lang, on_delta)
                    .await
            }
            Self::Gtx(service) => {
                service
                    .translate_stream(text, source_lang, target_lang, on_delta)
                    .await
            }
            Self::DeepSeek(service) => {
                service
                    .translate_stream(text, source_lang, target_lang, on_delta)
                    .await
            }
            Self::Claude(service) => {
                service
                    .translate_stream(text, source_lang, target_lang, on_delta)
                    .await
            }
            Self::OpenAICompat(service) => {
                service
                    .translate_stream(text, source_lang, target_lang, on_delta)
                    .await
            }
            Self::Gemini(service) => {
                service
                    .translate_stream(text, source_lang, target_lang, on_delta)
                    .await
            }
            Self::AzureOpenAI(service) => {
                service
                    .translate_stream(text, source_lang, target_lang, on_delta)
                    .await
            }
            Self::Nvidia(service) => {
                service
                    .translate_stream(text, source_lang, target_lang, on_delta)
                    .await
            }
            Self::CustomLLM(service) => {
                service
                    .translate_stream(text, source_lang, target_lang, on_delta)
                    .await
            }
        }
    }
}

/// Create a translation provider by name using config credentials.
pub fn create_provider(
    name: &str,
    ai_config: &AiConfig,
) -> Result<TranslationService, TranslationError> {
    let effective_config = apply_runtime_llm_provider_config(ai_config, name);

    match name {
        "deepl" => Ok(TranslationService::DeepL(deepl::DeepLService::new(
            &effective_config,
        ))),
        "deeplx" => Ok(TranslationService::DeepLX(deepl::DeepLXService::new(
            &effective_config,
        ))),
        "google" => Ok(TranslationService::Google(
            google::GoogleTranslateService::new(&effective_config),
        )),
        "azure" => Ok(TranslationService::Azure(
            azure::AzureTranslateService::new(&effective_config),
        )),
        "gtx" => Ok(TranslationService::Gtx(gtx::GtxFreeService::new())),
        "deepseek" => Ok(TranslationService::DeepSeek(
            deepseek::DeepSeekService::new(&effective_config),
        )),
        "claude" => Ok(TranslationService::Claude(claude::ClaudeService::new(
            &effective_config,
        ))),
        "openai" => Ok(TranslationService::OpenAICompat(
            openai_compat::OpenAICompatService::new(
                &effective_config,
                "openai",
                "https://api.openai.com/v1/chat/completions",
            ),
        )),
        "gemini" => Ok(TranslationService::Gemini(gemini::GeminiService::new(
            &effective_config,
        ))),
        "perplexity" => Ok(TranslationService::OpenAICompat(
            openai_compat::OpenAICompatService::new(
                &effective_config,
                "perplexity",
                "https://api.perplexity.ai/chat/completions",
            ),
        )),
        "azureopenai" => Ok(TranslationService::AzureOpenAI(
            azure_openai::AzureOpenAIService::new(&effective_config),
        )),
        "siliconflow" => Ok(TranslationService::OpenAICompat(
            openai_compat::OpenAICompatService::new(
                &effective_config,
                "siliconflow",
                "https://api.siliconflow.cn/v1/chat/completions",
            ),
        )),
        "groq" => Ok(TranslationService::OpenAICompat(
            openai_compat::OpenAICompatService::new(
                &effective_config,
                "groq",
                "https://api.groq.com/openai/v1/chat/completions",
            ),
        )),
        "openrouter" => Ok(TranslationService::OpenAICompat(
            openai_compat::OpenAICompatService::new(
                &effective_config,
                "openrouter",
                "https://openrouter.ai/api/v1/chat/completions",
            ),
        )),
        "nvidia" => Ok(TranslationService::Nvidia(nvidia::NvidiaNIMService::new(
            &effective_config,
        ))),
        "customllm" => Ok(TranslationService::CustomLLM(
            custom_llm::CustomLLMService::new(&effective_config),
        )),
        _ => Err(TranslationError::ProviderNotFound(name.to_string())),
    }
}

/// Translate text using a named provider.
pub async fn translate_with_provider(
    provider_name: &str,
    ai_config: &AiConfig,
    text: &str,
    source_lang: &str,
    target_lang: &str,
) -> Result<TranslationResult, TranslationError> {
    let provider = create_provider(provider_name, ai_config)?;
    provider.translate(text, source_lang, target_lang).await
}

/// Streaming translation using a named provider.
pub async fn translate_stream_with_provider(
    provider_name: &str,
    ai_config: &AiConfig,
    text: &str,
    source_lang: &str,
    target_lang: &str,
    on_delta: impl Fn(String) + Send + Sync + 'static,
) -> Result<(), TranslationError> {
    let provider = create_provider(provider_name, ai_config)?;
    provider
        .translate_stream(text, source_lang, target_lang, on_delta)
        .await
}

/// Get the appropriate base URL for an OpenAI-compatible LLM.
pub fn get_llm_base_url(provider: &str) -> &'static str {
    match provider {
        "deepseek" => "https://api.deepseek.com",
        "perplexity" => "https://api.perplexity.ai",
        "siliconflow" => "https://api.siliconflow.cn",
        "groq" => "https://api.groq.com",
        "openrouter" => "https://openrouter.ai/api/v1",
        "nvidia" => "https://integrate.api.nvidia.com",
        _ => "https://api.openai.com/v1",
    }
}
