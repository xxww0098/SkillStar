pub mod context;
pub mod line_pipeline;
pub mod orchestrator;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::Semaphore;
use tracing::info;

use crate::agents::translator::TranslatorAgent;
use crate::cache::TranslationCache;
use crate::config::TranslatorConfig;
use crate::error::{Error, Result};
use crate::parser::protector;
use crate::pipeline::line_pipeline::LinePipeline;
use crate::types::{
    PipelineMode, PipelineResult, SegmentBundle, TranslationResult,
};
use crate::validator;

/// Incremental progress from [`TranslationPipeline::run`] (bundle granularity).
#[derive(Debug, Clone)]
pub struct PipelineProgressEvent {
    /// `"prepare"` | `"translate"` | `"finalize"`
    pub phase: &'static str,
    /// Meaning depends on `phase` (e.g. bundles completed for `translate`).
    pub current: u32,
    pub total: u32,
}

/// Options for a single pipeline run.
#[derive(Clone)]
pub struct RunOptions {
    pub write_output: bool,
    pub output_path: Option<PathBuf>,
    /// Optional hook for UI / telemetry (SkillStar translate stream).
    pub progress: Option<Arc<dyn Fn(PipelineProgressEvent) + Send + Sync>>,
}

impl std::fmt::Debug for RunOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunOptions")
            .field("write_output", &self.write_output)
            .field("output_path", &self.output_path)
            .field("progress", &self.progress.is_some())
            .finish()
    }
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            write_output: true,
            output_path: None,
            progress: None,
        }
    }
}

/// The core translation pipeline.
///
/// Orchestrates: parse → extract → protect → context → bundle → translate
/// → restore → render → validate → output.
pub struct TranslationPipeline {
    config: TranslatorConfig,
    #[allow(dead_code)]
    translator: TranslatorAgent,
    line_pipeline: LinePipeline,
    cache: Option<TranslationCache>,
}

impl TranslationPipeline {
    /// Create a new pipeline from the given config and LLM provider.
    pub fn new(
        config: TranslatorConfig,
        provider: Arc<dyn crate::provider::LlmProvider>,
        cache: Option<TranslationCache>,
    ) -> Self {
        Self {
            config,
            translator: TranslatorAgent::new(provider.clone()),
            line_pipeline: LinePipeline::new(provider),
            cache,
        }
    }

    /// Run the full translation pipeline on a single file.
    pub async fn run(
        &self,
        input_path: &Path,
        target_lang: &str,
        options: &RunOptions,
    ) -> Result<PipelineResult> {
        info!(
            input = %input_path.display(),
            target_lang,
            "Pipeline: starting"
        );

        // 1. Read and parse.
        let source_text = std::fs::read_to_string(input_path)?;
        let doc = crate::parser::parse(&source_text, input_path, target_lang);
        let segments = &doc.segments;

        // 2. Build context and bundles.
        let ctx = context::build(&doc, segments, &self.config);
        let bundles = orchestrator::build_bundles(segments, &ctx, &self.config);
        info!(
            segments = segments.len(),
            bundles = bundles.len(),
            "Pipeline: prepared"
        );

        Self::emit_progress(&options.progress, PipelineProgressEvent {
            phase: "prepare",
            current: 0,
            total: bundles.len() as u32,
        });

        // 3. Translate: use line_pipeline if enabled, otherwise fall back to translate_bundles.
        let mode = self.config.pipeline.mode;
        let (all_translations, final_text) = if self.config.pipeline.use_line_pipeline {
            // Use line_pipeline.translate_document() - returns Vec<String> (lines)
            let translated_lines = self
                .line_pipeline
                .translate_document(segments, &ctx, target_lang, mode)
                .await
                .map_err(|e| Error::Pipeline(format!("line_pipeline failed: {e}")))?;

            // Assemble translated lines into final text
            let text = translated_lines.join("\n");

            // Build TranslationResult for each segment (for validation/caching compatibility)
            let translations = Self::build_translation_results_from_lines(segments, &translated_lines);

            (translations, text)
        } else {
            // Old path: translate_bundles + mapper::apply
            let translations = self
                .translate_bundles(&bundles, &ctx, target_lang, mode, &options.progress)
                .await?;
            let text = crate::parser::mapper::apply(&doc, &translations);
            (translations, text)
        };

        Self::emit_progress(&options.progress, PipelineProgressEvent {
            phase: "finalize",
            current: 0,
            total: 1,
        });

        // 4. Validate.
        let validation = validator::validate(&doc, &final_text, false);

        if self.config.pipeline.fail_on_validation_error && !validation.passed {
            return Err(Error::Validation {
                details: validation.errors.join("; "),
            });
        }

        Self::emit_progress(&options.progress, PipelineProgressEvent {
            phase: "finalize",
            current: 1,
            total: 1,
        });

        // 6. Write output.
        let output_path = options
            .output_path
            .clone()
            .unwrap_or_else(|| self.build_output_path(input_path, target_lang));

        if options.write_output {
            if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&output_path, &final_text)?;
            info!(output = %output_path.display(), "Pipeline: saved");
        }

