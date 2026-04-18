use crate::core::ai::translation_cache::{self, TranslationKind};
use crate::core::ai_provider;
use crate::core::translation_api::router::{
    self, TranslationAttempt, TranslationAttemptEngine, TranslationProviderType,
    TranslationRoutePlan,
};
use serde::Serialize;
use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, LazyLock, Mutex};
use tauri::AppHandle;
use tracing::{debug, error, info, warn};

use super::{ShortTextTranslationPayload, emit_translate_stream_event, ensure_ai_config};

/// Global concurrency limiter — at most 3 different skills translate at once.
static SKILL_TRANSLATION_GLOBAL: LazyLock<tokio::sync::Semaphore> =
    LazyLock::new(|| tokio::sync::Semaphore::new(3));

/// Per-content-hash locks so identical content serialises (prevents duplicate API calls)
/// while different content runs in parallel.
static SKILL_TRANSLATION_LOCKS: LazyLock<Mutex<HashMap<u64, Arc<tokio::sync::Semaphore>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn content_hash(content: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}

fn get_per_content_lock(hash: u64) -> Arc<tokio::sync::Semaphore> {
    let mut map = SKILL_TRANSLATION_LOCKS
        .lock()
        .expect("skill translation locks poisoned");
    map.entry(hash)
        .or_insert_with(|| Arc::new(tokio::sync::Semaphore::new(1)))
        .clone()
}

struct SkillTranslationGuard {
    _global_permit: tokio::sync::SemaphorePermit<'static>,
    _content_permit: tokio::sync::OwnedSemaphorePermit,
    content_hash: u64,
}

impl Drop for SkillTranslationGuard {
    fn drop(&mut self) {
        // Clean up per-content lock if no other waiters
        if let Ok(mut map) = SKILL_TRANSLATION_LOCKS.lock() {
            if let Some(sem) = map.get(&self.content_hash) {
                // Only remove if we're the sole remaining reference
                // (the one in the map itself + our Arc = 2 when no waiters)
                if Arc::strong_count(sem) <= 2 {
                    map.remove(&self.content_hash);
                }
            }
        }
    }
}

async fn acquire_skill_translation_session(content: &str) -> Result<SkillTranslationGuard, String> {
    let hash = content_hash(content);
    let per_content = get_per_content_lock(hash);

    // Acquire per-content lock first (serialise identical content)
    let content_permit = per_content
        .acquire_owned()
        .await
        .map_err(|_| "SKILL.md per-content translation lock is unavailable.".to_string())?;

    // Then acquire global concurrency slot
    let global_permit = SKILL_TRANSLATION_GLOBAL
        .acquire()
        .await
        .map_err(|_| "SKILL.md global translation session is unavailable.".to_string())?;

    Ok(SkillTranslationGuard {
        _global_permit: global_permit,
        _content_permit: content_permit,
        content_hash: hash,
    })
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SkillTranslationPayload {
    text: String,
    provider: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fallback_hop: Option<u8>,
}

fn short_text_source_from_provider_type(provider_type: TranslationProviderType) -> &'static str {
    match provider_type {
        TranslationProviderType::TranslationApi => "translation_api",
        TranslationProviderType::Llm => "llm",
    }
}

fn build_short_text_payload(
    text: String,
    plan: &TranslationRoutePlan,
    attempt: &TranslationAttempt,
) -> ShortTextTranslationPayload {
    ShortTextTranslationPayload {
        text,
        source: short_text_source_from_provider_type(attempt.provider_type).to_string(),
        provider: attempt.provider_label.clone(),
        provider_id: Some(attempt.provider_id.clone()),
        provider_type: Some(attempt.provider_type.as_str().to_string()),
        fallback_hop: Some(attempt.fallback_hop),
    }
}

fn build_skill_translation_payload(
    text: String,
    plan: &TranslationRoutePlan,
    attempt: &TranslationAttempt,
) -> SkillTranslationPayload {
    SkillTranslationPayload {
        text,
        provider: attempt.provider_label.clone(),
        provider_id: Some(attempt.provider_id.clone()),
        provider_type: Some(attempt.provider_type.as_str().to_string()),
        fallback_hop: Some(attempt.fallback_hop),
    }
}

#[derive(Clone)]
struct AttemptEventMeta {
    provider_label: Option<String>,
    provider_id: Option<String>,
    provider_type: Option<String>,
    fallback_hop: Option<u8>,
}

impl AttemptEventMeta {
    fn from_attempt(
        _plan: Option<&TranslationRoutePlan>,
        attempt: Option<&TranslationAttempt>,
    ) -> Self {
        Self {
            provider_label: attempt.map(|attempt| attempt.provider_label.clone()),
            provider_id: attempt.map(|attempt| attempt.provider_id.clone()),
            provider_type: attempt.map(|attempt| attempt.provider_type.as_str().to_string()),
            fallback_hop: attempt.map(|attempt| attempt.fallback_hop),
        }
    }
}

fn emit_translate_attempt_event(
    window: &tauri::Window,
    request_id: &str,
    event: &str,
    delta: Option<String>,
    message: Option<String>,
    meta: &AttemptEventMeta,
) -> Result<(), String> {
    emit_translate_stream_event(
        window,
        request_id,
        event,
        delta,
        message.or_else(|| meta.provider_label.clone()),
        meta.provider_id.clone(),
        meta.provider_type.clone(),
        None,
        meta.fallback_hop,
    )
}

fn resolve_batch_quality_lane(
    config: &ai_provider::AiConfig,
) -> Result<(ai_provider::AiConfig, String, String), String> {
    let plan = router::build_markdown_route_plan(config, true)?;
    let attempt = plan
        .attempts
        .first()
        .ok_or_else(|| route_unavailable_error("batch translation"))?;

    match &attempt.engine {
        TranslationAttemptEngine::QualityAi { config } => Ok((
            config.clone(),
            attempt.provider_label.clone(),
            attempt.cache_identity.clone(),
        )),
        _ => Err(route_unavailable_error("batch translation")),
    }
}

