//! File-agent loopback writer.
//!
//! `apply_gateway` points one file-based agent at the loopback gateway.
//! Paths and file shapes live here. The provider store is not read.
//! An agent this slice does not write returns [`ApplyError::NotManaged`]
//! before any file is opened.

mod body;

use std::fs;
use std::path::PathBuf;

use skillstar_core::infra::fs_ops::atomic_write;
use skillstar_core::infra::paths::home_dir;

use crate::codex::{self, ApplyError};
use crate::{PLACEHOLDER_BEARER, resolve_addr};

/// Agents whose magpie writer emits a loopback URL. Order is the spec's.
pub const FILE_AGENTS: &[&str] = &[
    "gemini",
    "opencode",
    "mimocode",
    "pi",
    "crush",
    "dsh",
    "commandcode",
    "fx",
    "omp",
    "hermes",
    "cline",
    "qoder",
    "qoder-cn",
    "grok",
    "zcode",
    "workbuddy",
    "claude",
];

pub(super) struct Written {
    rel: String,
    path: PathBuf,
    body: String,
}

/// Point `agent_id` at the loopback gateway, or restore the stashed files.
///
/// `model_ref` is `provider/model`, `group/<id>`, or empty. Empty releases
/// the agent. The home is `SKILLSTAR_TOOL_SYNC_HOME` when that is set, else
/// the process home. The URL host is always `127.0.0.1`; the port is the
/// gateway's listen port, never `3425`.
pub fn apply_gateway(agent_id: &str, model_ref: &str) -> Result<(), ApplyError> {
    let home = agent_home();
    let files = body::files(agent_id, &home, &loopback_origin(), model_ref)?;
    if model_ref.is_empty() {
        return restore(agent_id, &files);
    }
    remember(agent_id, &files)?;
    for file in &files {
        atomic_write(&file.path, file.body.as_bytes())?;
    }
    Ok(())
}

fn agent_home() -> PathBuf {
    if let Some(value) = std::env::var_os("SKILLSTAR_TOOL_SYNC_HOME")
        && !value.is_empty()
    {
        return PathBuf::from(value);
    }
    home_dir()
}

fn loopback_origin() -> String {
    let port = resolve_addr().map(|addr| addr.port()).unwrap_or(21847);
    format!("http://127.0.0.1:{port}")
}

fn snap_key(agent_id: &str, rel: &str) -> String {
    format!("{agent_id}|{rel}")
}

fn remember(agent_id: &str, files: &[Written]) -> Result<(), ApplyError> {
    let mut stash = codex::load_stash()?;
    let mut changed = false;
    for file in files {
        let key = snap_key(agent_id, &file.rel);
        if stash.contains_key(&key) {
            continue;
        }
        let prev = if file.path.exists() {
            fs::read_to_string(&file.path)?
        } else {
            String::new()
        };
        stash.insert(key, prev);
        changed = true;
    }
    if changed {
        codex::save_stash(&stash)?;
    }
    Ok(())
}

fn restore(agent_id: &str, files: &[Written]) -> Result<(), ApplyError> {
    let mut stash = codex::load_stash()?;
    let mut changed = false;
    for file in files {
        let key = snap_key(agent_id, &file.rel);
        let Some(prev) = stash.remove(&key) else {
            continue;
        };
        changed = true;
        if prev.is_empty() && file.path.exists() {
            fs::remove_file(&file.path)?;
        } else if !prev.is_empty() {
            atomic_write(&file.path, prev.as_bytes())?;
        }
    }
    if changed {
        codex::save_stash(&stash)?;
    }
    Ok(())
}

pub(crate) fn token_for(agent_id: &str) -> String {
    format!("{PLACEHOLDER_BEARER}-{agent_id}")
}

/// `SKILLSTAR_TOOL_SYNC_HOME` is the test sandbox. While it is set, agent
/// home overrides (`GROK_HOME`, `XDG_CONFIG_HOME`, …) stay ignored so a
/// test cannot follow the developer's real directory.
pub(crate) fn sandboxed() -> bool {
    std::env::var_os("SKILLSTAR_TOOL_SYNC_HOME").is_some_and(|value| !value.is_empty())
}

pub(crate) fn override_dir(var: &str, fallback: PathBuf) -> PathBuf {
    if sandboxed() {
        return fallback;
    }
    match std::env::var_os(var) {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => fallback,
    }
}
