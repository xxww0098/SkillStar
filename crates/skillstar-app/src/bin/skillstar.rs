fn main() {
    // The CLI binary delegates to the library's cli module.
    // In practice, the Tauri app's main.rs handles CLI dispatch;
    // this binary exists as a standalone entry point for direct CLI usage.
    eprintln!("skillstar CLI binary — use via the Tauri app or pass subcommands.");
    std::process::exit(1);
}
