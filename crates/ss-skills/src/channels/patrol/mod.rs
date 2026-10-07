//! Patrol: config and event DTOs.
//!
//! D-081 removed the generic per-skill patrol checker. The GPUI shell does
//! not run a background patrol loop; lock-hash refresh stays on the explicit
//! refresh path. This module owns only the persisted config and the
//! event/status DTOs.

pub mod config;
pub mod types;

pub use config::{load_config, save_config};
pub use types::{PatrolCheckEvent, PatrolConfig, PatrolStatus};
