use std::process::Command;

/// Build an enriched PATH that includes common binary directories.
///
/// GUI apps (especially on macOS) don't inherit the shell PATH from
/// `.zshrc`/`.bashrc`, so Homebrew/snap/Cargo-installed binaries (gh, git)
/// won't be found without explicit PATH enrichment.
///
/// This function is cross-platform:
/// - **macOS**: adds `/opt/homebrew/bin`, `/usr/local/bin`, etc.
/// - **Linux**: adds `~/.local/bin`, snap paths, `/usr/local/bin`, etc.
/// - **Windows**: adds common `gh` / `git` install locations under `Program Files`.
pub fn enriched_path() -> String {
    #[cfg(unix)]
    {
        enriched_path_unix()
    }
    #[cfg(windows)]
    {
        enriched_path_windows()
    }
}

#[cfg(unix)]
fn enriched_path_unix() -> String {
    let current = std::env::var("PATH").unwrap_or_default();
    let home = dirs::home_dir().unwrap_or_default();

    let mut extra_dirs: Vec<String> = Vec::new();

    // macOS: Homebrew ARM + Intel
    if cfg!(target_os = "macos") {
        extra_dirs.extend([
            "/opt/homebrew/bin".to_string(),
            "/opt/homebrew/sbin".to_string(),
        ]);
    }

    // Shared Unix paths
    extra_dirs.extend([
        "/usr/local/bin".to_string(),
        "/usr/local/sbin".to_string(),
        "/usr/bin".to_string(),
        "/usr/sbin".to_string(),
        "/bin".to_string(),
        "/sbin".to_string(),
    ]);

    // User-local directories (Linux snap, cargo, etc.)
    if !home.as_os_str().is_empty() {
        let home_str = home.to_string_lossy();
        extra_dirs.push(format!("{}/.local/bin", home_str));
        extra_dirs.push(format!("{}/.cargo/bin", home_str));

        // Linux: snap
        if cfg!(target_os = "linux") {
            extra_dirs.push("/snap/bin".to_string());
        }
    }

    let mut parts: Vec<&str> = extra_dirs.iter().map(|s| s.as_str()).collect();
    for segment in current.split(':') {
        if !segment.is_empty() && !parts.contains(&segment) {
            parts.push(segment);
        }
    }
    parts.join(":")
}

#[cfg(windows)]
fn enriched_path_windows() -> String {
    let current = std::env::var("PATH").unwrap_or_default();
    let home = dirs::home_dir().unwrap_or_default();

    let mut extra_dirs: Vec<String> = Vec::new();

    // Common Windows install locations for gh / git
    if let Ok(pf) = std::env::var("ProgramFiles") {
        extra_dirs.push(format!("{}\\GitHub CLI", pf));
        extra_dirs.push(format!("{}\\Git\\cmd", pf));
        extra_dirs.push(format!("{}\\Git\\bin", pf));
    }
    if let Ok(pf86) = std::env::var("ProgramFiles(x86)") {
        extra_dirs.push(format!("{}\\GitHub CLI", pf86));
        extra_dirs.push(format!("{}\\Git\\cmd", pf86));
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        // Scoop
        extra_dirs.push(format!("{}\\Programs\\Git\\cmd", local));
    }
    if !home.as_os_str().is_empty() {
        let home_str = home.to_string_lossy();
        // Scoop shims + cargo
        extra_dirs.push(format!("{}\\scoop\\shims", home_str));
        extra_dirs.push(format!("{}\\.cargo\\bin", home_str));
    }

    let mut parts: Vec<&str> = extra_dirs.iter().map(|s| s.as_str()).collect();
    for segment in current.split(';') {
        if !segment.is_empty() && !parts.contains(&segment) {
            parts.push(segment);
        }
    }
    parts.join(";")
}

/// Create a [`Command`] with enriched PATH so it can find Homebrew/snap/scoop binaries.
///
/// Use this instead of `Command::new()` for any external tool (git, gh, etc.)
/// that may be installed in a non-standard location.
///
/// On Windows, sets `CREATE_NO_WINDOW` to prevent CMD windows from flashing.
pub fn command_with_path(program: &str) -> Command {
    let mut cmd = Command::new(program);
    cmd.env("PATH", enriched_path());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW (0x08000000): prevents the console window from
        // flashing on screen when spawning git.exe / gh.exe.
        cmd.creation_flags(0x08000000);
    }

    cmd
}
