use serde::{Deserialize, Serialize};

use crate::core::ai_provider::{AiConfig, ApiFormat};
use crate::core::model_config::{circuit_breaker, providers};
use crate::core::translation_api::config::{
    TranslationFastProvider, TranslationQualityProviderRef, TranslationRouteMode,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranslationContentKind {
    ShortText,
    MarkdownDocument,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TranslationProviderType {
    #[default]
    TranslationApi,
    Llm,
    Fallback,
}

impl TranslationProviderType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TranslationApi => "translation_api",
            Self::Llm => "llm",
            Self::Fallback => "fallback",
        }
    }
}

#[derive(Debug, Clone)]
pub enum TranslationAttemptEngine {
    TranslationApi { provider: &'static str },
    QualityAi { config: AiConfig },
    EmergencyMymemory,
}

#[derive(Debug, Clone)]
pub struct TranslationAttempt {
    pub engine: TranslationAttemptEngine,
    pub provider_id: String,
    pub provider_label: String,
    pub provider_type: TranslationProviderType,
    pub cache_identity: String,
    pub fallback_hop: u8,
}

#[derive(Debug, Clone)]
pub struct TranslationRoutePlan {
    pub mode: TranslationRouteMode,
    pub target_language: String,
    pub attempts: Vec<TranslationAttempt>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationReadiness {
    pub fast_ready: bool,
    pub quality_ready: bool,
    pub emergency_ready: bool,
    pub issues: Vec<String>,
    pub recommended_mode: TranslationRouteMode,
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
            return Some(collected).filter(|value| !value.trim().is_empty());
        }
    }
    None
}

fn select_app<'a>(
    store: &'a providers::ProvidersStore,
    app_id: &str,
) -> Option<&'a providers::AppProviders> {
    match app_id {
        "claude" => Some(&store.claude),
        "codex" => Some(&store.codex),
        _ => None,
    }
}

fn build_quality_attempt_from_ref(
    config: &AiConfig,
    provider_ref: &TranslationQualityProviderRef,
) -> Result<TranslationAttempt, String> {
    let store = providers::read_store().map_err(|err| err.to_string())?;
    let app = select_app(&store, &provider_ref.app_id)
        .ok_or_else(|| format!("Unsupported quality provider app: {}", provider_ref.app_id))?;
    let entry = app
        .providers
        .get(&provider_ref.provider_id)
        .ok_or_else(|| format!("Unknown quality provider: {}", provider_ref.provider_id))?;

    let mut next = config.clone();
    let label = entry.name.clone();

    match provider_ref.app_id.as_str() {
        "claude" => {
            let env = entry
                .settings_config
                .get("env")
                .and_then(|value| value.as_object())
                .ok_or_else(|| "Claude provider env is missing".to_string())?;

            let api_key = env
                .get("ANTHROPIC_AUTH_TOKEN")
                .or_else(|| env.get("ANTHROPIC_API_KEY"))
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| format!("{} is missing an API key", label))?;

            let base_url = env
                .get("ANTHROPIC_BASE_URL")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("https://api.anthropic.com");

            let model = env
                .get("ANTHROPIC_MODEL")
                .or_else(|| env.get("CLAUDE_CODE_MODEL"))
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("claude-sonnet-4-20250514");

            next.enabled = true;
            next.api_format = ApiFormat::Anthropic;
            next.api_key = api_key.to_string();
            next.base_url = base_url.to_string();
            next.model = model.to_string();
        }
        "codex" => {
            let auth = entry
                .settings_config
                .get("auth")
                .and_then(|value| value.as_object())
                .ok_or_else(|| "Codex provider auth is missing".to_string())?;
            let config_text = entry
                .settings_config
                .get("config")
                .and_then(|value| value.as_str())
                .unwrap_or_default();

            let api_key = auth
                .get("OPENAI_API_KEY")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| format!("{} is missing an API key", label))?;

            let base_url = parse_toml_string_field(config_text, "openai_base_url")
                .or_else(|| parse_toml_string_field(config_text, "base_url"))
                .or_else(|| {
                    entry
                        .meta
                        .as_ref()
                        .and_then(|meta| meta.get("baseURL"))
                        .and_then(|value| value.as_str())
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string)
                })
                .unwrap_or_else(|| "https://api.openai.com/v1".to_string());
            let model =
                parse_toml_string_field(config_text, "model").unwrap_or_else(|| next.model.clone());

            next.enabled = true;
            next.api_format = ApiFormat::Openai;
            next.api_key = api_key.to_string();
            next.base_url = base_url;
            next.model = model.clone();
        }
        _ => {
            return Err(format!(
                "Unsupported quality provider app: {}",
                provider_ref.app_id
            ));
        }
    }

    Ok(TranslationAttempt {
        engine: TranslationAttemptEngine::QualityAi { config: next },
        provider_id: format!("{}:{}", provider_ref.app_id, provider_ref.provider_id),
        provider_label: label,
        provider_type: TranslationProviderType::Llm,
        cache_identity: format!("llm:{}:{}", provider_ref.app_id, provider_ref.provider_id),
        fallback_hop: 0,
    })
}

