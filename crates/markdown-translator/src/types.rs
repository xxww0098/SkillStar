use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

// ─── Protected Spans ────────────────────────────────────────────────────────

/// Type of content that is protected from translation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpanType {
    Md,
    Code,
    Html,
    Url,
}

/// A region of text replaced with a placeholder before translation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtectedSpan {
    pub placeholder: String,
    pub original: String,
    pub span_type: SpanType,
}

// ─── Segments ───────────────────────────────────────────────────────────────

/// A single translatable unit extracted from the document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub segment_id: String,
    pub node_type: NodeType,
    pub source_text: String,
    pub context_path: Vec<String>,
    /// Byte range within the **body** text (after front matter).
    pub byte_range: Range<usize>,
    /// Line range (0-indexed) within the body text, for compatibility.
    pub line_start: usize,
    pub line_end: usize,
    pub protected_spans: Vec<ProtectedSpan>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// The structural type of a segment's parent node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeType {
    Paragraph,
    Heading,
    Blockquote,
    TableCell,
    FrontMatter,
}

impl NodeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Paragraph => "paragraph",
            Self::Heading => "heading",
            Self::Blockquote => "blockquote",
            Self::TableCell => "table_cell",
            Self::FrontMatter => "front_matter",
        }
    }
}

// ─── Bundles ────────────────────────────────────────────────────────────────

/// A batch of segments sent to the LLM in a single call.
#[derive(Debug, Clone)]
pub struct SegmentBundle {
    pub bundle_id: String,
    pub segments: Vec<Segment>,
    pub summary_before: String,
    pub summary_after: String,
    pub style_instructions: Vec<String>,
}

// ─── Translation Results ────────────────────────────────────────────────────

/// Result of translating a single segment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationResult {
    pub segment_id: String,
    pub translated_text: String,
    #[serde(default)]
    pub notes: Vec<String>,
    #[serde(default)]
    pub applied_terms: HashMap<String, String>,
    #[serde(default)]
    pub confidence: f64,
}

/// LLM response envelope (for serde deserialization).
#[derive(Debug, Deserialize)]
pub struct TranslationResponse {
    #[serde(default)]
    pub translations: Vec<TranslationResult>,
}

// ─── Document Context ───────────────────────────────────────────────────────

/// Contextual information extracted from the document, injected into prompts.
#[derive(Debug, Clone)]
pub struct DocumentContext {
    pub title: String,
    pub abstract_text: String,
    pub section_summaries: HashMap<String, String>,
    pub style_guide: Vec<String>,
    pub audience: String,
}

// ─── Parsed Document ────────────────────────────────────────────────────────

/// The result of parsing a Markdown document.
#[derive(Debug, Clone)]
pub struct ParsedDocument {
    pub source_path: PathBuf,
    pub source_text: String,
    pub body_text: String,
    pub target_lang: String,
    pub front_matter: Option<FrontMatter>,
    pub body_line_offset: usize,
    pub segments: Vec<Segment>,
}

/// YAML front matter data.
#[derive(Debug, Clone, Default)]
pub struct FrontMatter {
    pub raw: String,
    pub data: HashMap<String, serde_yaml::Value>,
}

// ─── Validation ─────────────────────────────────────────────────────────────

/// Result of validating a translated document.
#[derive(Debug, Clone, Default)]
pub struct ValidationReport {
    pub passed: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub metrics: HashMap<String, serde_json::Value>,
}

// ─── Pipeline Output ────────────────────────────────────────────────────────

/// Final output of a translation pipeline run.
#[derive(Debug)]
pub struct PipelineResult {
    pub input_path: PathBuf,
    pub output_path: Option<PathBuf>,
    pub translated_text: String,
    pub segments: Vec<Segment>,
    pub translations: Vec<TranslationResult>,
    pub validation: ValidationReport,
    pub api_usage: ApiUsage,
}

// ─── API Usage Tracking ─────────────────────────────────────────────────────

/// Lock-free token usage counter, shareable across async tasks.
#[derive(Debug, Clone, Default)]
pub struct ApiUsage {
    pub call_count: Arc<AtomicU64>,
    pub prompt_tokens: Arc<AtomicU64>,
    pub completion_tokens: Arc<AtomicU64>,
    pub total_tokens: Arc<AtomicU64>,
}

impl ApiUsage {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&self, prompt: u64, completion: u64, total: u64) {
        self.call_count.fetch_add(1, Ordering::Relaxed);
        self.prompt_tokens.fetch_add(prompt, Ordering::Relaxed);
        self.completion_tokens
            .fetch_add(completion, Ordering::Relaxed);
        self.total_tokens.fetch_add(total, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> ApiUsageSnapshot {
        ApiUsageSnapshot {
            call_count: self.call_count.load(Ordering::Relaxed),
            prompt_tokens: self.prompt_tokens.load(Ordering::Relaxed),
            completion_tokens: self.completion_tokens.load(Ordering::Relaxed),
            total_tokens: self.total_tokens.load(Ordering::Relaxed),
        }
    }
}

/// A point-in-time snapshot of API usage counters.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ApiUsageSnapshot {
    pub call_count: u64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

// ─── Pipeline Mode ──────────────────────────────────────────────────────────

/// Controls the strictness of the translation pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PipelineMode {
    Fast,
    #[default]
    Balanced,
    Strict,
}
