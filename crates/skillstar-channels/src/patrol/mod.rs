//! Patrol: config and event DTOs.
//!
//! D-081 removed the generic per-skill patrol checker — the loop in
//! `src-tauri/src/core/patrol.rs` runs the same lock-hash refresh the UI's
//! refresh button uses. This module owns only the persisted config and the
//! event/status DTOs.

pub mod config;
pub mod types;

pub use config::{load_config, save_config};
pub use types::{PatrolCheckEvent, PatrolConfig, PatrolStatus};
