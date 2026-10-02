//! App entry for saving Codex onto the local gateway.
//!
//! The route is an argument. This module does not read Usage or `auth.json`.

use skillstar_core::infra::paths::home_dir;
use skillstar_gateway::{ApplyError, apply_agent, published_origin};

pub use skillstar_gateway::CodexRoute;

/// Write the loopback Codex shape under the current home.
pub fn save_codex(route: CodexRoute, origin: &str) -> Result<(), ApplyError> {
    apply_agent("codex", route, origin, &home_dir())
}

/// Restore stashed Codex fields under the current home.
pub fn release_codex() -> Result<(), ApplyError> {
    skillstar_gateway::release_agent("codex", &home_dir())
}

/// Select `model_ref` as Codex's model and point it at the gateway.
///
/// This is the picker's save: the API route, the gateway's own origin, and a
/// release when the ref is empty.
pub fn save_codex_model(model_ref: &str) -> Result<(), ApplyError> {
    skillstar_gateway::apply_agent_with_model("codex", &published_origin(), &home_dir(), model_ref)
}