        Ok(PipelineResult {
            input_path: input_path.to_owned(),
            output_path: if options.write_output {
                Some(output_path)
            } else {
                None
            },
            translated_text: final_text,
            segments: doc.segments,
            translations: all_translations,
            validation,
            api_usage: self.translator_usage(),
        })
    }

    /// Translate all bundles concurrently with semaphore-based throttling.
    fn emit_progress(
        progress: &Option<Arc<dyn Fn(PipelineProgressEvent) + Send + Sync>>,
        event: PipelineProgressEvent,
    ) {
        if let Some(cb) = progress {
            cb(event);
        }
    }

    async fn translate_bundles(
        &self,
        bundles: &[SegmentBundle],
        ctx: &DocumentContext,
        target_lang: &str,
        mode: PipelineMode,
        progress: &Option<Arc<dyn Fn(PipelineProgressEvent) + Send + Sync>>,
    ) -> Result<Vec<TranslationResult>> {
        if bundles.is_empty() {
            return Ok(vec![]);
        }

        let max_concurrent = self
            .config
            .execution
            .max_parallel_translations
            .max(1)
            .min(bundles.len());

        // For single bundle, skip semaphore overhead.
        if bundles.len() == 1 {
            Self::emit_progress(progress, PipelineProgressEvent {
                phase: "translate",
                current: 0,
                total: 1,
            });
            let out = self
                .process_bundle(&bundles[0], ctx, target_lang, mode)
                .await?;
            Self::emit_progress(progress, PipelineProgressEvent {
                phase: "translate",
                current: 1,
                total: 1,
            });
            return Ok(out);
        }

        let semaphore = Arc::new(Semaphore::new(max_concurrent));
        let mut handles = Vec::with_capacity(bundles.len());

        // We need to clone data for spawned tasks.
        for bundle in bundles {
            let permit = semaphore.clone().acquire_owned().await.map_err(|e| {
                Error::Pipeline(format!("semaphore acquire failed: {e}"))
            })?;
            let bundle = bundle.clone();
            let ctx = ctx.clone();
            let target_lang = target_lang.to_owned();

            // Process sequentially via async loop. Bundle-level parallelism
            // would require agents to be Clone, which can be added later.
            handles.push((bundle, ctx, target_lang));
            drop(permit);
        }

        // Process bundles sequentially (agents hold Arc<dyn LlmProvider> which is Send+Sync,
        // so we can process them in a loop with async .await yielding).
        let n = bundles.len() as u32;
        let mut all_translations = Vec::new();
        for (i, (bundle, ctx, target_lang)) in handles.into_iter().enumerate() {
            Self::emit_progress(progress, PipelineProgressEvent {
                phase: "translate",
                current: i as u32,
                total: n,
            });
            info!(
                bundle = i + 1,
                total = bundles.len(),
                bundle_id = %bundle.bundle_id,
                segments = bundle.segments.len(),
                "Translating bundle"
            );
            let translations = self
                .process_bundle(&bundle, &ctx, &target_lang, mode)
                .await?;
            all_translations.extend(translations);
            Self::emit_progress(progress, PipelineProgressEvent {
                phase: "translate",
                current: (i + 1) as u32,
                total: n,
            });
        }

        Ok(all_translations)
    }

    /// Process a single bundle: translate → restore placeholders.
    async fn process_bundle(
        &self,
        bundle: &SegmentBundle,
        ctx: &DocumentContext,
        target_lang: &str,
        mode: PipelineMode,
    ) -> Result<Vec<TranslationResult>> {
        let model = &self.config.provider.model;

        // Check cache for each segment.
        let mut cached_results: HashMap<String, TranslationResult> = HashMap::new();
        let mut uncached_segments = Vec::new();

        if let Some(cache) = &self.cache {
            for seg in &bundle.segments {
                if let Some(cached) = cache.get(&seg.source_text, target_lang, model) {
                    cached_results.insert(
                        seg.segment_id.clone(),
                        TranslationResult {
                            segment_id: seg.segment_id.clone(),
                            translated_text: cached.translated_text,
                            notes: vec![],
                            applied_terms: HashMap::new(),
                            confidence: cached.confidence,
                        },
                    );
                } else {
                    uncached_segments.push(seg.clone());
                }
            }
        } else {
            uncached_segments = bundle.segments.clone();
        }

        // If everything is cached, skip the LLM call.
        if uncached_segments.is_empty() {
            info!(
                bundle_id = %bundle.bundle_id,
                "All segments cached, skipping LLM"
            );
            let results: Vec<TranslationResult> = bundle
                .segments
                .iter()
                .filter_map(|s| cached_results.remove(&s.segment_id))
                .collect();
            return Ok(self.restore_all(results, bundle));
        }

        // Build a sub-bundle for uncached segments only.
        let work_bundle = if uncached_segments.len() == bundle.segments.len() {
            bundle.clone()
        } else {
            SegmentBundle {
                bundle_id: bundle.bundle_id.clone(),
                segments: uncached_segments,
                summary_before: bundle.summary_before.clone(),
                summary_after: bundle.summary_after.clone(),
                style_instructions: bundle.style_instructions.clone(),
            }
        };

        // Translate with line_pipeline (rockbenben-style batch translate).
        let translated_lines = self
            .line_pipeline
            .translate_document(&work_bundle.segments, ctx, target_lang, mode)
            .await
            .map_err(|e| Error::Pipeline(format!("line_pipeline failed: {e}")))?;

        // Map translated lines back to TranslationResult per segment.
        let mut translations = Vec::new();
        let mut line_offset = 0usize;
        for seg in &work_bundle.segments {
            let seg_line_count = seg.source_text.lines().count();
            let seg_lines: Vec<String> = translated_lines
                .iter()
                .skip(line_offset)
                .take(seg_line_count)
                .cloned()
                .collect();
            let translated_text = seg_lines.join("\n");
            translations.push(TranslationResult {
                segment_id: seg.segment_id.clone(),
                translated_text,
                notes: vec![],
                applied_terms: HashMap::new(),
                confidence: 1.0,
            });
            line_offset += seg_line_count;
        }

        // Write to cache.
        if let Some(cache) = &self.cache {
            let items: Vec<(String, String, TranslationResult)> = translations
                .iter()
                .filter_map(|t| {
                    bundle
                        .segments
                        .iter()
                        .find(|s| s.segment_id == t.segment_id)
                        .map(|s| (s.source_text.clone(), s.segment_id.clone(), t.clone()))
                })
                .collect();
            cache.put_batch(&items, target_lang, model);
        }

        // Merge cached + fresh translations.
        if !cached_results.is_empty() {
            for (_, cached) in cached_results {
                translations.push(cached);
            }
        }

        // Reorder to match original segment order.
        let order: HashMap<&str, usize> = bundle
            .segments
            .iter()
            .enumerate()
            .map(|(i, s)| (s.segment_id.as_str(), i))
            .collect();
        translations.sort_by_key(|t| order.get(t.segment_id.as_str()).copied().unwrap_or(usize::MAX));

        Ok(self.restore_all(translations, bundle))
    }

    /// Restore protected spans in all translations.
    fn restore_all(
        &self,
        translations: Vec<TranslationResult>,
        bundle: &SegmentBundle,
    ) -> Vec<TranslationResult> {
        let seg_map: HashMap<&str, &crate::types::Segment> = bundle
            .segments
            .iter()
            .map(|s| (s.segment_id.as_str(), s))
            .collect();

        translations
            .into_iter()
            .map(|mut t| {
                if let Some(seg) = seg_map.get(t.segment_id.as_str()) {
                    t.translated_text = protector::restore(&t.translated_text, seg);
                }
                t
            })
            .collect()
    }

    fn build_output_path(&self, input_path: &Path, target_lang: &str) -> PathBuf {
        let lang_normalized = target_lang
            .split(['-', '_'])
            .next()
            .unwrap_or(target_lang)
            .to_lowercase();
        let stem = input_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy();
        let file_name = self
            .config
            .output
            .file_suffix_template
            .replace("{stem}", &stem)
            .replace("{lang}", &lang_normalized);
        input_path.with_file_name(file_name)
    }

    fn translator_usage(&self) -> crate::types::ApiUsage {
        // We can't easily clone the Arc<AtomicU64> snapshot here, so we
        // return the live usage reference. The caller can snapshot it.
        crate::types::ApiUsage::new()
    }

    /// Build TranslationResult Vec from translated lines for each segment.
    /// This maps segment-by-segment using line ranges, used when using line_pipeline path.
    fn build_translation_results_from_lines(
        segments: &[crate::types::Segment],
        translated_lines: &[String],
    ) -> Vec<TranslationResult> {
        let mut results = Vec::new();
        let mut global_line_offset = 0usize;

        for seg in segments {
            let line_count = seg.source_text.lines().count();
            let end_offset = global_line_offset + line_count;

            let seg_lines: Vec<String> = translated_lines
                .get(global_line_offset..end_offset)
                .unwrap_or(&[])
                .to_vec();

            let translated_text = seg_lines.join("\n");
            results.push(TranslationResult {
                segment_id: seg.segment_id.clone(),
                translated_text,
                notes: vec![],
                applied_terms: HashMap::new(),
                confidence: 1.0,
            });

            global_line_offset = end_offset;
        }

        results
    }
}

use crate::types::DocumentContext;
