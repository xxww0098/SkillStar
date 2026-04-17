//! # markdown-translator
//!
//! High-performance AI Agent-based Markdown translation pipeline.
//!
//! ## Architecture
//!
//! ```text
//! Input.md → Parser → SegmentExtractor → Orchestrator → TranslatorAgent
//!                                                         → ReviewerAgent
//!                                                         → FormatGuardAgent
//!         → AstMapper → Validator → Output.md
//! ```
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use markdown_translator::{config::TranslatorConfig, translate_file};
//!
//! # async fn example() -> markdown_translator::error::Result<()> {
//! let config = TranslatorConfig::default();
//! let result = translate_file("README.md", "chinese", &config, None).await?;
//! println!("Translated to: {:?}", result.output_path);
//! # Ok(())
//! # }
//! ```

pub mod agents;
pub mod cache;
pub mod config;
pub mod error;
pub mod parser;
pub mod pipeline;
pub use pipeline::PipelineProgressEvent;
pub mod provider;
pub mod types;
pub mod validator;

use std::path::Path;
use std::sync::Arc;

use config::TranslatorConfig;
use error::Result;
use pipeline::{RunOptions, TranslationPipeline};
use provider::openai::OpenAiProvider;
use types::PipelineResult;

/// Translate a Markdown file to the specified target language.
///
/// This is the primary public API. It sets up the full pipeline (provider, cache,
/// agents) from the given config and runs the translation.
///
/// # Arguments
/// * `input` — Path to the input Markdown file.
/// * `target_lang` — Target language name (e.g., "chinese", "japanese", "english").
/// * `config` — Translator configuration.
/// * `cache_db_path` — Optional path to a SQLite cache file. If `None`, caching is disabled.
pub async fn translate_file(
    input: impl AsRef<Path>,
    target_lang: &str,
    config: &TranslatorConfig,
    cache_db_path: Option<&Path>,
) -> Result<PipelineResult> {
    let provider = Arc::new(OpenAiProvider::new(&config.provider));
    let translation_cache = match cache_db_path {
        Some(path) => Some(cache::TranslationCache::open(path)?),
        None => None,
    };

    let pipeline = TranslationPipeline::new(config.clone(), provider, translation_cache);
    pipeline
        .run(input.as_ref(), target_lang, &RunOptions::default())
        .await
}

/// Translate a Markdown string in-memory (no file I/O).
///
/// Useful for programmatic usage or when integrating into other systems.
pub async fn translate_text(
    source_text: &str,
    target_lang: &str,
    config: &TranslatorConfig,
) -> Result<String> {
    let provider = Arc::new(OpenAiProvider::new(&config.provider));
    let pipeline = TranslationPipeline::new(config.clone(), provider, None);

    // Write to a temp path for parsing, but don't write output.
    let temp_path = std::path::PathBuf::from("__inline__.md");
    // We need to write the source to disk temporarily for the pipeline.
    // Instead, use the parser directly.
    let doc = parser::parse(source_text, &temp_path, target_lang);
    let ctx = pipeline::context::build(&doc, &doc.segments, config);
    let _bundles = pipeline::orchestrator::build_bundles(&doc.segments, &ctx, config);

    // For inline translation, use the pipeline's run with write_output=false.
    let options = RunOptions {
        write_output: false,
        output_path: None,
        progress: None,
    };

    // Write temp file, run pipeline, clean up.
    let temp_dir = std::env::temp_dir().join("mdtx");
    std::fs::create_dir_all(&temp_dir)?;
    let temp_file = temp_dir.join("__inline__.md");
    std::fs::write(&temp_file, source_text)?;

    let result = pipeline.run(&temp_file, target_lang, &options).await?;

    // Clean up temp file.
    let _ = std::fs::remove_file(&temp_file);

    Ok(result.translated_text)
}
