//! Launch Deck & Terminal: CLI detection, script generation, terminal launch,
//! deploy orchestration, and tmux support.

pub mod config;

/// Terminal backend: deploy, script generation, CLI detection.
pub use super::terminal_backend;
