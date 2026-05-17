//! Model provider configuration domain.
//!
//! This crate owns everything related to AI **model provider configuration**:
//!
//! - [`providers`]: persisted provider store (v1 per-app + v2 flat) with CRUD,
//!   migrations, and built-in presets, backed by
//!   `~/.skillstar/config/model_providers.json`.
//! - [`tool_sync`]: write provider credentials into external tool config files
//!   (Claude Code, Codex), with rolling backups and merge semantics.
//! - [`latency`]: provider `/models` endpoint reachability + latency probe used
//!   by the Health Dashboard.
//! - [`circuit_breaker`]: per-provider fault isolation persisted in
//!   `~/.skillstar/state/circuit_breakers.json`.
//! - [`provider_ref::AiProviderRef`]: the small reference type used by
//!   `AiConfig.provider_ref` to point into the provider store.
//!
//! Pure-inference logic (chat completion, summarisation, skill pick) lives in
//! `skillstar-ai`, which depends on this crate for provider resolution.

pub mod circuit_breaker;
pub mod latency;
pub mod provider_ref;
pub mod providers;
pub mod tool_sync;

pub use provider_ref::AiProviderRef;

#[cfg(test)]
mod providers_prop_tests;
