//! Parse the system OpenSSH config (`~/.ssh/config`) to surface hosts the
//! user has already defined (e.g. `Host vps-yy`) for quick reuse.
//!
//! Read-only: nothing here is persisted or written to `ssh_hosts.toml`. The UI
//! shows these as "system" connections and lets the user import one into the
//! managed store when they want SkillStar to own it.
//!
//! The parser is deliberately small and dependency-free — the ssh config format
//! we care about is a handful of directives. It handles:
//! - `#` comments and blank lines (skipped)
//! - `Include` with `~/` expansion and relative paths (recursive, depth-bounded)
//! - `Host <aliases...>` opening a new host block (first non-wildcard alias wins)
//! - `HostName` / `User` / `Port` / `IdentityFile`
//! - wildcard aliases (`*`, `*.example.com`) filtered out

use std::path::{Path, PathBuf};

use crate::types::SystemHost;

/// Max `Include` recursion depth to defuse include cycles.
const MAX_INCLUDE_DEPTH: u8 = 8;

/// Parse `~/.ssh/config` and return concrete (non-wildcard) host definitions.
///
/// Missing file or unreadable config → empty list (never errors); the feature
/// is best-effort discovery.
pub fn parse_system_hosts() -> Vec<SystemHost> {
    let Some(path) = user_ssh_config_path() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    parse_file(&path, 0, &mut out);
    // De-dup by alias (first occurrence wins); preserves discovery order.
    let mut seen = std::collections::HashSet::new();
    out.retain(|h| seen.insert(h.alias.clone()));
    out
}

/// Resolve `~/.ssh/config`, honouring the `HOME` env var on Unix.
fn user_ssh_config_path() -> Option<PathBuf> {
    let home = home_dir()?;
    Some(home.join(".ssh").join("config"))
}

/// `$HOME` (or `%USERPROFILE%` on Windows). SSH config is a Unix concept but
/// we resolve gracefully everywhere.
fn home_dir() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
    #[cfg(not(unix))]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
}

fn parse_file(path: &Path, depth: u8, out: &mut Vec<SystemHost>) {
    if depth >= MAX_INCLUDE_DEPTH {
        return;
    }
    let Ok(content) = std::fs::read_to_string(path) else {
        return;
    };

    // A pending host block being assembled from successive directives.
    let mut current: Option<PendingHost> = None;

    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // Split into keyword + value on the first whitespace run.
        let (key, value) = match split_kv(line) {
            Some(kv) => kv,
            None => continue,
        };
        let key_lower = key.to_ascii_lowercase();

        match key_lower.as_str() {
            "host" => {
                // Close out the previous block.
                if let Some(finished) = current.take() {
                    if let Some(host) = finish_host(finished) {
                        out.push(host);
                    }
                }
                current = Some(PendingHost::new(value));
            }
            "include" => {
                // Includes are processed relative to this file's directory.
                let dir = path.parent();
                for inc in expand_includes(value, dir) {
                    parse_file(&inc, depth + 1, out);
                }
            }
            "hostname" => {
                if let Some(p) = current.as_mut() {
                    p.host = Some(value.to_string());
                }
            }
            "user" => {
                if let Some(p) = current.as_mut() {
                    p.username = Some(value.to_string());
                }
            }
            "port" => {
                if let Some(p) = current.as_mut() {
                    p.port = value.parse::<u16>().ok();
                }
            }
            "identityfile" => {
                if let Some(p) = current.as_mut() {
                    p.identity_file = Some(value.to_string());
                }
            }
            _ => {}
        }
    }

    // Flush the trailing block.
    if let Some(finished) = current.take() {
        if let Some(host) = finish_host(finished) {
            out.push(host);
        }
    }
}

struct PendingHost {
    /// All aliases on the `Host` line, in order.
    aliases: Vec<String>,
    host: Option<String>,
    username: Option<String>,
    port: Option<u16>,
    identity_file: Option<String>,
}

impl PendingHost {
    fn new(value: &str) -> Self {
        Self {
            aliases: value.split_whitespace().map(String::from).collect(),
            host: None,
            username: None,
            port: None,
            identity_file: None,
        }
    }
}

