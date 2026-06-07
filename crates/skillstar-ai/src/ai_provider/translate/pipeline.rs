//! Translation orchestration: parallel batches, per-batch token budgets,
//! content cache, phase-level progress events.
//!
//! Speed strategy:
//!   - Moderate batches (~12 items / ~2200 chars) → fewer high-overhead
//!     provider round-trips while preserving fallback granularity
//!   - Fan out via `FuturesUnordered`; the AI semaphore in `acquire_ai_request_permit`
//!     naturally caps concurrent in-flight to `max_concurrent_requests` (default 4)
//!   - Per-batch `max_tokens` is target-aware (`input + headroom` for CJK,
//!     larger expansion budget elsewhere) to avoid KV-cache over-allocation
//!     penalty noted in `chat_completion_capped`
//!   - Cache hits short-circuit before the LLM call (no permit acquired, no network)
//!   - Skip-when-already-target via cheap CJK char-ratio heuristic
//!
//! Progress is reported via `PipelinePhase` callbacks the caller wires to a
//! Tauri event so the UI's `TranslationWaitBanner` can fill its progress bar.

use anyhow::{Context, Result};
use futures::stream::{FuturesUnordered, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::warn;

use super::ast::{self, TranslatableNode};
use super::batch::{self, BatchConfig, XmlBatch};
use super::cache;
use crate::ai_provider::{
    AiConfig, ChatCompletionUsage, chat_completion_capped_with_usage, language_display_name,
    request_timeout_duration, resolve_runtime_config,
};

const TRANSLATE_PROMPT: &str = include_str!("../../../../../src-tauri/prompts/ai/translate.md");
const MAX_CHARS_PER_SEGMENT: usize = 1200;

/// Pipeline phase reported to the UI.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PipelinePhase {
    Prepare,
    Translate,
    Finalize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineProgress {
    pub phase: PipelinePhase,
    pub current: u32,
    pub total: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationMetrics {
    pub model: String,
    pub target_language: String,
    pub elapsed_ms: u64,
    pub input_chars: usize,
    pub output_chars: usize,
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    pub total_tokens: Option<u32>,
    pub tps: Option<f64>,
    pub cache_hit: bool,
    pub model_calls: u32,
}

#[derive(Debug, Clone)]
pub struct TranslateResult {
    pub content: String,
    pub metrics: TranslationMetrics,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct TranslateOptions {
    /// Bypass backend translation caches and call the configured model again.
    pub force_refresh: bool,
}

#[derive(Debug, Clone)]
struct TranslationUnit {
    id: usize,
    node_id: usize,
    order: usize,
    text: String,
}

#[derive(Debug, Clone, Default)]
struct UsageAccumulator {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
    has_prompt_tokens: bool,
    has_completion_tokens: bool,
    has_total_tokens: bool,
    model_calls: u32,
}

impl UsageAccumulator {
    fn add_batch(&mut self, batch: &BatchTranslation) {
        self.model_calls += batch.model_calls;
        if batch.has_prompt_tokens {
            self.prompt_tokens += batch.prompt_tokens;
            self.has_prompt_tokens = true;
        }
        if batch.has_completion_tokens {
            self.completion_tokens += batch.completion_tokens;
            self.has_completion_tokens = true;
        }
        if batch.has_total_tokens {
            self.total_tokens += batch.total_tokens;
            self.has_total_tokens = true;
        }
    }
}

#[derive(Debug, Clone, Default)]
struct BatchTranslation {
    segments: HashMap<usize, String>,
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
    has_prompt_tokens: bool,
    has_completion_tokens: bool,
    has_total_tokens: bool,
    model_calls: u32,
}

impl BatchTranslation {
    fn from_segments(segments: HashMap<usize, String>, usage: Option<ChatCompletionUsage>) -> Self {
        let mut out = Self {
            segments,
            model_calls: 1,
            ..Self::default()
        };
        out.add_usage(usage.as_ref());
        out
    }

    fn add_usage(&mut self, usage: Option<&ChatCompletionUsage>) {
        if let Some(usage) = usage {
            if let Some(tokens) = usage.prompt_tokens {
                self.prompt_tokens += tokens;
                self.has_prompt_tokens = true;
            }
            if let Some(tokens) = usage.completion_tokens {
                self.completion_tokens += tokens;
                self.has_completion_tokens = true;
            }
            if let Some(tokens) = usage.total_tokens {
                self.total_tokens += tokens;
                self.has_total_tokens = true;
            }
        }
    }

    fn merge_success(&mut self, other: BatchTranslation) {
        self.model_calls += other.model_calls;
        self.segments.extend(other.segments);
        if other.has_prompt_tokens {
            self.prompt_tokens += other.prompt_tokens;
            self.has_prompt_tokens = true;
        }
        if other.has_completion_tokens {
            self.completion_tokens += other.completion_tokens;
            self.has_completion_tokens = true;
        }
        if other.has_total_tokens {
            self.total_tokens += other.total_tokens;
            self.has_total_tokens = true;
        }
    }
}

/// Translate a SKILL.md, preserving frontmatter verbatim and using AST
/// substitution for the body.
///
/// `on_progress` is invoked for every phase transition + each batch completion.
/// Errors from `on_progress` are logged but never abort translation.
pub async fn translate_skill<F>(config: &AiConfig, markdown: &str, on_progress: F) -> Result<String>
where
    F: FnMut(PipelineProgress) + Send,
{
    translate_skill_with_options(config, markdown, TranslateOptions::default(), on_progress).await
}

pub async fn translate_skill_with_options<F>(
    config: &AiConfig,
    markdown: &str,
    options: TranslateOptions,
    on_progress: F,
) -> Result<String>
where
    F: FnMut(PipelineProgress) + Send,
{
    translate_skill_with_report(config, markdown, options, on_progress)
        .await
        .map(|result| result.content)
}

pub async fn translate_skill_with_report<F>(
    config: &AiConfig,
    markdown: &str,
    options: TranslateOptions,
    mut on_progress: F,
) -> Result<TranslateResult>
where
    F: FnMut(PipelineProgress) + Send,
{
    let started = Instant::now();
    on_progress(PipelineProgress {
        phase: PipelinePhase::Prepare,
        current: 0,
        total: 1,
    });

    // ── Resolve runtime config once (resolves provider_ref) ──────────
    let resolved = resolve_runtime_config(config).context("resolving AI config")?;
    let effective_model = resolved.model.clone();
    let target_lang = resolved.target_language.clone();
    let target_lang_display = language_display_name(&target_lang).to_string();

    // ── Whole-document fast paths ────────────────────────────────────
    if should_skip_translation(markdown, &target_lang) {
        on_progress(PipelineProgress {
            phase: PipelinePhase::Finalize,
            current: 1,
            total: 1,
        });
        return Ok(build_translate_result(
            markdown.to_string(),
            markdown,
            &effective_model,
            &target_lang,
            started,
            &UsageAccumulator::default(),
            false,
        ));
    }

    if !options.force_refresh
        && let Some(hit) = cache::get_document(markdown, &target_lang, &effective_model)
    {
        on_progress(PipelineProgress {
            phase: PipelinePhase::Translate,
            current: 1,
            total: 1,
        });
        on_progress(PipelineProgress {
            phase: PipelinePhase::Finalize,
            current: 1,
            total: 1,
        });
        return Ok(build_translate_result(
            hit,
            markdown,
            &effective_model,
            &target_lang,
            started,
            &UsageAccumulator::default(),
            true,
        ));
    }

    // ── Split frontmatter (preserve verbatim) ────────────────────────
    let (frontmatter, body) = split_frontmatter(markdown);

    // ── Parse AST + extract translatable nodes ───────────────────────
    let nodes = ast::extract(body);
    let total_node_count = nodes.len();

    on_progress(PipelineProgress {
        phase: PipelinePhase::Prepare,
        current: 1,
        total: 1,
    });

    if nodes.is_empty() {
        on_progress(PipelineProgress {
            phase: PipelinePhase::Finalize,
            current: 1,
            total: 1,
        });
        return Ok(build_translate_result(
            markdown.to_string(),
            markdown,
            &effective_model,
            &target_lang,
            started,
            &UsageAccumulator::default(),
            false,
        ));
    }

    // ── Cache lookup pass — drop cached nodes from the work list ────
    let mut cached_translations: HashMap<usize, String> = HashMap::new();
    let mut chunk_totals: HashMap<usize, usize> = HashMap::new();
    let mut chunk_translations: HashMap<(usize, usize), String> = HashMap::new();
    let mut work_units: Vec<TranslationUnit> = Vec::with_capacity(nodes.len());
    let mut next_unit_id = 1usize;

    for node in nodes {
        if should_skip_translation(&node.text, &target_lang) {
            cached_translations.insert(node.id, node.text);
        } else if !options.force_refresh
            && let Some(hit) = cache::get(&node.text, &target_lang, &effective_model)
        {
            cached_translations.insert(node.id, hit);
        } else {
            let chunks = split_text_for_translation(&node.text, MAX_CHARS_PER_SEGMENT);
            chunk_totals.insert(node.id, chunks.len());
            for (order, chunk) in chunks.into_iter().enumerate() {
                if !options.force_refresh
                    && let Some(hit) = cache::get(&chunk, &target_lang, &effective_model)
                {
                    chunk_translations.insert((node.id, order), hit);
                    continue;
                }
                work_units.push(TranslationUnit {
                    id: next_unit_id,
                    node_id: node.id,
                    order,
                    text: chunk,
                });
                next_unit_id += 1;
            }
        }
    }
    cached_translations.extend(assemble_completed_chunks(
        &chunk_totals,
        &chunk_translations,
    ));

    // Everything cached? Skip the LLM entirely.
    if work_units.is_empty() {
        on_progress(PipelineProgress {
            phase: PipelinePhase::Translate,
            current: 1,
            total: 1,
        });
        on_progress(PipelineProgress {
            phase: PipelinePhase::Finalize,
            current: 0,
            total: 1,
        });
        let rendered = ast::replace_and_render(body, &cached_translations);
        let result = reattach_frontmatter(frontmatter, &rendered);
        cache::insert_document(markdown, &target_lang, &effective_model, result.clone());
        on_progress(PipelineProgress {
            phase: PipelinePhase::Finalize,
            current: 1,
            total: 1,
        });
        return Ok(build_translate_result(
            result,
            markdown,
            &effective_model,
            &target_lang,
            started,
            &UsageAccumulator::default(),
            true,
        ));
    }

    // ── Pack into XML batches ────────────────────────────────────────
    let batch_nodes: Vec<TranslatableNode> = work_units
        .iter()
        .map(|unit| TranslatableNode {
            id: unit.id,
            text: unit.text.clone(),
        })
        .collect();
    let batches = batch::pack(&batch_nodes, &BatchConfig::default());
    let total_batches = batches.len() as u32;

    let progress = Arc::new(Mutex::new(0u32));

    // ── Fan out batch translations ───────────────────────────────────
    on_progress(PipelineProgress {
        phase: PipelinePhase::Translate,
        current: 0,
        total: total_batches.max(1),
    });

    let mut futures = FuturesUnordered::new();
    let work_by_id: Arc<HashMap<usize, TranslationUnit>> = Arc::new(
        work_units
            .iter()
            .cloned()
            .map(|unit| (unit.id, unit))
            .collect(),
    );
    for b in batches {
        let cfg = resolved.clone();
        let target_display = target_lang_display.clone();
        let target_code = target_lang.clone();
        let model = effective_model.clone();
        let fallback_nodes = Arc::clone(&work_by_id);
        futures.push(async move {
            let result = translate_batch_with_fallback(
                &cfg,
                &target_display,
                &target_code,
                &b,
                &fallback_nodes,
            )
            .await;
            (b, target_code, model, result)
        });
    }

    let mut usage = UsageAccumulator::default();
    while let Some((batch_info, target_code, model, result)) = futures.next().await {
        let batch_result = result
            .with_context(|| format!("translation batch (ids={:?}) failed", batch_info.ids))?;
        usage.add_batch(&batch_result);

        // Map results back, and warm the cache so re-runs are instant.
        let id_to_text: HashMap<usize, String> = batch_info
            .ids
            .iter()
            .filter_map(|id| batch_result.segments.get(id).map(|t| (*id, t.clone())))
            .collect();

        for id in &batch_info.ids {
            if let (Some(translated), Some(unit)) = (id_to_text.get(id), work_by_id.get(id)) {
                cache::insert(&unit.text, &target_code, &model, translated.clone());
                chunk_translations.insert((unit.node_id, unit.order), translated.clone());
            }
        }

        let mut p = progress.lock().expect("progress lock");
        *p += 1;
        on_progress(PipelineProgress {
            phase: PipelinePhase::Translate,
            current: *p,
            total: total_batches.max(1),
        });
    }

    // ── Merge cache + LLM translations, render back ──────────────────
    on_progress(PipelineProgress {
        phase: PipelinePhase::Finalize,
        current: 0,
        total: 1,
    });

    let mut all = cached_translations;
    all.extend(assemble_completed_chunks(
        &chunk_totals,
        &chunk_translations,
    ));

    let rendered = ast::replace_and_render(body, &all);
    let final_md = reattach_frontmatter(frontmatter, &rendered);
    if all.len() == total_node_count {
        cache::insert_document(markdown, &target_lang, &effective_model, final_md.clone());
    }

    on_progress(PipelineProgress {
        phase: PipelinePhase::Finalize,
        current: 1,
        total: 1,
    });

    Ok(build_translate_result(
        final_md,
        markdown,
        &effective_model,
        &target_lang,
        started,
        &usage,
        false,
    ))
}

fn build_translate_result(
    content: String,
    source_markdown: &str,
    model: &str,
    target_language: &str,
    started: Instant,
    usage: &UsageAccumulator,
    cache_hit: bool,
) -> TranslateResult {
    let elapsed_ms = started.elapsed().as_millis() as u64;
    let completion_tokens = usage
        .has_completion_tokens
        .then_some(usage.completion_tokens);
    let tps = completion_tokens.and_then(|tokens| {
        if elapsed_ms == 0 {
            None
        } else {
            Some(tokens as f64 * 1000.0 / elapsed_ms as f64)
        }
    });

    let metrics = TranslationMetrics {
        model: model.to_string(),
        target_language: target_language.to_string(),
        elapsed_ms,
        input_chars: source_markdown.chars().count(),
        output_chars: content.chars().count(),
        prompt_tokens: usage.has_prompt_tokens.then_some(usage.prompt_tokens),
        completion_tokens,
        total_tokens: usage.has_total_tokens.then_some(usage.total_tokens),
        tps,
        cache_hit,
        model_calls: usage.model_calls,
    };

    TranslateResult { content, metrics }
}

async fn translate_one_batch(
    config: &AiConfig,
    target_lang_display: &str,
    target_lang_code: &str,
    batch: &XmlBatch,
) -> Result<BatchTranslation> {
    let system_prompt = TRANSLATE_PROMPT.replace("{target_lang}", target_lang_display);
    let user_prompt = format!(
        "Translate the following XML batch into {target_lang_display}.\n\
         Preserve every <seg> tag exactly (id attribute and tag boundaries).\n\
         Return ONLY the XML — no prose, no code fences.\n\n{xml}",
        xml = batch.xml,
    );

    // English → CJK usually contracts, and over-large max_tokens slows
    // providers that pre-allocate KV cache or spend unused reasoning budget.
    // Non-CJK targets keep the more conservative expansion allowance.
    // Floor at 512 so even tiny single-segment batches have headroom.
    let output_budget = if is_cjk_target(target_lang_code) {
        batch.input_chars + 384
    } else {
        (batch.input_chars * 2) + 256
    };
    let max_tokens = output_budget.clamp(512, 32_768) as u32;

    let output =
        chat_completion_capped_with_usage(config, &system_prompt, &user_prompt, max_tokens).await?;
    let raw = output.content;

    let parsed = batch::parse(&raw, &batch.ids).map_err(|e| {
        anyhow::anyhow!(
            "translation response could not be parsed (ids={:?}): {e}\n\nResponse: {}",
            batch.ids,
            raw.chars().take(500).collect::<String>()
        )
    })?;
    Ok(BatchTranslation::from_segments(parsed, output.usage))
}

async fn translate_one_batch_guarded(
    config: &AiConfig,
    target_lang_display: &str,
    target_lang_code: &str,
    batch: &XmlBatch,
) -> Result<BatchTranslation> {
    let timeout_duration = translation_batch_timeout(config);
    match tokio::time::timeout(
        timeout_duration,
        translate_one_batch(config, target_lang_display, target_lang_code, batch),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => anyhow::bail!(
            "translation batch timed out after {}s",
            timeout_duration.as_secs()
        ),
    }
}

fn translation_batch_timeout(config: &AiConfig) -> Duration {
    // The HTTP client already has its own timeout, but some OpenAI-compatible
    // gateways can keep a request pending longer than expected. Keep a pipeline
    // watchdog so one slow batch cannot stall an entire large SKILL.md.
    let base = request_timeout_duration(config).as_secs().clamp(20, 120);
    Duration::from_secs(base + 10)
}

async fn translate_batch_with_fallback(
    config: &AiConfig,
    target_lang_display: &str,
    target_lang_code: &str,
    batch: &XmlBatch,
    nodes_by_id: &HashMap<usize, TranslationUnit>,
) -> Result<BatchTranslation> {
    match translate_one_batch_guarded(config, target_lang_display, target_lang_code, batch).await {
        Ok(parsed) => Ok(parsed),
        Err(batch_err) => {
            warn!(
                target: "ai_translate",
                ids = ?batch.ids,
                error = %batch_err,
                "translation batch failed, retrying as single segments"
            );

            if batch_err.to_string().contains("timed out") {
                warn!(
                    target: "ai_translate",
                    ids = ?batch.ids,
                    error = %batch_err,
                    "translation batch timed out; failed segments will keep original text"
                );
                return Ok(BatchTranslation::default());
            }

            let mut recovered = BatchTranslation::default();
            let mut failures = Vec::new();
            for id in &batch.ids {
                let Some(node) = nodes_by_id.get(id) else {
                    failures.push(format!("id {id}: original node not found"));
                    continue;
                };
                let single_batches = batch::pack(
                    &[TranslatableNode {
                        id: node.id,
                        text: node.text.clone(),
                    }],
                    &BatchConfig {
                        max_chars_per_batch: usize::MAX,
                        max_items_per_batch: 1,
                    },
                );
                let Some(single_batch) = single_batches.first() else {
                    failures.push(format!("id {id}: could not build fallback batch"));
                    continue;
                };
                match translate_one_batch_guarded(
                    config,
                    target_lang_display,
                    target_lang_code,
                    single_batch,
                )
                .await
                {
                    Ok(mut parsed) => {
                        if let Some(text) = parsed.segments.remove(id) {
                            let mut single_success = BatchTranslation {
                                segments: HashMap::from([(*id, text)]),
                                ..BatchTranslation::default()
                            };
                            single_success.merge_success(parsed);
                            recovered.merge_success(single_success);
                        } else {
                            failures.push(format!("id {id}: fallback response missing segment"));
                        }
                    }
                    Err(err) => failures.push(format!("id {id}: {err}")),
                }
            }

            if recovered.segments.is_empty() {
                warn!(
                    target: "ai_translate",
                    ids = ?batch.ids,
                    error = %batch_err,
                    failures = %failures.join("; "),
                    "translation batch could not be recovered; failed segments will keep original text"
                );
                Ok(recovered)
            } else {
                if !failures.is_empty() {
                    warn!(
                        target: "ai_translate",
                        ids = ?batch.ids,
                        recovered = recovered.segments.len(),
                        failures = %failures.join("; "),
                        "translation batch partially recovered; failed segments will keep original text"
                    );
                }
                Ok(recovered)
            }
        }
    }
}

fn assemble_completed_chunks(
    chunk_totals: &HashMap<usize, usize>,
    chunk_translations: &HashMap<(usize, usize), String>,
) -> HashMap<usize, String> {
    let mut out = HashMap::new();
    for (&node_id, &total) in chunk_totals {
        let mut combined = String::new();
        for order in 0..total {
            let Some(chunk) = chunk_translations.get(&(node_id, order)) else {
                combined.clear();
                break;
            };
            combined.push_str(chunk);
        }
        if !combined.is_empty() || total == 0 {
            out.insert(node_id, combined);
        }
    }
    out
}

fn split_text_for_translation(text: &str, max_chars: usize) -> Vec<String> {
    if max_chars == 0 || text.chars().count() <= max_chars {
        return vec![text.to_string()];
    }

    let mut boundaries: Vec<usize> = text.char_indices().map(|(idx, _)| idx).collect();
    boundaries.push(text.len());

    let mut chunks = Vec::new();
    let mut start_char = 0usize;
    let total_chars = boundaries.len().saturating_sub(1);

    while start_char < total_chars {
        let hard_end_char = (start_char + max_chars).min(total_chars);
        if hard_end_char == total_chars {
            chunks.push(text[boundaries[start_char]..].to_string());
            break;
        }

        let min_soft_end_char = start_char + (max_chars / 2).max(1);
        let mut end_char = hard_end_char;
        for candidate in (min_soft_end_char..=hard_end_char).rev() {
            let char_start = boundaries[candidate - 1];
            let char_end = boundaries[candidate];
            let Some(ch) = text[char_start..char_end].chars().next() else {
                continue;
            };
            if is_soft_split_char(ch) {
                end_char = candidate;
                break;
            }
        }

        chunks.push(text[boundaries[start_char]..boundaries[end_char]].to_string());
        start_char = end_char;
    }
    chunks
}

fn is_soft_split_char(ch: char) -> bool {
    ch.is_whitespace()
        || matches!(
            ch,
            '.' | ','
                | ';'
                | ':'
                | '!'
                | '?'
                | ')'
                | ']'
                | '}'
                | '。'
                | '，'
                | '；'
                | '：'
                | '！'
                | '？'
                | '、'
                | '）'
                | '】'
                | '》'
        )
}

// ── Frontmatter helpers ─────────────────────────────────────────────

/// Split YAML frontmatter (`---\n…\n---\n`) from body. Returns
/// `(Some(frontmatter_including_fences), body)` or `(None, full_input)`.
fn split_frontmatter(input: &str) -> (Option<&str>, &str) {
    // Mirror src/lib/frontmatter.ts (FRONTMATTER_RE).
    let bom_stripped = input.strip_prefix('\u{FEFF}').unwrap_or(input);
    if !bom_stripped.starts_with("---") {
        return (None, input);
    }
    let after_open = &bom_stripped[3..];
    let after_open = after_open
        .strip_prefix("\r\n")
        .or_else(|| after_open.strip_prefix('\n'))
        .unwrap_or(after_open);

    let Some(close_idx) = find_closing_fence(after_open) else {
        return (None, input);
    };

    let bom_offset = input.len() - bom_stripped.len();
    let total_open = bom_offset + 3 + (bom_stripped[3..].len() - after_open.len());
    let frontmatter_end_offset = total_open + close_idx;
    let after_close = &after_open[close_idx..];

    // Consume the closing "---" line.
    let after_dashes = after_close.strip_prefix("---").unwrap_or(after_close);
    let body_start_offset_relative = after_close.len() - after_dashes.len();
    let body_after_close = after_dashes
        .strip_prefix("\r\n")
        .or_else(|| after_dashes.strip_prefix('\n'))
        .unwrap_or(after_dashes);
    let trailing_newline = after_dashes.len() - body_after_close.len();

    let frontmatter_block_end =
        frontmatter_end_offset + body_start_offset_relative + trailing_newline;

    let frontmatter = &input[..frontmatter_block_end];
    let body = &input[frontmatter_block_end..];
    (Some(frontmatter), body)
}

fn find_closing_fence(s: &str) -> Option<usize> {
    let mut start = 0;
    for line in s.split_inclusive('\n') {
        let trimmed = line.trim_end_matches('\n').trim_end_matches('\r');
        if trimmed == "---" {
            return Some(start);
        }
        start += line.len();
    }
    None
}

fn reattach_frontmatter(frontmatter: Option<&str>, body: &str) -> String {
    match frontmatter {
        Some(fm) => format!("{fm}{body}"),
        None => body.to_string(),
    }
}

// ── Skip-when-already-target heuristic ──────────────────────────────

fn should_skip_translation(content: &str, target_lang: &str) -> bool {
    if is_cjk_target(target_lang) {
        let cjk = cjk_ratio(content);
        let ascii_alpha = ascii_alpha_ratio(content);
        // Keep mixed English/Chinese skill docs translatable. Only skip content
        // that already reads predominantly CJK, while allowing technical names
        // like React, Tauri, CLI, etc. to remain as-is.
        return cjk > 0.55 && ascii_alpha < 0.35;
    }
    let target = target_lang.to_ascii_lowercase();
    if target == "en" {
        // Target English: skip if the text already looks predominantly ASCII letters.
        return ascii_alpha_ratio(content) > 0.75 && cjk_ratio(content) < 0.05;
    }
    false
}

fn is_cjk_target(target_lang: &str) -> bool {
    let target = target_lang.to_ascii_lowercase();
    target.starts_with("zh") || target == "ja" || target == "ko"
}

fn ascii_alpha_ratio(s: &str) -> f32 {
    let total = s.chars().filter(|c| !c.is_whitespace()).count().max(1);
    let ascii_alpha = s.chars().filter(|c| c.is_ascii_alphabetic()).count();
    ascii_alpha as f32 / total as f32
}

fn cjk_ratio(s: &str) -> f32 {
    let total = s.chars().filter(|c| !c.is_whitespace()).count().max(1);
    let cjk = s
        .chars()
        .filter(|c| {
            matches!(*c as u32,
                0x3040..=0x30FF      // Hiragana + Katakana
                | 0x3400..=0x4DBF    // CJK Ext A
                | 0x4E00..=0x9FFF    // CJK Unified
                | 0xAC00..=0xD7AF    // Hangul Syllables
                | 0xF900..=0xFAFF    // CJK Compatibility Ideographs
            )
        })
        .count();
    cjk as f32 / total as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_frontmatter_extracts_block() {
        let md = "---\nname: foo\ndescription: bar\n---\n\n# Body\n";
        let (fm, body) = split_frontmatter(md);
        assert_eq!(fm, Some("---\nname: foo\ndescription: bar\n---\n"));
        assert_eq!(body, "\n# Body\n");
    }

    #[test]
    fn split_frontmatter_returns_none_when_absent() {
        let md = "# Just a heading\n\nBody.\n";
        let (fm, body) = split_frontmatter(md);
        assert_eq!(fm, None);
        assert_eq!(body, md);
    }

    #[test]
    fn split_frontmatter_with_crlf() {
        let md = "---\r\nname: foo\r\n---\r\nBody\r\n";
        let (fm, _body) = split_frontmatter(md);
        assert!(fm.is_some());
        assert!(fm.unwrap().contains("name: foo"));
    }

    #[test]
    fn reattach_preserves_frontmatter() {
        let md = "---\nname: foo\n---\n# Heading\n";
        let (fm, body) = split_frontmatter(md);
        let rebuilt = reattach_frontmatter(fm, body);
        assert_eq!(rebuilt, md);
    }

    #[test]
    fn skip_when_already_chinese_target_zh() {
        let zh = "# 标题\n\n这是一段中文文本，足够多了。\n";
        assert!(should_skip_translation(zh, "zh-CN"));
    }

    #[test]
    fn dont_skip_english_when_target_zh() {
        let en = "# Heading\n\nThis is some English text content.\n";
        assert!(!should_skip_translation(en, "zh-CN"));
    }

    #[test]
    fn dont_skip_mixed_doc_when_target_zh() {
        let mixed = "# 已有中文\n\nThis English section still needs translation.\n";
        assert!(!should_skip_translation(mixed, "zh-CN"));
    }

    #[test]
    fn skip_when_already_english_target_en() {
        let en = "# Heading\n\nAll lowercase ascii body text here.\n";
        assert!(should_skip_translation(en, "en"));
    }

    #[test]
    fn long_segment_split_preserves_text_and_bounds() {
        let source = "This is a long technical paragraph about Markdown translation, code blocks, links, and cached AI provider routing. ".repeat(80);
        let chunks = split_text_for_translation(&source, MAX_CHARS_PER_SEGMENT);
        assert!(chunks.len() > 1);
        assert_eq!(chunks.concat(), source);
        assert!(
            chunks
                .iter()
                .all(|chunk| chunk.chars().count() <= MAX_CHARS_PER_SEGMENT)
        );
    }

    #[test]
    fn long_segment_split_preserves_utf8_boundaries() {
        let source = "Translate this carefully，保留中文标点和 emoji 🚀 while keeping technical identifiers intact. ".repeat(90);
        let chunks = split_text_for_translation(&source, MAX_CHARS_PER_SEGMENT);
        assert!(chunks.len() > 1);
        assert_eq!(chunks.concat(), source);
        assert!(
            chunks
                .iter()
                .all(|chunk| chunk.is_char_boundary(chunk.len()))
        );
    }

    #[test]
    fn large_skill_file_plan_from_env_has_bounded_segments() {
        let Ok(path) = std::env::var("SKILLSTAR_LARGE_SKILL_MD") else {
            eprintln!("SKILLSTAR_LARGE_SKILL_MD not set; skipping large-file plan test");
            return;
        };
        let markdown = std::fs::read_to_string(&path).expect("read large SKILL.md");
        assert!(
            markdown.len() > 20_000,
            "large fixture should be meaningfully large: {} bytes",
            markdown.len()
        );

        let (_frontmatter, body) = split_frontmatter(&markdown);
        let nodes = ast::extract(body);
        assert!(
            !nodes.is_empty(),
            "large fixture should have translatable nodes"
        );

        let mut unit_id = 1usize;
        let mut units = Vec::new();
        for node in &nodes {
            for chunk in split_text_for_translation(&node.text, MAX_CHARS_PER_SEGMENT) {
                units.push(TranslatableNode {
                    id: unit_id,
                    text: chunk,
                });
                unit_id += 1;
            }
        }

        let batches = batch::pack(&units, &BatchConfig::default());
        let max_unit_chars = units
            .iter()
            .map(|unit| unit.text.chars().count())
            .max()
            .unwrap_or(0);
        println!(
            "large skill plan: bytes={} nodes={} units={} batches={} max_unit_chars={}",
            markdown.len(),
            nodes.len(),
            units.len(),
            batches.len(),
            max_unit_chars
        );

        assert!(
            batches.len() > 1,
            "large fixture should span multiple batches"
        );
        assert!(max_unit_chars <= MAX_CHARS_PER_SEGMENT);
    }

    #[tokio::test]
    #[ignore = "requires live AI provider env and makes network calls"]
    async fn live_large_skill_translation_from_env() {
        use crate::ai_provider::ApiFormat;

        let path = std::env::var("SKILLSTAR_LIVE_TRANSLATE_SKILL_MD")
            .expect("SKILLSTAR_LIVE_TRANSLATE_SKILL_MD must point at a large SKILL.md");
        let api_key = std::env::var("SKILLSTAR_LIVE_TRANSLATE_API_KEY")
            .expect("SKILLSTAR_LIVE_TRANSLATE_API_KEY is required");
        let base_url = std::env::var("SKILLSTAR_LIVE_TRANSLATE_BASE_URL")
            .unwrap_or_else(|_| "https://api.openai.com/v1".to_string());
        let model = std::env::var("SKILLSTAR_LIVE_TRANSLATE_MODEL")
            .unwrap_or_else(|_| "gpt-4o-mini".to_string());
        let request_timeout_secs = std::env::var("SKILLSTAR_LIVE_TRANSLATE_TIMEOUT_SECS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(90);

        let mut markdown = std::fs::read_to_string(&path).expect("read live large SKILL.md");
        if let Ok(max_bytes) = std::env::var("SKILLSTAR_LIVE_TRANSLATE_MAX_BYTES")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .ok_or(())
        {
            let end = markdown
                .char_indices()
                .map(|(idx, _)| idx)
                .chain(std::iter::once(markdown.len()))
                .take_while(|idx| *idx <= max_bytes)
                .last()
                .unwrap_or(markdown.len());
            markdown.truncate(end);
        }

        assert!(
            markdown.len() > 20_000,
            "live fixture should stay large: {} bytes",
            markdown.len()
        );

        let config = AiConfig {
            enabled: true,
            api_format: ApiFormat::Openai,
            provider_ref: None,
            base_url,
            api_key,
            model,
            target_language: "zh-CN".to_string(),
            max_concurrent_requests: 6,
            request_timeout_secs: Some(request_timeout_secs),
            ..AiConfig::default()
        };
        let force_refresh = std::env::var("SKILLSTAR_LIVE_TRANSLATE_FORCE_REFRESH")
            .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        let started = std::time::Instant::now();
        let mut progress_events = Vec::new();
        let translated = translate_skill_with_options(
            &config,
            &markdown,
            TranslateOptions { force_refresh },
            |progress| {
                println!(
                    "progress {:?} {}/{}",
                    progress.phase, progress.current, progress.total
                );
                progress_events.push(progress);
            },
        )
        .await
        .expect("live large translation should complete");

        println!(
            "live large translation: input_bytes={} output_bytes={} elapsed_ms={}",
            markdown.len(),
            translated.len(),
            started.elapsed().as_millis()
        );
        assert!(!progress_events.is_empty());
        assert!(translated.len() > markdown.len() / 4);
        assert!(
            cjk_ratio(&translated) > 0.05,
            "translated output should contain visible Chinese"
        );
    }
}
