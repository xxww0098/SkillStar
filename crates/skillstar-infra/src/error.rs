//! Unified error type for SkillStar backend.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Skill '{name}' not found")]
    SkillNotFound { name: String },

    #[error("Lockfile error: {0}")]
    Lockfile(String),

    #[error("Git operation failed: {0}")]
    Git(String),

    #[error("Agent profile error: {0}")]
    AgentProfile(String),

    #[error("Project error: {0}")]
    Project(String),

    #[error("Marketplace error: {0}")]
    Marketplace(String),

    #[error("AI provider error: {0}")]
    AiProvider(String),

    #[allow(dead_code)]
    #[error("Security scan error: {0}")]
    SecurityScan(String),

    #[error("Bundle error: {0}")]
    Bundle(String),

    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("Task join error: {0}")]
    TaskJoin(#[from] tokio::task::JoinError),

    #[error("{0}")]
    Anyhow(#[from] anyhow::Error),

    #[error("{0}")]
    Other(String),
}

impl serde::Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl From<String> for AppError {
    fn from(s: String) -> Self {
        AppError::Other(s)
    }
}

impl From<&str> for AppError {
    fn from(s: &str) -> Self {
        AppError::Other(s.to_string())
    }
}
