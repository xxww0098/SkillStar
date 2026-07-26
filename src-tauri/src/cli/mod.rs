//! CLI process adapter for the shared `skillstar` binary.
//!
//! Domain/use-case handlers live in `skillstar_app::cli`. This module only
//! wires the process entry and optional Tauri-side marketplace snapshot init.

use skillstar_app::cli::{CliHandlers, default_handlers};

fn migrate_and_run() {
    skillstar_core::infra::migration::migrate_legacy_paths();
    if let Err(err) = crate::core::marketplace_snapshot::initialize() {
        eprintln!("⚠ Marketplace snapshot init failed: {err}");
    }
}

/// Handlers used when launched from the Tauri package (prefer Tauri snapshot init).
pub fn cli_handlers() -> CliHandlers {
    let mut handlers = default_handlers();
    handlers.migrate_and_run = migrate_and_run;
    handlers
}

pub fn run(args: Vec<String>) {
    skillstar_app::cli::run(args, cli_handlers());
}