/// Finalise a pending block into a [`SystemHost`], filtering out wildcard-only
/// aliases (e.g. `*`, `*.example.com`, `10.0.0.?`).
fn finish_host(p: PendingHost) -> Option<SystemHost> {
    // Pick the first alias without glob characters as the display name.
    let alias = p
        .aliases
        .iter()
        .find(|a| !a.contains('*') && !a.contains('?'))?
        .clone();

    // HostName defaults to the alias when absent (standard ssh behaviour).
    let host = p.host.unwrap_or_else(|| alias.clone());

    Some(SystemHost {
        alias,
        host,
        port: p.port.unwrap_or(22),
        username: p.username.unwrap_or_default(),
        identity_file: p.identity_file,
    })
}

/// Split `HostName 64.83.38.21` → `("HostName", "64.83.38.21")`.
fn split_kv(line: &str) -> Option<(&str, &str)> {
    let mut iter = line.splitn(2, char::is_whitespace);
    let key = iter.next()?.trim();
    let value = iter.next()?.trim();
    if value.is_empty() {
        return None;
    }
    Some((key, value))
}

/// Expand an `Include` value into concrete paths.
///
/// Handles `~/` (home-relative), multiple glob/paths separated by spaces, and
/// paths relative to the including file's directory. Returns existing files
/// only (globs that match nothing are dropped silently).
fn expand_includes(value: &str, base_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for token in value.split_whitespace() {
        let resolved = resolve_one_include(token, base_dir);
        // `Include` supports glob patterns; do a simple `*`/`?` match against
        // the parent dir if the token contains them.
        if token.contains('*') || token.contains('?') {
            paths.extend(glob_match(&resolved));
        } else if resolved.is_file() {
            paths.push(resolved);
        }
    }
    paths
}

fn resolve_one_include(token: &str, base_dir: Option<&Path>) -> PathBuf {
    if let Some(rest) = token.strip_prefix("~/") {
        if let Some(home) = home_dir() {
            return home.join(rest);
        }
    }
    let p = PathBuf::from(token);
    if p.is_absolute() {
        p
    } else if let Some(base) = base_dir {
        base.join(&p)
    } else {
        p
    }
}

/// Minimal glob expansion for `Include` tokens with `*`/`?`. Matches files in
/// the token's parent directory against the pattern.
fn glob_match(pattern_path: &Path) -> Vec<PathBuf> {
    let parent = match pattern_path.parent() {
        Some(p) => p,
        None => return Vec::new(),
    };
    let Some(pattern_name) = pattern_path.file_name().and_then(|n| n.to_str()) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(parent) else {
        return Vec::new();
    };
    let mut matched = Vec::new();
    for entry in entries.flatten() {
        if let Some(name) = entry.file_name().to_str() {
            if glob_equals(pattern_name, name) && entry.path().is_file() {
                matched.push(entry.path());
            }
        }
    }
    matched.sort();
    matched
}

/// Tiny `*`/`?` matcher (no char classes). Case-sensitive.
fn glob_equals(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    glob_match_inner(&p, 0, &n, 0)
}

fn glob_match_inner(p: &[char], pi: usize, n: &[char], ni: usize) -> bool {
    if pi == p.len() {
        return ni == n.len();
    }
    match p[pi] {
        '*' => {
            // `*` matches zero or more: try every remaining suffix.
            for skip in ni..=n.len() {
                if glob_match_inner(p, pi + 1, n, skip) {
                    return true;
                }
            }
            false
        }
        '?' => ni < n.len() && glob_match_inner(p, pi + 1, n, ni + 1),
        c => ni < n.len() && n[ni] == c && glob_match_inner(p, pi + 1, n, ni + 1),
    }
}

