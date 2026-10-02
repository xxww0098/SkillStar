//! File-agent loopback writer.
//!
//! `apply_gateway` points one file-based agent at the loopback gateway.
//! Paths and file shapes live here. The provider store is not read.
//! An agent this slice does not write returns [`ApplyError::NotManaged`]
//! before any file is opened.

mod alma;
mod body;
mod cindy;
mod desktop;
mod hanako;

use std::fs;
use std::path::PathBuf;

use skillstar_core::infra::fs_ops::atomic_write;
use skillstar_core::infra::paths::home_dir;

use crate::codex::{self, ApplyError};
use crate::PLACEHOLDER_BEARER;

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
    "claude-desktop",
];

pub use cindy::{cindy_imported, cindy_link};
pub use desktop::{
    DESKTOP_PROFILE_ID, desktop_accepts, desktop_alias, desktop_dirs, desktop_effort_alias,
};

pub(super) struct Written {
    rel: String,
    path: PathBuf,
    body: String,
}

/// The id the file writers spell this agent as. The board says `claude-code`;
/// every written file and stash key says `claude`.
fn file_agent(agent_id: &str) -> &str {
    match agent_id {
        "claude-code" => "claude",
        other => other,
    }
}

/// Point `agent_id` at the loopback gateway, or restore the stashed files.
///
/// `model_ref` is `provider/model`, `group/<id>`, or empty. Empty releases
/// the agent. The home is `SKILLSTAR_TOOL_SYNC_HOME` when that is set, else
/// the process home. The URL host is always `127.0.0.1`; the port is the
/// gateway's listen port, never `3425`. Codex is not one of these writers:
/// its config takeover lives in [`crate::codex`].
pub fn apply_gateway(agent_id: &str, model_ref: &str) -> Result<(), ApplyError> {
    let agent_id = file_agent(agent_id);
    let home = agent_home();
    let origin = loopback_origin();
    if agent_id == "hanako" {
        return hanako::apply(&home, &origin, model_ref);
    }
    if agent_id == "alma" {
        return alma::apply(&origin, model_ref);
    }
    let files = body::files(agent_id, &home, &origin, model_ref)?;
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
    crate::published_origin()
}

/// Host and port already written for this agent, like `127.0.0.1:21847`.
///
/// Reads the files that writer owns. A missing file, or a file whose URL is
/// not `http://127.0.0.1:<port>`, is empty. This does not read the provider
/// store and does not invent an address.
pub fn written_loopback_label(agent_id: &str) -> String {
    for path in written_paths(agent_id) {
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        if let Some(label) = loopback_label_in(&text) {
            return label;
        }
    }
    String::new()
}

fn written_paths(agent_id: &str) -> Vec<PathBuf> {
    let agent_id = file_agent(agent_id);
    let home = agent_home();
    if agent_id == "codex" {
        return vec![home_dir().join(".codex").join("config.toml")];
    }
    if agent_id == "hanako" {
        return vec![hanako::catalog_path(&home)];
    }
    if !FILE_AGENTS.contains(&agent_id) {
        return Vec::new();
    }
    body::files(agent_id, &home, "http://127.0.0.1:1", "probe")
        .map(|files| files.into_iter().map(|file| file.path).collect())
        .unwrap_or_default()
}

/// The model ref this writer already wrote for `agent_id`, like
/// `deepseek/pro` or `group/fast`. Reads the same files the writer owns; a
/// missing file, or a value this writer did not write, is empty. Codex's own
/// model is read here too: a user-chosen `model` stays theirs.
pub fn written_model_ref(agent_id: &str) -> String {
    for path in written_paths(agent_id) {
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        if let Some(reference) = model_ref_in(file_agent(agent_id), &text) {
            return reference;
        }
    }
    String::new()
}

/// Where one written file hides the active model ref. Most bodies quote the
/// full `skillstar/<ref>` form; the bare-form writers each have one field
/// that holds the ref and no other model-shaped value before it.
fn model_ref_in(agent: &str, text: &str) -> Option<String> {
    match agent {
        "pi" => quoted_field(text, "defaultModel"),
        "crush" | "cline" | "claude" => quoted_field(text, "model"),
        "fx" => quoted_field(text, "skillstar"),
        "zcode" => quoted_field(text, "modelId"),
        "workbuddy" => quoted_field(text, "id"),
        "hermes" => plain_field(text, "default"),
        "dsh" => plain_field(text, "model"),
        _ => skillstar_prefixed(text),
    }
}

/// First `"key": "ref"` whose value is a ref. A hit that is a value, not a
/// key, is skipped.
fn quoted_field(text: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let mut rest = text;
    while let Some(at) = rest.find(&needle) {
        let after_key = &rest[at + needle.len()..];
        let Some(after_colon) = after_key.trim_start().strip_prefix(':') else {
            rest = after_key;
            continue;
        };
        let Some(after_quote) = after_colon.trim_start().strip_prefix('"') else {
            rest = after_colon;
            continue;
        };
        let Some(end) = after_quote.find('"') else {
            return None;
        };
        if looks_like_model_ref(&after_quote[..end]) {
            return Some(after_quote[..end].to_string());
        }
        rest = &after_quote[end + 1..];
    }
    None
}

/// First `key: ref` mapping line (YAML) whose value is a ref. A longer key
/// that merely starts with `key` (`models:`) does not match.
fn plain_field(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        let Some(rest) = line.trim_start().strip_prefix(key) else {
            continue;
        };
        let Some(value) = rest.trim_start().strip_prefix(':') else {
            continue;
        };
        let value = value.trim().trim_matches('"');
        if looks_like_model_ref(value) {
            return Some(value.to_string());
        }
    }
    None
}

/// First quoted `skillstar/<ref>`, the full form the writer stamps as the
/// model value for most agents.
fn skillstar_prefixed(text: &str) -> Option<String> {
    const MARKER: &str = "\"skillstar/";
    let mut rest = text;
    while let Some(at) = rest.find(MARKER) {
        let after = &rest[at + MARKER.len()..];
        let end = after.find('"')?;
        if looks_like_model_ref(&after[..end]) {
            return Some(after[..end].to_string());
        }
        rest = &after[end + 1..];
    }
    None
}

/// `provider/model` or `group/<id>`: one slash, no scheme, no space, and
/// only the characters a catalog id is built from.
fn looks_like_model_ref(value: &str) -> bool {
    if value.is_empty() || value.len() > 200 {
        return false;
    }
    let mut slashes = 0usize;
    value.chars().all(|ch| match ch {
        '/' => {
            slashes += 1;
            true
        }
        'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '-' | '_' => true,
        _ => false,
    }) && slashes == 1
}

fn loopback_label_in(text: &str) -> Option<String> {
    const MARKER: &str = "http://127.0.0.1:";
    let rest = &text[text.find(MARKER)? + MARKER.len()..];
    let port: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if port.is_empty() || port.len() > 5 {
        return None;
    }
    Some(format!("127.0.0.1:{port}"))
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

pub fn token_for(agent_id: &str) -> String {
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

#[cfg(test)]
mod label_tests {
    use super::loopback_label_in;

    #[test]
    fn a_vendor_url_is_not_a_loopback_label() {
        assert_eq!(loopback_label_in("https://api.openai.com/v1"), None);
        assert_eq!(
            loopback_label_in("base_url = \"http://127.0.0.1:21847/v1\""),
            Some("127.0.0.1:21847".to_string())
        );
        assert_eq!(
            loopback_label_in("http://127.0.0.1:21847/backend-api/codex"),
            Some("127.0.0.1:21847".to_string())
        );
    }
}
