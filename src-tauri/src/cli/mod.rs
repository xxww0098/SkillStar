//! CLI entry for the `skillstar` binary (shared with the GUI build).
//!
//! Thin glue over `skillstar_app::cli`: this module implements the concrete
//! command handlers (install / update / remove / publish / doctor / pack) using
//! `crate::core` + the domain crates, and wires them into the shared CLI runner
//! via [`cli_handlers`].
//!
//! Split by concern for navigability:
//! - this `mod.rs` — input classification helpers + entry wiring.
//! - [`install`] — the `install`/`add` surface (repo / bundle / local dir).
//! - [`manage`] — update / remove / publish / doctor / pack commands.

use crate::core::marketplace;
use skillstar_app::cli::CliHandlers;
use std::path::{Path, PathBuf};

mod install;
mod manage;

/// Classify a raw `install`/`add` argument to route to the right installer.
pub(super) enum AddKind {
    /// Repo URL, owner/repo, or any git source handled by the scan+install pipeline.
    Repo,
    /// Local `.ags` / `.agd` bundle file on disk.
    Bundle(PathBuf),
    /// Local directory that contains (at least one) `SKILL.md` → adopt as local skill(s).
    LocalDir(PathBuf),
}

pub(super) fn classify_add_input(input: &str) -> AddKind {
    let trimmed = input.trim();

    // URL schemes and owner/repo fall through to Repo.
    let lower = trimmed.to_lowercase();
    if lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("git@")
        || lower.starts_with("ssh://")
    {
        return AddKind::Repo;
    }

    // Heuristic: anything with a filesystem separator or starting with . / ~ / an
    // absolute Windows drive letter is a path. `owner/repo` has exactly one slash
    // and no leading dot — keep the existing shorthand semantics.
    let looks_like_path = trimmed.starts_with('.')
        || trimmed.starts_with('/')
        || trimmed.starts_with('~')
        || trimmed.starts_with('\\')
        || (trimmed.len() >= 2 && trimmed.chars().nth(1) == Some(':'));

    if looks_like_path {
        let expanded = expand_tilde(trimmed);
        let path = PathBuf::from(&expanded);
        if is_bundle_file(&path) {
            return AddKind::Bundle(path);
        }
        if path.is_dir() {
            return AddKind::LocalDir(path);
        }
    }

    // Heuristic: no scheme but two or more segments separated by '/' AND no spaces
    // and the second-to-last segment is not a drive-looking token → treat as repo
    // shorthand (owner/repo possibly with subpath). This matches `Source::parse`.
    AddKind::Repo
}

fn expand_tilde(input: &str) -> String {
    if let Some(rest) = input.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest).to_string_lossy().to_string();
    }
    if input == "~"
        && let Some(home) = dirs::home_dir()
    {
        return home.to_string_lossy().to_string();
    }
    input.to_string()
}

fn is_bundle_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("ags") | Some("agd")
    )
}

fn migrate_and_run() {
    skillstar_core::infra::migration::migrate_legacy_paths();
    // Point the marketplace snapshot runtime at the real data dir + DB before
    // any CLI command (notably `find`) touches the snapshot. Without this the
    // snapshot falls back to a throwaway `/tmp` DB, which always looks empty
    // and triggers a blocking remote seed on every search.
    if let Err(err) = marketplace::initialize_local_snapshot() {
        eprintln!("⚠ Marketplace snapshot init failed: {err}");
    }
}

pub fn cli_handlers() -> CliHandlers {
    CliHandlers {
        migrate_and_run,
        install: install::cmd_install,
        update: manage::cmd_update,
        remove: manage::cmd_remove,
        publish: manage::cmd_publish,
        doctor: manage::cmd_doctor,
        pack_list: manage::cmd_pack_list,
        pack_remove: manage::cmd_pack_remove,
        gui: || {
            println!("Launching SkillStar GUI...");
        },
    }
}

pub fn run(args: Vec<String>) {
    skillstar_app::cli::run(args, cli_handlers());
}