const SHORT_TEXT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);
const MARKDOWN_TRANSLATION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(180);

fn route_unavailable_error(kind: &str) -> String {
    format!(
        "Translation Center is not ready for {}. Connect a Fast or Quality lane in Translation Center.",
        kind
    )
}

fn cached_provider_label(
    cached: &translation_cache::CachedTranslation,
    attempt: &TranslationAttempt,
) -> String {
    cached
        .source_provider
        .clone()
        .unwrap_or_else(|| attempt.provider_label.clone())
}

fn get_cached_translation_for_attempt(
    kind: TranslationKind,
    target_language: &str,
    content: &str,
    attempt: &TranslationAttempt,
    log_context: &str,
) -> Option<translation_cache::CachedTranslation> {
    match translation_cache::get_cached_translation_for_provider(
        kind,
        target_language,
        content,
        Some(&attempt.cache_identity),
    ) {
        Ok(Some(cached)) => {
            debug!(
                target: "translate",
                context = %log_context,
                kind = ?kind,
                provider_id = %attempt.provider_id,
                provider_type = %attempt.provider_type.as_str(),
                fallback_hop = attempt.fallback_hop,
                "route cache HIT"
            );
            Some(cached)
        }
        Ok(None) => {
            debug!(
                target: "translate",
                context = %log_context,
                kind = ?kind,
                provider_id = %attempt.provider_id,
                "route cache MISS"
            );
            None
        }
        Err(err) => {
            warn!(
                target: "translate",
                context = %log_context,
                kind = ?kind,
                provider_id = %attempt.provider_id,
                error = %err,
                "route cache read failed"
            );
            None
        }
    }
}

fn get_cached_route_translation<'a>(
    kind: TranslationKind,
    plan: &'a TranslationRoutePlan,
    content: &str,
    log_context: &str,
) -> Option<(&'a TranslationAttempt, translation_cache::CachedTranslation)> {
    for attempt in &plan.attempts {
        if let Some(cached) = get_cached_translation_for_attempt(
            kind,
            &plan.target_language,
            content,
            attempt,
            log_context,
        ) {
            return Some((attempt, cached));
        }
    }
    None
}

fn store_translation_for_attempt(
    kind: TranslationKind,
    target_language: &str,
    source_text: &str,
    translated_text: &str,
    attempt: &TranslationAttempt,
) -> Result<(), String> {
    translation_cache::upsert_translation_for_provider(
        kind,
        target_language,
        source_text,
        translated_text,
        Some(&attempt.provider_label),
        Some(&attempt.cache_identity),
    )
    .map_err(|err| err.to_string())
}

fn validate_translated_text(
    target_language: &str,
    source_text: &str,
    translated_text: String,
    attempt: &TranslationAttempt,
) -> Result<String, String> {
    if translated_text.trim().is_empty() {
        return Err(format!(
            "{} returned an empty response",
            attempt.provider_label
        ));
    }

    if !ai_provider::translation_looks_translated(target_language, source_text, &translated_text) {
        return Err(format!(
            "{} returned untranslated content",
            attempt.provider_label
        ));
    }

    Ok(translated_text)
}

async fn execute_translation_api_attempt(
    config: &ai_provider::AiConfig,
    provider: &str,
    target_language: &str,
    text: &str,
    timeout: std::time::Duration,
    attempt: &TranslationAttempt,
) -> Result<String, String> {
    let response = tokio::time::timeout(
        timeout,
        crate::core::translation_api::services::translate_with_provider(
            provider,
            config,
            text,
            "auto",
            target_language,
        ),
    )
    .await
    .map_err(|_| format!("{} timed out", attempt.provider_label))?
    .map_err(|err| err.to_string())?;

    validate_translated_text(target_language, text, response.translated_text, attempt)
}

fn emit_clear_delta(
    window: &tauri::Window,
    request_id: &str,
    meta: &AttemptEventMeta,
) -> Result<(), String> {
    emit_translate_attempt_event(
        window,
        request_id,
        "delta",
        Some("\0CLEAR\0".to_string()),
        None,
        meta,
    )
}

async fn execute_short_attempt(
    window: &tauri::Window,
    request_id: &str,
    base_config: &ai_provider::AiConfig,
    plan: &TranslationRoutePlan,
    attempt: &TranslationAttempt,
    content: &str,
) -> Result<(String, bool), String> {
    let meta = AttemptEventMeta::from_attempt(Some(plan), Some(attempt));
    let mut emitted_delta = false;

    match &attempt.engine {
        TranslationAttemptEngine::TranslationApi { provider } => {
            let translated = execute_translation_api_attempt(
                base_config,
                provider,
                &plan.target_language,
                content,
                SHORT_TEXT_TIMEOUT,
                attempt,
            )
            .await?;
            emit_translate_attempt_event(
                window,
                request_id,
                "delta",
                Some(translated.clone()),
                None,
                &meta,
            )?;
            emitted_delta = true;
            Ok((translated, emitted_delta))
        }
        TranslationAttemptEngine::QualityAi { config } => {
            let mut on_delta = |delta: &str| -> anyhow::Result<()> {
                emitted_delta = true;
                emit_translate_attempt_event(
                    window,
                    request_id,
                    "delta",
                    Some(delta.to_string()),
                    None,
                    &meta,
                )
                .map_err(anyhow::Error::msg)
            };

            let translated = tokio::time::timeout(
                SHORT_TEXT_TIMEOUT,
                ai_provider::translate_short_text_streaming(config, content, &mut on_delta),
            )
            .await
            .map_err(|_| format!("{} timed out", attempt.provider_label))?
            .map_err(|err| err.to_string())?;

            let translated =
                validate_translated_text(&plan.target_language, content, translated, attempt)?;
            Ok((translated, emitted_delta))
        }
    }
}

