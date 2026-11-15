//! Link flags for the product binary only.
//!
//! The debug link of `skillstar` prints `ld: __eh_frame section too large (max
//! 16MB)` because its own unwind tables outgrow ld64's compact unwind
//! encoding. The cure is `-Wl,-no_compact_unwind`, but applying it
//! workspace-wide (the old `.cargo/config.toml` rustflags) also removed the
//! compact unwind tables every other binary needs: in test and dev builds a
//! panic then could not unwind at all (`fatal runtime error: failed to
//! initiate panic, error 5`), which silently disabled `tokio`'s JoinError
//! fallback and every `catch_unwind` path. See docs/errors.md, entries
//! 2026-10-09 and 2026-11-15.
//!
//! `cargo:rustc-link-arg-bin` scopes the flag to this one binary: the warning
//! stays quiet here, and every test binary keeps working unwinding.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-arg-bin=skillstar=-Wl,-no_compact_unwind");
    }
}
