//! AI provider integration: translation cache, and (future) split modules.

pub mod mdtx_bridge;
pub mod translation_cache;
pub mod translation_log;

/// AI provider config, translation, summarization, skill pick.
/// (will be split in a future phase)
#[allow(unused_imports)]
pub use super::ai_provider;