fn resolve_quality_attempt(config: &AiConfig) -> Result<Option<TranslationAttempt>, String> {
    if let Some(provider_ref) = config.translation_settings.quality_provider_ref.as_ref() {
        return build_quality_attempt_from_ref(config, provider_ref).map(Some);
    }

    Ok(None)
}

fn fast_attempt(provider: &'static str, label: &str) -> TranslationAttempt {
    TranslationAttempt {
        engine: TranslationAttemptEngine::TranslationApi { provider },
        provider_id: provider.to_string(),
        provider_label: label.to_string(),
        provider_type: TranslationProviderType::TranslationApi,
        cache_identity: format!("translation_api:{provider}"),
        fallback_hop: 0,
    }
}

fn fast_attempts(config: &AiConfig, _kind: TranslationContentKind) -> Vec<TranslationAttempt> {
    match config.translation_settings.fast_provider {
        TranslationFastProvider::DeepL if !config.translation_api.deepl_key.trim().is_empty() => {
            vec![fast_attempt("deepl", "DeepL")]
        }
        TranslationFastProvider::Google if !config.translation_api.google_key.trim().is_empty() => {
            vec![fast_attempt("google", "Google Translate")]
        }
        TranslationFastProvider::Azure
            if !config.translation_api.azure_key.trim().is_empty()
                && !config.translation_api.azure_region.trim().is_empty() =>
        {
            vec![fast_attempt("azure", "Azure Translator")]
        }
        TranslationFastProvider::Experimental
            if config.translation_settings.experimental_providers_enabled =>
        {
            let mut attempts = Vec::new();
            attempts.push(fast_attempt("deeplx", "DeepLX"));
            attempts.push(fast_attempt("gtx", "GTX"));
            attempts
        }
        _ => Vec::new(),
    }
}

fn with_fallback_hops(mut attempts: Vec<TranslationAttempt>) -> Vec<TranslationAttempt> {
    for (idx, attempt) in attempts.iter_mut().enumerate() {
        attempt.fallback_hop = idx as u8;
    }
    attempts
}

pub fn build_translation_readiness(config: &AiConfig) -> TranslationReadiness {
    let mut issues = Vec::new();
    let fast_ready = !fast_attempts(config, TranslationContentKind::ShortText).is_empty()
        || !fast_attempts(config, TranslationContentKind::MarkdownDocument).is_empty();

    if !fast_ready {
        issues.push(
            "Fast lane is not ready. Connect DeepL, Google, Azure, or enable Experimental Sources for the free fallback lane."
                .to_string(),
        );
    }

    let quality_resolution = resolve_quality_attempt(config).ok().flatten();
    let quality_ready = quality_resolution.is_some();
    if !quality_ready
        && matches!(config.translation_settings.mode, TranslationRouteMode::Quality)
    {
        issues.push(
            "Quality lane is not ready. Select a provider from Models or switch to Fast mode."
                .to_string(),
        );
    }

    let emergency_ready = config.translation_settings.allow_emergency_fallback;
    if !emergency_ready {
        issues.push("Emergency fallback is disabled. MyMemory will not be used when the primary route fails.".to_string());
    }

    let recommended_mode = if fast_ready && quality_ready {
        TranslationRouteMode::Balanced
    } else if quality_ready {
        TranslationRouteMode::Quality
    } else {
        TranslationRouteMode::Fast
    };

    TranslationReadiness {
        fast_ready,
        quality_ready,
        emergency_ready,
        issues,
        recommended_mode,
    }
}

