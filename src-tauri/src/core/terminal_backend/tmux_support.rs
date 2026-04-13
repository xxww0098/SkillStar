use std::path::Path;
#[cfg(target_os = "windows")]
use std::path::PathBuf;

use super::types::TmuxStatus;

fn read_tmux_version(output: std::process::Output) -> Option<String> {
    if !output.status.success() {
        return None;
    }

    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if version.is_empty() {
        Some("tmux".to_string())
    } else {
        Some(version)
    }
}

fn tmux_version_from_executable(executable: &Path) -> Option<String> {
    let output = std::process::Command::new(executable)
        .arg("-V")
        .output()
        .ok()?;
    read_tmux_version(output)
}

#[cfg(target_os = "windows")]
fn windows_bash_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(path) = which::which("bash") {
        candidates.push(path);
    }

    if let Ok(program_files) = std::env::var("ProgramFiles") {
        let base = PathBuf::from(program_files);
        candidates.push(base.join("Git\\bin\\bash.exe"));
        candidates.push(base.join("Git\\usr\\bin\\bash.exe"));
    }

    if let Ok(program_files_x86) = std::env::var("ProgramFiles(x86)") {
        let base = PathBuf::from(program_files_x86);
        candidates.push(base.join("Git\\bin\\bash.exe"));
        candidates.push(base.join("Git\\usr\\bin\\bash.exe"));
    }

    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        let base = PathBuf::from(local_app_data);
        candidates.push(base.join("Programs\\Git\\bin\\bash.exe"));
        candidates.push(base.join("Programs\\MSYS2\\usr\\bin\\bash.exe"));
    }

    candidates.push(PathBuf::from(r"C:\msys64\usr\bin\bash.exe"));
    if let Some(home) = dirs::home_dir() {
        candidates.push(home.join("msys64\\usr\\bin\\bash.exe"));
    }

    candidates
}

#[cfg(target_os = "windows")]
fn windows_tmux_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(path) = which::which("tmux") {
        candidates.push(path);
    }

    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        let base = PathBuf::from(local_app_data);
        candidates.push(base.join("Programs\\MSYS2\\usr\\bin\\tmux.exe"));
    }

    candidates.push(PathBuf::from(r"C:\msys64\usr\bin\tmux.exe"));
    if let Some(home) = dirs::home_dir() {
        candidates.push(home.join("msys64\\usr\\bin\\tmux.exe"));
    }

    candidates
}

#[cfg(target_os = "windows")]
fn tmux_version_via_bash(bash: &Path) -> Option<String> {
    let output = std::process::Command::new(bash)
        .args(["--login", "-c", "tmux -V"])
        .output()
        .ok()?;
    read_tmux_version(output)
}

#[cfg(target_os = "windows")]
pub(crate) fn resolve_windows_bash_with_tmux() -> Option<(PathBuf, String)> {
    let mut seen = std::collections::HashSet::new();
    for candidate in windows_bash_candidates() {
        if !seen.insert(candidate.clone()) || !candidate.exists() {
            continue;
        }

        if let Some(version) = tmux_version_via_bash(&candidate) {
            return Some((candidate, version));
        }
    }
    None
}

/// Check if tmux is installed and get its version.
pub(crate) fn check_tmux() -> TmuxStatus {
    if let Ok(tmux) = which::which("tmux") {
        if let Some(version) = tmux_version_from_executable(&tmux) {
            return TmuxStatus {
                installed: true,
                version: Some(version),
            };
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Some((_, version)) = resolve_windows_bash_with_tmux() {
            return TmuxStatus {
                installed: true,
                version: Some(version),
            };
        }

        let mut seen = std::collections::HashSet::new();
        for tmux in windows_tmux_candidates() {
            if !seen.insert(tmux.clone()) || !tmux.exists() {
                continue;
            }

            if let Some(version) = tmux_version_from_executable(&tmux) {
                return TmuxStatus {
                    installed: true,
                    version: Some(version),
                };
            }
        }
    }

    TmuxStatus {
        installed: false,
        version: None,
    }
}
