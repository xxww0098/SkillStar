//! Keep `gen/bundle.js` — the compiled TSX bundle `src/lib.rs` embeds with
//! `include_str!` — present and **fresh** for `cargo check`/`cargo build`.
//!
//! The freshness rule is the point. Cargo only reruns this script when a file
//! it names changes, and `include_str!` alone does not know that the TSX and
//! the bridge crate are inputs: an embedded bundle that is merely out of date
//! ships a UI that imports bridge exports which no longer exist, and the app
//! dies at start with `bundle failed: Could not find export …` (measured
//! 2026-11-15 after the SSH scope was deleted). So this script
//! (a) tells cargo to rerun when the app sources change, and
//! (b) rebuilds the bundle whenever it is missing, empty, or older than the
//! newest source file.
//!
//! States:
//! 1. Bundle fresh → nothing to do.
//! 2. Stale or missing, and `nsh` is on PATH (or at ~/.nsh/bin/nsh) → run the
//!    documented prep step here. `nsh build` writes to `-o`, never into
//!    tracked files.
//! 3. Stale or missing and no `nsh` → empty stub (first build) or keep the
//!    stale bundle (later builds) plus a `cargo:warning`. `cargo check` stays
//!    green; `run()` refuses an empty stub, so a stub never silently becomes
//!    the shipped UI.
//!
//! The script never modifies anything outside `gen/` next to this crate.

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    time::SystemTime,
};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let gen_dir = manifest_dir.join("gen");
    let bundle = gen_dir.join("bundle.js");
    let app_dir = manifest_dir
        .parent()
        .expect("host/ sits inside gui/")
        .to_path_buf();

    // Inputs: the TSX app, its config/theme, and the bridge crate the bundle
    // imports. Cargo reruns us when any of them changes.
    let inputs = [
        app_dir.join("App.tsx"),
        app_dir.join("nsh.json"),
        app_dir.join("theme.json"),
        app_dir.join("assets"),
        app_dir.join("src"),
        app_dir.join("native/src"),
        app_dir.join("native/Cargo.toml"),
    ];
    for input in &inputs {
        println!("cargo:rerun-if-changed={}", input.display());
    }
    println!("cargo:rerun-if-changed={}", bundle.display());
    println!("cargo:rerun-if-changed=build.rs");

    let bundle_fresh = bundle_age_seconds(&bundle).is_some_and(|bundle_age| {
        newest_source_mtime(&inputs)
            .and_then(|newest| SystemTime::now().duration_since(newest).ok())
            .is_some_and(|source_age| source_age >= bundle_age)
    });
    if bundle_fresh {
        return;
    }

    let rebuilt = which_nsh().is_some_and(|nsh| {
        let status = Command::new(&nsh)
            .arg("build")
            .arg("-o")
            .arg(&gen_dir)
            .arg(&app_dir)
            .status();
        matches!(status, Ok(status) if status.success())
            && fs::metadata(&bundle).map(|m| m.len() > 0).unwrap_or(false)
    });
    if rebuilt {
        return;
    }

    let _ = fs::create_dir_all(&gen_dir);
    let has_bundle = fs::metadata(&bundle).map(|m| m.len() > 0).unwrap_or(false);
    if has_bundle {
        println!(
            "cargo:warning=gen/bundle.js is stale and was NOT rebuilt (no nsh CLI). \
             The embedded UI may reference exports that no longer exist. Rebuild it with: \
             nsh build -o {} {}",
            gen_dir.display(),
            app_dir.display()
        );
        return;
    }
    match fs::write(&bundle, "") {
        Ok(()) => println!(
            "cargo:warning=gen/bundle.js is an empty stub (no nsh CLI found or build failed). \
             cargo check passes, but the host refuses to run. Build the real bundle with: \
             nsh build -o {} {}",
            gen_dir.display(),
            app_dir.display()
        ),
        Err(err) => println!("cargo:warning=could not write stub gen/bundle.js: {err}"),
    }
}

/// How long ago `bundle` was written, or `None` when it is missing or empty.
fn bundle_age_seconds(bundle: &Path) -> Option<std::time::Duration> {
    let metadata = fs::metadata(bundle).ok()?;
    if metadata.len() == 0 {
        return None;
    }
    SystemTime::now().duration_since(metadata.modified().ok()?).ok()
}

/// The newest modification time among the bundle's inputs (recursing into
/// directories). `None` when nothing could be read.
fn newest_source_mtime(inputs: &[PathBuf]) -> Option<SystemTime> {
    inputs
        .iter()
        .filter_map(|path| newest_mtime(path))
        .max()
}

fn newest_mtime(path: &Path) -> Option<SystemTime> {
    let metadata = fs::metadata(path).ok()?;
    if metadata.is_file() {
        return metadata.modified().ok();
    }
    let mut newest = metadata.modified().ok();
    for entry in fs::read_dir(path).ok()? {
        let Ok(entry) = entry else { continue };
        if let Some(child) = newest_mtime(&entry.path()) {
            newest = Some(match newest {
                Some(current) if current >= child => current,
                _ => child,
            });
        }
    }
    newest
}

/// `nsh` on PATH, else the installer's default location.
fn which_nsh() -> Option<PathBuf> {
    if let Ok(path) = env::var("PATH") {
        for dir in path.split(':') {
            if dir.is_empty() {
                continue;
            }
            let candidate = PathBuf::from(dir).join("nsh");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    let home = env::var_os("HOME").map(PathBuf::from)?;
    let fallback = home.join(".nsh/bin/nsh");
    fallback.is_file().then_some(fallback)
}
