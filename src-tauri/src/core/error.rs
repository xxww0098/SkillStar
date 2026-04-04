//! Unified error type for SkillStar backend.
//!
//! All Tauri commands should return `Result<T, AppError>` instead of
//! `Result<T, String>` so that:
//! 1. Error context is structured and retains its source chain.
//! 2. The frontend still receives a human-readable string (via `Serialize`).
//! 3. Core modules can use `?` without manual `.map_err(|e| e.to_string())`.

use thiserror::Error;

/// Application-level error type.
///
/// Implements `Serialize` as a plain string so Tauri can return it
/// to the frontend without the command helper needing `.map_err`.
#[derive(Debug, Error)]
pub enum AppError {
    // ── Domain errors ───────────────────────────────────────────────
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

    // ── Infrastructure errors (auto-converted via `From`) ───────────
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("Task join error: {0}")]
    TaskJoin(#[from] tokio::task::JoinError),

    // ── Catch-all for existing `anyhow` returns ─────────────────────
    #[error("{0}")]
    Anyhow(#[from] anyhow::Error),

    #[error("{0}")]
    Other(String),
}

// Tauri requires the error type to implement `Serialize`.
// We serialize as a plain string — the frontend only displays the message.
impl serde::Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

// Convenience: allow `String` → `AppError` for legacy callers.
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
