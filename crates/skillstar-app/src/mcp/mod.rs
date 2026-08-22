//! MCP cross-domain use cases.
//!
//! MCP spans two domain crates that must not know about each other:
//! `skillstar-marketplace` owns the catalog (what servers exist, in what
//! shapes) and `skillstar-models` owns the store and the per-tool projection
//! (what is installed, where it is written). Everything that reads one and
//! produces the other belongs here, per AGENTS.md's "cross-domain use cases go
//! in `skillstar-app`" rule.
//!
//! Until this module existed there was no MCP orchestration layer at all: the
//! Tauri command layer called both domain crates directly and carried ~200
//! lines of mapping logic of its own (`docs/others/mcp-current-state-audit.md`
//! §C.1). `src-tauri/src/commands/mcp_*.rs` is now command registration, DTO
//! adaptation and error mapping only.
//!
//! Layout:
//! - [`runtime`] — rank the shapes a server publishes against this machine.
//! - [`draft`] — registry server → prefilled `McpServerEntry`, provenance included.
//! - [`install`] — the pre-install confirmation payload (command preview + inputs)
//!   and the answers→entry fold behind it ([`install::preview_install`]).
//! - [`presets`] — curated catalog row → preset chip.

pub mod draft;
pub mod install;
pub mod presets;
pub mod runtime;

#[cfg(test)]
mod tests;

pub use draft::{registry_to_entry, registry_to_entry_for};
pub use install::{
    McpInstallAnswer, McpInstallInput, McpInstallInputScope, McpInstallInputVariable,
    McpInstallMissingInput, McpInstallPlan, McpInstallPreview, McpSecretPolicy, McpSecretStorage,
    build_install_plan, build_install_plan_with, preview_install,
};
pub use presets::curated_server_to_preset;
pub use runtime::{
    McpRuntimeCandidate, McpRuntimeSelection, McpRuntimeShape, select_runtime, select_runtime_with,
};
