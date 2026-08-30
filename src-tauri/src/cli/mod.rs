//! CLI process adapter for the shared `skillstar` binary.
//!
//! Domain/use-case handlers live in `skillstar_app::cli`. This module only
//! wires the process entry and optional Tauri-side marketplace snapshot init.

fn migrate_and_run() {
    skillstar_core::infra::migration::migrate_legacy_paths();
    if let Err(err) = crate::core::marketplace_snapshot::initialize() {
        eprintln!("⚠ Marketplace snapshot init failed: {err}");
    }
}

pub fn run(args: Vec<String>) {
    skillstar_app::cli::run(args, migrate_and_run);
}
