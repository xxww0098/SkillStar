use crate::core::ai_provider;
use crate::core::security_scan::ScannedFile;
use crate::core::translation_cache::{self, TranslationKind};
use serde::Serialize;
use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, LazyLock, Mutex};
use tauri::Emitter;
use tracing::{debug, error, info, warn};

/// Global concurrency limiter — at most 3 different skills translate at once.
static SKILL_TRANSLATION_GLOBAL: LazyLock<tokio::sync::Semaphore> =
    LazyLock::new(|| tokio::sync::Semaphore::new(3));

/// Per-content-hash locks so identical content serialises (prevents duplicate API calls)
/// while different content runs in parallel.
static SKILL_TRANSLATION_LOCKS: LazyLock<Mutex<HashMap<u64, Arc<tokio::sync::Semaphore>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AiStreamPayload {
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
}

#[derive(Clone, Serialize)]
pub struct MymemoryUsagePayload {
    total_chars_sent: u64,
    daily_chars_sent: u64,
    daily_reset_date: String,
    updated_at: String,
}

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

fn get_cached_skill_translation(
    target_language: &str,
    content: &str,
    log_context: &str,
) -> Option<String> {
    match translation_cache::get_cached_translation(
        TranslationKind::Skill,
        target_language,
        content,
    ) {
        Ok(Some(cached)) => {
            debug!(target: "translate", context = %log_context, lang = %target_language, content_len = content.len(), "skill cache HIT");
            Some(cached.translated_text)
        }
        Ok(None) => {
            debug!(target: "translate", context = %log_context, lang = %target_language, content_len = content.len(), "skill cache MISS");
            None
        }
        Err(err) => {
            error!(target: "translate", context = %log_context, error = %err, "skill cache read error");
            None
        }
    }
}

fn short_text_source_from_provider(source_provider: Option<&str>) -> &'static str {
    match source_provider {
        Some("mymemory") => "mymemory",
        _ => "ai",
    }
}

fn cached_short_translation_usable(
    cached: &translation_cache::CachedTranslation,
    requires_ai: bool,
) -> bool {
    !requires_ai || matches!(cached.source_provider.as_deref(), Some("ai"))
}

