//! App entry for saving Codex onto the local gateway.
//!
//! The route is an argument. This module does not read Usage or `auth.json`.

use skillstar_core::infra::paths::home_dir;
use skillstar_gateway::{ApplyError, apply_agent};

pub use skillstar_gateway::CodexRoute;

/// Write the loopback Codex shape under the current home.
pub fn save_codex(route: CodexRoute, origin: &str) -> Result<(), ApplyError> {
    apply_agent("codex", route, origin, &home_dir())
}

/// Restore stashed Codex fields under the current home.
pub fn release_codex() -> Result<(), ApplyError> {
    skillstar_gateway::release_agent("codex", &home_dir())
}
