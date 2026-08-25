//! Patrol: config, types, and pure check/batch logic.
//!
//! Domain ownership:
//! - [`check`] — collect hub skills, prefetch, per-skill update detection
//! - [`config`] / [`types`] — persisted config and event DTOs
//!
//! Tauri owns only State, tokio spawn, and Emitter adapters (`src-tauri/src/core/patrol.rs`).

pub mod check;
pub mod config;
pub mod types;

pub use check::{
    check_hub_skills_local_in_session, check_skill_update_local_in_session, collect_hub_skills,
    detect_new_skills_in_cached_repos, prefetch_failed_repos_in_session,
};
pub use config::{load_config, save_config};
pub use types::{HubSkillEntry, PatrolCheckEvent, PatrolConfig, PatrolStatus};
