//! The LAN inbound gate and the install-level gateway key.
//!
//! On loopback the gateway still accepts any bearer (the attribution channel
//! `skillstar-<agent>`, the selection channel `skillstar/<model>`, and the
//! local GET surface are untouched). Once a request arrives from a
//! non-loopback peer (LAN, WSL NAT), every request other than the Claude MCP
//! callback — which carries its own loopback-plus-token gate — must present
//! the install-level gateway key in one of the slots magpie's
//! `internal/gateway/lan.go` `callerKey` reads: `Authorization` (after
//! stripping the `Bearer ` prefix) → `x-api-key` → `x-goog-api-key` → `?key=`.
//! Any matching slot passes. The key is the hex of ≥32 random bytes in
//! `config_dir()/gateway.key`, written `0600` after `redact::write_key`. It
//! never reaches a DTO, an event, a log line, the ledger, or any Debug
//! output; this module defines no type that carries it.

use std::fs;
use std::io::{self, Write};
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};

/// Key file name, inside `config_dir()`.
const KEY_FILE: &str = "gateway.key";
/// Lower bound of random bytes; hex-encoded this is 64 characters.
const KEY_BYTES: usize = 32;
/// Shortest accepted file content: anything shorter is treated as no key, so
/// readers reject it and `gateway_key` generates a fresh one.
const MIN_KEY_CHARS: usize = 32;

/// Read or lazily create the install-level gateway key.
///
/// An existing file whose content (trimmed) reaches [`MIN_KEY_CHARS`] is
/// returned as is; a missing, too-short, or unreadable one is regenerated
/// from ≥32 random bytes and written `0600` (the parent directory is created
/// `0700`). A write failure is the caller's to map — both `save_listen("lan")`
/// and a non-loopback `serve` refuse to continue without a usable key.
pub fn gateway_key() -> io::Result<String> {
    if let Some(key) = stored_key() {
        return Ok(key);
    }
    let key = fresh_key();
    write_key(&key_path(), key.as_bytes())?;
    Ok(key)
}

/// The inbound gate for non-loopback requests. A loopback peer passes without
/// the key file ever being read; every other peer passes only when one of the
/// slots matches the install-level key. A missing or invalid key file
/// rejects. This function never generates a key or writes a file.
pub fn check_inbound(
    peer: &SocketAddr,
    authorization: &str,
    api_key: &str,
    goog_key: &str,
    query_key: &str,
) -> bool {
    if normalize(peer.ip()).is_loopback() {
        return true;
    }
    let Some(key) = stored_key() else {
        return false;
    };
    let bearer = strip_bearer_prefix(authorization);
    [bearer, api_key.trim(), goog_key.trim(), query_key.trim()]
        .iter()
        .any(|slot| !slot.is_empty() && same_secret(slot, &key))
}

/// Unfold IPv4-mapped IPv6 (`::ffff:127.0.0.1`), which dual-stack sockets
/// hand out for IPv4 peers, so mapped loopback counts as loopback and every
/// other mapped address — a LAN peer included — stays non-loopback.
pub(crate) fn normalize(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6
            .to_ipv4_mapped()
            .map(IpAddr::from)
            .unwrap_or(IpAddr::V6(v6)),
        v4 @ IpAddr::V4(_) => v4,
    }
}

/// Whether an origin like `http://127.0.0.1:21847` points at this computer.
pub(crate) fn origin_is_loopback(origin: &str) -> bool {
    let authority = origin
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .split('/')
        .next()
        .unwrap_or("");
    // Strip the suffix only when everything after the colon is digits (a
    // port), so a bare IPv6 literal is not torn apart.
    let host = match authority.rsplit_once(':') {
        Some((head, tail)) if !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit()) => head,
        _ => authority,
    };
    let host = host.trim_matches(['[', ']']);
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    host.parse::<IpAddr>()
        .map(normalize)
        .map(|ip| ip.is_loopback())
        .unwrap_or(false)
}

fn key_path() -> PathBuf {
    skillstar_core::infra::paths::config_dir().join(KEY_FILE)
}

/// Read an existing key without creating one. Content below the minimum
/// length counts as no key at all.
fn stored_key() -> Option<String> {
    let text = fs::read_to_string(key_path()).ok()?;
    let key = text.trim();
    if key.len() < MIN_KEY_CHARS {
        return None;
    }
    Some(key.to_string())
}

fn fresh_key() -> String {
    let mut bytes = [0u8; KEY_BYTES];
    for byte in bytes.iter_mut() {
        *byte = rand::random::<u8>();
    }
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// `Bearer <key>` becomes `<key>`; a value without the prefix stays as it is
/// (magpie's `callerKey` does the same).
fn strip_bearer_prefix(authorization: &str) -> &str {
    let trimmed = authorization.trim();
    trimmed
        .strip_prefix("Bearer ")
        .unwrap_or(trimmed)
        .trim()
}

/// Constant-time compare: inputs of different lengths still walk the whole
/// way, nothing returns early.
fn same_secret(left: &str, right: &str) -> bool {
    let (left, right) = (left.as_bytes(), right.as_bytes());
    // The length difference folds into the same accumulator as a plain
    // inequality, so no XOR of the lengths can cancel byte differences.
    let mut diff = (left.len() != right.len()) as u8;
    for index in 0..left.len().max(right.len()) {
        diff |= left.get(index).copied().unwrap_or(0) ^ right.get(index).copied().unwrap_or(0);
    }
    diff == 0
}

/// Write `0600`, after the `redact::write_key` precedent (OpenOptions mode
/// plus set_permissions, Unix-only guards included).
fn write_key(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent()
        && !parent.exists()
    {
        fs::create_dir_all(parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
        }
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}