pub fn build_short_text_route_plan(
    config: &AiConfig,
    force_quality: bool,
) -> Result<TranslationRoutePlan, String> {
    let mode = if force_quality {
        TranslationRouteMode::Quality
    } else {
        config.translation_settings.mode
    };

    let mut attempts = Vec::new();
    let quality = resolve_quality_attempt(config)?;
    let fast = fast_attempts(config, TranslationContentKind::ShortText);

    match mode {
        TranslationRouteMode::Balanced => {
            attempts.extend(fast);
            if config.translation_settings.allow_emergency_fallback {
                attempts.push(TranslationAttempt {
                    engine: TranslationAttemptEngine::EmergencyMymemory,
                    provider_id: "mymemory".to_string(),
                    provider_label: "MyMemory".to_string(),
                    provider_type: TranslationProviderType::Fallback,
                    cache_identity: "fallback:mymemory".to_string(),
                    fallback_hop: 0,
                });
            }
            if let Some(quality) = quality {
                attempts.push(quality);
            }
        }
        TranslationRouteMode::Fast => {
            attempts.extend(fast);
            if config.translation_settings.allow_emergency_fallback {
                attempts.push(TranslationAttempt {
                    engine: TranslationAttemptEngine::EmergencyMymemory,
                    provider_id: "mymemory".to_string(),
                    provider_label: "MyMemory".to_string(),
                    provider_type: TranslationProviderType::Fallback,
                    cache_identity: "fallback:mymemory".to_string(),
                    fallback_hop: 0,
                });
            }
        }
        TranslationRouteMode::Quality => {
            if let Some(quality) = quality {
                attempts.push(quality);
            }
            if !force_quality && config.translation_settings.allow_emergency_fallback {
                attempts.push(TranslationAttempt {
                    engine: TranslationAttemptEngine::EmergencyMymemory,
                    provider_id: "mymemory".to_string(),
                    provider_label: "MyMemory".to_string(),
                    provider_type: TranslationProviderType::Fallback,
                    cache_identity: "fallback:mymemory".to_string(),
                    fallback_hop: 0,
                });
            }
        }
    }

    Ok(TranslationRoutePlan {
        mode,
        target_language: config.translation_settings.target_language.clone(),
        attempts: with_fallback_hops(attempts),
    })
}

pub fn build_markdown_route_plan(
    config: &AiConfig,
    force_quality: bool,
) -> Result<TranslationRoutePlan, String> {
    let mode = if force_quality {
        TranslationRouteMode::Quality
    } else {
        config.translation_settings.mode
    };

    let mut attempts = Vec::new();
    let quality = resolve_quality_attempt(config)?;
    let fast = fast_attempts(config, TranslationContentKind::MarkdownDocument);

    match mode {
        TranslationRouteMode::Balanced => {
            if let Some(quality) = quality {
                attempts.push(quality);
            }
            attempts.extend(fast);
        }
        TranslationRouteMode::Fast => {
            attempts.extend(fast);
            if let Some(quality) = quality {
                attempts.push(quality);
            }
        }
        TranslationRouteMode::Quality => {
            if let Some(quality) = quality {
                attempts.push(quality);
            }
            if !force_quality {
                attempts.extend(fast);
            }
        }
    }

    Ok(TranslationRoutePlan {
        mode,
        target_language: config.translation_settings.target_language.clone(),
        attempts: with_fallback_hops(attempts),
    })
}

pub async fn attempt_is_available(attempt: &TranslationAttempt) -> bool {
    if matches!(attempt.provider_type, TranslationProviderType::Fallback) {
        return true;
    }
    circuit_breaker::is_provider_available("translation", &attempt.provider_id).await
}

pub async fn record_attempt_success(attempt: &TranslationAttempt) {
    if matches!(attempt.provider_type, TranslationProviderType::Fallback) {
        return;
    }
    circuit_breaker::record_success("translation", &attempt.provider_id).await;
}

pub async fn record_attempt_failure(attempt: &TranslationAttempt) {
    if matches!(attempt.provider_type, TranslationProviderType::Fallback) {
        return;
    }
    circuit_breaker::record_failure("translation", &attempt.provider_id).await;
}

