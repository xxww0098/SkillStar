//! Bridge between SkillStar's AI provider infrastructure and the
//! `markdown-translator` crate.
//!
//! Instead of duplicating HTTP client code, we wrap SkillStar's existing
//! `chat_completion_capped` (which already handles OpenAI, Anthropic, and
//! Local/Ollama) as a `markdown_translator::provider::LlmProvider`.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use markdown_translator::cache::TranslationCache;
use markdown_translator::config::{ProviderConfig, TranslatorConfig};
use markdown_translator::error::Result as MdtxResult;
use markdown_translator::pipeline::{PipelineProgressEvent, RunOptions, TranslationPipeline};
use markdown_translator::provider::LlmProvider;
use markdown_translator::types::{ApiUsage, PipelineMode};
use tracing::{debug, info};

use crate::core::ai::translation_log::{self, TranslationMdtxLogCtx};
use crate::core::ai_provider::{self, AiConfig};
use crate::core::infra::util::sha256_hex;

// ── LlmProvider adapter ─────────────────────────────────────────────

/// Wraps SkillStar's existing AI provider as a `markdown_translator::LlmProvider`.
///
/// Advantages: all API format support (OpenAI / Anthropic / Local) comes for free.
pub struct SkillStarProvider {
    config: AiConfig,
    usage: ApiUsage,
}

impl SkillStarProvider {
    pub fn new(config: AiConfig) -> Self {
        Self {
            config,
            usage: ApiUsage::new(),
        }
    }
}

#[async_trait::async_trait]
impl LlmProvider for SkillStarProvider {
    async fn chat_json(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        call_label: &str,
    ) -> MdtxResult<serde_json::Value> {
        debug!(
            call_label,
            system_chars = system_prompt.len(),
            user_chars = user_prompt.len(),
            "mdtx bridge: chat_json"
        );

        // Estimate a generous max_tokens for translation output.
        let max_tokens = ai_provider::estimate_translation_max_tokens(user_prompt);
        const SECTION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);
        const MAX_RETRIES: u8 = 3;

        let mut last_error = String::new();

        for attempt in 0..=MAX_RETRIES {
            let result = tokio::time::timeout(
                SECTION_TIMEOUT,
                ai_provider::chat_completion_capped(
                    &self.config,
                    system_prompt,
                    user_prompt,
                    max_tokens,
                ),
            )
            .await;

            match result {
                Ok(Ok(text)) => {
                    // Parse the response as JSON.
                    let parsed: serde_json::Value = serde_json::from_str(&text).map_err(|e| {
                        markdown_translator::error::Error::LlmOutputParse {
                            bundle_id: call_label.to_owned(),
                            source: e,
                        }
                    })?;
                    return Ok(parsed);
                }
                Ok(Err(e)) => {
                    last_error = e.to_string();
                    tracing::warn!(
                        call_label,
                        attempt,
                        error = %e,
                        "mdtx bridge HTTP error, retrying"
                    );
                }
                Err(e) => {
                    last_error = format!("timeout: {e}");
                    tracing::warn!(
                        call_label,
                        attempt,
                        error = %last_error,
                        "mdtx bridge timeout, retrying"
                    );
                }
            }

            if attempt < MAX_RETRIES {
                let backoff = std::time::Duration::from_secs(2_u64.pow(attempt as u32));
                tokio::time::sleep(backoff).await;
            }
        }

