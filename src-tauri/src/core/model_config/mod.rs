//! Model configuration domain shim.
//!
//! Reusable provider/state/config logic lives in `skillstar-model-config`.
//! App-specific Tauri/OAuth/account wiring stays here.

#[allow(unused_imports)]
pub use skillstar_model_config::{
    atomic_write, circuit_breaker, claude, cloud_sync, codex, health, opencode, provider_states,
    providers, quota, speedtest,
};

pub mod codex_accounts;
pub mod codex_oauth;
pub mod gemini_oauth;
pub mod gemini_quota;
