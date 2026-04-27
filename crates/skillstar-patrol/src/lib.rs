//! Patrol data, config, and pure helper logic.
//!
//! This crate owns the pure types and helpers for background patrol.
//! The `PatrolManager`, `PatrolInner`, and async `patrol_loop` live in
//! `src-tauri/src/core/patrol.rs` alongside Tauri/Tokio coupling.

pub mod config;
pub mod helpers;
pub mod types;

pub use config::{load_config, save_config};
pub use helpers::check_skill_update_local;
pub use types::{HubSkillEntry, PatrolCheckEvent, PatrolConfig, PatrolStatus};