async fn execute_skill_attempt(
    base_config: &ai_provider::AiConfig,
    plan: &TranslationRoutePlan,
    attempt: &TranslationAttempt,
    content: &str,
    force_refresh: bool,
) -> Result<String, String> {
    match &attempt.engine {
        TranslationAttemptEngine::TranslationApi { provider } => {
            tokio::time::timeout(
                MARKDOWN_TRANSLATION_TIMEOUT,
                crate::core::translation_api::markdown::translate_markdown_with_provider(
                    base_config,
                    provider,
                    content,
                    &plan.target_language,
                    Some(&attempt.cache_identity),
                ),
            )
            .await
            .map_err(|_| format!("{} timed out", attempt.provider_label))?
        }
        TranslationAttemptEngine::QualityAi { config } => {
            crate::core::ai::mdtx_bridge::translate_skill_content(
                config,
                content,
                force_refresh,
            )
            .await
            .and_then(|translated| {
                validate_translated_text(&plan.target_language, content, translated, attempt)
            })
        }
    }
}

fn is_markdown_heading(trimmed_line: &str) -> bool {
    let hash_count = trimmed_line.chars().take_while(|c| *c == '#').count();
    if hash_count == 0 || hash_count > 6 {
        return false;
    }
    trimmed_line
        .chars()
        .nth(hash_count)
        .map(|ch| ch == ' ')
        .unwrap_or(false)
}

pub(super) fn split_markdown_sections(content: &str) -> Vec<String> {
    if content.is_empty() {
        return vec![String::new()];
    }

    let mut sections = Vec::new();
    let mut current = String::new();
    let mut in_fenced_code_block = false;

    for line in content.split_inclusive('\n') {
        let trimmed_start = line.trim_start();
        let starts_fence = trimmed_start.starts_with("```") || trimmed_start.starts_with("~~~");
        let heading_boundary = !in_fenced_code_block && is_markdown_heading(trimmed_start);

        if heading_boundary && !current.is_empty() {
            sections.push(current);
            current = String::new();
        }

        current.push_str(line);

        if starts_fence {
            in_fenced_code_block = !in_fenced_code_block;
        }
    }

    if !current.is_empty() {
        sections.push(current);
    }

    if sections.is_empty() {
        vec![content.to_string()]
    } else {
        sections
    }
}

fn maybe_fix_trailing_newline(source: &str, translated: &str) -> String {
    if source.ends_with('\n') && !translated.ends_with('\n') {
        let mut owned = translated.to_string();
        owned.push('\n');
        owned
    } else {
        translated.to_string()
    }
}

pub(super) async fn assemble_sections_with_cache<FGet, FStore, FEmitCached, FTranslate, Fut>(
    sections: &[String],
    force_refresh: bool,
    mut get_cached: FGet,
    mut store: FStore,
    mut emit_cached: FEmitCached,
    mut translate: FTranslate,
) -> Result<String, String>
where
    FGet: FnMut(&str) -> Result<Option<String>, String>,
    FStore: FnMut(&str, &str) -> Result<(), String>,
    FEmitCached: FnMut(&str) -> Result<(), String>,
    FTranslate: FnMut(String) -> Fut,
    Fut: Future<Output = Result<String, String>>,
{
    // Phase 1: resolve cache — collect which sections need fresh translation.
    let mut cached_results: Vec<Option<String>> = Vec::with_capacity(sections.len());
    let mut uncached_indices: Vec<usize> = Vec::new();

    for (idx, section) in sections.iter().enumerate() {
        if !force_refresh {
            match get_cached(section) {
                Ok(Some(cached_text)) => {
                    let normalized = maybe_fix_trailing_newline(section, &cached_text);
                    cached_results.push(Some(normalized));
                    continue;
                }
                Ok(None) => {}
                Err(err) => {
                    warn!(target: "translate", error = %err, "skill section cache read failed");
                }
            }
        }
        cached_results.push(None);
        uncached_indices.push(idx);
    }

    // Phase 2: translate all uncached sections.
    let mut fresh_map: std::collections::HashMap<usize, String> =
        std::collections::HashMap::with_capacity(uncached_indices.len());

    for &idx in &uncached_indices {
        let fresh = translate(sections[idx].clone()).await?;
        let normalized = maybe_fix_trailing_newline(&sections[idx], &fresh);
        if let Err(err) = store(&sections[idx], &normalized) {
            warn!(target: "translate", error = %err, "skill section cache write failed");
        }
        fresh_map.insert(idx, normalized);
    }

    // Phase 3: assemble in order.
    let mut assembled = String::new();
    for (idx, section) in sections.iter().enumerate() {
        if let Some(cached) = &cached_results[idx] {
            emit_cached(cached)?;
            assembled.push_str(cached);
        } else if let Some(fresh) = fresh_map.remove(&idx) {
            assembled.push_str(&fresh);
        } else {
            // Fallback — should not happen
            assembled.push_str(section);
        }
    }

    Ok(assembled)
}

fn skill_should_split_translation_sections(config: &ai_provider::AiConfig, content: &str) -> bool {
    let budget = ai_provider::skill_translation_single_pass_char_budget(config);
    let sections = split_markdown_sections(content);
    sections.len() > 1 && content.len() >= budget
}