fn get_cached_short_translation(
    target_language: &str,
    content: &str,
    log_context: &str,
    requires_ai: bool,
) -> Option<ShortTextTranslationPayload> {
    match translation_cache::get_cached_translation(
        TranslationKind::Short,
        target_language,
        content,
    ) {
        Ok(Some(cached)) if cached_short_translation_usable(&cached, requires_ai) => {
            debug!(target: "translate", context = %log_context, provider = cached.source_provider.as_deref().unwrap_or("?"), "short cache HIT");
            Some(ShortTextTranslationPayload {
                text: cached.translated_text,
                source: short_text_source_from_provider(cached.source_provider.as_deref())
                    .to_string(),
            })
        }
        Ok(Some(_)) => {
            debug!(target: "translate", context = %log_context, requires_ai, "short cache SKIP (provider mismatch)");
            None
        }
        Ok(None) => {
            debug!(target: "translate", context = %log_context, "short cache MISS");
            None
        }
        Err(err) => {
            error!(target: "translate", context = %log_context, error = %err, "short cache read error");
            None
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

fn split_markdown_sections(content: &str) -> Vec<String> {
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

async fn assemble_sections_with_cache<FGet, FStore, FEmitCached, FTranslate, Fut>(
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
    // FnMut closures cannot be shared across spawned tasks, so we translate
    // sequentially. The real speed win comes from chunk-level parallelism
    // inside `translate_text` / `translate_text_streaming`.
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

/// Section-splitting is only worthwhile for documents large enough to benefit
/// from per-section caching.  Below this threshold, translate the whole
/// document in one request (which is faster because it avoids N round-trips).
const SECTION_SPLIT_MIN_CHARS: usize = 4_000;

async fn translate_skill_with_section_cache(
    config: &ai_provider::AiConfig,
    content: &str,
    force_refresh: bool,
) -> Result<String, String> {
    // Short-circuit: small documents go through a single request.
    if content.len() < SECTION_SPLIT_MIN_CHARS {
        debug!(target: "translate", chars = content.len(), threshold = SECTION_SPLIT_MIN_CHARS, "small doc → single request");
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
            translation_cache::get_cached_translation(
                TranslationKind::SkillSection,
                &config.target_language,
                section,
            )
            .map(|opt| opt.map(|cached| cached.translated_text))
            .map_err(|e| e.to_string())
        },
        |section, normalized| {
            translation_cache::upsert_translation(
                TranslationKind::SkillSection,
                &config.target_language,
                section,
                normalized,
                None,
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
    config: &ai_provider::AiConfig,
    content: &str,
    force_refresh: bool,
) -> Result<String, String> {
    // For small-to-medium documents, skip section splitting entirely.
    // Section splitting adds overhead (one API call per section), that
    // dominates for documents under the section split threshold.
    let sections = split_markdown_sections(content);
    if sections.len() <= 1 || content.len() < SECTION_SPLIT_MIN_CHARS {
        let mut on_delta = |delta: &str| -> anyhow::Result<()> {
            emit_translate_stream_event(window, request_id, "delta", Some(delta.to_string()), None)
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
            translation_cache::get_cached_translation(
                TranslationKind::SkillSection,
                &config.target_language,
                section,
            )
            .map(|opt| opt.map(|cached| cached.translated_text))
            .map_err(|e| e.to_string())
        },
        |section, normalized| {
            translation_cache::upsert_translation(
                TranslationKind::SkillSection,
                &config.target_language,
                section,
                normalized,
                None,
            )
            .map_err(|e| e.to_string())
        },
        |cached_text| {
            emit_translate_stream_event(
                window,
                request_id,
                "delta",
                Some(cached_text.to_string()),
                None,
            )
        },
        |section| async move {
            let mut on_delta = |delta: &str| -> anyhow::Result<()> {
                emit_translate_stream_event(
                    window,
                    request_id,
                    "delta",
                    Some(delta.to_string()),
                    None,
                )
                .map_err(anyhow::Error::msg)
            };
            ai_provider::translate_text_streaming(config, &section, &mut on_delta)
                .await
                .map_err(|e| e.to_string())
        },
    )
    .await
}

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
) -> Result<(), String> {
    emit_ai_stream_event(
        window,
        "ai://translate-stream",
        request_id,
        event,
        delta,
        message,
    )
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

#[tauri::command]
pub async fn get_ai_config() -> Result<ai_provider::AiConfig, String> {
    Ok(ai_provider::load_config_async().await)
}

#[tauri::command]
pub async fn save_ai_config(config: ai_provider::AiConfig) -> Result<(), String> {
    ai_provider::save_config_async(&config)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ai_translate_skill(
    content: String,
    force_refresh: Option<bool>,
) -> Result<String, String> {
    let force_refresh = force_refresh.unwrap_or(false);
    debug!(target: "translate", content_len = content.len(), force_refresh, "ai_translate_skill ENTER");
    let config = ensure_ai_config().await?;

    if !force_refresh {
        if let Some(cached) =
            get_cached_skill_translation(&config.target_language, &content, "skill read")
        {
            debug!(target: "translate", result_len = cached.len(), "ai_translate_skill → cached");
            return Ok(cached);
        }
    }

    debug!(target: "translate", "ai_translate_skill → acquiring session");
    let _session = acquire_skill_translation_session(&content).await?;
    debug!(target: "translate", "ai_translate_skill → session acquired");
    if !force_refresh {
        if let Some(cached) =
            get_cached_skill_translation(&config.target_language, &content, "skill read after wait")
        {
            debug!(target: "translate", result_len = cached.len(), "ai_translate_skill → cached after wait");
            return Ok(cached);
        }
    }

    debug!(target: "translate", "ai_translate_skill → calling AI");
    let translated = translate_skill_with_section_cache(&config, &content, force_refresh).await?;
    debug!(target: "translate", result_len = translated.len(), "ai_translate_skill → AI done");

    if let Err(err) = translation_cache::upsert_translation(
        TranslationKind::Skill,
        &config.target_language,
        &content,
        &translated,
        None,
    ) {
        warn!(target: "translate", error = %err, "skill cache write failed");
    }

    Ok(translated)
}

#[tauri::command]
pub async fn ai_translate_skill_stream(
    window: tauri::Window,
    request_id: String,
    content: String,
    force_refresh: Option<bool>,
) -> Result<String, String> {
    let force_refresh = force_refresh.unwrap_or(false);
    debug!(target: "translate", req = %request_id, content_len = content.len(), force_refresh, "ai_translate_skill_stream ENTER");
    let config = ensure_ai_config().await?;

    let _ = emit_translate_stream_event(&window, &request_id, "start", None, None);
    if !force_refresh {
        if let Some(cached) =
            get_cached_skill_translation(&config.target_language, &content, "skill stream read")
        {
            debug!(target: "translate", result_len = cached.len(), "ai_translate_skill_stream → cached");
            let _ = emit_translate_stream_event(&window, &request_id, "complete", None, None);
            return Ok(cached);
        }
    }

    debug!(target: "translate", "ai_translate_skill_stream → acquiring session");
    let _session = acquire_skill_translation_session(&content).await?;
    debug!(target: "translate", "ai_translate_skill_stream → session acquired");
    if !force_refresh {
        if let Some(cached) = get_cached_skill_translation(
            &config.target_language,
            &content,
            "skill stream read after wait",
        ) {
            debug!(target: "translate", "ai_translate_skill_stream → cached after wait");
            let _ = emit_translate_stream_event(&window, &request_id, "complete", None, None);
            return Ok(cached);
        }
    }

    debug!(target: "translate", "ai_translate_skill_stream → streaming from AI");
    match translate_skill_stream_with_section_cache(
        &window,
        &request_id,
        &config,
        &content,
        force_refresh,
    )
    .await
    {
        Ok(result) => {
            debug!(target: "translate", result_len = result.len(), "ai_translate_skill_stream → done");
            if let Err(err) = translation_cache::upsert_translation(
                TranslationKind::Skill,
                &config.target_language,
                &content,
                &result,
                None,
            ) {
                warn!(target: "translate", error = %err, "skill stream cache write failed");
            }
            let _ = emit_translate_stream_event(&window, &request_id, "complete", None, None);
            Ok(result)
        }
        Err(err) => {
            let message = err.to_string();
            error!(target: "translate", error = %message, "ai_translate_skill_stream failed");
            let _ = emit_translate_stream_event(
                &window,
                &request_id,
                "error",
                None,
                Some(message.clone()),
            );
            Err(message)
        }
    }
}

#[tauri::command]
pub async fn get_mymemory_usage_stats() -> Result<MymemoryUsagePayload, String> {
    let stats = ai_provider::get_mymemory_usage_stats_async().await;
    Ok(MymemoryUsagePayload {
        total_chars_sent: stats.total_chars_sent,
        daily_chars_sent: stats.daily_chars_sent,
        daily_reset_date: stats.daily_reset_date,
        updated_at: stats.updated_at,
    })
}

#[tauri::command]
pub async fn ai_translate_short_text_stream_with_source(
    window: tauri::Window,
    request_id: String,
    content: String,
    force_refresh: Option<bool>,
    force_ai: Option<bool>,
) -> Result<ShortTextTranslationPayload, String> {
    let requires_ai = force_ai.unwrap_or(false);
    let force_refresh = force_refresh.unwrap_or(false);
    debug!(target: "translate", req = %request_id, force_refresh, requires_ai, "short_text_stream ENTER");
    let config = if requires_ai {
        ensure_ai_config().await?
    } else {
        ai_provider::load_config_async().await
    };

    let _ = emit_translate_stream_event(&window, &request_id, "start", None, None);
    if !force_refresh {
        if let Some(cached) = get_cached_short_translation(
            &config.target_language,
            &content,
            "short stream read",
            requires_ai,
        ) {
            debug!(target: "translate", source = %cached.source, "short_text_stream → cached");
            let _ = emit_translate_stream_event(&window, &request_id, "complete", None, None);
            return Ok(cached);
        }
    }

    debug!(target: "translate", "short_text_stream → calling provider");
    let mut on_delta = |delta: &str| -> anyhow::Result<()> {
        emit_translate_stream_event(&window, &request_id, "delta", Some(delta.to_string()), None)
            .map_err(anyhow::Error::msg)
    };

    let translate_result = if requires_ai {
        ai_provider::translate_short_text_streaming(&config, &content, &mut on_delta)
            .await
            .map(|result| (result, ai_provider::ShortTextSource::Ai))
    } else {
        ai_provider::translate_short_text_streaming_with_priority_source(
            &config,
            &content,
            &mut on_delta,
        )
        .await
    };

    match translate_result {
        Ok((result, source)) => {
            debug!(target: "translate", source = source.as_str(), result_len = result.len(), "short_text_stream → done");
            if let Err(err) = translation_cache::upsert_translation(
                TranslationKind::Short,
                &config.target_language,
                &content,
                &result,
                Some(source.as_str()),
            ) {
                warn!(target: "translate", error = %err, "short stream cache write failed");
            }
            let _ = emit_translate_stream_event(&window, &request_id, "complete", None, None);
            Ok(ShortTextTranslationPayload {
                text: result,
                source: source.as_str().to_string(),
            })
        }
        Err(err) => {
            let message = err.to_string();
            error!(target: "translate", error = %message, "short_text_stream failed");
            let _ = emit_translate_stream_event(
                &window,
                &request_id,
                "error",
                None,
                Some(message.clone()),
            );
            Err(message)
        }
    }
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

#[tauri::command]
pub async fn ai_summarize_skill(content: String) -> Result<String, String> {
    let config = ensure_ai_config().await?;

    // Check cache
    if let Ok(Some(cached)) = translation_cache::get_cached_translation(
        TranslationKind::Summary,
        &config.target_language,
        &content,
    ) {
        return Ok(cached.translated_text);
    }

    let result = ai_provider::summarize_text(&config, &content)
        .await
        .map_err(|e| e.to_string())?;

    let _ = translation_cache::upsert_translation(
        TranslationKind::Summary,
        &config.target_language,
        &content,
        &result,
        Some("ai"),
    );

    Ok(result)
}

#[tauri::command]
pub async fn ai_summarize_skill_stream(
    window: tauri::Window,
    request_id: String,
    content: String,
) -> Result<String, String> {
    let config = ensure_ai_config().await?;

    let _ = emit_summarize_stream_event(&window, &request_id, "start", None, None);

    if let Ok(Some(cached)) = translation_cache::get_cached_translation(
        TranslationKind::Summary,
        &config.target_language,
        &content,
    ) {
        let _ = emit_summarize_stream_event(&window, &request_id, "complete", None, None);
        return Ok(cached.translated_text);
    }

    let mut on_delta = |delta: &str| -> anyhow::Result<()> {
        emit_summarize_stream_event(&window, &request_id, "delta", Some(delta.to_string()), None)
            .map_err(anyhow::Error::msg)
    };

    match ai_provider::summarize_text_streaming(&config, &content, &mut on_delta).await {
        Ok(result) => {
            let _ = translation_cache::upsert_translation(
                TranslationKind::Summary,
                &config.target_language,
                &content,
                &result,
                Some("ai"),
            );
            let _ = emit_summarize_stream_event(&window, &request_id, "complete", None, None);
            Ok(result)
        }
        Err(err) => {
            let message = err.to_string();
            let _ = emit_summarize_stream_event(
                &window,
                &request_id,
                "error",
                None,
                Some(message.clone()),
            );
            Err(message)
        }
    }
}

use tauri::AppHandle;

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
    crate::core::paths::batch_translate_pending_path()
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
    let config = ensure_ai_config().await?;
    let total = skill_names.len();
    info!(target: "translate", total, skills = ?&skill_names[..skill_names.len().min(5)], "ai_batch_process_skills ENTER");

    // Persist task so it can be resumed after restart.
    save_pending_batch(&skill_names);

    tauri::async_runtime::spawn(async move {
        use tauri::Emitter;
        let completed = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

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
        // Collect untranslated descriptions with their indices.
        let mut desc_items: Vec<(usize, String)> = Vec::new(); // (index_in_skill_contents, description)
        for (i, (_name, content)) in skill_contents.iter().enumerate() {
            if let Some(ref desc) = content.description {
                if desc.trim().is_empty() {
                    continue;
                }
                let cached = translation_cache::get_cached_translation(
                    TranslationKind::Short,
                    &config.target_language,
                    desc,
                )
                .unwrap_or(None);
                let is_usable = cached.as_ref().map_or(false, |c| {
                    matches!(c.source_provider.as_deref(), Some("ai"))
                });
                if !is_usable {
                    desc_items.push((i, desc.clone()));
                }
            }
        }

        // Translate in chunks of BATCH_DESC_CHUNK_SIZE
        for chunk in desc_items.chunks(BATCH_DESC_CHUNK_SIZE) {
            let texts: Vec<&str> = chunk.iter().map(|(_, d)| d.as_str()).collect();
            match ai_provider::translate_short_texts_batch(&config, &texts).await {
                Ok(translations) => {
                    for (j, (_idx, original_desc)) in chunk.iter().enumerate() {
                        if let Some(translated) = translations.get(j) {
                            if !translated.trim().is_empty() {
                                let _ = translation_cache::upsert_translation(
                                    TranslationKind::Short,
                                    &config.target_language,
                                    original_desc,
                                    translated,
                                    Some("ai"),
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
        let config = std::sync::Arc::new(config);
        let app = std::sync::Arc::new(app);

        let mut handles = Vec::new();
        for (name, content) in skill_contents {
            let sem = semaphore.clone();
            let cfg = config.clone();
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
                        &cfg.target_language,
                        &content.content,
                    )
                    .unwrap_or(None);

                    if cached.is_none() {
                        if let Ok(summary) =
                            ai_provider::summarize_text(&cfg, &content.content).await
                        {
                            let _ = translation_cache::upsert_translation(
                                TranslationKind::Summary,
                                &cfg.target_language,
                                &content.content,
                                &summary,
                                Some("ai"),
                            );
                        }
                    }
                }

                // 2b. SKILL.md full content translation
                if !content.content.trim().is_empty() {
                    let cached = translation_cache::get_cached_translation(
                        TranslationKind::Skill,
                        &cfg.target_language,
                        &content.content,
                    )
                    .unwrap_or(None);

                    if cached.is_none() {
                        if let Ok(translated) =
                            translate_skill_with_section_cache(&cfg, &content.content, false).await
                        {
                            let _ = translation_cache::upsert_translation(
                                TranslationKind::Skill,
                                &cfg.target_language,
                                &content.content,
                                &translated,
                                None,
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

#[tauri::command]
pub async fn ai_test_connection() -> Result<u64, String> {
    let config = ensure_ai_config().await?;
    ai_provider::test_connection(&config)
        .await
        .map_err(|e| e.to_string())
}

#[derive(serde::Deserialize)]
pub struct SkillMeta {
    pub name: String,
    pub description: String,
}

#[tauri::command]
pub async fn ai_pick_skills(
    prompt: String,
    skills: Vec<SkillMeta>,
) -> Result<ai_provider::SkillPickResponse, String> {
    let config = ensure_ai_config().await?;
    let candidates = skills
        .into_iter()
        .map(|skill| ai_provider::SkillPickCandidate {
            name: skill.name,
            description: skill.description,
        })
        .collect();

    ai_provider::pick_skills(&config, &prompt, candidates)
        .await
        .map_err(|e| e.to_string())
}

// ── Security Scan Commands ──────────────────────────────────────────

use crate::core::security_scan::{
    self, FileRole, FileScanResult, PreparedChunk, PreparedSkillScan, SecurityScanPolicy,
    SecurityScanReportFormat, SecurityScanResult,
};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tokio::sync::{Semaphore as TokioSemaphore, mpsc};

pub static CANCEL_SCAN: AtomicBool = AtomicBool::new(false);

#[tauri::command]
pub async fn cancel_security_scan() -> Result<(), String> {
    CANCEL_SCAN.store(true, Ordering::Relaxed);
    Ok(())
}

use crate::core::security_scan::ScanMode;

fn parse_scan_mode(mode: &str) -> ScanMode {
    match mode {
        "smart" => ScanMode::Smart,
        "deep" => ScanMode::Deep,
        "static" => ScanMode::Static,
        // Legacy "ai" maps to Smart for backward compatibility
        "ai" => ScanMode::Smart,
        _ => ScanMode::Static,
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SecurityScanPayload {
    request_id: String,
    event: String, // skill-start | file-start | skill-complete | progress | error | done
    skill_name: Option<String>,
    file_name: Option<String>,
    result: Option<SecurityScanResult>,
    scanned: Option<usize>,
    total: Option<usize>,
    skill_file_scanned: Option<usize>,
    skill_file_total: Option<usize>,
    skill_chunk_completed: Option<usize>,
    skill_chunk_total: Option<usize>,
    active_chunk_workers: Option<usize>,
    max_chunk_workers: Option<usize>,
    message: Option<String>,
    phase: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityScanEstimatePayload {
    requested_mode: String,
    effective_mode: String,
    total_skills: usize,
    total_files: usize,
    ai_eligible_files: usize,
    estimated_chunks: usize,
    estimated_api_calls: usize,
    estimated_total_chars: usize,
    chunk_char_limit: usize,
}

struct PreparedSkillExecution {
    prepared: PreparedSkillScan,
    fresh_file_results: Vec<FileScanResult>,
    worker_failures: Vec<(String, FileRole, String)>,
    next_chunk_index: usize,
    inflight_chunks: usize,
    completed_chunks: usize,
}

impl PreparedSkillExecution {
    fn pending_chunks(&self) -> usize {
        self.prepared
            .chunks
            .len()
            .saturating_sub(self.next_chunk_index)
    }
}

struct ChunkTaskOutcome {
    #[allow(dead_code)]
    skill_idx: usize,
    chunk: PreparedChunk,
    result: Result<Vec<FileScanResult>, String>,
}

/// Result of a parallel skill preparation task.
enum PrepResult {
    Ok {
        name: String,
        prepared: Box<PreparedSkillScan>,
    },
    Err {
        name: String,
        error: String,
    },
}

fn emit_scan_event(window: &tauri::Window, payload: SecurityScanPayload) {
    let _ = window.emit("ai://security-scan", payload);
}

fn effective_scan_mode(requested: ScanMode, config: &ai_provider::AiConfig) -> ScanMode {
    if requested.requires_ai() && config.enabled && !config.api_key.trim().is_empty() {
        requested
    } else {
        ScanMode::Static
    }
}

fn select_next_skill_index(states: &[(usize, usize)]) -> Option<usize> {
    let guaranteed = states
        .iter()
        .enumerate()
        .filter(|(_, (pending, inflight))| *pending > 0 && *inflight == 0)
        .max_by_key(|(idx, (pending, _))| (*pending, usize::MAX - idx));

    guaranteed
        .or_else(|| {
            states
                .iter()
                .enumerate()
                .filter(|(_, (pending, _))| *pending > 0)
                .max_by_key(|(idx, (pending, _))| (*pending, usize::MAX - idx))
        })
        .map(|(idx, _)| idx)
}

fn select_next_skill_for_chunk(skills: &[PreparedSkillExecution]) -> Option<usize> {
    let states = skills
        .iter()
        .map(|skill| (skill.pending_chunks(), skill.inflight_chunks))
        .collect::<Vec<_>>();
    select_next_skill_index(&states)
}

#[tauri::command]
pub async fn estimate_security_scan(
    skill_names: Vec<String>,
    mode: String,
) -> Result<SecurityScanEstimatePayload, String> {
    let config = ai_provider::load_config_async().await;
    let parsed_mode = parse_scan_mode(&mode);
    let effective_mode = effective_scan_mode(parsed_mode, &config);
    let resolved = ai_provider::resolve_scan_params(&config);
    let chunk_limit = resolved.chunk_char_limit;

    let hub_dir = crate::core::paths::hub_skills_dir();
    let target_names: Vec<String> = if skill_names.is_empty() {
        match std::fs::read_dir(&hub_dir) {
            Ok(entries) => entries
                .flatten()
                .filter(|e| e.path().is_dir())
                .filter_map(|e| e.file_name().to_str().map(String::from))
                .collect(),
            Err(_) => vec![],
        }
    } else {
        skill_names
    };

    let mut total_files = 0usize;
    let mut ai_eligible_files = 0usize;
    let mut estimated_chunks = 0usize;
    let mut estimated_api_calls = 0usize;
    let mut estimated_total_chars = 0usize;

    for name in &target_names {
        let skill_dir = hub_dir.join(name);
        let real_dir = std::fs::canonicalize(&skill_dir).unwrap_or(skill_dir.clone());
        if !real_dir.is_dir() {
            continue;
        }

        let (files, _) = security_scan::collect_scannable_files(&real_dir);
        if files.is_empty() {
            continue;
        }
        let classifications = security_scan::classify_files(&files);
        let estimate =
            security_scan::estimate_scan(&files, &classifications, effective_mode, chunk_limit);

        total_files += estimate.total_files;
        ai_eligible_files += estimate.ai_eligible_files;
        estimated_chunks += estimate.estimated_chunks;
        estimated_api_calls += estimate.estimated_api_calls;
        estimated_total_chars += estimate.estimated_total_chars;
    }

    Ok(SecurityScanEstimatePayload {
        requested_mode: parsed_mode.label().to_string(),
        effective_mode: effective_mode.label().to_string(),
        total_skills: target_names.len(),
        total_files,
        ai_eligible_files,
        estimated_chunks,
        estimated_api_calls,
        estimated_total_chars,
        chunk_char_limit: chunk_limit,
    })
}

type PreCollectedFiles = HashMap<String, (Vec<ScannedFile>, String)>;

struct PreparedScanState {
    cached_results: Vec<SecurityScanResult>,
    cached_skill_names: Vec<String>,
    needs_scan: Vec<String>,
    pre_collected_files: PreCollectedFiles,
}

struct PrepareScanArgs<'a> {
    force: bool,
    target_names: &'a [String],
    hub_dir: &'a std::path::Path,
    resolved_mode: ScanMode,
    config: &'a ai_provider::AiConfig,
    window: &'a Arc<tauri::Window>,
    request_id: &'a Arc<String>,
    scanned_count: &'a Arc<AtomicUsize>,
    total: usize,
    ai_concurrency: usize,
}

fn prepare_scan(args: PrepareScanArgs<'_>) -> PreparedScanState {
    let mut cached_results: Vec<SecurityScanResult> = Vec::new();
    let mut needs_scan: Vec<String> = Vec::new();
    let mut pre_collected_files: PreCollectedFiles = HashMap::new();
    let mut cached_skill_names: Vec<String> = Vec::new();

    CANCEL_SCAN.store(false, Ordering::Relaxed);

    if !args.force {
        for name in args.target_names {
            let skill_dir = args.hub_dir.join(name);
            let real_dir = std::fs::canonicalize(&skill_dir).unwrap_or(skill_dir.clone());
            if !real_dir.is_dir() {
                continue;
            }

            let (files, content_hash) = security_scan::collect_scannable_files(&real_dir);
            if let Some(cached) = security_scan::try_reuse_cached(
                name,
                args.resolved_mode,
                Some(&content_hash),
                &args.config.target_language,
            ) {
                security_scan::log_cached_skill_result(name, Some(&content_hash), &cached);
                let scanned = args.scanned_count.fetch_add(1, Ordering::Relaxed) + 1;
                emit_scan_event(
                    args.window,
                    SecurityScanPayload {
                        request_id: args.request_id.to_string(),
                        event: "skill-complete".to_string(),
                        skill_name: Some(name.clone()),
                        file_name: None,
                        result: Some(cached.clone()),
                        scanned: Some(scanned),
                        total: Some(args.total),
                        skill_file_scanned: Some(cached.files_scanned),
                        skill_file_total: Some(cached.files_scanned),
                        skill_chunk_completed: Some(cached.chunks_used),
                        skill_chunk_total: Some(cached.chunks_used),
                        active_chunk_workers: Some(0),
                        max_chunk_workers: Some(args.ai_concurrency),
                        message: Some("cached".to_string()),
                        phase: Some("done".to_string()),
                    },
                );
                cached_results.push(cached);
                cached_skill_names.push(name.clone());
                continue;
            }

            pre_collected_files.insert(name.clone(), (files, content_hash));
            needs_scan.push(name.clone());
        }
    } else {
        for name in args.target_names {
            let skill_dir = args.hub_dir.join(name);
            let real_dir = std::fs::canonicalize(&skill_dir).unwrap_or(skill_dir.clone());
            if real_dir.is_dir() {
                needs_scan.push(name.clone());
            }
        }
    }

    PreparedScanState {
        cached_results,
        cached_skill_names,
        needs_scan,
        pre_collected_files,
    }
}

struct AggregateScanArgs<'a> {
    window: &'a Arc<tauri::Window>,
    request_id: &'a Arc<String>,
    request_id_value: &'a str,
    requested_mode: &'a str,
    resolved_mode: ScanMode,
    force: bool,
    run_started_at: chrono::DateTime<chrono::Utc>,
    total: usize,
    ai_concurrency: usize,
    telemetry_enabled: bool,
    cached_skill_names: &'a [String],
    cached_results: Vec<SecurityScanResult>,
    scan_results: Vec<SecurityScanResult>,
    scan_errors: &'a [(String, String)],
}

fn aggregate_results(args: AggregateScanArgs<'_>) -> Vec<SecurityScanResult> {
    let mut all_results = args.cached_results;
    all_results.extend(args.scan_results);
    let run_finished_at = chrono::Utc::now();
    let log_path = security_scan::persist_scan_run_log(
        args.request_id_value,
        args.requested_mode,
        args.resolved_mode.label(),
        args.force,
        args.run_started_at,
        run_finished_at,
        args.total,
        args.cached_skill_names,
        &all_results,
        args.scan_errors,
    )
    .ok()
    .map(|p| p.to_string_lossy().to_string());

    if args.telemetry_enabled {
        if let Err(err) = security_scan::persist_scan_telemetry(
            args.request_id_value,
            args.requested_mode,
            args.resolved_mode.label(),
            args.force,
            args.run_started_at,
            run_finished_at,
            args.total,
            &all_results,
            args.scan_errors,
        ) {
            warn!(
                target: "security_scan",
                error = %err,
                "failed to persist scan telemetry"
            );
        }
    }

    emit_scan_event(
        args.window,
        SecurityScanPayload {
            request_id: args.request_id.to_string(),
            event: "done".to_string(),
            skill_name: None,
            file_name: None,
            result: None,
            scanned: Some(args.total),
            total: Some(args.total),
            skill_file_scanned: None,
            skill_file_total: None,
            skill_chunk_completed: None,
            skill_chunk_total: None,
            active_chunk_workers: Some(0),
            max_chunk_workers: Some(args.ai_concurrency),
            message: log_path,
            phase: Some("done".to_string()),
        },
    );

    all_results
}

/// Batch security scan: up to 4 skills processed concurrently, files within
/// each skill analyzed concurrently via sub-agent workers.
async fn run_ai_scan_pipeline(
    window: tauri::Window,
    request_id: String,
    skill_names: Vec<String>,
    force: bool,
    mode: String,
) -> Result<Vec<SecurityScanResult>, String> {
    let run_started_at = chrono::Utc::now();
    let requested_mode = mode.clone();
    let request_id_value = request_id.clone();
    let config = Arc::new(ai_provider::load_config_async().await);
    let telemetry_enabled = config.security_scan_telemetry_enabled;

    // Resolve skill directories
    let hub_dir = crate::core::paths::hub_skills_dir();
    let target_names: Vec<String> = if skill_names.is_empty() {
        // Scan all installed skills — is_dir() already follows symlinks
        match std::fs::read_dir(&hub_dir) {
            Ok(entries) => entries
                .flatten()
                .filter(|e| e.path().is_dir())
                .filter_map(|e| e.file_name().to_str().map(String::from))
                .collect(),
            Err(_) => vec![],
        }
    } else {
        skill_names
    };

    let total = target_names.len();
    let scanned_count = Arc::new(AtomicUsize::new(0));

    // Global AI concurrency semaphore — shared across all skill scans
    let resolved = ai_provider::resolve_scan_params(&config);
    let ai_concurrency = resolved.max_concurrent_requests.max(1) as usize;
    let ai_semaphore = Arc::new(TokioSemaphore::new(ai_concurrency));

    // Preparation concurrency: parallelize file I/O + static scan across skills
    let prep_semaphore = Arc::new(TokioSemaphore::new(ai_concurrency + 2));
    let (prep_tx, mut prep_rx) = mpsc::channel::<PrepResult>(ai_concurrency + 2);

    // Shared state across concurrent tasks
    let window = Arc::new(window);
    let request_id = Arc::new(request_id);
    let hub_dir = Arc::new(hub_dir);
    let parsed_mode = parse_scan_mode(&mode);
    let resolved_mode = effective_scan_mode(parsed_mode, &config);
    let PreparedScanState {
        cached_results,
        cached_skill_names,
        needs_scan,
        mut pre_collected_files,
    } = prepare_scan(PrepareScanArgs {
        force,
        target_names: &target_names,
        hub_dir: hub_dir.as_ref(),
        resolved_mode,
        config: config.as_ref(),
        window: &window,
        request_id: &request_id,
        scanned_count: &scanned_count,
        total,
        ai_concurrency,
    });

    let mut scan_results: Vec<SecurityScanResult> = Vec::new();
    let mut scan_errors: Vec<(String, String)> = Vec::new();

    // --- Phase 2: Streaming pipeline (no batch barriers) ---
    // Architecture: prepare skills in parallel, feed chunks into a shared
    // JoinSet of workers, finalize each skill as soon as all its chunks
    // complete.  The semaphore is the sole concurrency gate.
    //
    // Each execution tracks its owner skill_name so we can map outcomes back
    // even after Vec removals.
    let mut executions: Vec<PreparedSkillExecution> = Vec::new();
    let mut chunk_join_set: tokio::task::JoinSet<(String, ChunkTaskOutcome)> =
        tokio::task::JoinSet::new();
    let mut active_chunk_workers = 0usize;
    let mut skill_queue_idx = 0usize;
    let mut pending_prep: usize = 0; // in-flight preparation tasks

    loop {
        if CANCEL_SCAN.load(Ordering::Relaxed) {
            break;
        }

        // ── A. Spawn parallel preparation tasks ──────────────────────
        // File I/O + static scan + chunk building are CPU/IO-bound and
        // independent per skill, so we run them in parallel using
        // tokio::spawn, bounded by prep_semaphore.
        while skill_queue_idx < needs_scan.len()
            && (executions.len() + pending_prep) < ai_concurrency + 1
            && !CANCEL_SCAN.load(Ordering::Relaxed)
        {
            let name = needs_scan[skill_queue_idx].clone();
            skill_queue_idx += 1;

            let skill_dir = hub_dir.join(&name);
            let real_dir = std::fs::canonicalize(&skill_dir).unwrap_or(skill_dir);

            let cfg = (*config).clone();
            let progress_window = window.clone();
            let r_id = request_id.clone();
            let sn = name.clone();
            let on_progress = move |stage: &str, file_name: Option<&str>| {
                emit_scan_event(
                    &progress_window,
                    SecurityScanPayload {
                        request_id: r_id.to_string(),
                        event: if file_name.is_some() {
                            "file-start".to_string()
                        } else {
                            "progress".to_string()
                        },
                        skill_name: Some(sn.clone()),
                        file_name: file_name.map(String::from),
                        result: None,
                        scanned: None,
                        total: None,
                        skill_file_scanned: None,
                        skill_file_total: None,
                        skill_chunk_completed: None,
                        skill_chunk_total: None,
                        active_chunk_workers: Some(0),
                        max_chunk_workers: Some(ai_concurrency),
                        message: Some(stage.to_string()),
                        phase: Some(stage.to_string()),
                    },
                );
            };

            let pre_collected = pre_collected_files.remove(&name);
            let tx = prep_tx.clone();
            let sem = prep_semaphore.clone();

            pending_prep += 1;
            tokio::spawn(async move {
                let _permit = sem.acquire().await;
                let result = security_scan::prepare_skill_scan(
                    &cfg,
                    &name,
                    &real_dir,
                    resolved_mode,
                    Some(&on_progress),
                    pre_collected,
                )
                .await;
                let prep = match result {
                    Ok(prepared) => PrepResult::Ok {
                        name,
                        prepared: Box::new(prepared),
                    },
                    Err(e) => PrepResult::Err {
                        name,
                        error: e.to_string(),
                    },
                };
                let _ = tx.send(prep).await;
            });
        }

        // ── A2. Drain any completed preparation results ─────────────
        while let Ok(prep) = prep_rx.try_recv() {
            pending_prep = pending_prep.saturating_sub(1);
            match prep {
                PrepResult::Ok { name, prepared } => {
                    let prepared = *prepared;
                    let chunk_total = prepared.chunks.len();
                    let phase = if chunk_total > 0 {
                        "ai-analyze"
                    } else if prepared.actual_mode.requires_ai() {
                        "aggregate"
                    } else {
                        "static"
                    };
                    emit_scan_event(
                        &window,
                        SecurityScanPayload {
                            request_id: request_id.to_string(),
                            event: "skill-start".to_string(),
                            skill_name: Some(name.clone()),
                            file_name: None,
                            result: None,
                            scanned: Some(scanned_count.load(Ordering::Relaxed)),
                            total: Some(total),
                            skill_file_scanned: Some(prepared.files.len()),
                            skill_file_total: Some(prepared.files.len()),
                            skill_chunk_completed: Some(0),
                            skill_chunk_total: Some(chunk_total),
                            active_chunk_workers: Some(active_chunk_workers),
                            max_chunk_workers: Some(ai_concurrency),
                            message: None,
                            phase: Some(phase.to_string()),
                        },
                    );
                    executions.push(PreparedSkillExecution {
                        prepared,
                        fresh_file_results: Vec::new(),
                        worker_failures: Vec::new(),
                        next_chunk_index: 0,
                        inflight_chunks: 0,
                        completed_chunks: 0,
                    });
                }
                PrepResult::Err { name, error } => {
                    let scanned = scanned_count.fetch_add(1, Ordering::Relaxed) + 1;
                    emit_scan_event(
                        &window,
                        SecurityScanPayload {
                            request_id: request_id.to_string(),
                            event: "error".to_string(),
                            skill_name: Some(name.clone()),
                            file_name: None,
                            result: None,
                            scanned: Some(scanned),
                            total: Some(total),
                            skill_file_scanned: Some(0),
                            skill_file_total: Some(0),
                            skill_chunk_completed: Some(0),
                            skill_chunk_total: Some(0),
                            active_chunk_workers: Some(0),
                            max_chunk_workers: Some(ai_concurrency),
                            message: Some(error.clone()),
                            phase: Some("error".to_string()),
                        },
                    );
                    scan_errors.push((name, error));
                }
            }
        }

        // ── B. Dispatch pending chunks ──────────────────────────────
        while !CANCEL_SCAN.load(Ordering::Relaxed) && active_chunk_workers < ai_concurrency {
            let Some(skill_idx) = select_next_skill_for_chunk(&executions) else {
                break;
            };
            let execution = &mut executions[skill_idx];
            let chunk = execution.prepared.chunks[execution.next_chunk_index].clone();
            execution.next_chunk_index += 1;
            execution.inflight_chunks += 1;
            active_chunk_workers += 1;

            let owner = execution.prepared.skill_name.clone();

            emit_scan_event(
                &window,
                SecurityScanPayload {
                    request_id: request_id.to_string(),
                    event: "progress".to_string(),
                    skill_name: Some(owner.clone()),
                    file_name: chunk.chunk_paths.first().cloned(),
                    result: None,
                    scanned: None,
                    total: None,
                    skill_file_scanned: Some(execution.prepared.files.len()),
                    skill_file_total: Some(execution.prepared.files.len()),
                    skill_chunk_completed: Some(execution.completed_chunks),
                    skill_chunk_total: Some(execution.prepared.chunks.len()),
                    active_chunk_workers: Some(active_chunk_workers),
                    max_chunk_workers: Some(ai_concurrency),
                    message: Some(format!("chunk {}/{}", chunk.chunk_num, chunk.total_chunks)),
                    phase: Some("ai-analyze".to_string()),
                },
            );

            let cfg = config.clone();
            let log_ctx = execution.prepared.log_ctx.clone();
            let ai_sem = ai_semaphore.clone();
            let owner_name = owner.clone();

            chunk_join_set.spawn(async move {
                let permit = ai_sem.acquire_owned().await.map_err(|e| e.to_string());
                let result = match permit {
                    Ok(permit) => {
                        let r = security_scan::analyze_prepared_chunk(
                            &cfg,
                            &owner_name,
                            &chunk,
                            &log_ctx,
                        )
                        .await
                        .map_err(|e| e.to_string());
                        drop(permit);
                        r
                    }
                    Err(err) => Err(err),
                };
                (
                    owner,
                    ChunkTaskOutcome {
                        skill_idx,
                        chunk,
                        result,
                    },
                )
            });
        }

        // ── C. Finalize zero-chunk skills ──────────────────────────
        let mut to_finalize: Vec<usize> = Vec::new();
        for (idx, exec) in executions.iter().enumerate() {
            if exec.prepared.chunks.is_empty() && exec.inflight_chunks == 0 {
                to_finalize.push(idx);
            }
        }
        for &idx in to_finalize.iter().rev() {
            let execution = executions.remove(idx);
            let skill_name = execution.prepared.skill_name.clone();
            let file_total = execution.prepared.files.len();
            let cfg = config.clone();
            let ai_sem = ai_semaphore.clone();
            let cancelled = CANCEL_SCAN.load(Ordering::Relaxed);

            let result = security_scan::finalize_prepared_skill::<fn(&str, Option<&str>)>(
                &cfg,
                execution.prepared,
                execution.fresh_file_results,
                execution.worker_failures,
                cancelled,
                ai_sem.as_ref(),
                None,
            )
            .await
            .map_err(|e| e.to_string());

            let scanned = scanned_count.fetch_add(1, Ordering::Relaxed) + 1;
            match result {
                Ok(r) => {
                    emit_scan_event(
                        &window,
                        SecurityScanPayload {
                            request_id: request_id.to_string(),
                            event: "skill-complete".to_string(),
                            skill_name: Some(skill_name),
                            file_name: None,
                            result: Some(r.clone()),
                            scanned: Some(scanned),
                            total: Some(total),
                            skill_file_scanned: Some(file_total),
                            skill_file_total: Some(file_total),
                            skill_chunk_completed: Some(0),
                            skill_chunk_total: Some(0),
                            active_chunk_workers: Some(active_chunk_workers),
                            max_chunk_workers: Some(ai_concurrency),
                            message: None,
                            phase: Some("done".to_string()),
                        },
                    );
                    scan_results.push(r);
                }
                Err(msg) => {
                    emit_scan_event(
                        &window,
                        SecurityScanPayload {
                            request_id: request_id.to_string(),
                            event: "error".to_string(),
                            skill_name: Some(skill_name.clone()),
                            file_name: None,
                            result: None,
                            scanned: Some(scanned),
                            total: Some(total),
                            skill_file_scanned: Some(file_total),
                            skill_file_total: Some(file_total),
                            skill_chunk_completed: Some(0),
                            skill_chunk_total: Some(0),
                            active_chunk_workers: Some(active_chunk_workers),
                            max_chunk_workers: Some(ai_concurrency),
                            message: Some(msg.clone()),
                            phase: Some("error".to_string()),
                        },
                    );
                    scan_errors.push((skill_name, msg));
                }
            }
        }

        // ── D. Exit conditions ─────────────────────────────────────
        if active_chunk_workers == 0 && executions.is_empty() && pending_prep == 0 {
            if skill_queue_idx >= needs_scan.len() {
                break;
            }
            continue;
        }
        if active_chunk_workers == 0 && pending_prep == 0 {
            continue;
        }

        // ── E. Wait for next chunk completion or prep result ──────
        // Use select! so we don't stall when a preparation task finishes
        // while all chunk workers are idle.
        let chunk_fut = chunk_join_set.join_next();
        let prep_fut = prep_rx.recv();

        tokio::select! {
            biased; // prioritize chunk completions (AI workers are expensive)

            Some(joined) = chunk_fut, if active_chunk_workers > 0 => {
                let (owner_name, outcome) = joined.map_err(|e| e.to_string())?;
                active_chunk_workers = active_chunk_workers.saturating_sub(1);

                let Some(exec_idx) = executions
                    .iter()
                    .position(|e| e.prepared.skill_name == owner_name)
                else {
                    continue;
                };

                let execution = &mut executions[exec_idx];
                execution.inflight_chunks = execution.inflight_chunks.saturating_sub(1);
                execution.completed_chunks += 1;

                match outcome.result {
                    Ok(results) => {
                        execution.fresh_file_results.extend(results);
                    }
                    Err(err_msg) => {
                        emit_scan_event(
                            &window,
                            SecurityScanPayload {
                                request_id: request_id.to_string(),
                                event: "chunk-error".to_string(),
                                skill_name: Some(owner_name.clone()),
                                file_name: outcome.chunk.chunk_paths.first().cloned(),
                                result: None,
                                scanned: None,
                                total: None,
                                skill_file_scanned: None,
                                skill_file_total: None,
                                skill_chunk_completed: Some(execution.completed_chunks),
                                skill_chunk_total: Some(execution.prepared.chunks.len()),
                                active_chunk_workers: Some(active_chunk_workers),
                                max_chunk_workers: Some(ai_concurrency),
                                message: Some(format!(
                                    "Chunk {}/{} failed: {}",
                                    outcome.chunk.chunk_num, outcome.chunk.total_chunks, err_msg
                                )),
                                phase: Some("error".to_string()),
                            },
                        );
                        for path in &outcome.chunk.chunk_paths {
                            execution.fresh_file_results.push(FileScanResult {
                                file_path: path.clone(),
                                role: FileRole::General,
                                findings: vec![],
                                file_risk: security_scan::RiskLevel::Low,
                                tokens_hint: 0,
                            });
                            execution.worker_failures.push((
                                path.clone(),
                                FileRole::General,
                                err_msg.clone(),
                            ));
                        }
                    }
                }

                emit_scan_event(
                    &window,
                    SecurityScanPayload {
                        request_id: request_id.to_string(),
                        event: "progress".to_string(),
                        skill_name: Some(execution.prepared.skill_name.clone()),
                        file_name: outcome.chunk.chunk_paths.first().cloned(),
                        result: None,
                        scanned: None,
                        total: None,
                        skill_file_scanned: Some(execution.prepared.files.len()),
                        skill_file_total: Some(execution.prepared.files.len()),
                        skill_chunk_completed: Some(execution.completed_chunks),
                        skill_chunk_total: Some(execution.prepared.chunks.len()),
                        active_chunk_workers: Some(active_chunk_workers),
                        max_chunk_workers: Some(ai_concurrency),
                        message: Some(format!(
                            "chunk {}/{}",
                            outcome.chunk.chunk_num, outcome.chunk.total_chunks
                        )),
                        phase: Some("ai-analyze".to_string()),
                    },
                );

                // ── F. Finalize completed skill ────────────────────
                if executions[exec_idx].pending_chunks() == 0 && executions[exec_idx].inflight_chunks == 0 {
                    let execution = executions.remove(exec_idx);
                    let skill_name = execution.prepared.skill_name.clone();
                    let completed_chunks = execution.completed_chunks;
                    let total_chunks = execution.prepared.chunks.len();
                    let file_total = execution.prepared.files.len();

                    emit_scan_event(
                        &window,
                        SecurityScanPayload {
                            request_id: request_id.to_string(),
                            event: "progress".to_string(),
                            skill_name: Some(skill_name.clone()),
                            file_name: None,
                            result: None,
                            scanned: None,
                            total: None,
                            skill_file_scanned: Some(file_total),
                            skill_file_total: Some(file_total),
                            skill_chunk_completed: Some(completed_chunks),
                            skill_chunk_total: Some(total_chunks),
                            active_chunk_workers: Some(active_chunk_workers),
                            max_chunk_workers: Some(ai_concurrency),
                            message: Some("aggregating...".to_string()),
                            phase: Some("aggregate".to_string()),
                        },
                    );

                    let cfg = config.clone();
                    let ai_sem = ai_semaphore.clone();
                    let cancelled = CANCEL_SCAN.load(Ordering::Relaxed);

                    let result = security_scan::finalize_prepared_skill::<fn(&str, Option<&str>)>(
                        &cfg,
                        execution.prepared,
                        execution.fresh_file_results,
                        execution.worker_failures,
                        cancelled,
                        ai_sem.as_ref(),
                        None,
                    )
                    .await
                    .map_err(|e| e.to_string());

                    let scanned = scanned_count.fetch_add(1, Ordering::Relaxed) + 1;
                    match result {
                        Ok(r) => {
                            emit_scan_event(
                                &window,
                                SecurityScanPayload {
                                    request_id: request_id.to_string(),
                                    event: "skill-complete".to_string(),
                                    skill_name: Some(skill_name),
                                    file_name: None,
                                    result: Some(r.clone()),
                                    scanned: Some(scanned),
                                    total: Some(total),
                                    skill_file_scanned: Some(file_total),
                                    skill_file_total: Some(file_total),
                                    skill_chunk_completed: Some(completed_chunks),
                                    skill_chunk_total: Some(total_chunks),
                                    active_chunk_workers: Some(active_chunk_workers),
                                    max_chunk_workers: Some(ai_concurrency),
                                    message: None,
                                    phase: Some("done".to_string()),
                                },
                            );
                            scan_results.push(r);
                        }
                        Err(msg) => {
                            emit_scan_event(
                                &window,
                                SecurityScanPayload {
                                    request_id: request_id.to_string(),
                                    event: "error".to_string(),
                                    skill_name: Some(skill_name.clone()),
                                    file_name: None,
                                    result: None,
                                    scanned: Some(scanned),
                                    total: Some(total),
                                    skill_file_scanned: Some(file_total),
                                    skill_file_total: Some(file_total),
                                    skill_chunk_completed: Some(completed_chunks),
                                    skill_chunk_total: Some(total_chunks),
                                    active_chunk_workers: Some(active_chunk_workers),
                                    max_chunk_workers: Some(ai_concurrency),
                                    message: Some(msg.clone()),
                                    phase: Some("error".to_string()),
                                },
                            );
                            scan_errors.push((skill_name, msg));
                        }
                    }
                }
            }

            Some(prep) = prep_fut, if pending_prep > 0 => {
                pending_prep = pending_prep.saturating_sub(1);
                match prep {
                    PrepResult::Ok { name, prepared } => {
                        let prepared = *prepared;
                        let chunk_total = prepared.chunks.len();
                        let phase = if chunk_total > 0 {
                            "ai-analyze"
                        } else if prepared.actual_mode.requires_ai() {
                            "aggregate"
                        } else {
                            "static"
                        };
                        emit_scan_event(
                            &window,
                            SecurityScanPayload {
                                request_id: request_id.to_string(),
                                event: "skill-start".to_string(),
                                skill_name: Some(name.clone()),
                                file_name: None,
                                result: None,
                                scanned: Some(scanned_count.load(Ordering::Relaxed)),
                                total: Some(total),
                                skill_file_scanned: Some(prepared.files.len()),
                                skill_file_total: Some(prepared.files.len()),
                                skill_chunk_completed: Some(0),
                                skill_chunk_total: Some(chunk_total),
                                active_chunk_workers: Some(active_chunk_workers),
                                max_chunk_workers: Some(ai_concurrency),
                                message: None,
                                phase: Some(phase.to_string()),
                            },
                        );
                        executions.push(PreparedSkillExecution {
                            prepared,
                            fresh_file_results: Vec::new(),
                            worker_failures: Vec::new(),
                            next_chunk_index: 0,
                            inflight_chunks: 0,
                            completed_chunks: 0,
                        });
                    }
                    PrepResult::Err { name, error } => {
                        let scanned = scanned_count.fetch_add(1, Ordering::Relaxed) + 1;
                        emit_scan_event(
                            &window,
                            SecurityScanPayload {
                                request_id: request_id.to_string(),
                                event: "error".to_string(),
                                skill_name: Some(name.clone()),
                                file_name: None,
                                result: None,
                                scanned: Some(scanned),
                                total: Some(total),
                                skill_file_scanned: Some(0),
                                skill_file_total: Some(0),
                                skill_chunk_completed: Some(0),
                                skill_chunk_total: Some(0),
                                active_chunk_workers: Some(0),
                                max_chunk_workers: Some(ai_concurrency),
                                message: Some(error.clone()),
                                phase: Some("error".to_string()),
                            },
                        );
                        scan_errors.push((name, error));
                    }
                }
            }

            else => {
                break;
            }
        }
    }

    // Finalize remaining skills (cancelled mid-flight)
    let scan_cancelled = CANCEL_SCAN.load(Ordering::Relaxed);
    while let Some(execution) = executions.pop() {
        let skill_name = execution.prepared.skill_name.clone();
        let completed_chunks = execution.completed_chunks;
        let total_chunks = execution.prepared.chunks.len();
        let file_total = execution.prepared.files.len();
        let cfg = config.clone();
        let ai_sem = ai_semaphore.clone();

        let result = security_scan::finalize_prepared_skill::<fn(&str, Option<&str>)>(
            &cfg,
            execution.prepared,
            execution.fresh_file_results,
            execution.worker_failures,
            scan_cancelled,
            ai_sem.as_ref(),
            None,
        )
        .await
        .map_err(|e| e.to_string());

        let scanned = scanned_count.fetch_add(1, Ordering::Relaxed) + 1;
        match result {
            Ok(r) => {
                emit_scan_event(
                    &window,
                    SecurityScanPayload {
                        request_id: request_id.to_string(),
                        event: "skill-complete".to_string(),
                        skill_name: Some(skill_name),
                        file_name: None,
                        result: Some(r.clone()),
                        scanned: Some(scanned),
                        total: Some(total),
                        skill_file_scanned: Some(file_total),
                        skill_file_total: Some(file_total),
                        skill_chunk_completed: Some(completed_chunks),
                        skill_chunk_total: Some(total_chunks),
                        active_chunk_workers: Some(0),
                        max_chunk_workers: Some(ai_concurrency),
                        message: None,
                        phase: Some("done".to_string()),
                    },
                );
                scan_results.push(r);
            }
            Err(msg) => {
                emit_scan_event(
                    &window,
                    SecurityScanPayload {
                        request_id: request_id.to_string(),
                        event: "error".to_string(),
                        skill_name: Some(skill_name.clone()),
                        file_name: None,
                        result: None,
                        scanned: Some(scanned),
                        total: Some(total),
                        skill_file_scanned: Some(file_total),
                        skill_file_total: Some(file_total),
                        skill_chunk_completed: Some(completed_chunks),
                        skill_chunk_total: Some(total_chunks),
                        active_chunk_workers: Some(0),
                        max_chunk_workers: Some(ai_concurrency),
                        message: Some(msg.clone()),
                        phase: Some("error".to_string()),
                    },
                );
                scan_errors.push((skill_name, msg));
            }
        }
    }

    Ok(aggregate_results(AggregateScanArgs {
        window: &window,
        request_id: &request_id,
        request_id_value: &request_id_value,
        requested_mode: &requested_mode,
        resolved_mode,
        force,
        run_started_at,
        total,
        ai_concurrency,
        telemetry_enabled,
        cached_skill_names: &cached_skill_names,
        cached_results,
        scan_results,
        scan_errors: &scan_errors,
    }))
}

#[tauri::command]
pub async fn ai_security_scan(
    window: tauri::Window,
    request_id: String,
    skill_names: Vec<String>,
    force: bool,
    mode: String,
) -> Result<Vec<SecurityScanResult>, String> {
    run_ai_scan_pipeline(window, request_id, skill_names, force, mode).await
}

/// Load all cached scan results (used by skill cards for badge display).
#[tauri::command]
pub async fn get_cached_scan_results() -> Result<Vec<SecurityScanResult>, String> {
    let hub_dir = crate::core::paths::hub_skills_dir();
    Ok(security_scan::load_all_cached()
        .into_iter()
        .filter(|result| {
            let skill_path = hub_dir.join(&result.skill_name);
            skill_path.is_dir() || skill_path.is_symlink()
        })
        .collect())
}

/// Clear the security scan cache.
#[tauri::command]
pub async fn clear_security_scan_cache() -> Result<(), String> {
    security_scan::clear_cache().map_err(|e| e.to_string())?;
    security_scan::clear_logs().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_security_scan_logs(
    limit: Option<usize>,
) -> Result<Vec<security_scan::SecurityScanLogEntry>, String> {
    let limit = limit.unwrap_or(30).clamp(1, 200);
    Ok(security_scan::list_scan_log_entries(limit))
}

#[tauri::command]
pub async fn get_security_scan_log_dir() -> Result<String, String> {
    Ok(security_scan::scan_logs_dir().to_string_lossy().to_string())
}

#[tauri::command]
pub async fn get_security_scan_policy() -> Result<SecurityScanPolicy, String> {
    Ok(security_scan::get_policy())
}

#[tauri::command]
pub async fn save_security_scan_policy(policy: SecurityScanPolicy) -> Result<(), String> {
    security_scan::save_policy(&policy).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn export_security_scan_sarif(
    skill_names: Option<Vec<String>>,
    request_label: Option<String>,
) -> Result<String, String> {
    let hub_dir = crate::core::paths::hub_skills_dir();
    let mut results = security_scan::load_all_cached()
        .into_iter()
        .filter(|result| {
            let skill_path = hub_dir.join(&result.skill_name);
            skill_path.is_dir() || skill_path.is_symlink()
        })
        .collect::<Vec<_>>();

    if let Some(names) = skill_names {
        if !names.is_empty() {
            let requested: std::collections::HashSet<String> = names.into_iter().collect();
            results.retain(|result| requested.contains(&result.skill_name));
        }
    }

    let path = security_scan::export_sarif_report(&results, request_label.as_deref())
        .map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn export_security_scan_report(
    format: String,
    skill_names: Option<Vec<String>>,
    request_label: Option<String>,
) -> Result<String, String> {
    let hub_dir = crate::core::paths::hub_skills_dir();
    let mut results = security_scan::load_all_cached()
        .into_iter()
        .filter(|result| {
            let skill_path = hub_dir.join(&result.skill_name);
            skill_path.is_dir() || skill_path.is_symlink()
        })
        .collect::<Vec<_>>();

    if let Some(names) = skill_names {
        if !names.is_empty() {
            let requested: std::collections::HashSet<String> = names.into_iter().collect();
            results.retain(|result| requested.contains(&result.skill_name));
        }
    }

    let parsed_format = SecurityScanReportFormat::parse_loose(&format);
    let path = security_scan::export_scan_report(&results, parsed_format, request_label.as_deref())
        .map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::{assemble_sections_with_cache, select_next_skill_index, split_markdown_sections};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    #[test]
    fn scheduler_gives_first_lane_to_idle_skill_before_extra_lane() {
        let states = vec![(11, 1), (2, 0)];
        assert_eq!(select_next_skill_index(&states), Some(1));
    }

    #[test]
    fn scheduler_prefers_largest_backlog_once_each_skill_has_a_lane() {
        let states = vec![(12, 1), (5, 1), (2, 1)];
        assert_eq!(select_next_skill_index(&states), Some(0));
    }

    #[test]
    fn scheduler_returns_none_when_no_skill_has_pending_chunks() {
        let states = vec![(0, 0), (0, 1), (0, 0)];
        assert_eq!(select_next_skill_index(&states), None);
    }

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
        // Source sections end with newline, helper should preserve trailing newline.
        assert_eq!(result, "T\nT\n");
    }
}