#[cfg(test)]
mod tests {
    use super::{
        TranslationContentKind, TranslationProviderType, build_markdown_route_plan,
        build_short_text_route_plan, build_translation_readiness, fast_attempts,
    };
    use crate::core::ai_provider::{AiConfig, ApiFormat};
    use crate::core::translation_api::config::{TranslationFastProvider, TranslationRouteMode};

    fn configured_fast_config() -> AiConfig {
        let mut config = AiConfig::default();
        config.translation_settings.mode = TranslationRouteMode::Balanced;
        config.translation_settings.fast_provider = TranslationFastProvider::DeepL;
        config.translation_api.deepl_key = "deepl-key".to_string();
        config
    }

    #[test]
    fn balanced_short_route_prefers_fast_then_mymemory_when_quality_missing() {
        let config = configured_fast_config();
        let plan = build_short_text_route_plan(&config, false).expect("plan");
        let provider_ids: Vec<_> = plan
            .attempts
            .iter()
            .map(|attempt| attempt.provider_id.as_str())
            .collect();

        assert_eq!(plan.mode, TranslationRouteMode::Balanced);
        assert_eq!(provider_ids, vec!["deepl", "mymemory"]);
        assert_eq!(
            plan.attempts[0].provider_type,
            TranslationProviderType::TranslationApi
        );
        assert_eq!(
            plan.attempts[1].provider_type,
            TranslationProviderType::Fallback
        );
    }

    #[test]
    fn quality_short_force_route_requires_explicit_quality_provider() {
        let config = configured_fast_config();
        let plan = build_short_text_route_plan(&config, true).expect("plan");

        assert_eq!(plan.mode, TranslationRouteMode::Quality);
        assert!(plan.attempts.is_empty());
    }

    #[test]
    fn fast_markdown_route_uses_fast_only_when_quality_missing() {
        let mut config = configured_fast_config();
        config.translation_settings.mode = TranslationRouteMode::Fast;

        let plan = build_markdown_route_plan(&config, false).expect("plan");
        let provider_ids: Vec<_> = plan
            .attempts
            .iter()
            .map(|attempt| attempt.provider_id.as_str())
            .collect();

        assert_eq!(provider_ids, vec!["deepl"]);
    }

    #[test]
    fn quality_markdown_route_uses_fast_fallback_only_when_not_forced() {
        let mut config = configured_fast_config();
        config.translation_settings.mode = TranslationRouteMode::Quality;

        let normal = build_markdown_route_plan(&config, false).expect("normal plan");
        let forced = build_markdown_route_plan(&config, true).expect("forced plan");

        let normal_ids: Vec<_> = normal
            .attempts
            .iter()
            .map(|attempt| attempt.provider_id.as_str())
            .collect();
        let forced_ids: Vec<_> = forced
            .attempts
            .iter()
            .map(|attempt| attempt.provider_id.as_str())
            .collect();

        assert_eq!(normal_ids, vec!["deepl"]);
        assert!(forced_ids.is_empty());
    }

    #[test]
    fn readiness_does_not_treat_global_ai_as_quality_lane() {
        let mut config = AiConfig::default();
        config.enabled = true;
        config.api_format = ApiFormat::Anthropic;
        config.api_key = "sk-quality".to_string();
        config.base_url = "https://api.anthropic.com".to_string();
        config.model = "claude-sonnet-4-20250514".to_string();

        let readiness = build_translation_readiness(&config);

        assert!(!readiness.fast_ready);
        assert!(!readiness.quality_ready);
        assert_eq!(readiness.recommended_mode, TranslationRouteMode::Fast);
    }

    #[test]
    fn experimental_fast_attempts_apply_to_short_text_and_markdown() {
        let mut config = AiConfig::default();
        config.translation_settings.fast_provider = TranslationFastProvider::Experimental;
        config.translation_settings.experimental_providers_enabled = true;
        config.translation_api.deeplx_url = "http://127.0.0.1:1188".to_string();

        let short_attempts = fast_attempts(&config, TranslationContentKind::ShortText);
        let markdown_attempts = fast_attempts(&config, TranslationContentKind::MarkdownDocument);

        assert_eq!(short_attempts.len(), 2);
        assert_eq!(markdown_attempts.len(), 2);
    }
}