pub(super) async fn translate_skill_with_section_cache(
    config: &ai_provider::AiConfig,
    content: &str,
    force_refresh: bool,
    provider_identity: Option<&str>,
) -> Result<String, String> {
    if !skill_should_split_translation_sections(config, content) {
        let budget = ai_provider::skill_translation_single_pass_char_budget(config);
        debug!(
            target: "translate",
            chars = content.len(),
            budget,
            "whole-doc or small → single translate request"
        );
        return ai_provider::translate_text(config, content)
            .await
            .map_err(|e| e.to_string());
    }

    let sections = split_markdown_sections(content);
    debug!(target: "translate", chars = content.len(), sections = sections.len(), force_refresh, "section_cache split");
    if sections.len() <= 1 {
        return ai_provider::translate_text(config, content)
            .await
            .map_err(|e| e.to_string());
    }

    assemble_sections_with_cache(
        &sections,
        force_refresh,
        |section| {
            translation_cache::get_cached_translation_for_provider(
                TranslationKind::SkillSection,
                &config.target_language,
                section,
                provider_identity,
            )
            .map(|opt| opt.map(|cached| cached.translated_text))
            .map_err(|e| e.to_string())
        },
        |section, normalized| {
            translation_cache::upsert_translation_for_provider(
                TranslationKind::SkillSection,
                &config.target_language,
                section,
                normalized,
                Some("llm_section"),
                provider_identity,
            )
            .map_err(|e| e.to_string())
        },
        |_cached_text| Ok(()),
        |section| async move {
            ai_provider::translate_text(config, &section)
                .await
                .map_err(|e| e.to_string())
        },
    )
    .await
}

async fn translate_skill_stream_with_section_cache(
    window: &tauri::Window,
    request_id: &str,
    plan: &TranslationRoutePlan,
    attempt: &TranslationAttempt,
    config: &ai_provider::AiConfig,
    content: &str,
    force_refresh: bool,
    provider_identity: Option<&str>,
) -> Result<String, String> {
    let meta = AttemptEventMeta::from_attempt(Some(plan), Some(attempt));
    let sections = split_markdown_sections(content);
    if sections.len() <= 1 || !skill_should_split_translation_sections(config, content) {
        let mut on_delta = |delta: &str| -> anyhow::Result<()> {
            emit_translate_attempt_event(
                window,
                request_id,
                "delta",
                Some(delta.to_string()),
                None,
                &meta,
            )
            .map_err(anyhow::Error::msg)
        };
        return ai_provider::translate_text_streaming(config, content, &mut on_delta)
            .await
            .map_err(|e| e.to_string());
    }

    assemble_sections_with_cache(
        &sections,
        force_refresh,
        |section| {
            translation_cache::get_cached_translation_for_provider(
                TranslationKind::SkillSection,
                &config.target_language,
                section,
                provider_identity,
            )
            .map(|opt| opt.map(|cached| cached.translated_text))
            .map_err(|e| e.to_string())
        },
        |section, normalized| {
            translation_cache::upsert_translation_for_provider(
                TranslationKind::SkillSection,
                &config.target_language,
                section,
                normalized,
                Some("llm_section"),
                provider_identity,
            )
            .map_err(|e| e.to_string())
        },
        |cached_text| {
            emit_translate_attempt_event(
                window,
                request_id,
                "delta",
                Some(cached_text.to_string()),
                None,
                &meta,
            )
        },
        |section| {
            let meta = meta.clone();
            async move {
                let mut on_delta = |delta: &str| -> anyhow::Result<()> {
                    emit_translate_attempt_event(
                        window,
                        request_id,
                        "delta",
                        Some(delta.to_string()),
                        None,
                        &meta,
                    )
                    .map_err(anyhow::Error::msg)
                };
                ai_provider::translate_text_streaming(config, &section, &mut on_delta)
                    .await
                    .map_err(|e| e.to_string())
            }
        },
    )
    .await
}

// ── Tauri Commands ──────────────────────────────────────────────────

#[tauri::command]
pub async fn ai_translate_skill(
    content: String,
    force: Option<bool>,
    force_quality: Option<bool>,
) -> Result<String, String> {
    let config = ai_provider::load_config_async().await;
    let force_refresh = force.unwrap_or(false);
    let force_quality = force_quality.unwrap_or(false);
    let plan = router::build_markdown_route_plan(&config, force_quality)?;

    if plan.attempts.is_empty() {
        return Err(route_unavailable_error("markdown translation"));
    }

    if !force_refresh {
        if let Some((_attempt, cached)) = get_cached_route_translation(
            TranslationKind::Skill,
            &plan,
            &content,
            "ai_translate_skill",
        ) {
            return Ok(cached.translated_text);
        }
    }

    let _session = acquire_skill_translation_session(&content).await?;

    if !force_refresh {
        if let Some((_attempt, cached)) = get_cached_route_translation(
            TranslationKind::Skill,
            &plan,
            &content,
            "ai_translate_skill (after acquire)",
        ) {
            return Ok(cached.translated_text);
        }
    }

    let mut errors = Vec::new();
    for attempt in &plan.attempts {
        if !router::attempt_is_available(attempt).await {
            warn!(
                target: "translate",
                provider_id = %attempt.provider_id,
                fallback_hop = attempt.fallback_hop,
                "skipping markdown attempt because provider is cooling down"
            );
            errors.push(format!("{}: provider cooling down", attempt.provider_label));
            continue;
        }

        match execute_skill_attempt(&config, &plan, attempt, &content, force_refresh).await {
            Ok(result) => {
                router::record_attempt_success(attempt).await;
                if let Err(err) = store_translation_for_attempt(
                    TranslationKind::Skill,
                    &plan.target_language,
                    &content,
                    &result,
                    attempt,
                ) {
                    warn!(target: "translate", error = %err, "skill cache write failed");
                }
                return Ok(result);
            }
            Err(err) => {
                router::record_attempt_failure(attempt).await;
                warn!(
                    target: "translate",
                    provider_id = %attempt.provider_id,
                    provider_type = %attempt.provider_type.as_str(),
                    route_mode = "auto",
                    fallback_hop = attempt.fallback_hop,
                    error = %err,
                    "markdown attempt failed"
                );
                errors.push(format!("{}: {}", attempt.provider_label, err));
            }
        }
    }

    Err(format!(
        "All markdown translation routes failed: {}",
        errors.join(" | ")
    ))
}

