//! Error type for the local decision model.
//!
//! The reference implementation raises `ValueError` for every contract
//! violation, and the *text* of those messages is part of what callers see
//! (the CLI prints it, the UI shows it). Messages are therefore mirrored
//! verbatim in [`crate::contract`] and never re-worded here.

use std::path::PathBuf;

/// Result alias for this crate.
pub type Result<T> = std::result::Result<T, DecisionError>;

/// Everything that can go wrong while downloading, loading or running the
/// local decision model.
#[derive(Debug, thiserror::Error)]
pub enum DecisionError {
    /// The request payload violates the decision contract. Mirrors the
    /// reference implementation's `ValueError` messages.
    #[error("{0}")]
    Contract(String),

    /// A required model file is absent, truncated or fails its digest.
    #[error("{0}")]
    ModelFiles(String),

    /// Tensor/IO failure inside the inference stack.
    #[error("decision model inference failed: {0}")]
    Inference(String),

    /// Filesystem failure, with the path that caused it.
    #[error("decision model I/O failed at {path}: {source}")]
    Io {
        /// Path involved in the failure.
        path: PathBuf,
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },

    /// Remote download failure.
    #[error("decision model download failed: {0}")]
    Download(String),
}

impl DecisionError {
    /// Attach a path to a `std::io::Error`.
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    /// Contract violation with a verbatim reference-implementation message.
    pub(crate) fn contract(message: impl Into<String>) -> Self {
        Self::Contract(message.into())
    }
}

impl From<candle_core::Error> for DecisionError {
    fn from(error: candle_core::Error) -> Self {
        Self::Inference(error.to_string())
    }
}