/// Look up a single host by alias (used by `import_system_host`).
pub fn find_host_by_alias(alias: &str) -> Option<SystemHost> {
    parse_system_hosts()
        .into_iter()
        .find(|h| h.alias == alias)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write(dir: &Path, name: &str, content: &str) -> PathBuf {
        let p = dir.join(name);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&p, content).unwrap();
        p
    }

    /// Build a temp dir, write a config, and point HOME at it so
    /// `user_ssh_config_path` resolves to our fixture.
    fn with_home(content: &str, f: impl FnOnce(&Path)) {
        // Hold the crate-wide env lock so parallel tests don't clobber HOME.
        let _lock = crate::test_support::env_lock().lock().unwrap();
        let tmp = TempDir::new().unwrap();
        let cfg = write(tmp.path(), ".ssh/config", content);
        // SAFETY: the env lock above serialises all with_home callers.
        unsafe {
            std::env::set_var("HOME", tmp.path());
        }
        let _ = &cfg;
        f(tmp.path());
        unsafe {
            std::env::remove_var("HOME");
        }
    }

    #[test]
    fn parses_basic_host() {
        with_home(
            r#"
Host vps-yy
    HostName 64.83.38.21
    User root
    Port 2222
    IdentityFile ~/.ssh/id_ed25519_dstools
"#,
            |_| {
                let hosts = parse_system_hosts();
                assert_eq!(hosts.len(), 1);
                let h = &hosts[0];
                assert_eq!(h.alias, "vps-yy");
                assert_eq!(h.host, "64.83.38.21");
                assert_eq!(h.username, "root");
                assert_eq!(h.port, 2222);
                assert_eq!(
                    h.identity_file.as_deref(),
                    Some("~/.ssh/id_ed25519_dstools")
                );
            },
        );
    }

    #[test]
    fn hostname_defaults_to_alias() {
        with_home(
            "Host mybox\n    User ubuntu\n",
            |_| {
                let hosts = parse_system_hosts();
                assert_eq!(hosts.len(), 1);
                assert_eq!(hosts[0].host, "mybox");
                assert_eq!(hosts[0].username, "ubuntu");
                assert_eq!(hosts[0].port, 22);
            },
        );
    }

    #[test]
    fn filters_wildcard_aliases() {
        with_home(
            "Host *\n    User foo\nHost real-host\n    HostName 1.2.3.4\n",
            |_| {
                let hosts = parse_system_hosts();
                assert_eq!(hosts.len(), 1);
                assert_eq!(hosts[0].alias, "real-host");
            },
        );
    }

    #[test]
    fn picks_first_concrete_alias() {
        with_home("Host * alpha\n    HostName 5.6.7.8\n", |_| {
            let hosts = parse_system_hosts();
            assert_eq!(hosts.len(), 1);
            assert_eq!(hosts[0].alias, "alpha");
        });
    }

    #[test]
    fn include_is_recursive() {
        with_home(
            "Include extra.conf\nHost main\n    HostName 9.9.9.9\n",
            |home| {
                // extra.conf is resolved relative to the config dir (~/.ssh/).
                write(home, ".ssh/extra.conf", "Host nested\n    HostName 8.8.8.8\n");
                let hosts = parse_system_hosts();
                let aliases: Vec<_> = hosts.iter().map(|h| h.alias.as_str()).collect();
                assert!(aliases.contains(&"nested"));
                assert!(aliases.contains(&"main"));
            },
        );
    }

    #[test]
    fn missing_config_returns_empty() {
        with_home("", |_| {
            // empty HOME → no .ssh/config file present
        });
        // Point HOME at a fresh empty dir to guarantee no config.
        let tmp = TempDir::new().unwrap();
        unsafe {
            std::env::set_var("HOME", tmp.path());
        }
        assert!(parse_system_hosts().is_empty());
        unsafe {
            std::env::remove_var("HOME");
        }
    }

    #[test]
    fn find_host_by_alias_works() {
        with_home("Host vps-yy\n    HostName 64.83.38.21\n    User root\n", |_| {
            let h = find_host_by_alias("vps-yy").unwrap();
            assert_eq!(h.host, "64.83.38.21");
            assert!(find_host_by_alias("nope").is_none());
        });
    }

    /// Full vps-yy fixture: key auth + non-default port (typical VPS ssh config).
    #[test]
    fn vps_yy_system_host_resolves_for_remote_probe() {
        with_home(
            r#"
Host vps-yy
    HostName 64.83.38.21
    User root
    Port 2222
    IdentityFile ~/.ssh/id_ed25519_dstools
"#,
            |_| {
                let h = find_host_by_alias("vps-yy").expect("vps-yy must parse");
                assert_eq!(h.alias, "vps-yy");
                assert_eq!(h.host, "64.83.38.21");
                assert_eq!(h.username, "root");
                assert_eq!(h.port, 2222);
                assert_eq!(
                    h.identity_file.as_deref(),
                    Some("~/.ssh/id_ed25519_dstools")
                );
            },
        );
    }

    #[test]
    fn glob_equals_matches_star_and_question() {
        assert!(glob_equals("*.conf", "a.conf"));
        assert!(!glob_equals("*.conf", "a.txt"));
        assert!(glob_equals("host?", "host1"));
        assert!(!glob_equals("host?", "host12"));
        assert!(glob_equals("*", "anything"));
    }
}
