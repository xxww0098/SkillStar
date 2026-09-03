//! Match running processes to an instance's `--user-data-dir`.

use std::path::Path;
use std::process::Command;

/// True when `cmdline` uses `dir` as Chromium `--user-data-dir` (space or `=` form).
///
/// Prefix matches are rejected: `/tmp/inst/a` must not match `/tmp/inst/ab`.
pub fn cmdline_uses_user_data_dir(cmdline: &str, dir: &Path) -> bool {
    let expected = normalize_dir(&dir.to_string_lossy());
    extract_user_data_dirs(cmdline)
        .into_iter()
        .any(|found| normalize_dir(&found) == expected)
}

fn normalize_dir(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim_end_matches('/')
        .to_string()
}

fn extract_user_data_dirs(cmdline: &str) -> Vec<String> {
    let mut dirs = Vec::new();
    let mut rest = cmdline;
    while let Some(idx) = rest.find("--user-data-dir") {
        let after = &rest[idx + "--user-data-dir".len()..];
        if let Some(value) = after.strip_prefix('=') {
            let taken = take_arg(value);
            if !taken.is_empty() {
                dirs.push(unquote(taken));
            }
            rest = &value[taken.len()..];
        } else if after.starts_with(char::is_whitespace) {
            let trimmed = after.trim_start();
            let taken = take_arg(trimmed);
            if !taken.is_empty() {
                dirs.push(unquote(taken));
            }
            rest = &trimmed[taken.len()..];
        } else {
            rest = after;
        }
    }
    dirs
}

fn take_arg(s: &str) -> &str {
    let s = s.trim_start();
    if let Some(stripped) = s.strip_prefix('"') {
        return stripped.split_once('"').map(|(v, _)| v).unwrap_or(stripped);
    }
    s.split_whitespace().next().unwrap_or("")
}

fn unquote(s: &str) -> String {
    s.trim().trim_matches('"').to_string()
}

pub fn parse_ps_line(line: &str) -> Option<(u32, &str)> {
    let line = line.trim();
    let (pid_str, cmd) = line.split_once(char::is_whitespace)?;
    let pid = pid_str.parse().ok()?;
    if pid == 0 {
        return None;
    }
    Some((pid, cmd.trim()))
}

/// PIDs whose argv isolate to `dir`. Empty on platforms without `/bin/ps`.
pub fn pids_for_user_data_dir(dir: &Path) -> Vec<u32> {
    let Ok(output) = Command::new("/bin/ps")
        .args(["-axww", "-o", "pid=,command="])
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let self_pid = std::process::id();
    stdout
        .lines()
        .filter_map(parse_ps_line)
        .filter(|(pid, cmd)| *pid != self_pid && cmdline_uses_user_data_dir(cmd, dir))
        .map(|(pid, _)| pid)
        .collect()
}

pub fn stop_pids(pids: &[u32]) {
    for pid in pids {
        let _ = Command::new("/bin/kill")
            .args(["-TERM", &pid.to_string()])
            .status();
    }
    std::thread::sleep(std::time::Duration::from_millis(250));
    for pid in pids {
        if pid_alive(*pid) {
            let _ = Command::new("/bin/kill")
                .args(["-KILL", &pid.to_string()])
                .status();
        }
    }
}

fn pid_alive(pid: u32) -> bool {
    Command::new("/bin/kill")
        .args(["-0", &pid.to_string()])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}
