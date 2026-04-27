use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::types::PipelineMode;

/// Top-level translator configuration, compatible with the Python version's `config.yaml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TranslatorConfig {
    pub target_languages: Vec<String>,
    pub execution: ExecutionConfig,
    pub provider: ProviderConfig,
    pub pipeline: PipelineConfig,
    pub segmentation: SegmentationConfig,
    pub input: InputConfig,
    pub style: StyleConfig,
    pub output: OutputConfig,
}

impl Default for TranslatorConfig {
    fn default() -> Self {
        Self {
            target_languages: vec!["english".into()],
            execution: ExecutionConfig::default(),
            provider: ProviderConfig::default(),
            pipeline: PipelineConfig::default(),
            segmentation: SegmentationConfig::default(),
            input: InputConfig::default(),
            style: StyleConfig::default(),
            output: OutputConfig::default(),
        }
    }
}

impl TranslatorConfig {
    /// Load configuration from a YAML file.
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).map_err(|e| Error::Config {
            message: format!("failed to read config {}: {e}", path.display()),
        })?;
        Self::from_yaml(&content)
    }

    /// Parse configuration from a YAML string.
    pub fn from_yaml(yaml: &str) -> Result<Self> {
        serde_yaml::from_str(yaml).map_err(|e| Error::Config {
            message: format!("invalid config YAML: {e}"),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ExecutionConfig {
    pub max_parallel_translations: usize,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            max_parallel_translations: 10,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderConfig {
    pub name: String,
    pub base_url: String,
    #[serde(default)]
    pub api_key: Option<String>,
    pub api_key_env: String,
    pub model: String,
    pub temperature: f64,
    pub max_tokens: u32,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            name: "openai".into(),
            base_url: "https://api.openai.com/v1".into(),
            api_key: None,
            api_key_env: "OPENAI_API_KEY".into(),
            model: "gpt-4o".into(),
            temperature: 0.2,
            max_tokens: 8000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PipelineConfig {
    pub mode: PipelineMode,
    pub enable_review: bool,
    pub enable_format_guard: bool,
    pub fail_on_validation_error: bool,
    /// Use line_pipeline.translate_document() instead of translate_bundles() + mapper::apply().
    /// When true (default), uses the line-based approach that avoids byte-range issues.
    pub use_line_pipeline: bool,
    /// Minimum number of segments in a bundle to trigger review in balanced mode.
    pub review_min_segments: usize,
    /// Minimum total chars in a bundle to trigger review in balanced mode.
    pub review_min_bundle_chars: usize,
    /// Confidence threshold below which review is triggered in balanced mode.
    pub review_confidence_threshold: f64,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            mode: PipelineMode::Balanced,
            enable_review: false,
            enable_format_guard: false,
            fail_on_validation_error: true,
            use_line_pipeline: true,
            review_min_segments: 8,
            review_min_bundle_chars: 3000,
            review_confidence_threshold: 0.7,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SegmentationConfig {
    pub max_bundle_chars: usize,
    pub max_bundle_segments: usize,
}

impl Default for SegmentationConfig {
    fn default() -> Self {
        Self {
            max_bundle_chars: 6000,
            max_bundle_segments: 36,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct InputConfig {
    pub file_pattern: String,
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            file_pattern: "*.md".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StyleConfig {
    pub tone: String,
    pub audience: String,
    #[serde(default)]
    pub preserve_terms: Vec<String>,
    #[serde(default)]
    pub instructions: Vec<String>,
}

impl Default for StyleConfig {
    fn default() -> Self {
        Self {
            tone: "technical".into(),
            audience: "developers".into(),
            preserve_terms: vec![],
            instructions: vec![
                "Keep protected placeholders unchanged.".into(),
                "Do not alter Markdown control syntax.".into(),
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputConfig {
    pub directory: String,
    pub file_suffix_template: String,
    pub write_report: bool,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            directory: "output".into(),
            file_suffix_template: "{stem}.{lang}.md".into(),
            write_report: false,
        }
    }
}
