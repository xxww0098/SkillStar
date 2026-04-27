use std::process::Command;

/// Build an enriched PATH that includes common binary directories.
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
        extra_dirs.push(format!("{}/.local/bin", home_str));
        extra_dirs.push(format!("{}/.cargo/bin", home_str));

        if cfg!(target_os = "linux") {
            extra_dirs.push("/snap/bin".to_string());
        }
    }

    let mut parts: Vec<&str> = extra_dirs.iter().map(String::as_str).collect();
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
    if !home.as_os_str().is_empty() {
        let home_str = home.to_string_lossy();
        extra_dirs.push(format!("{}\\scoop\\shims", home_str));
        extra_dirs.push(format!("{}\\.cargo\\bin", home_str));
    }

    let mut parts: Vec<&str> = extra_dirs.iter().map(String::as_str).collect();
    for segment in current.split(';') {
        if !segment.is_empty() && !parts.contains(&segment) {
            parts.push(segment);
        }
    }
    parts.join(";")
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

#[cfg(test)]
mod tests {
    use super::enriched_path;

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
}
