//! Model provider configuration + AI inference domain.
//!
//! - [`providers`]: provider store, presets, CRUD
//! - [`tool_sync`]: external tool config projection
//! - [`latency`]: provider health probes
//! - [`mcp`]: MCP types / local store helpers
//! - [`ai_provider`]: pure inference (chat, summarize, skill pick)
//!
//! Formerly split across `skillstar-models` + `skillstar-ai` (Wave 2A merge).

pub mod ai_provider;
pub mod diagnostics;
pub mod latency;
pub mod mcp;
mod provider_ref;
pub mod providers;
pub mod tool_sync;

pub use provider_ref::AiProviderRef;
