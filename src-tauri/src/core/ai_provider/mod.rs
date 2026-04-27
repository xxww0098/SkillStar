//! AI provider domain shim.
//!
//! Reusable provider/state/config logic lives in `skillstar-ai`.
//! Translation-specific orchestration moved with the provider core for structural
//! coherence; T15 will further refine the translation boundary.

pub use skillstar_ai::ai_provider::*;

// Re-export nested modules so that `crate::core::ai_provider::config::AiConfig`
// and similar paths continue to resolve.
pub use skillstar_ai::ai_provider::config;
pub use skillstar_ai::ai_provider::constants;
pub use skillstar_ai::ai_provider::http_client;
pub use skillstar_ai::ai_provider::scan_params;
pub use skillstar_ai::ai_provider::skill_pick;
