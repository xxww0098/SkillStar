use std::path::PathBuf;
use std::process::Command;

/// Build an enriched PATH that includes common binary directories.
///
/// GUI-launched desktop apps (Tauri) often inherit a minimal login PATH that
/// omits Homebrew, user-local bins, and agent self-install directories. Every
/// binary presence probe in SkillStar must search this enriched PATH via
/// [`which_in_enriched`] rather than the raw process `PATH`.
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

/// Resolve a CLI binary on the [`enriched_path`].
///
/// Returns the first matching absolute path, or `None` if the binary is not
/// reachable. Prefer this over `which::which` anywhere SkillStar decides
/// "is this agent/tool installed on this machine?".
pub fn which_in_enriched(binary: &str) -> Option<PathBuf> {
    let path_str = enriched_path();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    // `which_in` takes the PATH as a single OsStr (colon/semicolon-joined).
    which::which_in(binary, Some(path_str.as_str()), &cwd).ok()
}

/// Whether a CLI binary is reachable on the enriched PATH.
pub fn binary_on_enriched_path(binary: &str) -> bool {
    which_in_enriched(binary).is_some()
}

#[cfg(unix)]
fn enriched_path_unix() -> String {
    let current = std::env::var("PATH").unwrap_or_default();
    let home = dirs::home_dir().unwrap_or_default();

    let mut extra_dirs: Vec<String> = Vec::new();

    if cfg!(target_os = "macos") {
        extra_dirs.extend([
            "/opt/homebrew/bin".to_string(),
            "/opt/homebrew/sbin".to_string(),
        ]);
    }

    extra_dirs.extend([
        "/usr/local/bin".to_string(),
        "/usr/local/sbin".to_string(),
        "/usr/bin".to_string(),
        "/usr/sbin".to_string(),
        "/bin".to_string(),
        "/sbin".to_string(),
    ]);

    if !home.as_os_str().is_empty() {
        let home_str = home.to_string_lossy();
        // User-local + language toolchains
        extra_dirs.push(format!("{}/.local/bin", home_str));
        extra_dirs.push(format!("{}/.cargo/bin", home_str));
        extra_dirs.push(format!("{}/.bun/bin", home_str));
        extra_dirs.push(format!("{}/.volta/bin", home_str));
        extra_dirs.push(format!("{}/.npm-global/bin", home_str));
        // Agent self-install bins (their installers put CLIs here and may not
        // symlink into Homebrew /usr/local — GUI PATH often misses these).
        extra_dirs.push(format!("{}/.opencode/bin", home_str));
        extra_dirs.push(format!("{}/.grok/bin", home_str));
        extra_dirs.push(format!("{}/.claude/local/bin", home_str));

        if cfg!(target_os = "linux") {
            extra_dirs.push("/snap/bin".to_string());
        }
    }

    join_path_parts(&extra_dirs, &current, ':')
}

#[cfg(windows)]
fn enriched_path_windows() -> String {
    let current = std::env::var("PATH").unwrap_or_default();
    let home = dirs::home_dir().unwrap_or_default();

    let mut extra_dirs: Vec<String> = Vec::new();

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
        extra_dirs.push(format!("{}\\Programs\\Git\\cmd", local));
    }
    if let Ok(appdata) = std::env::var("APPDATA") {
        // npm global shims on Windows default to %APPDATA%\npm
        extra_dirs.push(format!("{}\\npm", appdata));
    }
    if !home.as_os_str().is_empty() {
        let home_str = home.to_string_lossy();
        extra_dirs.push(format!("{}\\scoop\\shims", home_str));
        extra_dirs.push(format!("{}\\.cargo\\bin", home_str));
        extra_dirs.push(format!("{}\\.local\\bin", home_str));
        extra_dirs.push(format!("{}\\.bun\\bin", home_str));
        extra_dirs.push(format!("{}\\.volta\\bin", home_str));
        extra_dirs.push(format!("{}\\.opencode\\bin", home_str));
        extra_dirs.push(format!("{}\\.grok\\bin", home_str));
        extra_dirs.push(format!("{}\\.claude\\local\\bin", home_str));
    }

    join_path_parts(&extra_dirs, &current, ';')
}

