//! Session file discovery for the codex family.
//!
//! Codex writes one rollout file per session segment under
//! `$CODEX_HOME/sessions/<yyyy>/<mm>/<dd>/rollout-<timestamp>-<thread
//! id>[_<segment>].jsonl` (a session picked up again later continues in
//! another file with the same thread id). The Codex app's "compress local
//! chat history" packs older rollouts into `.jsonl.zst`; while it is packing,
//! both forms sit side by side and the compressed one may be half written —
//! a rollout found in both forms is read once, as the plain one (magpie
//! sessions.go precedent).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::SessionFile;

/// The suffix the Codex app's "compress local chat history" appends.
pub(super) const ZST_SUFFIX: &str = ".zst";

/// Codex's home directory: `$CODEX_HOME`, default `<home>/.codex`. The
/// sandbox (`SKILLSTAR_TOOL_SYNC_HOME`) always wins — isomorphic to
/// tool_paths.rs's `codex_home()`, so tests never escape into a developer's
/// real `~/.codex` even when `CODEX_HOME` is exported.
pub(super) fn codex_home_dir(home: &Path) -> PathBuf {
    if crate::tool_paths::is_tool_sync_sandboxed() {
        return home.join(".codex");
    }
    match std::env::var_os("CODEX_HOME") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => home.join(".codex"),
    }
}

/// The thread id a rollout file name carries, or `None` for any other name:
/// `rollout-<yyyy>-<mm>-<dd>T<hh>-<mm>-<ss>-<thread id>[_<segment>].jsonl[.zst]`
/// (magpie `rolloutName` precedent, hand-rolled: the usage crate has no regex
/// dependency). Neither the thread id nor the segment may contain `_`, so the
/// id is everything before the first underscore.
pub(super) fn rollout_thread_id(file_name: &str) -> Option<String> {
    let stem = file_name
        .strip_suffix(".jsonl.zst")
        .or_else(|| file_name.strip_suffix(".jsonl"))?;
    let rest = stem.strip_prefix("rollout-")?;
    // "<yyyy>-<mm>-<dd>T<hh>-<mm>-<ss>-" is 20 bytes.
    if rest.len() <= 20 || !is_date_time(&rest.as_bytes()[..19]) || rest.as_bytes()[19] != b'-' {
        return None;
    }
    let tail = &rest[20..];
    match tail.split_once('_') {
        None => is_thread_id(tail).then(|| tail.to_string()),
        Some((thread, segment)) => {
            if thread.is_empty() || segment.is_empty() {
                return None;
            }
            (is_thread_id(thread) && is_thread_id(segment)).then(|| thread.to_string())
        }
    }
}

/// `YYYY-MM-DDTHH-MM-SS`: digits everywhere except `-` between the date and
/// time parts and `T` between them.
fn is_date_time(bytes: &[u8]) -> bool {
    if bytes.len() != 19 {
        return false;
    }
    bytes.iter().enumerate().all(|(index, &byte)| {
        match index {
            4 | 7 | 13 | 16 => byte == b'-',
            10 => byte == b'T',
            _ => byte.is_ascii_digit(),
        }
    })
}

/// Thread and segment ids are `[0-9A-Za-z-]+`.
fn is_thread_id(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// Find codex's rollout files under `home`, recursively under
/// `<codex home>/sessions`, keeping each rollout once when it sits there in
/// both the plain and the compressed form.
pub(super) fn codex_files(agent: &'static str, home: &Path) -> Vec<SessionFile> {
    let mut files = Vec::new();
    walk_rollouts(agent, &codex_home_dir(home).join("sessions"), &mut files);
    // A rollout found both plain and compressed (the Codex app packing it) is
    // read once, as the plain one: the other may be half written, and the two
    // say the same.
    let plain: HashSet<PathBuf> = files
        .iter()
        .map(|f| f.path.clone())
        .filter(|path| !path.ends_with(ZST_SUFFIX))
        .collect();
    files.retain(|f| match f.path.to_str().and_then(|p| p.strip_suffix(ZST_SUFFIX)) {
        Some(trimmed) => !plain.contains(Path::new(trimmed)),
        None => true,
    });
    files
}

/// Recursive walk collecting files whose name is a rollout's (any depth; the
/// CLI nests them by date, but only the name shape decides).
fn walk_rollouts(agent: &'static str, dir: &Path, out: &mut Vec<SessionFile>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let file_type = entry.file_type().ok();
        if file_type.is_some_and(|t| t.is_dir()) {
            walk_rollouts(agent, &path, out);
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_string) else { continue };
        if rollout_thread_id(&name).is_some()
            && let Ok(meta) = entry.metadata()
            && meta.is_file()
        {
            let modified_ms = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            out.push(SessionFile {
                agent,
                path: path.clone(),
                size: meta.len(),
                modified_ms,
            });
        }
    }
}