        Err(markdown_translator::error::Error::Pipeline(format!(
            "failed after {} attempts. Last error: {}",
            MAX_RETRIES + 1,
            last_error
        )))
    }

    fn usage(&self) -> &ApiUsage {
        &self.usage
    }

    async fn chat_text(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        call_label: &str,
    ) -> MdtxResult<String> {
        debug!(
            call_label,
            system_chars = system_prompt.len(),
            user_chars = user_prompt.len(),
            "mdtx bridge: chat_text"
        );

        let max_tokens = ai_provider::estimate_translation_max_tokens(user_prompt);
        const SECTION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);
        const MAX_RETRIES: u8 = 3;
        let mut last_error = String::new();

        for attempt in 0..=MAX_RETRIES {
            let result = tokio::time::timeout(
                SECTION_TIMEOUT,
                ai_provider::chat_completion_capped(
                    &self.config,
                    system_prompt,
                    user_prompt,
                    max_tokens,
                ),
            )
            .await;

            match result {
                Ok(Ok(text)) => return Ok(text),
                Ok(Err(e)) => {
                    last_error = e.to_string();
                    tracing::warn!(
                        call_label,
                        attempt,
                        error = %last_error,
                        "mdtx bridge text HTTP error, retrying"
                    );
                }
                Err(e) => {
                    last_error = format!("timeout: {e}");
                    tracing::warn!(
                        call_label,
                        attempt,
                        error = %last_error,
                        "mdtx bridge text timeout, retrying"
                    );
                }
            }

            if attempt < MAX_RETRIES {
                let backoff = std::time::Duration::from_secs(2_u64.pow(attempt as u32));
                tokio::time::sleep(backoff).await;
            }
        }

        Err(markdown_translator::error::Error::Pipeline(format!(
            "chat_text failed after {} attempts. Last error: {}",
            MAX_RETRIES + 1,
            last_error
        )))
    }
}

// ── Config bridge ───────────────────────────────────────────────────

/// Build a `markdown_translator::config::TranslatorConfig` from SkillStar's `AiConfig`.
pub fn build_translator_config(config: &AiConfig) -> TranslatorConfig {
    let target_lang = ai_provider::language_display_name(&config.target_language).to_string();

    // Derive concurrency from context_window_k.
    let max_parallel = ai_provider::resolve_scan_params(config)
        .max_concurrent_requests
        .clamp(1, 4) as usize;

    TranslatorConfig {
        target_languages: vec![target_lang],
        execution: markdown_translator::config::ExecutionConfig {
            max_parallel_translations: max_parallel,
        },
        provider: ProviderConfig {
            name: "skillstar-bridge".into(),
            base_url: config.base_url.clone(),
            api_key: Some(config.api_key.clone()),
            api_key_env: String::new(),
            model: config.model.clone(),
            temperature: 0.2,
            max_tokens: ai_provider::estimate_translation_max_tokens("x".repeat(8000).as_str()),
        },
        pipeline: markdown_translator::config::PipelineConfig {
            mode: PipelineMode::Balanced,
            enable_review: false,
            enable_format_guard: false,
            fail_on_validation_error: false, // Don't fail, just report — matches old behavior.
            ..Default::default()
        },
        segmentation: markdown_translator::config::SegmentationConfig {
            max_bundle_chars: 6000,
            max_bundle_segments: 36,
        },
        style: markdown_translator::config::StyleConfig {
            tone: "technical".into(),
            audience: "developers".into(),
            preserve_terms: vec![],
            instructions: vec![
                "Translate all human-readable prose.".into(),
                "Keep YAML keys unchanged. Keep the `name` field value exactly as original.".into(),
                "Do NOT translate code blocks, inline code, URLs, or identifiers.".into(),
                "Preserve document structure exactly.".into(),
            ],
        },
        ..Default::default()
    }
}

// ── Cache path ──────────────────────────────────────────────────────

fn mdtx_cache_db_path() -> PathBuf {
    crate::core::infra::paths::db_dir().join("mdtx_cache.db")
}

fn split_leading_frontmatter(text: &str) -> Option<(&str, &str)> {
    if !text.starts_with("---\n") {
        return None;
    }

    let mut offset = 4;
    for line in text[4..].split_inclusive('\n') {
        let trimmed = line.trim_end_matches('\n');
        if trimmed == "---" {
            let end = offset + line.len();
            return Some((&text[..end], &text[end..]));
        }
        offset += line.len();
    }

    if text[4..].ends_with("\n---") {
        let end = text.len();
        return Some((&text[..end], ""));
    }

    None
}

fn restore_frontmatter_if_missing(original: &str, translated: String) -> String {
    let Some((frontmatter, _)) = split_leading_frontmatter(original) else {
        return translated;
    };

    if translated.starts_with("---\n") {
        return translated;
    }

    if translated.is_empty() {
        return frontmatter.to_string();
    }

    if translated.starts_with('\n') {
        format!("{frontmatter}{translated}")
    } else {
        format!("{frontmatter}\n{translated}")
    }
}

// ── Public translation API ──────────────────────────────────────────

