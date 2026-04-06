use crate::core::ai_provider;
use crate::core::translation_cache::{self, TranslationKind};
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

/// Section-splitting is only worthwhile for documents large enough to benefit
/// from per-section caching.
const SECTION_SPLIT_MIN_CHARS: usize = 4_000;

pub(super) async fn translate_skill_with_section_cache(
    config: &ai_provider::AiConfig,
    content: &str,
    force_refresh: bool,
) -> Result<String, String> {
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

// ── Tauri Commands ──────────────────────────────────────────────────

#[tauri::command]
pub async fn ai_translate_skill(content: String, force: Option<bool>) -> Result<String, String> {
    let config = ensure_ai_config().await?;

    let force_refresh = force.unwrap_or(false);

    if !force_refresh {
        if let Some(cached) =
            get_cached_skill_translation(&config.target_language, &content, "ai_translate_skill")
        {
            return Ok(cached);
        }
    }

    let _session = acquire_skill_translation_session(&content).await?;

    if !force_refresh {
        if let Some(cached) = get_cached_skill_translation(
            &config.target_language,
            &content,
            "ai_translate_skill (after acquire)",
        ) {
            return Ok(cached);
        }
    }

    let result = translate_skill_with_section_cache(&config, &content, force_refresh).await?;

    let _ = translation_cache::upsert_translation(
        TranslationKind::Skill,
        &config.target_language,
        &content,
        &result,
        None,
    );

    Ok(result)
}

#[tauri::command]
pub async fn ai_translate_skill_stream(
    window: tauri::Window,
    request_id: String,
    content: String,
    force: Option<bool>,
) -> Result<String, String> {
    let config = ensure_ai_config().await?;

    let force_refresh = force.unwrap_or(false);

    let _ = emit_translate_stream_event(&window, &request_id, "start", None, None);

    if !force_refresh {
        if let Some(cached) = get_cached_skill_translation(
            &config.target_language,
            &content,
            "ai_translate_skill_stream",
        ) {
            let _ = emit_translate_stream_event(
                &window,
                &request_id,
                "delta",
                Some(cached.clone()),
                None,
            );
            let _ = emit_translate_stream_event(&window, &request_id, "complete", None, None);
            return Ok(cached);
        }
    }

    let _session = acquire_skill_translation_session(&content).await?;

    if !force_refresh {
        if let Some(cached) = get_cached_skill_translation(
            &config.target_language,
            &content,
            "ai_translate_skill_stream (after acquire)",
        ) {
            let _ = emit_translate_stream_event(
                &window,
                &request_id,
                "delta",
                Some(cached.clone()),
                None,
            );
            let _ = emit_translate_stream_event(&window, &request_id, "complete", None, None);
            return Ok(cached);
        }
    }

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
            let _ = translation_cache::upsert_translation(
                TranslationKind::Skill,
                &config.target_language,
                &content,
                &result,
                None,
            );
            let _ = emit_translate_stream_event(&window, &request_id, "complete", None, None);
            Ok(result)
        }
        Err(err) => {
            let _ =
                emit_translate_stream_event(&window, &request_id, "error", None, Some(err.clone()));
            Err(err)
        }
    }
}

#[tauri::command]
pub async fn get_mymemory_usage_stats() -> Result<super::MymemoryUsagePayload, String> {
    let stats = ai_provider::get_mymemory_usage_stats_async().await;
    Ok(super::MymemoryUsagePayload {
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
        super::ensure_ai_config().await?
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

    const SHORT_TEXT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);

    let translate_result = if requires_ai {
        tokio::time::timeout(
            SHORT_TEXT_TIMEOUT,
            ai_provider::translate_short_text_streaming(&config, &content, &mut on_delta),
        )
        .await
        .unwrap_or_else(|_| {
            warn!(target: "translate", "short text AI-only translation timed out after 45s");
            Err(anyhow::anyhow!("Translation timed out"))
        })
        .map(|result| (result, ai_provider::ShortTextSource::Ai))
    } else {
        tokio::time::timeout(
            SHORT_TEXT_TIMEOUT,
            ai_provider::translate_short_text_streaming_with_priority_source(
                &config,
                &content,
                &mut on_delta,
            ),
        )
        .await
        .unwrap_or_else(|_| {
            warn!(target: "translate", "short text priority translation timed out after 45s");
            Err(anyhow::anyhow!("Translation timed out"))
        })
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
        let mut desc_items: Vec<(usize, String)> = Vec::new();
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
