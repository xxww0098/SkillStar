//! Translation orchestration: parallel batches, per-batch token budgets,
//! content cache, phase-level progress events.
//!
//! Speed strategy:
//!   - Small batches (~8 items / ~1500 chars) → high parallelism
//!   - Fan out via `FuturesUnordered`; the AI semaphore in `acquire_ai_request_permit`
//!     naturally caps concurrent in-flight to `max_concurrent_requests` (default 4)
//!   - Per-batch `max_tokens = input_chars * 2 + 256` — avoids KV-cache over-allocation
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
use tracing::warn;

use super::ast::{self, TranslatableNode};
use super::batch::{self, BatchConfig, XmlBatch};
use super::cache;
use crate::ai_provider::{
    AiConfig, chat_completion_capped, language_display_name, resolve_runtime_config,
};

const TRANSLATE_PROMPT: &str = include_str!("../../../../../src-tauri/prompts/ai/translate.md");

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

/// Translate a SKILL.md, preserving frontmatter verbatim and using AST
/// substitution for the body.
///
/// `on_progress` is invoked for every phase transition + each batch completion.
/// Errors from `on_progress` are logged but never abort translation.
pub async fn translate_skill<F>(
    config: &AiConfig,
    markdown: &str,
    mut on_progress: F,
) -> Result<String>
where
    F: FnMut(PipelineProgress) + Send,
{
    // ── Skip-when-already-target ─────────────────────────────────────
    if should_skip_translation(markdown, &config.target_language) {
        on_progress(PipelineProgress {
            phase: PipelinePhase::Finalize,
            current: 1,
            total: 1,
        });
        return Ok(markdown.to_string());
    }

    on_progress(PipelineProgress {
        phase: PipelinePhase::Prepare,
        current: 0,
        total: 1,
    });

    // ── Split frontmatter (preserve verbatim) ────────────────────────
    let (frontmatter, body) = split_frontmatter(markdown);

    // ── Resolve runtime config once (resolves provider_ref) ──────────
    let resolved = resolve_runtime_config(config).context("resolving AI config")?;
    let effective_model = resolved.model.clone();
    let target_lang = resolved.target_language.clone();
    let target_lang_display = language_display_name(&target_lang).to_string();

    // ── Parse AST + extract translatable nodes ───────────────────────
    let nodes = ast::extract(body);

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
        return Ok(markdown.to_string());
    }

    // ── Cache lookup pass — drop cached nodes from the work list ────
    let mut cached_translations: HashMap<usize, String> = HashMap::new();
    let mut work: Vec<TranslatableNode> = Vec::with_capacity(nodes.len());
    for node in nodes {
        if let Some(hit) = cache::get(&node.text, &target_lang, &effective_model) {
            cached_translations.insert(node.id, hit);
        } else {
            work.push(node);
        }
    }

    // Everything cached? Skip the LLM entirely.
    if work.is_empty() {
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
        on_progress(PipelineProgress {
            phase: PipelinePhase::Finalize,
            current: 1,
            total: 1,
        });
        return Ok(result);
    }

    // ── Pack into XML batches ────────────────────────────────────────
    let batches = batch::pack(&work, &BatchConfig::default());
    let total_batches = batches.len() as u32;

    let progress = Arc::new(Mutex::new(0u32));

    // ── Fan out batch translations ───────────────────────────────────
    on_progress(PipelineProgress {
        phase: PipelinePhase::Translate,
        current: 0,
        total: total_batches.max(1),
    });

    let mut futures = FuturesUnordered::new();
    let work_by_id: Arc<HashMap<usize, TranslatableNode>> =
        Arc::new(work.iter().cloned().map(|node| (node.id, node)).collect());
    for b in batches {
        let cfg = resolved.clone();
        let target_display = target_lang_display.clone();
        let target_code = target_lang.clone();
        let model = effective_model.clone();
        let fallback_nodes = Arc::clone(&work_by_id);
        futures.push(async move {
            let result =
                translate_batch_with_fallback(&cfg, &target_display, &b, &fallback_nodes).await;
            (b, target_code, model, result)
        });
    }

    let mut llm_translations: HashMap<usize, String> = HashMap::new();
    while let Some((batch_info, target_code, model, result)) = futures.next().await {
        let parsed = result
            .with_context(|| format!("translation batch (ids={:?}) failed", batch_info.ids))?;

        // Map results back, and warm the cache so re-runs are instant.
        let id_to_text: HashMap<usize, String> = batch_info
            .ids
            .iter()
            .filter_map(|id| parsed.get(id).map(|t| (*id, t.clone())))
            .collect();

        // To populate cache we need original text for each id — recover from `work`.
        for id in &batch_info.ids {
            if let (Some(translated), Some(original)) =
                (id_to_text.get(id), work.iter().find(|n| n.id == *id))
            {
                cache::insert(&original.text, &target_code, &model, translated.clone());
                llm_translations.insert(*id, translated.clone());
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
    all.extend(llm_translations);

    let rendered = ast::replace_and_render(body, &all);
    let final_md = reattach_frontmatter(frontmatter, &rendered);

    on_progress(PipelineProgress {
        phase: PipelinePhase::Finalize,
        current: 1,
        total: 1,
    });

    Ok(final_md)
}

async fn translate_one_batch(
    config: &AiConfig,
    target_lang_display: &str,
    batch: &XmlBatch,
) -> Result<HashMap<usize, String>> {
    let system_prompt = TRANSLATE_PROMPT.replace("{target_lang}", target_lang_display);
    let user_prompt = format!(
        "Translate the following XML batch into {target_lang_display}.\n\
         Preserve every <seg> tag exactly (id attribute and tag boundaries).\n\
         Return ONLY the XML — no prose, no code fences.\n\n{xml}",
        xml = batch.xml,
    );

    // Output ~ input * 2 covers expansion for CJK / European languages.
    // +256 safety so the last `</seg>` isn't truncated.
    // Floor at 512 so even tiny single-segment batches have headroom.
    let max_tokens = ((batch.input_chars * 2) + 256).clamp(512, 32_768) as u32;

    let raw = chat_completion_capped(config, &system_prompt, &user_prompt, max_tokens).await?;

    let parsed = batch::parse(&raw, &batch.ids).map_err(|e| {
        anyhow::anyhow!(
            "translation response could not be parsed (ids={:?}): {e}\n\nResponse: {}",
            batch.ids,
            raw.chars().take(500).collect::<String>()
        )
    })?;
    Ok(parsed)
}

async fn translate_batch_with_fallback(
    config: &AiConfig,
    target_lang_display: &str,
    batch: &XmlBatch,
    nodes_by_id: &HashMap<usize, TranslatableNode>,
) -> Result<HashMap<usize, String>> {
    match translate_one_batch(config, target_lang_display, batch).await {
        Ok(parsed) => Ok(parsed),
        Err(batch_err) => {
            warn!(
                target: "ai_translate",
                ids = ?batch.ids,
                error = %batch_err,
                "translation batch failed, retrying as single segments"
            );

            let mut recovered = HashMap::new();
            let mut failures = Vec::new();
            for id in &batch.ids {
                let Some(node) = nodes_by_id.get(id) else {
                    failures.push(format!("id {id}: original node not found"));
                    continue;
                };
                let single_batches = batch::pack(
                    std::slice::from_ref(node),
                    &BatchConfig {
                        max_chars_per_batch: usize::MAX,
                        max_items_per_batch: 1,
                    },
                );
                let Some(single_batch) = single_batches.first() else {
                    failures.push(format!("id {id}: could not build fallback batch"));
                    continue;
                };
                match translate_one_batch(config, target_lang_display, single_batch).await {
                    Ok(mut parsed) => {
                        if let Some(text) = parsed.remove(id) {
                            recovered.insert(*id, text);
                        } else {
                            failures.push(format!("id {id}: fallback response missing segment"));
                        }
                    }
                    Err(err) => failures.push(format!("id {id}: {err}")),
                }
            }

            if recovered.is_empty() {
                Err(batch_err).with_context(|| {
                    format!(
                        "single-segment fallback also failed: {}",
                        failures.join("; ")
                    )
                })
            } else {
                if !failures.is_empty() {
                    warn!(
                        target: "ai_translate",
                        ids = ?batch.ids,
                        recovered = recovered.len(),
                        failures = %failures.join("; "),
                        "translation batch partially recovered; failed segments will keep original text"
                    );
                }
                Ok(recovered)
            }
        }
    }
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
    let target = target_lang.to_ascii_lowercase();
    if target.starts_with("zh") || target == "ja" || target == "ko" {
        return cjk_ratio(content) > 0.30;
    }
    if target == "en" {
        // Target English: skip if the text already looks predominantly ASCII letters.
        let total = content
            .chars()
            .filter(|c| !c.is_whitespace())
            .count()
            .max(1);
        let ascii_alpha = content.chars().filter(|c| c.is_ascii_alphabetic()).count();
        return (ascii_alpha as f32 / total as f32) > 0.75 && cjk_ratio(content) < 0.05;
    }
    false
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
    fn skip_when_already_english_target_en() {
        let en = "# Heading\n\nAll lowercase ascii body text here.\n";
        assert!(should_skip_translation(en, "en"));
    }
}
