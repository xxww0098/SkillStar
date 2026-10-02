//! Codex inside a WSL distro.
//!
//! Each running distro is its own agent, `codex@wsl:<distro>`. Its config is
//! opened through `\\wsl.localhost\<distro>`. A stopped distro is not started.
//! Mirrored networking writes `127.0.0.1`. NAT writes the Windows address that
//! distro sees, and the provider table then carries the install-level gateway
//! key instead of the placeholder bearer — that peer is not loopback.
//! `wsl.exe` runs only on Windows.

use std::collections::HashSet;
use std::io;
use std::net::IpAddr;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::codex::{self, ApplyError, CodexRoute};
use crate::resolve_addr;

const PREFIX: &str = "codex@wsl:";
const PROBE: &str = concat!(
    "echo \"home:$HOME\"; [ -d \"$HOME/.codex\" ] && echo dir:.codex; ",
    "command -v codex >/dev/null 2>&1 && echo bin:codex; ",
    "ip route show default 2>/dev/null | head -n1 | sed 's/^/route:/'; ",
    "grep -m1 '^nameserver' /etc/resolv.conf 2>/dev/null | sed 's/^/ns:/'; true"
);

static EXE_CALLS: AtomicU64 = AtomicU64::new(0);

/// `codex@wsl:<distro>`.
pub fn wsl_codex_id(distro: &str) -> String {
    format!("{PREFIX}{distro}")
}

/// `\\wsl.localhost\<distro>` plus `linux_path` with backslashes.
pub fn wsl_codex_open_path(distro: &str, linux_path: &str) -> String {
    let rel = linux_path.trim_start_matches('/').replace('/', "\\");
    format!(r"\\wsl.localhost\{distro}\{rel}")
}

/// One distro's Codex, after a list. `home` is the Linux path (`/home/me`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WslCodex {
    pub name: String,
    pub home: String,
    pub gateway: String,
    pub mirrored: bool,
    pub running: bool,
    pub has_codex: bool,
}

impl WslCodex {
    pub fn id(&self) -> String {
        wsl_codex_id(&self.name)
    }

    /// Gateway origin this distro can open. Mirrored, and a missing NAT
    /// address, stay on loopback. The port is the gateway's listen port.
    pub fn origin(&self, port: u16) -> String {
        if self.mirrored || self.gateway.is_empty() {
            format!("http://127.0.0.1:{port}")
        } else {
            format!("http://{}:{port}", self.gateway)
        }
    }
}

/// Distros whose Codex can be edited. `run` stands in for `wsl.exe`.
///
/// Stopped distros are skipped: nothing is asked of them, because asking
/// starts them. A failed running-list probes nobody.
pub fn wsl_codex_list(
    run: &dyn Fn(&[&str]) -> io::Result<Vec<u8>>,
    wslconfig: &str,
) -> Vec<WslCodex> {
    let Ok(installed_bytes) = run(&["-l", "-q"]) else {
        return Vec::new();
    };
    let Ok(running_bytes) = run(&["-l", "--running", "-q"]) else {
        return Vec::new();
    };
    let running: HashSet<String> = parse_names(&running_bytes).into_iter().collect();
    let mirrored = wsl_mirrored(wslconfig);
    let mut out = Vec::new();
    for name in parse_names(&installed_bytes) {
        if !running.contains(&name) {
            continue;
        }
        let Ok(body) = run(&["-d", &name, "-e", "sh", "-lc", PROBE]) else {
            continue;
        };
        let Ok(text) = String::from_utf8(body) else {
            continue;
        };
        let Some(probe) = parse_probe(&text) else {
            continue;
        };
        if !probe.has_codex {
            continue;
        }
        out.push(WslCodex {
            name,
            home: probe.home,
            gateway: probe.gateway,
            mirrored,
            running: true,
            has_codex: true,
        });
    }
    out
}

/// Running distros on Windows. Off Windows this returns nothing and does
/// not enter the `wsl.exe` launcher.
pub fn wsl_codex_discover() -> Vec<WslCodex> {
    if !cfg!(windows) {
        return Vec::new();
    }
    let config = read_wslconfig();
    wsl_codex_list(&spawn_wsl, &config)
}

