//! Patrol data, config, and pure helper logic.
//!
//! This module owns the pure types and config for background patrol.
//! The `PatrolManager`, `PatrolInner`, and async `patrol_loop` live in
//! `src-tauri/src/core/patrol.rs` alongside Tauri/Tokio coupling.
//!
//! The `check_skill_update_local` helper lives in `src-tauri/src/core/patrol.rs`
//! because it depends on `skillstar-skills` which would create a cyclic dependency.

pub mod config;
pub mod types;

pub use config::{load_config, save_config};
pub use types::{HubSkillEntry, PatrolCheckEvent, PatrolConfig, PatrolStatus};
