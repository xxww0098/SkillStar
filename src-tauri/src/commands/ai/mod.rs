//! AI command module — split into domain-specific submodules.
//!
//! - `translate`: SKILL.md translation, short text, batch processing
//! - `summarize`: summarization, AI connection test, skill pick
//! - `scan`: security scan pipeline, estimates, reports, policy

pub mod scan;
pub mod summarize;
pub mod translate;

use crate::core::ai_provider;
use crate::core::translation_api::config::{
    TranslationDefaultProvider, TranslationEnabledProviders, TranslationFastProvider,
    TranslationProviderSettings, TranslationQualityProviderRef, TranslationRouteMode,
    TranslationSettings,
};
use crate::core::translation_api::router;
use serde::{Deserialize, Serialize};
use tauri::Emitter;

// ── Shared Types ────────────────────────────────────────────────────

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiStreamPayload {
    request_id: String,
    event: String,
    delta: Option<String>,
    message: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortTextTranslationPayload {
    text: String,
    source: String,
    provider: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    route_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fallback_hop: Option<u8>,
}

#[derive(Clone, Serialize)]
pub struct MymemoryUsagePayload {
    total_chars_sent: u64,
    daily_chars_sent: u64,
    daily_reset_date: String,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TranslationQualityProviderRefPayload {
    #[serde(default)]
    pub app_id: String,
    #[serde(default)]
    pub provider_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TranslationSettingsPayload {
    #[serde(default = "default_translation_target_language")]
    pub target_language: String,
    #[serde(default = "default_translation_mode_name")]
    pub mode: String,
    #[serde(default = "default_translation_fast_provider_name")]
    pub fast_provider: String,
    #[serde(default)]
    pub quality_provider_ref: Option<TranslationQualityProviderRefPayload>,
    #[serde(default = "default_translation_allow_emergency_fallback")]
    pub allow_emergency_fallback: bool,
    #[serde(default)]
    pub experimental_providers_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TranslationReadinessPayload {
    pub fast_ready: bool,
    pub quality_ready: bool,
    pub emergency_ready: bool,
    #[serde(default)]
    pub issues: Vec<String>,
    #[serde(default = "default_translation_mode_name")]
    pub recommended_mode: String,
}

// ── Shared Helpers ──────────────────────────────────────────────────

fn emit_ai_stream_event(
    window: &tauri::Window,
    channel: &str,
    request_id: &str,
    event: &str,
    delta: Option<String>,
    message: Option<String>,
) -> Result<(), String> {
    let payload = AiStreamPayload {
        request_id: request_id.to_string(),
        event: event.to_string(),
        delta,
        message,
    };

    window
        .emit(channel, payload)
        .map_err(|e| format!("Failed to emit {} event: {}", channel, e))
}

fn emit_translate_stream_event(
    window: &tauri::Window,
    request_id: &str,
    event: &str,
    delta: Option<String>,
    message: Option<String>,
    provider_id: Option<String>,
    provider_type: Option<String>,
    route_mode: Option<String>,
    fallback_hop: Option<u8>,
) -> Result<(), String> {
    let payload = serde_json::json!({
        "requestId": request_id,
        "event": event,
        "delta": delta,
        "message": message,
        "providerId": provider_id,
        "providerType": provider_type,
        "routeMode": route_mode,
        "fallbackHop": fallback_hop,
    });

    window
        .emit("ai://translate-stream", payload)
        .map_err(|e| format!("Failed to emit ai://translate-stream event: {}", e))
}

fn emit_summarize_stream_event(
    window: &tauri::Window,
    request_id: &str,
    event: &str,
    delta: Option<String>,
    message: Option<String>,
) -> Result<(), String> {
    emit_ai_stream_event(
        window,
        "ai://summarize-stream",
        request_id,
        event,
        delta,
        message,
    )
}

async fn ensure_ai_config() -> Result<ai_provider::AiConfig, String> {
    let config = ai_provider::load_config_async().await;
    if !config.enabled {
        return Err("AI provider is disabled. Please enable it in Settings.".to_string());
    }
    if config.api_key.trim().is_empty() && config.api_format != ai_provider::ApiFormat::Local {
        return Err(
            "AI provider is not configured. Please set up your API key in Settings.".to_string(),
        );
    }
    Ok(config)
}

/// Public wrapper for other command modules that need AI config validation.
pub async fn ensure_ai_config_pub() -> Result<ai_provider::AiConfig, String> {
    ensure_ai_config().await
}

// ── Config Commands (stay in mod.rs, too small to warrant a file) ───

#[tauri::command]
pub async fn get_ai_config() -> Result<ai_provider::AiConfig, String> {
    Ok(ai_provider::load_config_async().await)
}

#[tauri::command]
pub async fn save_ai_config(config: ai_provider::AiConfig) -> Result<(), String> {
    ai_provider::save_config(&config).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TranslationProviderSettingsPayload {
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub temperature: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TranslationApiConfigPayload {
    #[serde(default)]
    pub deepl_key: String,
    #[serde(default)]
    pub deeplx_url: String,
    #[serde(default)]
    pub google_key: String,
    #[serde(default)]
    pub azure_key: String,
    #[serde(default)]
    pub azure_region: String,
    #[serde(default)]
    pub gtx_api_key: String,

    #[serde(default)]
    pub deepseek_key: String,
    #[serde(default)]
    pub claude_key: String,
    #[serde(default)]
    pub openai_key: String,
    #[serde(default)]
    pub gemini_key: String,
    #[serde(default)]
    pub perplexity_key: String,
    #[serde(default)]
    pub azure_openai_key: String,
    #[serde(default)]
    pub siliconflow_key: String,
    #[serde(default)]
    pub groq_key: String,
    #[serde(default)]
    pub openrouter_key: String,
    #[serde(default)]
    pub nvidia_key: String,
    #[serde(default)]
    pub custom_llm_key: String,
    #[serde(default)]
    pub custom_llm_base_url: String,

    #[serde(default)]
    pub deepseek_settings: TranslationProviderSettingsPayload,
    #[serde(default)]
    pub claude_settings: TranslationProviderSettingsPayload,
    #[serde(default)]
    pub openai_settings: TranslationProviderSettingsPayload,
    #[serde(default)]
    pub gemini_settings: TranslationProviderSettingsPayload,
    #[serde(default)]
    pub perplexity_settings: TranslationProviderSettingsPayload,
    #[serde(default)]
    pub azure_openai_settings: TranslationProviderSettingsPayload,
    #[serde(default)]
    pub siliconflow_settings: TranslationProviderSettingsPayload,
    #[serde(default)]
    pub groq_settings: TranslationProviderSettingsPayload,
    #[serde(default)]
    pub openrouter_settings: TranslationProviderSettingsPayload,
    #[serde(default)]
    pub nvidia_settings: TranslationProviderSettingsPayload,
    #[serde(default)]
    pub custom_llm_settings: TranslationProviderSettingsPayload,

    #[serde(default)]
    pub enabled_providers: Vec<String>,
    #[serde(default = "default_provider_name")]
    pub default_provider: String,
    #[serde(default = "default_skill_provider_name")]
    pub default_skill_provider: String,
}

fn default_provider_name() -> String {
    "deepl".to_string()
}

fn default_skill_provider_name() -> String {
    "deepseek".to_string()
}

fn default_translation_target_language() -> String {
    "zh-CN".to_string()
}

fn default_translation_mode_name() -> String {
    "balanced".to_string()
}

fn default_translation_fast_provider_name() -> String {
    "deepl".to_string()
}

fn default_translation_allow_emergency_fallback() -> bool {
    true
}

fn translation_mode_from_str(raw: &str) -> TranslationRouteMode {
    match raw.trim().to_ascii_lowercase().as_str() {
        "fast" => TranslationRouteMode::Fast,
        "quality" => TranslationRouteMode::Quality,
        _ => TranslationRouteMode::Balanced,
    }
}

fn translation_mode_name(mode: TranslationRouteMode) -> &'static str {
    mode.as_str()
}

fn translation_fast_provider_from_str(raw: &str) -> TranslationFastProvider {
    match raw.trim().to_ascii_lowercase().as_str() {
        "google" => TranslationFastProvider::Google,
        "azure" => TranslationFastProvider::Azure,
        "experimental" | "deeplx" | "gtx" => TranslationFastProvider::Experimental,
        _ => TranslationFastProvider::DeepL,
    }
}

fn translation_fast_provider_name(provider: TranslationFastProvider) -> &'static str {
    provider.as_str()
}

fn to_ui_provider_name(provider: TranslationDefaultProvider) -> &'static str {
    match provider {
        TranslationDefaultProvider::DeepL => "deepl",
        TranslationDefaultProvider::DeepLX => "deeplx",
        TranslationDefaultProvider::Google => "google",
        TranslationDefaultProvider::Azure => "azure",
        TranslationDefaultProvider::Gtx => "gtx",
        TranslationDefaultProvider::DeepSeek => "deepseek",
        TranslationDefaultProvider::Claude => "claude",
        TranslationDefaultProvider::OpenAI => "openai",
        TranslationDefaultProvider::Gemini => "gemini",
        TranslationDefaultProvider::Perplexity => "perplexity",
        TranslationDefaultProvider::AzureOpenAI => "azure_openai",
        TranslationDefaultProvider::SiliconFlow => "siliconflow",
        TranslationDefaultProvider::Groq => "groq",
        TranslationDefaultProvider::OpenRouter => "openrouter",
        TranslationDefaultProvider::Nvidia => "nvidia",
        TranslationDefaultProvider::CustomLLM => "custom_llm",
    }
}

fn normalize_provider_token(raw: &str) -> Option<&'static str> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "deepl" | "deepl_key" => Some("deepl"),
        "deeplx" | "deeplx_url" => Some("deeplx"),
        "google" | "google_key" => Some("google"),
        "azure" | "azure_key" => Some("azure"),
        "gtx" | "gtx_api_key" => Some("gtx"),
        "deepseek" | "deepseek_key" => Some("deepseek"),
        "claude" | "claude_key" => Some("claude"),
        "openai" | "openai_key" => Some("openai"),
        "gemini" | "gemini_key" => Some("gemini"),
        "perplexity" | "perplexity_key" => Some("perplexity"),
        "azure_openai" | "azureopenai" | "azure_openai_key" => Some("azureopenai"),
        "siliconflow" | "siliconflow_key" => Some("siliconflow"),
        "groq" | "groq_key" => Some("groq"),
        "openrouter" | "openrouter_key" => Some("openrouter"),
        "nvidia" | "nvidia_key" => Some("nvidia"),
        "custom_llm" | "customllm" | "custom_llm_key" => Some("customllm"),
        _ => None,
    }
}

fn parse_default_provider(
    raw: &str,
    fallback: TranslationDefaultProvider,
) -> TranslationDefaultProvider {
    match normalize_provider_token(raw) {
        Some("deepl") => TranslationDefaultProvider::DeepL,
        Some("deeplx") => TranslationDefaultProvider::DeepLX,
        Some("google") => TranslationDefaultProvider::Google,
        Some("azure") => TranslationDefaultProvider::Azure,
        Some("gtx") => TranslationDefaultProvider::Gtx,
        Some("deepseek") => TranslationDefaultProvider::DeepSeek,
        Some("claude") => TranslationDefaultProvider::Claude,
        Some("openai") => TranslationDefaultProvider::OpenAI,
        Some("gemini") => TranslationDefaultProvider::Gemini,
        Some("perplexity") => TranslationDefaultProvider::Perplexity,
        Some("azureopenai") => TranslationDefaultProvider::AzureOpenAI,
        Some("siliconflow") => TranslationDefaultProvider::SiliconFlow,
        Some("groq") => TranslationDefaultProvider::Groq,
        Some("openrouter") => TranslationDefaultProvider::OpenRouter,
        Some("nvidia") => TranslationDefaultProvider::Nvidia,
        Some("customllm") => TranslationDefaultProvider::CustomLLM,
        _ => fallback,
    }
}

fn enabled_providers_to_vec(enabled: &TranslationEnabledProviders) -> Vec<String> {
    let mut out = Vec::new();
    if enabled.deepl {
        out.push("deepl".to_string());
    }
    if enabled.deeplx {
        out.push("deeplx".to_string());
    }
    if enabled.google {
        out.push("google".to_string());
    }
    if enabled.azure {
        out.push("azure".to_string());
    }
    if enabled.gtx {
        out.push("gtx".to_string());
    }
    if enabled.deepseek {
        out.push("deepseek".to_string());
    }
    if enabled.claude {
        out.push("claude".to_string());
    }
    if enabled.openai {
        out.push("openai".to_string());
    }
    if enabled.gemini {
        out.push("gemini".to_string());
    }
    if enabled.perplexity {
        out.push("perplexity".to_string());
    }
    if enabled.azure_openai {
        out.push("azure_openai".to_string());
    }
    if enabled.siliconflow {
        out.push("siliconflow".to_string());
    }
    if enabled.groq {
        out.push("groq".to_string());
    }
    if enabled.openrouter {
        out.push("openrouter".to_string());
    }
    if enabled.nvidia {
        out.push("nvidia".to_string());
    }
    if enabled.custom_llm {
        out.push("custom_llm".to_string());
    }
    out
}

fn enabled_providers_from_vec(items: &[String]) -> TranslationEnabledProviders {
    let mut enabled = TranslationEnabledProviders::default();
    for item in items {
        match normalize_provider_token(item) {
            Some("deepl") => enabled.deepl = true,
            Some("deeplx") => enabled.deeplx = true,
            Some("google") => enabled.google = true,
            Some("azure") => enabled.azure = true,
            Some("gtx") => enabled.gtx = true,
            Some("deepseek") => enabled.deepseek = true,
            Some("claude") => enabled.claude = true,
            Some("openai") => enabled.openai = true,
            Some("gemini") => enabled.gemini = true,
            Some("perplexity") => enabled.perplexity = true,
            Some("azureopenai") => enabled.azure_openai = true,
            Some("siliconflow") => enabled.siliconflow = true,
            Some("groq") => enabled.groq = true,
            Some("openrouter") => enabled.openrouter = true,
            Some("nvidia") => enabled.nvidia = true,
            Some("customllm") => enabled.custom_llm = true,
            _ => {}
        }
    }
    enabled
}

fn to_payload_settings(
    settings: &TranslationProviderSettings,
    api_key: &str,
) -> TranslationProviderSettingsPayload {
    TranslationProviderSettingsPayload {
        api_key: api_key.to_string(),
        base_url: settings.base_url.clone(),
        model: settings.model.clone(),
        temperature: settings.temperature,
    }
}

fn to_core_settings(settings: TranslationProviderSettingsPayload) -> TranslationProviderSettings {
    TranslationProviderSettings {
        base_url: settings.base_url,
        model: settings.model,
        temperature: settings.temperature,
    }
}

fn resolve_api_key(primary: String, settings: &TranslationProviderSettingsPayload) -> String {
    if primary.trim().is_empty() {
        settings.api_key.clone()
    } else {
        primary
    }
}

impl TranslationApiConfigPayload {
    fn from_core(core: &crate::core::translation_api::config::TranslationApiConfig) -> Self {
        Self {
            deepl_key: core.deepl_key.clone(),
            deeplx_url: core.deeplx_url.clone(),
            google_key: core.google_key.clone(),
            azure_key: core.azure_key.clone(),
            azure_region: core.azure_region.clone(),
            gtx_api_key: core.gtx_api_key.clone(),
            deepseek_key: core.deepseek_key.clone(),
            claude_key: core.claude_key.clone(),
            openai_key: core.openai_key.clone(),
            gemini_key: core.gemini_key.clone(),
            perplexity_key: core.perplexity_key.clone(),
            azure_openai_key: core.azure_openai_key.clone(),
            siliconflow_key: core.siliconflow_key.clone(),
            groq_key: core.groq_key.clone(),
            openrouter_key: core.openrouter_key.clone(),
            nvidia_key: core.nvidia_key.clone(),
            custom_llm_key: core.custom_llm_key.clone(),
            custom_llm_base_url: core.custom_llm_settings.base_url.clone(),
            deepseek_settings: to_payload_settings(&core.deepseek_settings, &core.deepseek_key),
            claude_settings: to_payload_settings(&core.claude_settings, &core.claude_key),
            openai_settings: to_payload_settings(&core.openai_settings, &core.openai_key),
            gemini_settings: to_payload_settings(&core.gemini_settings, &core.gemini_key),
            perplexity_settings: to_payload_settings(
                &core.perplexity_settings,
                &core.perplexity_key,
            ),
            azure_openai_settings: to_payload_settings(
                &core.azure_openai_settings,
                &core.azure_openai_key,
            ),
            siliconflow_settings: to_payload_settings(
                &core.siliconflow_settings,
                &core.siliconflow_key,
            ),
            groq_settings: to_payload_settings(&core.groq_settings, &core.groq_key),
            openrouter_settings: to_payload_settings(
                &core.openrouter_settings,
                &core.openrouter_key,
            ),
            nvidia_settings: to_payload_settings(&core.nvidia_settings, &core.nvidia_key),
            custom_llm_settings: to_payload_settings(
                &core.custom_llm_settings,
                &core.custom_llm_key,
            ),
            enabled_providers: enabled_providers_to_vec(&core.enabled_providers),
            default_provider: to_ui_provider_name(core.default_provider).to_string(),
            default_skill_provider: to_ui_provider_name(core.default_skill_provider).to_string(),
        }
    }

    fn into_core(self) -> crate::core::translation_api::config::TranslationApiConfig {
        let Self {
            deepl_key,
            deeplx_url,
            google_key,
            azure_key,
            azure_region,
            gtx_api_key,
            deepseek_key,
            claude_key,
            openai_key,
            gemini_key,
            perplexity_key,
            azure_openai_key,
            siliconflow_key,
            groq_key,
            openrouter_key,
            nvidia_key,
            custom_llm_key,
            custom_llm_base_url,
            deepseek_settings,
            claude_settings,
            openai_settings,
            gemini_settings,
            perplexity_settings,
            azure_openai_settings,
            siliconflow_settings,
            groq_settings,
            openrouter_settings,
            nvidia_settings,
            custom_llm_settings,
            enabled_providers,
            default_provider,
            default_skill_provider,
        } = self;

        let mut core = crate::core::translation_api::config::TranslationApiConfig::default();
        core.deepl_key = deepl_key;
        core.deeplx_url = deeplx_url;
        core.google_key = google_key;
        core.azure_key = azure_key;
        core.azure_region = azure_region;
        core.gtx_api_key = gtx_api_key;
        core.deepseek_key = resolve_api_key(deepseek_key, &deepseek_settings);
        core.claude_key = resolve_api_key(claude_key, &claude_settings);
        core.openai_key = resolve_api_key(openai_key, &openai_settings);
        core.gemini_key = resolve_api_key(gemini_key, &gemini_settings);
        core.perplexity_key = resolve_api_key(perplexity_key, &perplexity_settings);
        core.azure_openai_key = resolve_api_key(azure_openai_key, &azure_openai_settings);
        core.siliconflow_key = resolve_api_key(siliconflow_key, &siliconflow_settings);
        core.groq_key = resolve_api_key(groq_key, &groq_settings);
        core.openrouter_key = resolve_api_key(openrouter_key, &openrouter_settings);
        core.nvidia_key = resolve_api_key(nvidia_key, &nvidia_settings);
        core.custom_llm_key = resolve_api_key(custom_llm_key, &custom_llm_settings);
        core.deepseek_settings = to_core_settings(deepseek_settings);
        core.claude_settings = to_core_settings(claude_settings);
        core.openai_settings = to_core_settings(openai_settings);
        core.gemini_settings = to_core_settings(gemini_settings);
        core.perplexity_settings = to_core_settings(perplexity_settings);
        core.azure_openai_settings = to_core_settings(azure_openai_settings);
        core.siliconflow_settings = to_core_settings(siliconflow_settings);
        core.groq_settings = to_core_settings(groq_settings);
        core.openrouter_settings = to_core_settings(openrouter_settings);
        core.nvidia_settings = to_core_settings(nvidia_settings);
        core.custom_llm_settings = to_core_settings(custom_llm_settings);
        if core.custom_llm_settings.base_url.trim().is_empty() {
            core.custom_llm_settings.base_url = custom_llm_base_url;
        }
        core.enabled_providers = enabled_providers_from_vec(&enabled_providers);
        core.default_provider =
            parse_default_provider(&default_provider, TranslationDefaultProvider::DeepL);
        core.default_skill_provider = parse_default_provider(
            &default_skill_provider,
            TranslationDefaultProvider::DeepSeek,
        );
        core
    }
}

impl TranslationSettingsPayload {
    fn from_core(core: &TranslationSettings) -> Self {
        Self {
            target_language: core.target_language.clone(),
            mode: translation_mode_name(core.mode).to_string(),
            fast_provider: translation_fast_provider_name(core.fast_provider).to_string(),
            quality_provider_ref: core.quality_provider_ref.as_ref().and_then(|provider_ref| {
                normalize_quality_provider_ref(&provider_ref.app_id, &provider_ref.provider_id).map(
                    |provider_ref| TranslationQualityProviderRefPayload {
                        app_id: provider_ref.app_id,
                        provider_id: provider_ref.provider_id,
                    },
                )
            }),
            allow_emergency_fallback: core.allow_emergency_fallback,
            experimental_providers_enabled: core.experimental_providers_enabled,
        }
    }

    fn into_core(self) -> TranslationSettings {
        TranslationSettings {
            target_language: if self.target_language.trim().is_empty() {
                default_translation_target_language()
            } else {
                self.target_language
            },
            mode: translation_mode_from_str(&self.mode),
            fast_provider: translation_fast_provider_from_str(&self.fast_provider),
            quality_provider_ref: self.quality_provider_ref.and_then(|provider_ref| {
                normalize_quality_provider_ref(&provider_ref.app_id, &provider_ref.provider_id)
            }),
            allow_emergency_fallback: self.allow_emergency_fallback,
            experimental_providers_enabled: self.experimental_providers_enabled,
        }
    }
}

fn normalize_quality_provider_ref(
    app_id: &str,
    provider_id: &str,
) -> Option<TranslationQualityProviderRef> {
    let app_id = app_id.trim();
    let provider_id = provider_id.trim();

    if !matches!(app_id, "claude" | "codex") || provider_id.is_empty() {
        return None;
    }

    Some(TranslationQualityProviderRef {
        app_id: app_id.to_string(),
        provider_id: provider_id.to_string(),
    })
}

impl TranslationReadinessPayload {
    fn from_core(core: router::TranslationReadiness) -> Self {
        Self {
            fast_ready: core.fast_ready,
            quality_ready: core.quality_ready,
            emergency_ready: core.emergency_ready,
            issues: core.issues,
            recommended_mode: translation_mode_name(core.recommended_mode).to_string(),
        }
    }
}

#[tauri::command]
pub async fn get_translation_api_config() -> Result<TranslationApiConfigPayload, String> {
    let config = ai_provider::load_config_async().await;
    let mut payload = TranslationApiConfigPayload::from_core(&config.translation_api);
    payload.default_provider =
        translation_fast_provider_name(config.translation_settings.fast_provider).to_string();
    Ok(payload)
}

#[tauri::command]
pub async fn save_translation_api_config(
    config: TranslationApiConfigPayload,
) -> Result<(), String> {
    let mut ai_config = ai_provider::load_config_async().await;
    ai_config.translation_api = config.clone().into_core();
    ai_config.translation_settings.fast_provider =
        translation_fast_provider_from_str(&config.default_provider);
    ai_config
        .translation_settings
        .experimental_providers_enabled = config.enabled_providers.iter().any(|provider| {
        matches!(
            normalize_provider_token(provider),
            Some("deeplx") | Some("gtx")
        )
    });
    ai_provider::save_config(&ai_config).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_translation_settings() -> Result<TranslationSettingsPayload, String> {
    let config = ai_provider::load_config_async().await;
    Ok(TranslationSettingsPayload::from_core(
        &config.translation_settings,
    ))
}

#[tauri::command]
pub async fn save_translation_settings(settings: TranslationSettingsPayload) -> Result<(), String> {
    let mut ai_config = ai_provider::load_config_async().await;
    ai_config.translation_settings = settings.into_core();
    ai_config.target_language = ai_config.translation_settings.target_language.clone();
    ai_provider::save_config(&ai_config).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_translation_readiness() -> Result<TranslationReadinessPayload, String> {
    let config = ai_provider::load_config_async().await;
    Ok(TranslationReadinessPayload::from_core(
        router::build_translation_readiness(&config),
    ))
}

#[tauri::command]
pub async fn test_translation_provider(provider: String) -> Result<u64, String> {
    let config = ai_provider::load_config_async().await;
    let started = std::time::Instant::now();

    if let Some(rest) = provider.strip_prefix("quality:") {
        let (app_id, provider_id) = rest
            .split_once(':')
            .ok_or_else(|| format!("Unsupported quality provider token: {}", provider))?;
        let provider_ref = normalize_quality_provider_ref(app_id, provider_id).ok_or_else(|| {
            "Only Claude Code and Codex API providers from Models are supported as quality engines".to_string()
        })?;

        let mut probe_config = config.clone();
        probe_config.translation_settings.quality_provider_ref = Some(provider_ref);

        let plan = router::build_short_text_route_plan(&probe_config, true)?;
        let attempt = plan
            .attempts
            .first()
            .ok_or_else(|| "Selected quality engine is not ready".to_string())?;

        match &attempt.engine {
            router::TranslationAttemptEngine::QualityAi { config } => {
                let result = tokio::time::timeout(
                    std::time::Duration::from_secs(20),
                    ai_provider::translate_short_text(config, "hello"),
                )
                .await
                .map_err(|_| "Quality provider test timed out".to_string())?;
                result.map_err(|e| e.to_string())?;
            }
            _ => return Err("Selected quality engine could not be probed".to_string()),
        }
    } else {
        let Some(provider_name) = normalize_provider_token(&provider) else {
            return Err(format!("Unsupported translation provider: {}", provider));
        };

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(20),
            crate::core::translation_api::services::translate_with_provider(
                provider_name,
                &config,
                "hello",
                "en",
                "zh-CN",
            ),
        )
        .await
        .map_err(|_| "Translation provider test timed out".to_string())?;

        result.map_err(|e| e.to_string())?;
    }
    Ok(started.elapsed().as_millis() as u64)
}

// ── Re-exports ──────────────────────────────────────────────────────
// Only items referenced by other modules via `commands::ai::` are re-exported here.
// Tauri commands use their full submodule paths (e.g. commands::ai::scan::*).

pub use scan::CANCEL_SCAN;

#[cfg(test)]
mod tests {
    use super::{
        TranslationQualityProviderRefPayload, TranslationSettingsPayload,
    };

    #[test]
    fn translation_settings_payload_accepts_snake_case_quality_provider_ref() {
        let payload: TranslationSettingsPayload = serde_json::from_value(serde_json::json!({
            "target_language": "zh-CN",
            "mode": "quality",
            "fast_provider": "deepl",
            "quality_provider_ref": {
                "app_id": "claude",
                "provider_id": "demo"
            },
            "allow_emergency_fallback": true,
            "experimental_providers_enabled": false
        }))
        .expect("payload should deserialize");

        let provider_ref = payload
            .quality_provider_ref
            .expect("quality provider ref should be preserved");
        assert_eq!(provider_ref.app_id, "claude");
        assert_eq!(provider_ref.provider_id, "demo");
    }

    #[test]
    fn translation_settings_payload_ignores_camel_case_quality_provider_ref() {
        let payload: TranslationSettingsPayload = serde_json::from_value(serde_json::json!({
            "target_language": "zh-CN",
            "mode": "quality",
            "fast_provider": "deepl",
            "qualityProviderRef": {
                "appId": "claude",
                "providerId": "demo"
            },
            "allow_emergency_fallback": true,
            "experimental_providers_enabled": false
        }))
        .expect("payload should deserialize");

        assert!(payload.quality_provider_ref.is_none());
    }

    #[test]
    fn translation_settings_payload_serializes_snake_case_quality_provider_ref() {
        let payload = TranslationSettingsPayload {
            target_language: "zh-CN".to_string(),
            mode: "quality".to_string(),
            fast_provider: "deepl".to_string(),
            quality_provider_ref: Some(TranslationQualityProviderRefPayload {
                app_id: "codex".to_string(),
                provider_id: "provider-1".to_string(),
            }),
            allow_emergency_fallback: true,
            experimental_providers_enabled: false,
        };

        let json = serde_json::to_value(payload).expect("payload should serialize");
        assert!(json.get("quality_provider_ref").is_some());
        assert!(json.get("qualityProviderRef").is_none());
        assert_eq!(json.get("mode").and_then(|value| value.as_str()), Some("quality"));
    }
}
