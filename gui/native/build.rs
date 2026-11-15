//! Expose the repository root's product version — `[package].version` in
//! `../../Cargo.toml`, the field release checks compare — to the crate as
//! `SKILLSTAR_ROOT_VERSION`. The bridge reports it to the shell in later
//! waves (about pane, release check).

use std::{env, fs, path::PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let root = manifest_dir.join("../../Cargo.toml");
    println!("cargo:rerun-if-changed={}", root.display());
    println!(
        "cargo:rerun-if-changed={}",
        manifest_dir.join("../assets/icon.png").display()
    );

    let version = read_root_version(&root).unwrap_or_else(|| "unknown".to_owned());
    println!("cargo:rustc-env=SKILLSTAR_ROOT_VERSION={version}");
}

/// Minimal `[package].version` reader. The root manifest keeps the version
/// literal inside the `[package]` table, before any other table.
fn read_root_version(path: &PathBuf) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    text.lines()
        .skip_while(|line| !line.starts_with("[package]"))
        .skip(1)
        .take_while(|line| !line.starts_with('['))
        .find_map(|line| line.strip_prefix("version = "))
        .map(|value| value.trim().trim_matches('"').to_owned())
}
