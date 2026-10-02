//! Session file discovery for the claude family.
//!
//! Both family instances share the scan of the projects JSONL layout
//! ([`cc_files`]); they differ only in root directory resolution:
//! - **claude-code**: `projects` under `$CLAUDE_CONFIG_DIR` (default
//!   `~/.claude`); the sandbox (`SKILLSTAR_TOOL_SYNC_HOME`) always wins
//!   (tool_paths.rs precedent);
//! - **claude-desktop**: the Cowork sessions' own `.claude` homes under the
//!   Desktop data directory (the two profiles Claude / Claude-3p):
//!   `local-agent-mode-sessions/*/*/local_*/.claude` (magpie calls.go
//!   precedent; this layout has not been observed to actually exist on this
//!   machine, kept as an interface — Desktop attribution relies mainly on
//!   the inline entrypoint, see claude.rs).

use std::path::{Path, PathBuf};

use super::SessionFile;


/// Claude Code's config directory: `$CLAUDE_CONFIG_DIR`, default
/// `<home>/.claude`. The sandbox (`SKILLSTAR_TOOL_SYNC_HOME`) always wins —
/// isomorphic to tool_paths.rs's `codex_home()`.
pub(super) fn claude_code_config_dir(home: &Path) -> PathBuf {
    if crate::tool_paths::is_tool_sync_sandboxed() {
        return home.join(".claude");
    }
    match std::env::var_os("CLAUDE_CONFIG_DIR") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => home.join(".claude"),
    }
}

/// Claude Desktop's data directories (the two profiles Claude and
/// Claude-3p). When sandboxed, `LOCALAPPDATA` / `XDG_CONFIG_HOME` are
/// ignored and everything resolves relative to `home` (the same layout as
/// gateway/desktop.rs's desktop_dirs; this crate does not depend on
/// gateway).
pub(super) fn desktop_data_dirs(home: &Path) -> Vec<PathBuf> {
    let sandboxed = crate::tool_paths::is_tool_sync_sandboxed();
    if cfg!(target_os = "windows") {
        let local = if sandboxed {
            home.join("AppData").join("Local")
        } else {
            match std::env::var_os("LOCALAPPDATA") {
                Some(dir) if !dir.is_empty() => PathBuf::from(dir),
                _ => home.join("AppData").join("Local"),
            }
        };
        return vec![
            windows_claude_dir(&local, "Claude", false),
            windows_claude_dir(&local, "Claude-3p", true),
        ];
    }
    if cfg!(target_os = "macos") {
        let root = home.join("Library").join("Application Support");
        return vec![root.join("Claude"), root.join("Claude-3p")];
    }
    let root = if sandboxed {
        home.join(".config")
    } else {
        match std::env::var_os("XDG_CONFIG_HOME") {
            Some(dir) if !dir.is_empty() && Path::new(&dir).is_absolute() => PathBuf::from(dir),
            _ => home.join(".config"),
        }
    };
    vec![root.join("Claude"), root.join("Claude-3p")]
}

/// On Windows, `%LOCALAPPDATA%\Claude` (or `Claude-3p`); when absent, fall
/// back to the first directory whose name starts with `Claude` (with or
/// without `-3p`, consistent with threep) (magpie windowsClaudeDir
/// precedent).
fn windows_claude_dir(local: &Path, name: &str, threep: bool) -> PathBuf {
    let exact = local.join(name);
    if exact.is_dir() {
        return exact;
    }
    let Ok(entries) = std::fs::read_dir(local) else {
        return exact;
    };
    let mut found: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| {
            let file_name = entry.file_name();
            let Some(name) = file_name.to_str() else { return false };
            entry.path().is_dir()
                && name.starts_with("Claude")
                && name.contains("-3p") == threep
        })
        .map(|entry| entry.path())
        .collect();
    if found.is_empty() {
        return exact;
    }
    found.sort();
    found.remove(0)
}

/// The Claude Code home a Cowork session carries with it:
/// `<dir>/local-agent-mode-sessions/*/*/local_*/.claude`.
pub(super) fn cowork_claude_homes(dir: &Path) -> Vec<PathBuf> {
    let sessions = dir.join("local-agent-mode-sessions");
    let mut out = Vec::new();
    let Ok(first) = std::fs::read_dir(&sessions) else {
        return out;
    };
    for a in first.flatten() {
        let Ok(second) = std::fs::read_dir(a.path()) else {
            continue;
        };
        for b in second.flatten() {
            let Ok(local) = std::fs::read_dir(b.path()) else {
                continue;
            };
            for entry in local.flatten() {
                let file_name = entry.file_name();
                let Some(name) = file_name.to_str() else { continue };
                if name.starts_with("local_") {
                    out.push(entry.path().join(".claude"));
                }
            }
        }
    }
    out
}

/// File discovery for the projects JSONL layout (magpie ccFiles precedent):
/// `projects/<project>/<id>.jsonl` (main sessions) and
/// `<id>/subagents/*.jsonl`.
pub(super) fn cc_files(agent: &'static str, projects: &Path) -> Vec<SessionFile> {
    let mut out = Vec::new();
    let Ok(project_entries) = std::fs::read_dir(projects) else {
        return out;
    };
    for project in project_entries.flatten() {
        let root = project.path();
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                let subagents = path.join("subagents");
                let Ok(subs) = std::fs::read_dir(&subagents) else {
                    continue;
                };
                for sub in subs.flatten() {
                    let sub_path = sub.path();
                    if is_jsonl(&sub_path) {
                        push_session_file(&mut out, agent, &sub_path);
                    }
                }
            } else if is_jsonl(&path) {
                push_session_file(&mut out, agent, &path);
            }
        }
    }
    out
}

fn is_jsonl(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == "jsonl")
}

fn push_session_file(out: &mut Vec<SessionFile>, agent: &'static str, path: &Path) {
    let Ok(meta) = std::fs::metadata(path) else {
        return;
    };
    if !meta.is_file() {
        return;
    }
    // mtime is used only for cross-file ordering (earliest file first); a
    // missing value yields 0, which does not affect correctness.
    let modified_ms = meta.modified().ok().map(system_time_ms).unwrap_or(0);
    out.push(SessionFile {
        agent,
        path: path.to_path_buf(),
        size: meta.len(),
        modified_ms,
    });
}

fn system_time_ms(time: std::time::SystemTime) -> i64 {
    time.duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// The session id for a session file path: `<id>.jsonl`, or `<id>` taken
/// from `<id>/subagents/<agent>.jsonl` (magpie sessionOfPath precedent).
pub(super) fn session_of_path(path: &Path) -> String {
    let parent = path.parent();
    if parent
        .and_then(Path::file_name)
        .is_some_and(|name| name == "subagents")
    {
        return parent
            .and_then(Path::parent)
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
    }
    path.file_stem()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}