#[tauri::command]
pub async fn ai_translate_skill_stream(
    window: tauri::Window,
    request_id: String,
    content: String,
    force: Option<bool>,
    force_quality: Option<bool>,
) -> Result<SkillTranslationPayload, String> {
    let config = ai_provider::load_config_async().await;
    let force_refresh = force.unwrap_or(false);
    let force_quality = force_quality.unwrap_or(false);
    let plan = router::build_markdown_route_plan(&config, force_quality)?;

    if plan.attempts.is_empty() {
        let message = route_unavailable_error("markdown translation");
        let _ = emit_translate_attempt_event(
            &window,
            &request_id,
            "error",
            None,
            Some(message.clone()),
            &AttemptEventMeta::from_attempt(Some(&plan), None),
        );
        return Err(message);
    }

    if !force_refresh {
        if let Some((attempt, cached)) = get_cached_route_translation(
            TranslationKind::Skill,
            &plan,
            &content,
            "ai_translate_skill_stream",
        ) {
            let meta = AttemptEventMeta::from_attempt(Some(&plan), Some(attempt));
            let provider = cached_provider_label(&cached, attempt);
            let _ = emit_translate_attempt_event(&window, &request_id, "start", None, None, &meta);
            let _ = emit_translate_attempt_event(
                &window,
                &request_id,
                "delta",
                Some(cached.translated_text.clone()),
                None,
                &meta,
            );
            let _ =
                emit_translate_attempt_event(&window, &request_id, "complete", None, None, &meta);
            let mut payload =
                build_skill_translation_payload(cached.translated_text, &plan, attempt);
            payload.provider = provider;
            return Ok(payload);
        }
    }

    let _session = acquire_skill_translation_session(&content).await?;

    if !force_refresh {
        if let Some((attempt, cached)) = get_cached_route_translation(
            TranslationKind::Skill,
            &plan,
            &content,
            "ai_translate_skill_stream (after acquire)",
        ) {
            let meta = AttemptEventMeta::from_attempt(Some(&plan), Some(attempt));
            let provider = cached_provider_label(&cached, attempt);
            let _ = emit_translate_attempt_event(&window, &request_id, "start", None, None, &meta);
            let _ = emit_translate_attempt_event(
                &window,
                &request_id,
                "delta",
                Some(cached.translated_text.clone()),
                None,
                &meta,
            );
            let _ =
                emit_translate_attempt_event(&window, &request_id, "complete", None, None, &meta);
            let mut payload =
                build_skill_translation_payload(cached.translated_text, &plan, attempt);
            payload.provider = provider;
            return Ok(payload);
        }
    }

    let mut errors = Vec::new();
    for attempt in &plan.attempts {
        let meta = AttemptEventMeta::from_attempt(Some(&plan), Some(attempt));
        if !router::attempt_is_available(attempt).await {
            warn!(
                target: "translate",
                provider_id = %attempt.provider_id,
                fallback_hop = attempt.fallback_hop,
                "skipping markdown stream attempt because provider is cooling down"
            );
            errors.push(format!("{}: provider cooling down", attempt.provider_label));
            continue;
        }

        let _ = emit_translate_attempt_event(&window, &request_id, "start", None, None, &meta);
        let result = match &attempt.engine {
            TranslationAttemptEngine::TranslationApi { .. } => {
                execute_skill_attempt(&config, &plan, attempt, &content, force_refresh)
                    .await
                    .map(|translated| (translated, false))
            }
            TranslationAttemptEngine::QualityAi {
                config: quality_config,
            } => crate::core::ai::mdtx_bridge::translate_skill_content(
                quality_config,
                &content,
                force_refresh,
            )
            .await
            .and_then(|translated| {
                validate_translated_text(&plan.target_language, &content, translated, attempt)
            })
            .map(|translated| (translated, false)),
        };

        match result {
            Ok((translated, streamed)) => {
                if !streamed {
                    let _ = emit_translate_attempt_event(
                        &window,
                        &request_id,
                        "delta",
                        Some(translated.clone()),
                        None,
                        &meta,
                    );
                }
                router::record_attempt_success(attempt).await;
                if let Err(err) = store_translation_for_attempt(
                    TranslationKind::Skill,
                    &plan.target_language,
                    &content,
                    &translated,
                    attempt,
                ) {
                    warn!(target: "translate", error = %err, "skill cache write failed");
                }
                let _ = emit_translate_attempt_event(
                    &window,
                    &request_id,
                    "complete",
                    None,
                    None,
                    &meta,
                );
                return Ok(build_skill_translation_payload(translated, &plan, attempt));
            }
            Err(err) => {
                router::record_attempt_failure(attempt).await;
                warn!(
                    target: "translate",
                    provider_id = %attempt.provider_id,
                    provider_type = %attempt.provider_type.as_str(),
                    route_mode = "auto",
                    fallback_hop = attempt.fallback_hop,
                    error = %err,
                    "markdown stream attempt failed"
                );
                let _ = emit_clear_delta(&window, &request_id, &meta);
                errors.push(format!("{}: {}", attempt.provider_label, err));
            }
        }
    }

    let message = format!(
        "All markdown translation routes failed: {}",
        errors.join(" | ")
    );
    let _ = emit_translate_attempt_event(
        &window,
        &request_id,
        "error",
        None,
        Some(message.clone()),
        &AttemptEventMeta::from_attempt(Some(&plan), None),
    );
    Err(message)
}



