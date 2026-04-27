use serde::{Deserialize, Serialize};

use skillstar_ai::ai_provider::{self, AiConfig};
use skillstar_ai::translation_config::TranslationQualityProviderRef;
use skillstar_model_config::circuit_breaker;

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
}

impl TranslationProviderType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TranslationApi => "translation_api",
            Self::Llm => "llm",
        }
    }
}

#[derive(Debug, Clone)]
pub enum TranslationAttemptEngine {
    TranslationApi { provider: &'static str },
    QualityAi { config: AiConfig },
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
    pub target_language: String,
    pub attempts: Vec<TranslationAttempt>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationReadiness {
    pub ready: bool,
    pub quality_ready: bool,
    pub issues: Vec<String>,
}

fn build_quality_attempt_from_ref(
    config: &AiConfig,
    provider_ref: &TranslationQualityProviderRef,
) -> Result<TranslationAttempt, String> {
    let mut next = config.clone();
    let label = ai_provider::resolve_provider_ref_parts(
        &mut next,
        &provider_ref.app_id,
        &provider_ref.provider_id,
    )
    .map_err(|err| err.to_string())?;
    next.enabled = true;

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

fn fast_attempts(config: &AiConfig) -> Vec<TranslationAttempt> {
    let mut attempts = Vec::new();
    if !config.translation_api.deepl_key.trim().is_empty() {
        attempts.push(fast_attempt("deepl", "DeepL"));
    }
    attempts.push(fast_attempt("deeplx", "DeepLX"));
    attempts
}

fn with_fallback_hops(mut attempts: Vec<TranslationAttempt>) -> Vec<TranslationAttempt> {
    for (idx, attempt) in attempts.iter_mut().enumerate() {
        attempt.fallback_hop = idx as u8;
    }
    attempts
}

pub fn build_translation_readiness(config: &AiConfig) -> TranslationReadiness {
    let mut issues = Vec::new();

    let ready = true;

    let quality_resolution = resolve_quality_attempt(config).ok().flatten();
    let quality_ready = quality_resolution.is_some();

    if config.translation_api.deepl_key.trim().is_empty() {
        issues.push("DeepL key not configured — using free DeepLX endpoint.".to_string());
    }

    if !quality_ready {
        issues.push(
            "Quality LLM not configured — SKILL.md will use DeepL/DeepLX instead of AI."
                .to_string(),
        );
    }

    TranslationReadiness {
        ready,
        quality_ready,
        issues,
    }
}

pub fn build_short_text_route_plan(
    config: &AiConfig,
    force_quality: bool,
) -> Result<TranslationRoutePlan, String> {
    let mut attempts = Vec::new();

    if force_quality {
        if let Some(quality) = resolve_quality_attempt(config)? {
            attempts.push(quality);
        }
    } else {
        attempts.extend(fast_attempts(config));
    }

    attempts.push(fast_attempt("mymemory", "MyMemory"));

    Ok(TranslationRoutePlan {
        target_language: config.translation_settings.target_language.clone(),
        attempts: with_fallback_hops(attempts),
    })
}

pub fn build_markdown_route_plan(
    config: &AiConfig,
    force_quality: bool,
) -> Result<TranslationRoutePlan, String> {
    let mut attempts = Vec::new();

    if let Some(quality) = resolve_quality_attempt(config)? {
        attempts.push(quality);
    }

    if !force_quality {
        attempts.extend(fast_attempts(config));
    }

    Ok(TranslationRoutePlan {
        target_language: config.translation_settings.target_language.clone(),
        attempts: with_fallback_hops(attempts),
    })
}

pub async fn attempt_is_available(attempt: &TranslationAttempt) -> bool {
    circuit_breaker::is_provider_available("translation", &attempt.provider_id).await
}

pub async fn record_attempt_success(attempt: &TranslationAttempt) {
    circuit_breaker::record_success("translation", &attempt.provider_id).await;
}

pub async fn record_attempt_failure(attempt: &TranslationAttempt) {
    circuit_breaker::record_failure("translation", &attempt.provider_id).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use skillstar_ai::ai_provider::AiConfig;

    fn configured_deepl_config() -> AiConfig {
        let mut config = AiConfig::default();
        config.translation_api.deepl_key = "deepl-key".to_string();
        config
    }

    #[test]
    fn short_route_uses_deepl_then_deeplx() {
        let config = configured_deepl_config();
        let plan = build_short_text_route_plan(&config, false).expect("plan");
        let provider_ids: Vec<_> = plan
            .attempts
            .iter()
            .map(|attempt| attempt.provider_id.as_str())
            .collect();

        assert_eq!(provider_ids, vec!["deepl", "deeplx", "mymemory"]);
        assert_eq!(
            plan.attempts[0].provider_type,
            TranslationProviderType::TranslationApi
        );
    }

    #[test]
    fn short_route_without_deepl_uses_deeplx_only() {
        let config = AiConfig::default();
        let plan = build_short_text_route_plan(&config, false).expect("plan");
        let provider_ids: Vec<_> = plan
            .attempts
            .iter()
            .map(|attempt| attempt.provider_id.as_str())
            .collect();

        assert_eq!(provider_ids, vec!["deeplx", "mymemory"]);
    }

    #[test]
    fn quality_force_probe_returns_empty_without_quality_provider() {
        let config = configured_deepl_config();
        let plan = build_short_text_route_plan(&config, true).expect("plan");
        assert_eq!(plan.attempts.len(), 1);
        assert_eq!(plan.attempts[0].provider_id, "mymemory");
    }

    #[test]
    fn markdown_route_has_fast_fallback() {
        let config = configured_deepl_config();
        let plan = build_markdown_route_plan(&config, false).expect("plan");
        let provider_ids: Vec<_> = plan
            .attempts
            .iter()
            .map(|attempt| attempt.provider_id.as_str())
            .collect();

        assert_eq!(provider_ids, vec!["deepl", "deeplx"]);
    }

    #[test]
    fn readiness_always_ready_due_to_deeplx() {
        let config = AiConfig::default();
        let readiness = build_translation_readiness(&config);
        assert!(readiness.ready);
        assert!(!readiness.quality_ready);
    }
}
