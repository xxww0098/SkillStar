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

mod text;
use text::*;

const TRANSLATE_PROMPT: &str = include_str!("../../../../../../src-tauri/prompts/ai/translate.md");
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

#[cfg(test)]
mod tests;