#[tauri::command]
pub async fn ai_translate_short_text_stream_with_source(
    window: tauri::Window,
    request_id: String,
    content: String,
    force_refresh: Option<bool>,
    force_ai: Option<bool>,
) -> Result<ShortTextTranslationPayload, String> {
    let force_quality = force_ai.unwrap_or(false);
    let force_refresh = force_refresh.unwrap_or(false);
    debug!(
        target: "translate",
        req = %request_id,
        force_refresh,
        force_quality,
        "short_text_stream ENTER"
    );
    let config = ai_provider::load_config_async().await;
    let plan = router::build_short_text_route_plan(&config, force_quality)?;

    if plan.attempts.is_empty() {
        let message = route_unavailable_error("short-text translation");
        let _ = emit_translate_attempt_event(
            &window,
            &request_id,
            "error",
            None,
            Some(message.clone()),
            &AttemptEventMeta::from_attempt(Some(&plan), None),
        );
        return Err(message);
    }

    if !force_refresh {
        if let Some((attempt, cached)) = get_cached_route_translation(
            TranslationKind::Short,
            &plan,
            &content,
            "short stream read",
        ) {
            let meta = AttemptEventMeta::from_attempt(Some(&plan), Some(attempt));
            let mut payload =
                build_short_text_payload(cached.translated_text.clone(), &plan, attempt);
            payload.provider = cached_provider_label(&cached, attempt);
            let _ = emit_translate_attempt_event(&window, &request_id, "start", None, None, &meta);
            let _ = emit_translate_attempt_event(
                &window,
                &request_id,
                "delta",
                Some(cached.translated_text),
                None,
                &meta,
            );
            let _ =
                emit_translate_attempt_event(&window, &request_id, "complete", None, None, &meta);
            return Ok(payload);
        }
    }

    let mut errors = Vec::new();
    for attempt in &plan.attempts {
        let meta = AttemptEventMeta::from_attempt(Some(&plan), Some(attempt));
        if !router::attempt_is_available(attempt).await {
            warn!(
                target: "translate",
                provider_id = %attempt.provider_id,
                fallback_hop = attempt.fallback_hop,
                "skipping short-text attempt because provider is cooling down"
            );
            errors.push(format!("{}: provider cooling down", attempt.provider_label));
            continue;
        }

        let _ = emit_translate_attempt_event(&window, &request_id, "start", None, None, &meta);
        match execute_short_attempt(&window, &request_id, &config, &plan, attempt, &content).await {
            Ok((translated, emitted_delta)) => {
                router::record_attempt_success(attempt).await;
                if let Err(err) = store_translation_for_attempt(
                    TranslationKind::Short,
                    &plan.target_language,
                    &content,
                    &translated,
                    attempt,
                ) {
                    warn!(target: "translate", error = %err, "short-text cache write failed");
                }
                if !emitted_delta {
                    let _ = emit_translate_attempt_event(
                        &window,
                        &request_id,
                        "delta",
                        Some(translated.clone()),
                        None,
                        &meta,
                    );
                }
                let _ = emit_translate_attempt_event(
                    &window,
                    &request_id,
                    "complete",
                    None,
                    None,
                    &meta,
                );
                return Ok(build_short_text_payload(translated, &plan, attempt));
            }
            Err(err) => {
                router::record_attempt_failure(attempt).await;
                warn!(
                    target: "translate",
                    provider_id = %attempt.provider_id,
                    provider_type = %attempt.provider_type.as_str(),
                    route_mode = "auto",
                    fallback_hop = attempt.fallback_hop,
                    error = %err,
                    "short-text attempt failed"
                );
                let _ = emit_clear_delta(&window, &request_id, &meta);
                errors.push(format!("{}: {}", attempt.provider_label, err));
            }
        }
    }

    let message = format!(
        "All short-text translation routes failed: {}",
        errors.join(" | ")
    );
    error!(target: "translate", error = %message, "short_text_stream failed");
    let _ = emit_translate_attempt_event(
        &window,
        &request_id,
        "error",
        None,
        Some(message.clone()),
        &AttemptEventMeta::from_attempt(Some(&plan), None),
    );
    Err(message)
}

#[tauri::command]
pub async fn ai_retranslate_short_text_stream_with_source(
    window: tauri::Window,
    request_id: String,
    content: String,
) -> Result<ShortTextTranslationPayload, String> {
    ai_translate_short_text_stream_with_source(window, request_id, content, Some(true), Some(true))
        .await
}

// ── Batch Processing ────────────────────────────────────────────────

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BatchProgressPayload {
    completed: usize,
    total: usize,
    current_name: String,
}

/// Max short descriptions per batch API call.
const BATCH_DESC_CHUNK_SIZE: usize = 10;

/// Path to the pending batch translation task file.
fn batch_translate_pending_path() -> std::path::PathBuf {
    crate::core::infra::paths::batch_translate_pending_path()
}

fn save_pending_batch(names: &[String]) {
    let path = batch_translate_pending_path();
    let _ = std::fs::write(&path, serde_json::to_string(names).unwrap_or_default());
}

fn clear_pending_batch() {
    let path = batch_translate_pending_path();
    let _ = std::fs::remove_file(&path);
}

/// Returns skill names from a previously interrupted batch, or empty vec.
#[tauri::command]
pub async fn check_pending_batch_translate() -> Result<Vec<String>, String> {
    let path = batch_translate_pending_path();
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            let names: Vec<String> = serde_json::from_str(&content).unwrap_or_default();
            Ok(names)
        }
        Err(_) => Ok(vec![]),
    }
}

