use std::process::Command;

/// Git variables that pin commands to the *calling* repository.
///
/// SkillStar always operates on other repos (skill checkouts, shared
/// channels, test fixtures). Inheriting these from a git hook or parent
/// process would retarget those commands at the host repo and can rewrite
/// `.git/config` / branches. `GIT_EXEC_PATH` is left alone — it locates the
/// git implementation, not a work tree.
const HOST_GIT_ENV_VARS: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
    "GIT_OBJECT_DIRECTORY",
    "GIT_PREFIX",
    "GIT_NAMESPACE",
];

/// Build an enriched PATH that includes common binary directories.
///
/// GUI-launched desktop apps often inherit a minimal login PATH that
/// omits Homebrew, user-local bins, and agent self-install directories.
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
///
/// Also strips inherited git work-tree variables so spawned `git` always
/// operates on `current_dir` / explicit args, never the host process repo.
pub fn command_with_path(program: &str) -> Command {
    let mut cmd = Command::new(program);
    cmd.env("PATH", enriched_path());
    for var in HOST_GIT_ENV_VARS {
        cmd.env_remove(var);
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }

    cmd
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
    fn command_with_path_strips_host_git_dir_so_fixture_repos_are_not_retargeted() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path();
        let init = std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(repo)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .status()
            .expect("spawn git init");
        assert!(init.success(), "git init of fixture repo must succeed");

        let previous_dir = std::env::var_os("GIT_DIR");
        let previous_tree = std::env::var_os("GIT_WORK_TREE");
        unsafe {
            std::env::set_var("GIT_DIR", "/this-is-not-a-git-dir");
            std::env::set_var("GIT_WORK_TREE", "/this-is-not-a-work-tree");
        }

        let isolated = command_with_path("git")
            .args(["rev-parse", "--is-inside-work-tree"])
            .current_dir(repo)
            .output()
            .expect("spawn isolated git");

        let inherited = std::process::Command::new("git")
            .args(["rev-parse", "--is-inside-work-tree"])
            .current_dir(repo)
            .output()
            .expect("spawn inherited git");

        match previous_dir {
            Some(value) => unsafe { std::env::set_var("GIT_DIR", value) },
            None => unsafe { std::env::remove_var("GIT_DIR") },
        }
        match previous_tree {
            Some(value) => unsafe { std::env::set_var("GIT_WORK_TREE", value) },
            None => unsafe { std::env::remove_var("GIT_WORK_TREE") },
        }

        assert!(
            isolated.status.success(),
            "command_with_path must ignore host GIT_DIR; stderr={}",
            String::from_utf8_lossy(&isolated.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&isolated.stdout).trim(), "true");
        assert!(
            !inherited.status.success(),
            "control: a raw Command must still inherit GIT_DIR and fail against the bogus path"
        );
    }
}
