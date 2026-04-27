//! AI command module — split into domain-specific submodules.
//!
//! - `translate`: SKILL.md translation, short text, batch processing
//! - `summarize`: summarization, AI connection test, skill pick
//! - `scan`: security scan pipeline, estimates, reports, policy

pub mod scan;
pub mod summarize;
pub mod translate;

use crate::core::ai_provider;
use crate::core::translation_api::config::TranslationQualityProviderRef;
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
    fallback_hop: Option<u8>,
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
    #[serde(default)]
    pub quality_provider_ref: Option<TranslationQualityProviderRefPayload>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationReadinessPayload {
    pub ready: bool,
    pub quality_ready: bool,
    #[serde(default)]
    pub issues: Vec<String>,
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
    _route_mode: Option<String>,
    fallback_hop: Option<u8>,
) -> Result<(), String> {
    let payload = serde_json::json!({
        "requestId": request_id,
        "event": event,
        "delta": delta,
        "message": message,
        "providerId": provider_id,
        "providerType": provider_type,
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
    let config = ai_provider::resolve_runtime_config(&config).map_err(|e| e.to_string())?;
    if config.api_key.trim().is_empty() && config.api_format != ai_provider::ApiFormat::Local {
        return Err(
            "AI provider is not configured. Please choose a Models provider or local model in Settings.".to_string(),
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

// ── Simplified Translation API Config ───────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TranslationApiConfigPayload {
    #[serde(default)]
    pub deepl_key: String,
    #[serde(default)]
    pub deeplx_key: String,
    #[serde(default)]
    pub deeplx_url: String,
}

fn default_translation_target_language() -> String {
    "zh-CN".to_string()
}

impl TranslationApiConfigPayload {
    fn from_core(core: &crate::core::translation_api::config::TranslationApiConfig) -> Self {
        Self {
            deepl_key: core.deepl_key.clone(),
            deeplx_key: core.deeplx_key.clone(),
            deeplx_url: core.deeplx_url.clone(),
        }
    }

    fn into_core(self) -> crate::core::translation_api::config::TranslationApiConfig {
        let mut core = crate::core::translation_api::config::TranslationApiConfig::default();
        core.deepl_key = self.deepl_key;
        core.deeplx_key = self.deeplx_key;
        core.deeplx_url = self.deeplx_url;
        core
    }
}

impl TranslationSettingsPayload {
    fn from_core(core: &crate::core::translation_api::config::TranslationSettings) -> Self {
        Self {
            target_language: core.target_language.clone(),
            quality_provider_ref: core.quality_provider_ref.as_ref().and_then(|provider_ref| {
                normalize_quality_provider_ref(&provider_ref.app_id, &provider_ref.provider_id).map(
                    |provider_ref| TranslationQualityProviderRefPayload {
                        app_id: provider_ref.app_id,
                        provider_id: provider_ref.provider_id,
                    },
                )
            }),
        }
    }

    fn into_core(self) -> crate::core::translation_api::config::TranslationSettings {
        crate::core::translation_api::config::TranslationSettings {
            target_language: if self.target_language.trim().is_empty() {
                default_translation_target_language()
            } else {
                self.target_language
            },
            quality_provider_ref: self.quality_provider_ref.and_then(|provider_ref| {
                normalize_quality_provider_ref(&provider_ref.app_id, &provider_ref.provider_id)
            }),
            ..Default::default()
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
            ready: core.ready,
            quality_ready: core.quality_ready,
            issues: core.issues,
        }
    }
}

#[tauri::command]
pub async fn get_translation_api_config() -> Result<TranslationApiConfigPayload, String> {
    let config = ai_provider::load_config_async().await;
    Ok(TranslationApiConfigPayload::from_core(
        &config.translation_api,
    ))
}

#[tauri::command]
pub async fn save_translation_api_config(
    config: TranslationApiConfigPayload,
) -> Result<(), String> {
    let mut ai_config = ai_provider::load_config_async().await;
    ai_config.translation_api = config.into_core();
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
        // Only deepl and deeplx are valid provider names now
        let provider_name = match provider.trim().to_ascii_lowercase().as_str() {
            "deepl" | "deepl_key" => "deepl",
            "deeplx" | "deeplx_url" | "deeplx_key" => "deeplx",
            _ => return Err(format!("Unsupported translation provider: {}", provider)),
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
        TranslationApiConfigPayload, TranslationQualityProviderRefPayload,
        TranslationSettingsPayload, normalize_quality_provider_ref,
    };

    #[test]
    fn translation_settings_payload_accepts_snake_case_quality_provider_ref() {
        let payload: TranslationSettingsPayload = serde_json::from_value(serde_json::json!({
            "target_language": "zh-CN",
            "quality_provider_ref": {
                "app_id": "claude",
                "provider_id": "demo"
            }
        }))
        .expect("payload should deserialize");

        let provider_ref = payload
            .quality_provider_ref
            .expect("quality provider ref should be preserved");
        assert_eq!(provider_ref.app_id, "claude");
        assert_eq!(provider_ref.provider_id, "demo");
    }

    #[test]
    fn translation_settings_payload_serializes_snake_case_quality_provider_ref() {
        let payload = TranslationSettingsPayload {
            target_language: "zh-CN".to_string(),
            quality_provider_ref: Some(TranslationQualityProviderRefPayload {
                app_id: "codex".to_string(),
                provider_id: "provider-1".to_string(),
            }),
        };

        let json = serde_json::to_value(payload).expect("payload should serialize");
        assert!(json.get("quality_provider_ref").is_some());
        assert!(json.get("qualityProviderRef").is_none());
    }

    #[test]
    fn normalize_quality_provider_ref_claude_valid() {
        let result = normalize_quality_provider_ref("claude", "my-provider");
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.app_id, "claude");
        assert_eq!(r.provider_id, "my-provider");
    }

    #[test]
    fn normalize_quality_provider_ref_codex_valid() {
        let result = normalize_quality_provider_ref("codex", "codex-provider");
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.app_id, "codex");
        assert_eq!(r.provider_id, "codex-provider");
    }

    #[test]
    fn normalize_quality_provider_ref_invalid_app_id() {
        assert!(normalize_quality_provider_ref("openai", "provider").is_none());
        assert!(normalize_quality_provider_ref("gemini", "provider").is_none());
        assert!(normalize_quality_provider_ref("", "provider").is_none());
    }

    #[test]
    fn normalize_quality_provider_ref_empty_provider_id() {
        assert!(normalize_quality_provider_ref("claude", "").is_none());
        assert!(normalize_quality_provider_ref("codex", "  ").is_none());
    }

    #[test]
    fn normalize_quality_provider_ref_trims_whitespace() {
        let result = normalize_quality_provider_ref("  claude  ", "  provider  ").unwrap();
        assert_eq!(result.app_id, "claude");
        assert_eq!(result.provider_id, "provider");
    }

    #[test]
    fn translation_api_config_payload_roundtrips() {
        let core = crate::core::translation_api::config::TranslationApiConfig {
            deepl_key: "key1".to_string(),
            deeplx_key: "key2".to_string(),
            deeplx_url: "https://example.com".to_string(),
            ..Default::default()
        };
        let payload = TranslationApiConfigPayload::from_core(&core);
        assert_eq!(payload.deepl_key, "key1");
        assert_eq!(payload.deeplx_key, "key2");
        assert_eq!(payload.deeplx_url, "https://example.com");

        let back = payload.into_core();
        assert_eq!(back.deepl_key, "key1");
        assert_eq!(back.deeplx_key, "key2");
        assert_eq!(back.deeplx_url, "https://example.com");
    }
}