#[tauri::command]
pub async fn ai_batch_process_skills(
    app: AppHandle,
    skill_names: Vec<String>,
) -> Result<(), String> {
    let summary_config = ensure_ai_config().await?;
    let translation_config = ai_provider::load_config_async().await;
    let (batch_translation_config, batch_quality_label, batch_quality_identity) =
        resolve_batch_quality_lane(&translation_config)?;
    let total = skill_names.len();
    info!(target: "translate", total, skills = ?&skill_names[..skill_names.len().min(5)], "ai_batch_process_skills ENTER");

    // Persist task so it can be resumed after restart.
    save_pending_batch(&skill_names);

    tauri::async_runtime::spawn(async move {
        use tauri::Emitter;
        let completed = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

        // ── Emit initial progress so UI shows up immediately ──────────
        let _ = app.emit(
            "ai://batch-progress",
            BatchProgressPayload {
                completed: 0,
                total,
                current_name: String::new(),
            },
        );

        // ── Collect all skill content upfront ──────────────────────────
        let mut skill_contents: Vec<(String, crate::core::skill::SkillContent)> = Vec::new();
        for name in &skill_names {
            match crate::commands::read_skill_content(name.clone()).await {
                Ok(content) => skill_contents.push((name.clone(), content)),
                Err(err) => {
                    warn!(target: "ai_batch", skill = %name, error = %err, "failed to read skill");
                    completed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
            }
        }

        // ── Phase 1: Batch short description translation ──────────────
        let mut desc_items: Vec<(usize, String)> = Vec::new();
        for (i, (_name, content)) in skill_contents.iter().enumerate() {
            if let Some(ref desc) = content.description {
                if desc.trim().is_empty() {
                    continue;
                }
                let cached = translation_cache::get_cached_translation_for_provider(
                    TranslationKind::Short,
                    &batch_translation_config.target_language,
                    desc,
                    Some(&batch_quality_identity),
                )
                .unwrap_or(None);
                let is_usable = cached.as_ref().map_or(false, |c| {
                    ai_provider::translation_looks_translated(
                        &batch_translation_config.target_language,
                        desc,
                        &c.translated_text,
                    )
                });
                if !is_usable {
                    desc_items.push((i, desc.clone()));
                }
            }
        }

        // Translate in chunks of BATCH_DESC_CHUNK_SIZE
        for chunk in desc_items.chunks(BATCH_DESC_CHUNK_SIZE) {
            let texts: Vec<&str> = chunk.iter().map(|(_, d)| d.as_str()).collect();
            match ai_provider::translate_short_texts_batch(&batch_translation_config, &texts).await {
                Ok(translations) => {
                    for (j, (_idx, original_desc)) in chunk.iter().enumerate() {
                        if let Some(translated) = translations.get(j) {
                            // Only cache if the result actually looks translated.
                            // This prevents partial Chinese + English hybrids from being cached.
                            let looks_valid = ai_provider::translation_looks_translated(
                                &batch_translation_config.target_language,
                                original_desc,
                                translated,
                            );
                            if translated.trim().is_empty() || !looks_valid {
                                debug!(target: "ai_batch", desc_len = original_desc.len(), "skipping cache — empty or not translated");
                            } else {
                                let _ = translation_cache::upsert_translation_for_provider(
                                    TranslationKind::Short,
                                    &batch_translation_config.target_language,
                                    original_desc,
                                    translated,
                                    Some(&batch_quality_label),
                                    Some(&batch_quality_identity),
                                );
                            }
                        }
                        // Emit progress after each description
                        let _ = app.emit("ai://translations-updated", ());
                        let _ = app; // keep borrow checker happy
                    }
                }
                Err(err) => {
                    error!(target: "ai_batch", error = %err, "batch desc translation failed");
                }
            }
        }

        // ── Phase 2: Parallel summary + SKILL.md per skill ────────────
        let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(3));
        let summary_config = std::sync::Arc::new(summary_config);
        let translation_config = std::sync::Arc::new(batch_translation_config);
        let batch_quality_label = std::sync::Arc::new(batch_quality_label);
        let batch_quality_identity = std::sync::Arc::new(batch_quality_identity);
        let app = std::sync::Arc::new(app);

        let mut handles = Vec::new();
        for (name, content) in skill_contents {
            let sem = semaphore.clone();
            let summary_cfg = summary_config.clone();
            let translation_cfg = translation_config.clone();
            let batch_quality_label = batch_quality_label.clone();
            let batch_quality_identity = batch_quality_identity.clone();
            let app_ref = app.clone();
            let completed_ref = completed.clone();

            let handle = tokio::spawn(async move {
                let _permit = sem.acquire().await;

                // Emit progress
                let done = completed_ref.load(std::sync::atomic::Ordering::Relaxed);
                let _ = app_ref.emit(
                    "ai://batch-progress",
                    BatchProgressPayload {
                        completed: done,
                        total,
                        current_name: name.clone(),
                    },
                );

                // 2a. Summary
                if !content.content.trim().is_empty() {
                    let cached = translation_cache::get_cached_translation(
                        TranslationKind::Summary,
                        &summary_cfg.target_language,
                        &content.content,
                    )
                    .unwrap_or(None);

                    if cached.is_none() {
                        if let Ok(summary) =
                            ai_provider::summarize_text(&summary_cfg, &content.content).await
                        {
                            let _ = translation_cache::upsert_translation(
                                TranslationKind::Summary,
                                &summary_cfg.target_language,
                                &content.content,
                                &summary,
                                Some("ai"),
                            );
                        }
                    }
                }

                // 2b. SKILL.md full content translation
                if !content.content.trim().is_empty() {
                    let cached = translation_cache::get_cached_translation_for_provider(
                        TranslationKind::Skill,
                        &translation_cfg.target_language,
                        &content.content,
                        Some(batch_quality_identity.as_str()),
                    )
                    .unwrap_or(None);

                    if cached.is_none() {
                        if let Ok(translated) = translate_skill_with_section_cache(
                            &translation_cfg,
                            &content.content,
                            false,
                            Some(batch_quality_identity.as_str()),
                        )
                        .await
                        {
                            let _ = translation_cache::upsert_translation_for_provider(
                                TranslationKind::Skill,
                                &translation_cfg.target_language,
                                &content.content,
                                &translated,
                                Some(batch_quality_label.as_str()),
                                Some(batch_quality_identity.as_str()),
                            );
                        }
                    }
                }

                completed_ref.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                crate::core::installed_skill::invalidate_cache();
                let _ = app_ref.emit("ai://translations-updated", ());
            });

            handles.push(handle);
        }

        // Wait for all parallel tasks
        for handle in handles {
            let _ = handle.await;
        }

        // Clear pending task file — batch is done.
        clear_pending_batch();

        // Final progress event
        let _ = app.emit(
            "ai://batch-progress",
            BatchProgressPayload {
                completed: total,
                total,
                current_name: String::new(),
            },
        );
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{assemble_sections_with_cache, split_markdown_sections};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    #[test]
    fn split_markdown_sections_ignores_headings_inside_fenced_code_blocks() {
        let content = "# Intro\nhello\n```md\n# not-a-heading\n```\n# Next\nworld\n";
        let sections = split_markdown_sections(content);
        assert_eq!(sections.len(), 2);
        assert!(sections[0].contains("# not-a-heading"));
        assert!(sections[1].starts_with("# Next"));
    }

    #[tokio::test]
    async fn assemble_sections_with_cache_full_hit_skips_translation() {
        let sections = vec!["# A\nalpha\n".to_string(), "# B\nbeta\n".to_string()];
        let mut cache = HashMap::new();
        cache.insert(sections[0].clone(), "# A\n甲\n".to_string());
        cache.insert(sections[1].clone(), "# B\n乙\n".to_string());

        let translate_calls = Arc::new(Mutex::new(Vec::<String>::new()));
        let store_calls = Arc::new(Mutex::new(Vec::<String>::new()));

        let result = assemble_sections_with_cache(
            &sections,
            false,
            |section| Ok(cache.get(section).cloned()),
            {
                let store_calls = Arc::clone(&store_calls);
                move |section, _translated| {
                    store_calls
                        .lock()
                        .expect("store lock")
                        .push(section.to_string());
                    Ok(())
                }
            },
            |_cached_text| Ok(()),
            {
                let translate_calls = Arc::clone(&translate_calls);
                move |section| {
                    let translate_calls = Arc::clone(&translate_calls);
                    let section_owned = section;
                    async move {
                        translate_calls
                            .lock()
                            .expect("translate lock")
                            .push(section_owned.clone());
                        Ok(format!("AI::{section_owned}"))
                    }
                }
            },
        )
        .await
        .expect("assemble success");

        assert_eq!(result, "# A\n甲\n# B\n乙\n");
        assert!(translate_calls.lock().expect("translate lock").is_empty());
        assert!(store_calls.lock().expect("store lock").is_empty());
    }

    #[tokio::test]
    async fn assemble_sections_with_cache_partial_hit_translates_missing_only() {
        let sections = vec!["# A\nalpha\n".to_string(), "# B\nbeta\n".to_string()];
        let mut cache = HashMap::new();
        cache.insert(sections[0].clone(), "# A\n甲\n".to_string());

        let translate_calls = Arc::new(Mutex::new(Vec::<String>::new()));
        let store_calls = Arc::new(Mutex::new(Vec::<String>::new()));

        let result = assemble_sections_with_cache(
            &sections,
            false,
            |section| Ok(cache.get(section).cloned()),
            {
                let store_calls = Arc::clone(&store_calls);
                move |section, translated| {
                    store_calls
                        .lock()
                        .expect("store lock")
                        .push(format!("{section}=>{translated}"));
                    Ok(())
                }
            },
            |_cached_text| Ok(()),
            {
                let translate_calls = Arc::clone(&translate_calls);
                move |section| {
                    let translate_calls = Arc::clone(&translate_calls);
                    let section_owned = section;
                    async move {
                        translate_calls
                            .lock()
                            .expect("translate lock")
                            .push(section_owned.clone());
                        Ok(format!("AI<{section_owned}>"))
                    }
                }
            },
        )
        .await
        .expect("assemble success");

        assert_eq!(translate_calls.lock().expect("translate lock").len(), 1);
        assert_eq!(
            translate_calls.lock().expect("translate lock")[0],
            "# B\nbeta\n"
        );
        assert_eq!(store_calls.lock().expect("store lock").len(), 1);
        assert!(result.contains("# A\n甲\n"));
        assert!(result.contains("AI<# B\nbeta\n>"));
    }

    #[tokio::test]
    async fn assemble_sections_with_cache_force_refresh_bypasses_cache() {
        let sections = vec!["# A\nalpha\n".to_string(), "# B\nbeta\n".to_string()];
        let mut cache = HashMap::new();
        cache.insert(sections[0].clone(), "# A\n甲\n".to_string());
        cache.insert(sections[1].clone(), "# B\n乙\n".to_string());

        let translate_calls = Arc::new(Mutex::new(Vec::<String>::new()));

        let result = assemble_sections_with_cache(
            &sections,
            true,
            |section| Ok(cache.get(section).cloned()),
            |_section, _translated| Ok(()),
            |_cached_text| Ok(()),
            {
                let translate_calls = Arc::clone(&translate_calls);
                move |section| {
                    let translate_calls = Arc::clone(&translate_calls);
                    let section_owned = section;
                    async move {
                        translate_calls
                            .lock()
                            .expect("translate lock")
                            .push(section_owned.clone());
                        Ok("T".to_string())
                    }
                }
            },
        )
        .await
        .expect("assemble success");

        assert_eq!(translate_calls.lock().expect("translate lock").len(), 2);
        assert_eq!(result, "T\nT\n");
    }
}
