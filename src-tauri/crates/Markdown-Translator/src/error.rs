use std::path::PathBuf;

/// Top-level error type for the markdown translation pipeline.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("config error: {message}")]
    Config { message: String },

    #[error("parse error in {path}: {message}")]
    Parse { path: PathBuf, message: String },

    #[error("segment extraction failed: {0}")]
    Extraction(String),

    #[error("LLM API error ({status}): {body}")]
    Api { status: u16, body: String },

    #[error("LLM request failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error("LLM returned invalid JSON for bundle {bundle_id}: {source}")]
    LlmOutputParse {
        bundle_id: String,
        source: serde_json::Error,
    },

    #[error("bundle {bundle_id}: missing translations for segments: {missing:?}")]
    MissingSegments {
        bundle_id: String,
        missing: Vec<String>,
    },

    #[error("bundle {bundle_id}: model returned no translations")]
    EmptyResponse { bundle_id: String },

    #[error("structural validation failed: {details}")]
    Validation { details: String },

    #[error("translation cache error: {0}")]
    Cache(String),

    #[error("pipeline error: {0}")]
    Pipeline(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