/// Write this distro's Codex config with the same toml as the local writer.
///
/// `home` is the directory that contains `.codex/` as this process opens it
/// (a temp stand-in in tests, the `\\wsl.localhost` path on Windows).
/// A distro that is not running is left untouched.
pub fn apply_wsl_codex(
    distro: &WslCodex,
    route: CodexRoute,
    home: &Path,
) -> Result<(), ApplyError> {
    if !distro.running || !distro.has_codex {
        return Ok(());
    }
    let port = resolve_addr().map(|addr| addr.port()).unwrap_or(21847);
    let origin = distro.origin(port);
    let catalog = format!(
        "{}/.codex/skillstar-models.json",
        distro.home.trim_end_matches('/')
    );
    codex::apply_codex_home(&distro.id(), route, &origin, home, Some(&catalog))
}

fn read_wslconfig() -> String {
    let home = skillstar_core::infra::paths::home_dir();
    std::fs::read_to_string(home.join(".wslconfig")).unwrap_or_default()
}

fn spawn_wsl(args: &[&str]) -> io::Result<Vec<u8>> {
    EXE_CALLS.fetch_add(1, Ordering::SeqCst);
    if !cfg!(windows) {
        return Err(io::Error::other("wsl.exe is windows-only"));
    }
    let output = Command::new("wsl.exe")
        .args(args)
        .env("WSL_UTF8", "1")
        .output()?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(io::Error::other("wsl.exe failed"))
    }
}

#[cfg(test)]
fn exe_calls() -> u64 {
    EXE_CALLS.load(Ordering::SeqCst)
}

struct Probe {
    home: String,
    gateway: String,
    has_codex: bool,
}

fn parse_probe(text: &str) -> Option<Probe> {
    let mut home = String::new();
    let mut gateway = String::new();
    let mut nameserver = String::new();
    let mut has_codex = false;
    for raw in text.lines() {
        let line = raw.trim();
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        match key {
            "home" => home = value.trim_end_matches('/').to_string(),
            "dir" | "bin" => {
                if line == "dir:.codex" || line == "bin:codex" {
                    has_codex = true;
                }
            }
            "route" => {
                let fields: Vec<&str> = value.split_whitespace().collect();
                if fields.len() >= 3
                    && fields[1] == "via"
                    && fields[2].parse::<IpAddr>().is_ok()
                {
                    gateway = fields[2].to_string();
                }
            }
            "ns" => {
                let fields: Vec<&str> = value.split_whitespace().collect();
                if fields.len() >= 2 && fields[1].parse::<IpAddr>().is_ok() {
                    nameserver = fields[1].to_string();
                }
            }
            _ => {}
        }
    }
    if !home.starts_with('/') {
        return None;
    }
    if gateway.is_empty() {
        gateway = nameserver;
    }
    Some(Probe {
        home,
        gateway,
        has_codex,
    })
}

fn parse_names(bytes: &[u8]) -> Vec<String> {
    decode_wsl(bytes)
        .lines()
        .filter_map(|line| {
            let name = line.trim_matches(|ch: char| ch == '\0' || ch == '\u{feff}' || ch.is_whitespace());
            if name.is_empty() || name.to_ascii_lowercase().starts_with("docker-desktop") {
                None
            } else {
                Some(name.to_string())
            }
        })
        .collect()
}

fn decode_wsl(bytes: &[u8]) -> String {
    let utf16 = bytes.len() >= 2
        && ((bytes[0] == 0xff && bytes[1] == 0xfe) || (bytes[1] == 0 && bytes[0] != 0));
    if !utf16 {
        return String::from_utf8_lossy(bytes).into_owned();
    }
    let rest = if bytes[0] == 0xff && bytes[1] == 0xfe {
        &bytes[2..]
    } else {
        bytes
    };
    let units: Vec<u16> = rest
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    String::from_utf16_lossy(&units)
}

/// `[wsl2] networkingMode=mirrored`, ignoring comments and other sections.
fn wsl_mirrored(cfg: &str) -> bool {
    let mut section = String::new();
    let mut mirrored = false;
    for raw in cfg.trim_start_matches('\u{feff}').lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].trim().to_ascii_lowercase();
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if section != "wsl2" || !key.trim().eq_ignore_ascii_case("networkingMode") {
            continue;
        }
        let value = value.split(['#', ';']).next().unwrap_or(value);
        mirrored = value.trim().trim_matches('"').eq_ignore_ascii_case("mirrored");
    }
    mirrored
}

#[cfg(test)]
mod tests {
    use super::{exe_calls, wsl_codex_discover};

    #[test]
    fn wsl_codex_non_windows_skips_exe() {
        if cfg!(windows) {
            return;
        }
        let before = exe_calls();
        let found = wsl_codex_discover();
        assert_eq!(exe_calls(), before);
        assert!(found.is_empty());
    }
}