fn join_path_parts(extra_dirs: &[String], current: &str, sep: char) -> String {
    let mut parts: Vec<&str> = extra_dirs.iter().map(String::as_str).collect();
    for segment in current.split(sep) {
        if !segment.is_empty() && !parts.contains(&segment) {
            parts.push(segment);
        }
    }
    parts.join(&sep.to_string())
}

/// Create a [`Command`] with enriched PATH so it can find Homebrew/snap/scoop binaries.
pub fn command_with_path(program: &str) -> Command {
    let mut cmd = Command::new(program);
    cmd.env("PATH", enriched_path());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }

    cmd
}

/// Probe whether a desktop application is installed at a well-known path.
///
/// `app_name` is the product name without extension, e.g. `"Cursor"`, `"ZCode"`.
/// - macOS: `/Applications/{name}.app` and `~/Applications/{name}.app`
/// - Windows: `%LOCALAPPDATA%\Programs\{name}\{name}.exe`,
///   `%LOCALAPPDATA%\Programs\{name-lower}\{name}.exe`,
///   `%ProgramFiles%\{name}\{name}.exe`
/// - Linux: no stable official paths for these IDEs — returns false.
pub fn desktop_app_installed(app_name: &str) -> bool {
    desktop_app_path(app_name).is_some()
}

/// Resolve a desktop app install path when present (see [`desktop_app_installed`]).
pub fn desktop_app_path(app_name: &str) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let bundle = format!("{app_name}.app");
        let system = PathBuf::from("/Applications").join(&bundle);
        if system.is_dir() {
            return Some(system);
        }
        if let Some(home) = dirs::home_dir() {
            let user = home.join("Applications").join(&bundle);
            if user.is_dir() {
                return Some(user);
            }
        }
        None
    }
    #[cfg(target_os = "windows")]
    {
        use std::path::Path;
        let exe = format!("{app_name}.exe");
        let lower = app_name.to_ascii_lowercase();
        if let Some(local) = dirs::data_local_dir() {
            let candidates = [
                local.join("Programs").join(app_name).join(&exe),
                local.join("Programs").join(&lower).join(&exe),
            ];
            for c in candidates {
                if c.is_file() {
                    return Some(c);
                }
            }
        }
        if let Ok(pf) = std::env::var("ProgramFiles") {
            let c = Path::new(&pf).join(app_name).join(&exe);
            if c.is_file() {
                return Some(c);
            }
        }
        None
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = app_name;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enriched_path_preserves_existing_entries() {
        let original = std::env::var_os("PATH");
        unsafe {
            std::env::set_var("PATH", "/tmp/skillstar-custom-bin");
        }

        let path = enriched_path();
        assert!(path.contains("/tmp/skillstar-custom-bin"));

        match original {
            Some(value) => unsafe { std::env::set_var("PATH", value) },
            None => unsafe { std::env::remove_var("PATH") },
        }
    }

    #[test]
    fn enriched_path_includes_agent_self_install_bins() {
        let path = enriched_path();
        #[cfg(unix)]
        {
            // Home-relative segments appear whenever HOME resolves; on CI the
            // home dir is set, so these substrings must be present.
            assert!(
                path.contains(".opencode/bin") || path.contains(".opencode\\bin"),
                "enriched PATH must include ~/.opencode/bin; got {path}"
            );
            assert!(
                path.contains(".grok/bin") || path.contains(".grok\\bin"),
                "enriched PATH must include ~/.grok/bin; got {path}"
            );
            assert!(
                path.contains(".local/bin") || path.contains(".local\\bin"),
                "enriched PATH must include ~/.local/bin; got {path}"
            );
        }
        #[cfg(windows)]
        {
            assert!(
                path.to_ascii_lowercase().contains("opencode"),
                "enriched PATH must include .opencode\\bin; got {path}"
            );
        }
    }

    #[test]
    fn which_in_enriched_finds_cargo() {
        // cargo is present in this repo's toolchain; acts as a stable positive.
        assert!(
            which_in_enriched("cargo").is_some(),
            "cargo must resolve via enriched PATH in the Rust toolchain"
        );
    }

    #[test]
    fn which_in_enriched_missing_binary_returns_none() {
        assert!(
            which_in_enriched("skillstar-definitely-not-a-real-bin-xyz-99").is_none()
        );
    }

    #[test]
    fn desktop_app_missing_returns_none() {
        assert!(
            desktop_app_path("SkillStarNonexistentAppXYZ").is_none(),
            "unknown desktop app must not resolve"
        );
    }
}