/// Translate SKILL.md content using the `markdown-translator` pipeline.
///
/// This replaces the old `translate_skill_with_section_cache` with AST-level
/// translation that provides better Markdown format preservation.
///
/// - Uses SkillStar's existing LLM provider (supports OpenAI / Anthropic / Local)
/// - Includes built-in SQLite segment cache (incremental translation)
/// - Performs structural validation after translation
pub async fn translate_skill_content(
    config: &AiConfig,
    content: &str,
    force_refresh: bool,
) -> Result<String, String> {
    let provider: Arc<dyn LlmProvider> = Arc::new(SkillStarProvider::new(config.clone()));
    translate_skill_content_with_provider(
        config,
        content,
        force_refresh,
        provider,
        None,
        Some(TranslationMdtxLogCtx {
            request_id: None,
            command: "ai_translate_skill",
        }),
    )
    .await
}

/// Same as [`translate_skill_content`], but uses a custom `LlmProvider` (crate tests / tooling).
pub(crate) async fn translate_skill_content_with_provider(
    config: &AiConfig,
    content: &str,
    force_refresh: bool,
    provider: Arc<dyn LlmProvider>,
    progress: Option<Arc<dyn Fn(PipelineProgressEvent) + Send + Sync>>,
    log_ctx: Option<TranslationMdtxLogCtx>,
) -> Result<String, String> {
    let target_lang = ai_provider::language_display_name(&config.target_language).to_string();
    let content_sha256 = sha256_hex(content.as_bytes());
    let mdtx_started = Instant::now();

    info!(
        target: "translate",
        engine = "mdtx",
        content_len = content.len(),
        target_lang = %target_lang,
        force_refresh,
        "SKILL.md translation start"
    );

    // Build cache (skip if force_refresh).
    let cache = if force_refresh {
        None
    } else {
        TranslationCache::open(&mdtx_cache_db_path())
            .map(Some)
            .unwrap_or_else(|e| {
                tracing::warn!(error = %e, "mdtx cache open failed, proceeding without cache");
                None
            })
    };

    // Build pipeline.
    let translator_config = build_translator_config(config);
    let pipeline = TranslationPipeline::new(translator_config, provider, cache);

    // Write content to temp file for pipeline (it expects a file path).
    let temp_dir = std::env::temp_dir().join("skillstar_mdtx");
    std::fs::create_dir_all(&temp_dir).map_err(|e| format!("temp dir: {e}"))?;
    let temp_file = temp_dir.join("__skill__.md");
    std::fs::write(&temp_file, content).map_err(|e| format!("temp write: {e}"))?;

    let options = RunOptions {
        write_output: false,
        output_path: None,
        progress,
    };

    let pipeline_outcome = pipeline.run(&temp_file, &target_lang, &options).await;
    let mdtx_ms = mdtx_started.elapsed().as_millis();

    // Clean up temp file.
    let _ = std::fs::remove_file(&temp_file);

    let result = match pipeline_outcome {
        Ok(r) => r,
        Err(e) => {
            let msg = format!("mdtx pipeline: {e}");
            if let Some(ref ctx) = log_ctx {
                translation_log::skill_mdtx_error(
                    ctx,
                    &content_sha256,
                    content.len(),
                    &target_lang,
                    force_refresh,
                    mdtx_ms,
                    &msg,
                );
            }
            return Err(msg);
        }
    };

    let usage = result.api_usage.snapshot();
    info!(
        target: "translate",
        engine = "mdtx",
        segments = result.segments.len(),
        translations = result.translations.len(),
        validation_passed = result.validation.passed,
        api_calls = usage.call_count,
        "SKILL.md translation done"
    );

    if let Some(ref ctx) = log_ctx {
        translation_log::skill_mdtx_complete(
            ctx,
            &content_sha256,
            content.len(),
            &target_lang,
            force_refresh,
            mdtx_ms,
            result.segments.len(),
            result.translations.len(),
            result.validation.passed,
            usage.call_count,
        );
    }

    if !result.validation.passed {
        tracing::warn!(
            target: "translate",
            errors = ?result.validation.errors,
            "mdtx validation warnings (non-fatal)"
        );
    }

    Ok(restore_frontmatter_if_missing(
        content,
        result.translated_text,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use markdown_translator::error::Error as MdtxError;
    use serde_json::json;

    /// Parses translator `user_prompt` JSON and returns deterministic translations (no HTTP).
    struct JsonEchoMock {
        usage: ApiUsage,
    }

    impl JsonEchoMock {
        fn new() -> Self {
            Self {
                usage: ApiUsage::new(),
            }
        }
    }

    #[async_trait::async_trait]
    impl LlmProvider for JsonEchoMock {
        async fn chat_json(
            &self,
            _system_prompt: &str,
            user_prompt: &str,
            call_label: &str,
        ) -> markdown_translator::error::Result<serde_json::Value> {
            let payload: serde_json::Value = serde_json::from_str(user_prompt).map_err(|e| {
                MdtxError::Pipeline(format!("mock {call_label}: invalid user JSON: {e}"))
            })?;
            let task = payload.get("task").and_then(|t| t.as_str()).unwrap_or("");
            if task != "translate" {
                return Err(MdtxError::Pipeline(format!(
                    "mock: unexpected task {task:?} in {call_label}"
                )));
            }
            let Some(segments) = payload.get("segments").and_then(|s| s.as_array()) else {
                return Err(MdtxError::Pipeline("mock: missing segments array".into()));
            };
            let translations: Vec<serde_json::Value> = segments
                .iter()
                .filter_map(|seg| {
                    let id = seg.get("segment_id")?.as_str()?;
                    let text = seg.get("text")?.as_str()?;
                    Some(json!({
                        "segment_id": id,
                        "translated_text": format!("[mock] {text}"),
                        "confidence": 1.0
                    }))
                })
                .collect();
            Ok(json!({ "translations": translations }))
        }

        async fn chat_text(
            &self,
            _system_prompt: &str,
            user_prompt: &str,
            call_label: &str,
        ) -> markdown_translator::error::Result<String> {
            if call_label != "line_translate" {
                return Err(MdtxError::Pipeline(format!(
                    "mock: unexpected chat_text call {call_label:?}"
                )));
            }

            let mut out = Vec::new();
            for line in user_prompt.lines() {
                let trimmed = line.trim();
                if !trimmed.starts_with("FRAGMENT|") {
                    continue;
                }
                let rest = &trimmed["FRAGMENT|".len()..];
                let Some((fragment_id, text)) = rest.split_once('|') else {
                    continue;
                };
                let translated = text.replace("{{PIPE}}", "|");
                out.push(format!("FRAGMENT|{fragment_id}|[mock] {translated}"));
            }

            if out.is_empty() {
                return Err(MdtxError::Pipeline(
                    "mock: no FRAGMENT lines found in line_translate prompt".into(),
                ));
            }

            Ok(out.join("\n"))
        }

        fn usage(&self) -> &ApiUsage {
            &self.usage
        }
    }

    #[tokio::test]
    async fn skill_md_chain_runs_mdtx_pipeline_with_mock_llm() {
        let config = AiConfig::default();
        let skill = "---\nname: demo-skill\n---\n\n# Title\n\nParagraph one.\n\n- Tight bullet A\n- Tight bullet B\n";
        let provider = Arc::new(JsonEchoMock::new());
        let out = translate_skill_content_with_provider(&config, skill, true, provider, None, None)
            .await
            .expect("mdtx pipeline should complete");

        assert!(
            out.contains("[mock]"),
            "expected mock translator output in rendered markdown: {out}"
        );
        assert!(
            out.contains("[mock] Tight bullet A") && out.contains("[mock] Tight bullet B"),
            "tight list items should pass through the pipeline: {out}"
        );
        assert!(
            out.contains("demo-skill") || out.contains("name:"),
            "expected front matter preserved: {out}"
        );
    }

    #[test]
    fn build_translator_config_maps_ai_config() {
        let mut config = AiConfig::default();
        config.target_language = "ja".to_string();
        config.base_url = "https://api.example.com".to_string();
        config.api_key = "secret".to_string();
        config.model = "gpt-4".to_string();
        config.context_window_k = 64;
        config.max_concurrent_requests = 2;

        let tc = build_translator_config(&config);

        assert_eq!(tc.target_languages, vec!["Japanese"]);
        assert_eq!(tc.provider.name, "skillstar-bridge");
        assert_eq!(tc.provider.base_url, "https://api.example.com");
        assert_eq!(tc.provider.api_key, Some("secret".to_string()));
        assert_eq!(tc.provider.model, "gpt-4");
        assert_eq!(tc.execution.max_parallel_translations, 2);
        assert_eq!(tc.provider.temperature, 0.2);
        assert!(tc.pipeline.mode == PipelineMode::Balanced);
    }

    #[test]
    fn split_leading_frontmatter_basic() {
        let text = "---\nname: demo\n---\n\n# Body\n";
        let (fm, rest) = split_leading_frontmatter(text).unwrap();
        assert_eq!(fm, "---\nname: demo\n---\n");
        assert_eq!(rest, "\n# Body\n");
    }

    #[test]
    fn split_leading_frontmatter_no_terminator() {
        let text = "---\nname: demo\n";
        assert!(split_leading_frontmatter(text).is_none());
    }

    #[test]
    fn split_leading_frontmatter_no_leading_dashes() {
        let text = "# Title\n---\nname: demo\n---\n";
        assert!(split_leading_frontmatter(text).is_none());
    }

    #[test]
    fn restore_frontmatter_if_missing_preserves_when_present() {
        let original = "---\nname: demo\n---\n\n# Title\n";
        let translated = "---\nname: demo\n---\n\n# 标题\n".to_string();
        assert_eq!(
            restore_frontmatter_if_missing(original, translated.clone()),
            translated
        );
    }

    #[test]
    fn restore_frontmatter_if_missing_adds_when_absent() {
        let original = "---\nname: demo\n---\n\n# Title\n";
        let translated = "# 标题\n".to_string();
        let result = restore_frontmatter_if_missing(original, translated);
        assert!(result.starts_with("---\nname: demo\n---\n"));
        assert!(result.contains("# 标题"));
    }

    #[test]
    fn restore_frontmatter_if_missing_handles_empty_translation() {
        let original = "---\nname: demo\n---\n\n# Title\n";
        let translated = String::new();
        let result = restore_frontmatter_if_missing(original, translated);
        assert_eq!(result, "---\nname: demo\n---\n");
    }

    #[test]
    fn restore_frontmatter_if_missing_handles_leading_newline() {
        let original = "---\nname: demo\n---\n\n# Title\n";
        let translated = "\n# 标题\n".to_string();
        let result = restore_frontmatter_if_missing(original, translated);
        assert!(result.starts_with("---\nname: demo\n---\n"));
        assert!(result.contains("# 标题"));
    }

    struct FailingMock;

    #[async_trait::async_trait]
    impl LlmProvider for FailingMock {
        async fn chat_json(
            &self,
            _system_prompt: &str,
            _user_prompt: &str,
            _call_label: &str,
        ) -> MdtxResult<serde_json::Value> {
            Err(MdtxError::Pipeline("mock failure".into()))
        }

        async fn chat_text(
            &self,
            _system_prompt: &str,
            _user_prompt: &str,
            _call_label: &str,
        ) -> MdtxResult<String> {
            Err(MdtxError::Pipeline("mock failure".into()))
        }

        fn usage(&self) -> &ApiUsage {
            static USAGE: std::sync::OnceLock<ApiUsage> = std::sync::OnceLock::new();
            USAGE.get_or_init(ApiUsage::new)
        }
    }

    #[tokio::test]
    async fn skill_md_chain_returns_error_when_pipeline_fails() {
        let config = AiConfig::default();
        let skill = "# Title\n\nParagraph.\n";
        let provider = Arc::new(FailingMock);
        let result =
            translate_skill_content_with_provider(&config, skill, true, provider, None, None).await;

        assert!(result.is_err(), "expected error when pipeline fails");
        let err = result.unwrap_err();
        assert!(
            err.contains("mdtx pipeline") || err.contains("mock failure"),
            "error should mention pipeline or mock: {err}"
        );
    }

    #[test]
    fn build_translator_config_clamps_max_parallel() {
        let mut config = AiConfig::default();
        config.context_window_k = 128;
        config.max_concurrent_requests = 10;

        let tc = build_translator_config(&config);
        assert_eq!(tc.execution.max_parallel_translations, 4);
    }
}
